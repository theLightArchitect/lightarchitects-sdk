//! Response middleware: injects `lightarchitects-version`, `lightarchitects-beta`,
//! and `lightarchitects-api-version-hash` headers on every response.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, Request, Response};
use axum::middleware::Next;
use std::sync::Arc;

use super::super::api_version::API_VERSION_HASH;
use super::super::state::PlatformState;

/// Inject version headers on every response (OD-6 contract fingerprint).
///
/// Headers injected:
/// - `lightarchitects-version`: ISO date of this gateway revision
/// - `lightarchitects-beta: true`: beta-tier signal for SDK clients
/// - `lightarchitects-api-version-hash`: first 16 hex chars of the contract SHA-256
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
    headers.insert("lightarchitects-beta", HeaderValue::from_static("true"));
    headers.insert(
        "lightarchitects-api-version-hash",
        HeaderValue::from_static(API_VERSION_HASH),
    );
    resp
}
