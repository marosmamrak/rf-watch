//! The Svelte UI, embedded into the binary at compile time from `web/dist/`.

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "web/dist/"]
#[allow_missing = true]
struct Assets;

/// Serves a file of the build; paths without an extension get `index.html`
/// (client-side routes).
pub async fn handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return ([(header::CONTENT_TYPE, mime.as_ref())], file.data).into_response();
    }
    if !path.contains('.') {
        if let Some(index) = Assets::get("index.html") {
            return ([(header::CONTENT_TYPE, "text/html")], index.data).into_response();
        }
    }
    let msg = if Assets::get("index.html").is_none() {
        "UI not built: run `npm ci && npm run build` in web/ and rebuild"
    } else {
        "not found"
    };
    (StatusCode::NOT_FOUND, msg).into_response()
}
