//! Bearer read-token authentication middleware.
//!
//! Applies to all platform read endpoints except `/v1/platform/health` and
//! `/v1/admin/*` (admin has its own `x-admin-token` auth in the handler).
//!
//! When `state.read_token` is `None` the middleware is a pass-through —
//! read endpoints are freely accessible under the localhost trust model.
//! When `Some`, requests lacking a valid `Authorization: Bearer <token>`
//! header receive HTTP 401.
//!
//! The middleware also guards against scope confusion: a request that carries
//! a valid read bearer token directed at an `/v1/admin/*` path receives
//! HTTP 403 rather than proceeding to the admin handler.

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, Response, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use secrecy::ExposeSecret;
use serde_json::json;
use std::sync::Arc;
use subtle::ConstantTimeEq;

use super::super::state::PlatformState;

/// Read auth + scope enforcement middleware.
pub async fn read_auth_middleware(
    State(state): State<Arc<PlatformState>>,
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let path = req.uri().path();

    // Health probe — always bypass.
    if path == "/v1/platform/health" {
        return next.run(req).await;
    }

    let bearer = extract_bearer(req.headers());

    // Admin paths: read bearer → 403 (wrong scope). Admin token is checked separately in the handler.
    if path.starts_with("/v1/admin/") {
        if let (Some(stored), Some(provided)) = (&state.read_token, bearer.as_deref()) {
            let matches: bool = stored
                .expose_secret()
                .as_str()
                .as_bytes()
                .ct_eq(provided.as_bytes())
                .into();
            if matches {
                return scope_error().into_response();
            }
        }
        return next.run(req).await;
    }

    // Read paths: if read_token configured, require valid bearer.
    if let Some(stored) = &state.read_token {
        match bearer.as_deref() {
            None => return unauthorized().into_response(),
            Some(provided) => {
                let ok: bool = stored
                    .expose_secret()
                    .as_str()
                    .as_bytes()
                    .ct_eq(provided.as_bytes())
                    .into();
                if !ok {
                    return unauthorized().into_response();
                }
            }
        }
    }

    next.run(req).await
}

/// Extract the bearer token value from the `Authorization` header, if present.
fn extract_bearer(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// HTTP 401 — no or invalid token.
fn unauthorized() -> impl IntoResponse {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(json!({
            "error": {
                "code": "unauthorized",
                "message": "Valid Authorization: Bearer <token> required.",
                "status": 401
            }
        })),
    )
}

/// HTTP 403 — valid read token used against an admin-scope endpoint.
fn scope_error() -> impl IntoResponse {
    (
        StatusCode::FORBIDDEN,
        axum::Json(json!({
            "error": {
                "code": "insufficient_scope",
                "message": "Read token cannot access admin endpoints. Use x-admin-token.",
                "status": 403
            }
        })),
    )
}
