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

struct StaticReply {
    response: Response<Body>,
    app_id: Option<uuid::Uuid>,
}

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
    let reply = if uri.path() == "/healthz" {
        StaticReply {
            response: text_response(StatusCode::OK, "ok\n"),
            app_id: None,
        }
    } else {
        build_static_response(&store, &host, uri.path())
    };
    store.record_red(
        reply.app_id,
        started.elapsed().as_millis(),
        reply.response.status().as_u16(),
    );
    reply.response
}

fn build_static_response(store: &Store, host: &str, request_path: &str) -> StaticReply {
    let app = store.find_app_for_host(host).or_else(|| {
        let config = store.read();
        config.apps.into_iter().find(|app| app.hostnames.is_empty())
    });
    let Some(app) = app else {
        return error_reply(StatusCode::NOT_FOUND, None);
    };
    let app_id = Some(app.id);
    let Some(root) = store.current_release_dir(&app) else {
        return error_reply(StatusCode::SERVICE_UNAVAILABLE, app_id);
    };

    let Some(path) = resolve_path(&root, request_path) else {
        return error_reply(StatusCode::NOT_FOUND, app_id);
    };
    let Ok(bytes) = fs::read(&path) else {
        return error_reply(StatusCode::NOT_FOUND, app_id);
    };
    let Ok(metadata) = fs::metadata(&path) else {
        return error_reply(StatusCode::NOT_FOUND, app_id);
    };
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
    match builder.body(Body::from(bytes)) {
        Ok(response) => StaticReply { response, app_id },
        Err(_) => error_reply(StatusCode::INTERNAL_SERVER_ERROR, app_id),
    }
}

fn error_reply(status: StatusCode, app_id: Option<uuid::Uuid>) -> StaticReply {
    StaticReply {
        response: simple_response(status, status.canonical_reason().unwrap_or("error")),
        app_id,
    }
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
