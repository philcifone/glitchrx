//! The embedded frontend (see `build.rs`).

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// Serve an embedded file. `/` is `index.html`. Vite puts hashed file names
/// under `assets/`, so those can be cached forever; everything else is
/// revalidated so a rebuilt binary is picked up at once.
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match ASSETS.iter().find(|(p, _, _)| *p == path) {
        Some((p, mime, bytes)) => {
            let cache = if p.starts_with("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            (
                [
                    (header::CONTENT_TYPE, *mime),
                    (header::CACHE_CONTROL, cache),
                ],
                *bytes,
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found\n").into_response(),
    }
}
