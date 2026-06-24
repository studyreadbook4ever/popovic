use crate::store::Store;
use axum::{
    body::Body,
    extract::{OriginalUri, State},
    http::{HeaderMap, HeaderValue, Response, StatusCode, header},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Instant, UNIX_EPOCH},
};

pub async fn serve_static_app(
    State(store): State<Store>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> Response<Body> {
    let started = Instant::now();
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let response = if uri.path() == "/healthz" {
        text_response(StatusCode::OK, "ok\n")
    } else {
        match build_static_response(&store, &host, uri.path()) {
            Ok(response) => response,
            Err(status) => simple_response(status, status.canonical_reason().unwrap_or("error")),
        }
    };
    store.record_red(started.elapsed().as_millis(), response.status().as_u16());
    response
}

fn build_static_response(
    store: &Store,
    host: &str,
    request_path: &str,
) -> Result<Response<Body>, StatusCode> {
    let app = store.find_app_for_host(host).or_else(|| {
        let config = store.read();
        config.apps.into_iter().find(|app| app.hostnames.is_empty())
    });
    let Some(app) = app else {
        return Err(StatusCode::NOT_FOUND);
    };
    let Some(root) = store.current_release_dir(&app) else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let path = resolve_path(&root, request_path).ok_or(StatusCode::NOT_FOUND)?;
    let bytes = fs::read(&path).map_err(|_| StatusCode::NOT_FOUND)?;
    let metadata = fs::metadata(&path).map_err(|_| StatusCode::NOT_FOUND)?;
    let mime = mime_for(&path);

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, cache_control(&path))
        .header(header::ETAG, etag(&metadata));
    if let Ok(modified) = metadata.modified() {
        if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
            builder = builder.header("x-popovic-modified-unix", duration.as_secs().to_string());
        }
    }
    builder
        .body(Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn resolve_path(root: &Path, request_path: &str) -> Option<PathBuf> {
    let clean = clean_path(request_path)?;
    let direct = root.join(&clean);
    if direct.is_file() {
        return Some(direct);
    }
    let index = root.join(&clean).join("index.html");
    if index.is_file() {
        return Some(index);
    }
    let html = root.join(format!("{clean}.html"));
    if html.is_file() {
        return Some(html);
    }
    let fallback = root.join("index.html");
    fallback.is_file().then_some(fallback)
}

fn clean_path(path: &str) -> Option<String> {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Some("index.html".to_string());
    }
    if trimmed.contains("..") || trimmed.contains('\\') {
        return None;
    }
    Some(trimmed.to_string())
}

fn cache_control(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
    {
        "html" | "htm" | "txt" | "xml" | "json" | "webmanifest" => "no-cache",
        _ => "public, max-age=31536000, immutable",
    }
}

fn etag(metadata: &fs::Metadata) -> String {
    let len = metadata.len();
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("\"{len:x}-{modified:x}\"")
}

fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "tsv" => "text/tab-separated-values; charset=utf-8",
        "md" | "markdown" => "text/markdown; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "avif" => "image/avif",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "webmanifest" => "application/manifest+json; charset=utf-8",
        "pdf" => "application/pdf",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "wasm" => "application/wasm",
        "map" => "application/json; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn text_response(status: StatusCode, message: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        )
        .header(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))
        .body(Body::from(message.to_string()))
        .expect("static response builds")
}

fn simple_response(status: StatusCode, message: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        )
        .body(Body::from(message.to_string()))
        .expect("static response builds")
}
