//! Embedded static assets served from `lightarchitects-webshell-ui/dist/`.
//! Rebuilt: 2026-04-20 — BUG-004/005/006 fixes.
//!
//! The SPA is the Svelte Mockcli frontend (`~/Projects/lightarchitects-webshell-ui`).
//! Its built bundle is baked into the binary at compile time via
//! [`rust_embed`] so the webshell ships as a single self-contained artifact.
//!
//! To rebuild after frontend changes:
//! ```bash
//! cd ~/Projects/lightarchitects-webshell-ui && pnpm build
//! cd ~/Projects/lightarchitects-sdk/lightarchitects-webshell && cargo build --release
//! ```

use axum::{
    body::Body,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;

/// Embedded static asset bundle — every file under the Svelte Mockcli `dist/`.
#[derive(Embed)]
#[folder = "../lightarchitects-webshell-ui/dist/"]
pub struct Assets;

/// Serves a static asset by request path.
///
/// - Empty path (`/`) resolves to `index.html`.
/// - Known asset paths (found in the embedded bundle) are served directly.
/// - Unknown paths fall back to `index.html` to support Svelte client-side routing.
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

    // SPA fallback: serve index.html for unknown paths so the Svelte router
    // handles them client-side.
    if let Some(index) = Assets::get("index.html") {
        return Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(index.data.into_owned()))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    StatusCode::NOT_FOUND.into_response()
}
