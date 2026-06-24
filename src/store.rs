use crate::{
    bootstrap,
    models::{
        AppSnapshot, Config, DashboardSnapshot, HealthState, MetricBucket, RETENTION_DAYS,
        RedMetrics, StaticApp, TunnelStatus,
    },
};
use chrono::{Duration, Utc};
use serde::Serialize;
use std::{
    env, fs,
    io::{self, ErrorKind},
    path::PathBuf,
    sync::{Arc, RwLock},
};
use uuid::Uuid;

#[derive(Clone)]
pub struct Store {
    root: Arc<PathBuf>,
    config_path: Arc<PathBuf>,
    inner: Arc<RwLock<Config>>,
}

impl Store {
    pub fn open() -> io::Result<Self> {
        let root = data_root();
        fs::create_dir_all(root.join("repos"))?;
        fs::create_dir_all(root.join("releases"))?;
        fs::create_dir_all(root.join("staging"))?;
        fs::create_dir_all(root.join("logs"))?;

        let config_path = root.join("popovic.json");
        let mut config = match fs::read_to_string(&config_path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(error) if error.kind() == ErrorKind::NotFound => Config::default(),
            Err(error) => return Err(error),
        };
        bootstrap::apply_env_overrides(&mut config);

        let store = Self {
            root: Arc::new(root),
            config_path: Arc::new(config_path),
            inner: Arc::new(RwLock::new(config)),
        };
        store.save()?;
        Ok(store)
    }

    pub fn releases_root(&self) -> PathBuf {
        self.root.join("releases")
    }

    pub fn repos_root(&self) -> PathBuf {
        self.root.join("repos")
    }

    pub fn read(&self) -> Config {
        self.inner.read().expect("store lock poisoned").clone()
    }

    pub fn update<R>(&self, f: impl FnOnce(&mut Config) -> R) -> io::Result<R> {
        let result = {
            let mut config = self.inner.write().expect("store lock poisoned");
            let result = f(&mut config);
            prune_config(&mut config);
            result
        };
        self.save()?;
        Ok(result)
    }

    pub fn save(&self) -> io::Result<()> {
        let config = self.inner.read().expect("store lock poisoned").clone();
        let content = to_pretty_json(&config)?;
        let tmp = self.config_path.with_extension("json.tmp");
        fs::write(&tmp, content)?;
        fs::rename(tmp, self.config_path.as_ref())?;
        Ok(())
    }

    pub fn app_release_dir(&self, app: &StaticApp, release: &str) -> PathBuf {
        self.releases_root()
            .join(safe_name(&app.name))
            .join(release)
    }

    pub fn current_release_dir(&self, app: &StaticApp) -> Option<PathBuf> {
        app.current_release
            .as_ref()
            .map(|release| self.app_release_dir(app, release))
    }

    pub fn find_app_for_host(&self, host: &str) -> Option<StaticApp> {
        let host_without_port = host.split(':').next().unwrap_or(host);
        self.read().apps.into_iter().find(|app| {
            app.hostnames
                .iter()
                .any(|candidate| candidate == host_without_port)
        })
    }

    pub fn find_app(&self, app_id: Uuid) -> Option<StaticApp> {
        self.read().apps.into_iter().find(|app| app.id == app_id)
    }

    pub fn dashboard_snapshot(&self) -> DashboardSnapshot {
        let config = self.read();
        let latest = config.metric_buckets.last().cloned();
        let host = latest
            .as_ref()
            .map(|bucket| bucket.host.clone())
            .unwrap_or_default();
        let red = latest
            .as_ref()
            .map(|bucket| bucket.red.clone())
            .unwrap_or_default();

        let running_apps = config
            .apps
            .iter()
            .map(|app| AppSnapshot {
                id: app.id,
                name: app.name.clone(),
                domains: app.hostnames.clone(),
                status: app_status(app),
                last_deploy: app
                    .last_deploy_at
                    .map(|time| time.to_rfc3339())
                    .unwrap_or_else(|| "never".to_string()),
                requests_5m: red.requests,
                errors_5m: red.errors,
            })
            .collect();

        DashboardSnapshot {
            running_apps,
            tunnel_status: TunnelStatus {
                status: if config.settings.cloudflare_tunnel_id.is_empty() {
                    "not configured".to_string()
                } else {
                    "configured".to_string()
                },
                tunnel_id: config.settings.cloudflare_tunnel_id,
                route_count: config.apps.iter().map(|app| app.hostnames.len()).sum(),
            },
            cpu_percent: host.cpu_percent,
            ram_percent: host.ram_percent,
            alerts: config.alerts,
            use_red: config.metric_buckets,
        }
    }

    pub fn record_red(&self, elapsed_ms: u128, status: u16) {
        let _ = self.update(|config| {
            let bucket = ensure_current_bucket(config);
            bucket.red.requests += 1;
            if status >= 400 {
                bucket.red.errors += 1;
            }
            let elapsed = elapsed_ms as f32;
            bucket.red.p95_ms = bucket.red.p95_ms.max(elapsed);
            bucket.red.p99_ms = bucket.red.p99_ms.max(elapsed);
        });
    }

    pub fn record_host_metrics(&self, host: crate::metrics::HostSample) {
        let _ = self.update(|config| {
            let bucket = ensure_current_bucket(config);
            bucket.host.cpu_percent = host.cpu_percent;
            bucket.host.ram_percent = host.ram_percent;
            bucket.host.load1 = host.load1;
            bucket.host.disk_percent = host.disk_percent;
        });
    }
}

fn ensure_current_bucket(config: &mut Config) -> &mut MetricBucket {
    let now = Utc::now();
    let bucket_start = now
        - Duration::seconds(now.timestamp().rem_euclid(300))
        - Duration::nanoseconds(now.timestamp_subsec_nanos() as i64);
    let should_push = config
        .metric_buckets
        .last()
        .map(|bucket| bucket.start != bucket_start)
        .unwrap_or(true);
    if should_push {
        config.metric_buckets.push(MetricBucket {
            start: bucket_start,
            host: Default::default(),
            red: RedMetrics::default(),
        });
    }
    config.metric_buckets.last_mut().expect("bucket exists")
}

fn prune_config(config: &mut Config) {
    let cutoff = Utc::now() - Duration::days(RETENTION_DAYS);
    config
        .metric_buckets
        .retain(|bucket| bucket.start >= cutoff);
    config.alerts.retain(|alert| alert.created_at >= cutoff);
    config.agent_tasks.retain(|task| task.created_at >= cutoff);
}

fn to_pretty_json<T: Serialize>(value: &T) -> io::Result<String> {
    serde_json::to_string_pretty(value)
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
}

fn app_status(app: &StaticApp) -> String {
    match &app.health {
        HealthState::Healthy => "healthy".to_string(),
        HealthState::Unhealthy(reason) => format!("unhealthy: {reason}"),
        HealthState::Unknown => match &app.current_release {
            Some(_) => "running".to_string(),
            None => "not deployed".to_string(),
        },
    }
}

fn data_root() -> PathBuf {
    if let Ok(path) = env::var("POPOVIC_HOME") {
        return PathBuf::from(path);
    }
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/share/popovic")
}

pub fn safe_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    if out.is_empty() {
        "app".to_string()
    } else {
        out
    }
}
