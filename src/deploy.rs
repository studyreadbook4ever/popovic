use crate::{
    models::{Alert, DeployStatus, HealthState, StaticApp},
    secret,
    store::{Store, safe_name},
};
use chrono::Utc;
use std::{
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    process::Command,
};
use uuid::Uuid;

pub fn register_app(
    store: &Store,
    name: String,
    repo_url: String,
    repo_subdir: String,
    hostnames: Vec<String>,
) -> io::Result<Uuid> {
    let app = StaticApp::new(name, repo_url, repo_subdir, hostnames);
    let id = app.id;
    store.update(|config| {
        config.apps.retain(|existing| existing.name != app.name);
        config.apps.push(app);
    })?;
    Ok(id)
}

pub fn deploy_app(store: &Store, app_id: Uuid) -> io::Result<()> {
    let mut app = store
        .find_app(app_id)
        .ok_or_else(|| missing("app not found"))?;
    let source = prepare_source(store, &app)?;
    let release_id = Utc::now().format("%Y%m%d%H%M%S").to_string();
    let release_dir = store.app_release_dir(&app, &release_id);
    fs::create_dir_all(&release_dir)?;

    let source_subdir = if app.repo_subdir.trim().is_empty() {
        source
    } else {
        source.join(app.repo_subdir.trim())
    };
    validate_static_source(&source_subdir)?;
    copy_static_tree(&source_subdir, &release_dir)?;

    app.previous_release = app.current_release.clone();
    app.current_release = Some(release_id);
    app.last_deploy_at = Some(Utc::now());
    app.last_deploy_status = DeployStatus::Succeeded;
    app.health = HealthState::Healthy;
    store.update(|config| replace_app(config, app))?;
    Ok(())
}

pub fn rollback_app(store: &Store, app_id: Uuid) -> io::Result<()> {
    let mut app = store
        .find_app(app_id)
        .ok_or_else(|| missing("app not found"))?;
    let previous = app
        .previous_release
        .clone()
        .ok_or_else(|| missing("no previous release to roll back to"))?;
    let current = app.current_release.clone();
    app.current_release = Some(previous);
    app.previous_release = current;
    app.last_deploy_at = Some(Utc::now());
    app.last_deploy_status = DeployStatus::Succeeded;
    app.health = HealthState::Healthy;
    store.update(|config| replace_app(config, app))?;
    Ok(())
}

pub fn apply_file_changes(
    store: &Store,
    app_id: Uuid,
    changes: &[(String, String)],
) -> io::Result<()> {
    let mut app = store
        .find_app(app_id)
        .ok_or_else(|| missing("app not found"))?;
    let current_dir = store
        .current_release_dir(&app)
        .ok_or_else(|| missing("app has no current release"))?;
    let release_id = Utc::now().format("%Y%m%d%H%M%S").to_string();
    let release_dir = store.app_release_dir(&app, &release_id);
    copy_static_tree(&current_dir, &release_dir)?;

    for (relative, content) in changes {
        let path = safe_site_path(&release_dir, relative)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
    }

    validate_static_source(&release_dir)?;
    app.previous_release = app.current_release.clone();
    app.current_release = Some(release_id);
    app.last_deploy_at = Some(Utc::now());
    app.last_deploy_status = DeployStatus::Succeeded;
    app.health = HealthState::Healthy;
    store.update(|config| replace_app(config, app))?;
    Ok(())
}

fn prepare_source(store: &Store, app: &StaticApp) -> io::Result<PathBuf> {
    let repo_url = app.repo_url.trim();
    if repo_url.is_empty() {
        return Err(missing("repo url is required"));
    }
    let local_path = PathBuf::from(repo_url);
    if local_path.exists() {
        return Ok(local_path);
    }

    let repo_dir = store.repos_root().join(safe_name(&app.name));
    if repo_dir.exists() {
        run_git(&repo_dir, &["pull", "--ff-only"])?;
    } else {
        let clone_url = authenticated_github_url(store, repo_url);
        run_git(
            store.repos_root(),
            &["clone", &clone_url, repo_dir.to_string_lossy().as_ref()],
        )?;
    }
    Ok(repo_dir)
}

fn authenticated_github_url(store: &Store, repo_url: &str) -> String {
    let config = store.read();
    let token = secret::open(&config.settings.github_oauth_token_sealed);
    if token.is_empty() || !repo_url.starts_with("https://github.com/") {
        return repo_url.to_string();
    }
    repo_url.replacen(
        "https://github.com/",
        &format!("https://x-access-token:{token}@github.com/"),
        1,
    )
}

fn run_git<P: AsRef<Path>>(dir: P, args: &[&str]) -> io::Result<()> {
    let output = Command::new("git").args(args).current_dir(dir).output()?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(io::Error::new(
        ErrorKind::Other,
        format!("git command failed: {stderr}"),
    ))
}

fn validate_static_source(source: &Path) -> io::Result<()> {
    if !source.exists() {
        return Err(missing("static source directory does not exist"));
    }
    if !source.join("index.html").exists() {
        return Err(missing("static source must contain index.html"));
    }
    Ok(())
}

fn copy_static_tree(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let file_name = entry.file_name();
        if file_name == ".git" {
            continue;
        }
        let target_path = target.join(file_name);
        if source_path.is_dir() {
            copy_static_tree(&source_path, &target_path)?;
        } else if allowed_static_file(&source_path) {
            fs::copy(&source_path, &target_path)?;
        }
    }
    Ok(())
}

pub fn allowed_static_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    if matches!(
        name,
        "robots.txt" | "agents.txt" | "sitemap.xml" | "favicon.ico"
    ) {
        return true;
    }
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "html"
            | "htm"
            | "css"
            | "js"
            | "mjs"
            | "json"
            | "map"
            | "csv"
            | "tsv"
            | "md"
            | "markdown"
            | "png"
            | "jpg"
            | "jpeg"
            | "avif"
            | "gif"
            | "webp"
            | "svg"
            | "ico"
            | "woff"
            | "woff2"
            | "ttf"
            | "otf"
            | "txt"
            | "xml"
            | "webmanifest"
            | "pdf"
            | "mp3"
            | "mp4"
            | "wasm"
    )
}

fn safe_site_path(root: &Path, relative: &str) -> io::Result<PathBuf> {
    let relative = relative.trim_start_matches('/');
    if relative.contains("..") {
        return Err(missing("path traversal is not allowed"));
    }
    Ok(root.join(relative))
}

fn replace_app(config: &mut crate::models::Config, app: StaticApp) {
    if let Some(existing) = config
        .apps
        .iter_mut()
        .find(|existing| existing.id == app.id)
    {
        *existing = app;
    }
}

fn missing(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, message)
}

pub fn deploy_failed(store: &Store, app_id: Uuid, error: String) {
    let _ = store.update(|config| {
        if let Some(app) = config.apps.iter_mut().find(|app| app.id == app_id) {
            app.last_deploy_status = DeployStatus::Failed(error.clone());
            app.health = HealthState::Unhealthy(error.clone());
        }
        config.alerts.push(Alert::warning("deploy", error));
    });
}
