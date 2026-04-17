//! Embedded static assets served from `web/dist/`.
//!
//! During Phase 1 there is only a placeholder `index.html`. The full React
//! frontend lands in Phase 6 and gets baked into this embed at compile time
//! via [`rust_embed`] so the webshell ships as a single self-contained binary.

use axum::{
    body::Body,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;

/// Embedded static asset bundle. Includes every file under `web/dist/`
/// relative to this crate's root.
#[derive(Embed)]
#[folder = "web/dist/"]
pub struct Assets;

/// Serves a static asset by request path.
///
/// - Empty path (`/`) resolves to `index.html`.
/// - Known asset paths (found in the embedded bundle) are served directly.
/// - Unknown paths fall back to `index.html` to support client-side routing
///   (React Router picks up the path and renders the correct component).
/// - Returns 404 only when `index.html` itself is not found in the bundle
///   (which means the frontend was not compiled before the binary was built).
/// - MIME types come from `rust-embed`'s built-in guesser.
///
/// Registered as the router's fallback so API routes take precedence.
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let resolved = if path.is_empty() { "index.html" } else { path };

    // Try to serve the exact file first.
    if let Some(file) = Assets::get(resolved) {
        let mime = file.metadata.mimetype().to_owned();
        return Response::builder()
            .header(header::CONTENT_TYPE, mime)
            .body(Body::from(file.data.into_owned()))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    // SPA fallback: serve index.html for unknown paths so React Router can
    // handle them client-side.  This is the standard pattern for SPAs served
    // from a static file server — the server hands every unmatched path to the
    // frontend and lets the JS router decide what to render.
    if let Some(index) = Assets::get("index.html") {
        return Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(index.data.into_owned()))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    StatusCode::NOT_FOUND.into_response()
}
