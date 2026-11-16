//! The window's UI served from the daemon itself, so a browser tab can be the control panel.
//!
//! The daemon owns every bot; a browser tab (like the Tauri window) only reads and changes them over
//! `/v1`. Closing the tab changes nothing. Serving the built UI (`desktop/dist`) from this same port
//! means `http://127.0.0.1:47821/` is a working page with no CORS and no second server, which is what
//! the local process_monitor panel links to. Routes under `/v1` always win; this is only the fallback.
//!
//! Where the files come from: `EMPIRE_WEB_DIR` when set, else `desktop/dist` under the working
//! directory (what `cargo run -p empire-daemon` and process_monitor use). The app's own `--service`
//! process has neither, so it serves no page and behaves exactly as before.

use std::path::{Component, Path, PathBuf};

use axum::{
    Router,
    body::Body,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};

/// The UI folder, if one exists to serve.
pub fn web_dir() -> Option<PathBuf> {
    let configured = std::env::var("EMPIRE_WEB_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from);
    let dir = configured.unwrap_or_else(|| PathBuf::from("desktop/dist"));
    dir.join("index.html").is_file().then_some(dir)
}

/// Add the UI as the router's fallback when there is one to serve.
pub fn attach<S: Clone + Send + Sync + 'static>(router: Router<S>, dir: Option<PathBuf>) -> Router<S> {
    match dir {
        Some(dir) => router.fallback(move |uri: Uri| {
            let dir = dir.clone();
            async move { serve_file(&dir, uri.path()).await }
        }),
        None => router,
    }
}

async fn serve_file(dir: &Path, request_path: &str) -> Response {
    let Some(relative) = safe_relative(request_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let path = if relative.as_os_str().is_empty() { dir.join("index.html") } else { dir.join(&relative) };
    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            [(header::CONTENT_TYPE, content_type(&path)), (header::CACHE_CONTROL, "no-cache")],
            Body::from(bytes),
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// The request path as a path inside the UI folder, refusing anything that could leave it.
fn safe_relative(request_path: &str) -> Option<PathBuf> {
    let trimmed = request_path.trim_start_matches('/');
    let candidate = Path::new(trimmed);
    candidate
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
        .then(|| candidate.to_path_buf())
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::safe_relative;

    #[test]
    fn paths_inside_the_folder_are_served() {
        assert_eq!(safe_relative("/").unwrap().as_os_str(), "");
        assert_eq!(safe_relative("/assets/index-abc.js").unwrap().to_str(), Some("assets/index-abc.js"));
    }

    #[test]
    fn paths_that_climb_out_are_refused() {
        assert!(safe_relative("/../secret").is_none());
        assert!(safe_relative("/assets/../../x").is_none());
        assert!(safe_relative("/./index.html").is_none());
    }
}
