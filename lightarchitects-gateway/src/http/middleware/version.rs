//! Response middleware: injects `lightarchitects-version` and `lightarchitects-beta` headers.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, Request, Response};
use axum::middleware::Next;
use std::sync::Arc;

use super::super::state::PlatformState;

/// Inject `lightarchitects-version` + `lightarchitects-beta: true` on every response.
pub async fn version_header_middleware(
    State(state): State<Arc<PlatformState>>,
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let mut resp = next.run(req).await;
    let headers = resp.headers_mut();

    if let Ok(v) = HeaderValue::from_str(&state.config.version_date) {
        headers.insert("lightarchitects-version", v);
    }
    headers.insert(
        "lightarchitects-beta",
        HeaderValue::from_static("true"),
    );
    resp
}
