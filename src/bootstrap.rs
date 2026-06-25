use crate::{
    deploy,
    models::{Config, StaticApp},
    store::{Store, normalize_hostname},
};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
struct BootstrapApp {
    name: String,
    source: PathBuf,
    hostnames: Vec<String>,
}

pub fn run(store: &Store) -> io::Result<()> {
    if !env_bool("POPOVIC_BOOTSTRAP", true) {
        return Ok(());
    }

    let redeploy = env_bool("POPOVIC_BOOTSTRAP_REDEPLOY", true);
    for app in discover_apps()? {
        let app_id = upsert_app(store, &app)?;
        let should_deploy = redeploy
            || store
                .find_app(app_id)
                .and_then(|item| item.current_release)
                .is_none();
        if should_deploy {
            if let Err(error) = deploy::deploy_app(store, app_id) {
                deploy::deploy_failed(store, app_id, error.to_string());
            }
        }
    }
    Ok(())
}

pub fn apply_env_overrides(config: &mut Config) {
    if let Ok(addr) = env::var("POPOVIC_DASHBOARD_ADDR") {
        if !addr.trim().is_empty() {
            config.dashboard_addr = addr.trim().to_string();
        }
    }
    if let Ok(addr) = env::var("POPOVIC_STATIC_ADDR") {
        if !addr.trim().is_empty() {
            config.static_addr = addr.trim().to_string();
        }
    }
}

fn discover_apps() -> io::Result<Vec<BootstrapApp>> {
    let mut apps = Vec::new();

    let single_root = env_path("POPOVIC_SITE_ROOT", "/site");
    if single_root.join("index.html").exists() {
        apps.push(BootstrapApp {
            name: env::var("POPOVIC_SITE_NAME").unwrap_or_else(|_| "site".to_string()),
            hostnames: hostnames_for(&single_root, "POPOVIC_SITE_HOSTS"),
            source: single_root,
        });
    }

    let sites_root = env_path("POPOVIC_SITES_ROOT", "/sites");
    if sites_root.is_dir() {
        for entry in fs::read_dir(sites_root)? {
            let entry = entry?;
            let path = entry.path();
            if !path.join("index.html").exists() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let env_name = format!("POPOVIC_HOSTS_{}", env_key(&name));
            apps.push(BootstrapApp {
                name,
                hostnames: hostnames_for(&path, &env_name),
                source: path,
            });
        }
    }

    Ok(apps)
}

fn upsert_app(store: &Store, app: &BootstrapApp) -> io::Result<uuid::Uuid> {
    let source = app.source.to_string_lossy().to_string();
    let existing = store
        .read()
        .apps
        .into_iter()
        .find(|item| item.name == app.name);
    match existing {
        Some(existing) => {
            let mut updated = existing.clone();
            updated.repo_url = source;
            updated.repo_subdir.clear();
            updated.hostnames = app.hostnames.clone();
            store.update(|config| replace_app(config, updated))?;
            Ok(existing.id)
        }
        None => deploy::register_app(
            store,
            app.name.clone(),
            source,
            String::new(),
            app.hostnames.clone(),
        ),
    }
}

fn hostnames_for(path: &Path, env_name: &str) -> Vec<String> {
    if let Ok(value) = env::var(env_name) {
        return split_hostnames(&value);
    }
    let hosts_file = path.join(".popovic-hosts");
    fs::read_to_string(hosts_file)
        .map(|value| split_hostnames(&value))
        .unwrap_or_default()
}

fn split_hostnames(value: &str) -> Vec<String> {
    value
        .split([',', '\n'])
        .map(normalize_hostname)
        .filter(|item| !item.is_empty())
        .collect()
}

fn env_path(name: &str, default: &str) -> PathBuf {
    env::var(name)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default))
}

fn env_key(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn replace_app(config: &mut Config, app: StaticApp) {
    if let Some(existing) = config
        .apps
        .iter_mut()
        .find(|existing| existing.id == app.id)
    {
        *existing = app;
    }
}

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "y" | "on"
            )
        })
        .unwrap_or(default)
}
