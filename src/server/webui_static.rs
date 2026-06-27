//! Embedded WebUI static assets (built from `webui/dist` via `rust-embed`).
//! Serves the SPA at `/ui` with a fallback to `index.html` for unknown paths.

use axum::body::Body;
use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "webui/dist/"]
struct WebuiAssets;

pub fn index_html() -> Response {
    asset_response("index.html")
}

pub fn asset_response(path: &str) -> Response {
    match WebuiAssets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.essence_str())
                .body(Body::from(file.data.into_owned()))
                .unwrap()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

pub async fn webui_root() -> Response {
    index_html()
}

pub async fn webui_path(Path(p): Path<String>) -> Response {
    if p.is_empty() {
        return index_html();
    }
    if WebuiAssets::get(&p).is_some() {
        return asset_response(&p);
    }
    // SPA fallback: unknown sub-paths return index.html so the client can render.
    index_html()
}
