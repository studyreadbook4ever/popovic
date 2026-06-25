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
    let release_id = new_release_id();
    let release_dir = store.app_release_dir(&app, &release_id);

    let source_subdir = safe_source_subdir(&source, &app.repo_subdir)?;
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
    let release_id = new_release_id();
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

fn new_release_id() -> String {
    let suffix = Uuid::new_v4().simple().to_string();
    format!("{}-{}", Utc::now().format("%Y%m%d%H%M%S%3f"), &suffix[..8])
}

fn safe_source_subdir(source: &Path, repo_subdir: &str) -> io::Result<PathBuf> {
    let raw = repo_subdir.trim();
    if Path::new(raw).is_absolute() {
        return Err(missing("repo subdir must stay inside the source root"));
    }
    let trimmed = raw.trim_matches('/');
    if trimmed.is_empty() {
        return Ok(source.to_path_buf());
    }
    if trimmed.contains("..") || trimmed.contains('\\') {
        return Err(missing("repo subdir must stay inside the source root"));
    }
    Ok(source.join(trimmed))
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
    let stderr = sanitize_command_output(&String::from_utf8_lossy(&output.stderr));
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
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let target_path = target.join(file_name);
        if file_type.is_dir() {
            copy_static_tree(&source_path, &target_path)?;
        } else if file_type.is_file() && allowed_static_file(&source_path) {
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
    if relative.trim().is_empty() || relative.contains("..") || relative.contains('\\') {
        return Err(missing("path traversal is not allowed"));
    }
    Ok(root.join(relative))
}

fn sanitize_command_output(value: &str) -> String {
    let mut output = value.to_string();
    let prefix = "https://x-access-token:";
    let mut cursor = 0;
    while let Some(relative_start) = output[cursor..].find(prefix) {
        let token_start = cursor + relative_start + prefix.len();
        let Some(relative_end) = output[token_start..].find('@') else {
            break;
        };
        let token_end = token_start + relative_end;
        output.replace_range(token_start..token_end, "***");
        cursor = token_start + 3;
    }
    output
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

#[cfg(test)]
mod tests {
    use super::{new_release_id, safe_source_subdir, sanitize_command_output};
    use std::path::Path;

    #[test]
    fn release_ids_do_not_collide_under_fast_redeploys() {
        let first = new_release_id();
        let second = new_release_id();
        assert_ne!(first, second);
    }

    #[test]
    fn source_subdir_cannot_escape_source_root() {
        let source = Path::new("/site");
        assert_eq!(
            safe_source_subdir(source, "public/assets").unwrap(),
            Path::new("/site/public/assets")
        );
        assert!(safe_source_subdir(source, "../private").is_err());
        assert!(safe_source_subdir(source, "/etc").is_err());
    }

    #[test]
    fn git_errors_do_not_echo_embedded_tokens() {
        let output = sanitize_command_output(
            "fatal: https://x-access-token:secret-value@github.com/user/repo failed",
        );
        assert!(!output.contains("secret-value"));
        assert!(output.contains("x-access-token:***@github.com"));
    }
}
