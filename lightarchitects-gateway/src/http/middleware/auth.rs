//! Bearer read-token authentication middleware with per-IP failure governor.
//!
//! Applies to all platform read endpoints except `/v1/platform/health` and
//! `/v1/admin/*` (admin has its own `x-admin-token` auth in the handler).
//!
//! When `state.read_token` is `None` the middleware is a pass-through —
//! read endpoints are freely accessible under the localhost trust model.
//! When `Some`, requests lacking a valid `Authorization: Bearer <token>`
//! header receive HTTP 401.
//!
//! ### Auth-failure governor
//!
//! Each IP is subject to two independent limits:
//! - **Rate limit** (5 failures/min via `governor`) — excess failures return
//!   HTTP 429 with an exponential `Retry-After` based on the total failure count.
//! - **Hard lockout** (20 total failures) — returns HTTP 429 regardless of
//!   the rate-limit window. Reset on the first successful authentication.
//!
//! The middleware also guards against scope confusion: a request that carries
//! a valid read bearer token directed at an `/v1/admin/*` path receives
//! HTTP 403 rather than proceeding to the admin handler.

use axum::body::Body;
use axum::extract::State;
use axum::extract::connect_info::ConnectInfo;
use axum::http::{Request, Response, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use secrecy::ExposeSecret;
use serde_json::json;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use subtle::{Choice, ConstantTimeEq};

use super::super::state::PlatformState;

/// Maximum number of unique IPs tracked for auth-failure counting.
///
/// At ~40 bytes/entry this bounds the DashMap to ~2 MB regardless of how many
/// distinct source addresses an attacker presents (IPv6 prefix rotation).
const MAX_TRACKED_AUTH_FAIL_IPS: usize = 50_000;

/// Read auth + scope enforcement middleware.
pub async fn read_auth_middleware(
    State(state): State<Arc<PlatformState>>,
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let path = req.uri().path().to_owned();

    // Health probe — always bypass.
    if path == "/v1/platform/health" {
        return next.run(req).await;
    }

    let bearer = extract_bearer(req.headers());

    // Admin paths: read bearer → 403 (wrong scope). Admin token is checked separately in the handler.
    if path.starts_with("/v1/admin/") {
        if let (Some(stored), Some(provided)) = (&state.read_token, bearer.as_deref()) {
            let a = stored.expose_secret().as_str().as_bytes();
            let b = provided.as_bytes();
            let min_len = a.len().min(b.len());
            let len_eq = Choice::from(u8::from(a.len() == b.len()));
            let matches: bool = (len_eq & a[..min_len].ct_eq(&b[..min_len])).into();
            if matches {
                tracing::warn!(ip = %peer_ip(&req), path = %path, "scope confusion: read token on admin path");
                return scope_error().into_response();
            }
        }
        return next.run(req).await;
    }

    // Read paths: if read_token configured, require valid bearer.
    if let Some(stored) = &state.read_token {
        let ip = peer_ip_addr(&req);

        // Hard-lockout check — fastest rejection path (no crypto work).
        if let Some(addr) = ip {
            if state.auth_fail_counts.get(&addr).is_some_and(|c| *c >= 20) {
                tracing::warn!(%addr, "auth: hard lockout (>=20 failures)");
                return hard_lockout().into_response();
            }
        }

        match bearer.as_deref() {
            None => {
                tracing::warn!(ip = %peer_ip(&req), path = %path, "auth failure: missing bearer");
                return on_auth_failure(&state, ip).into_response();
            }
            Some(provided) => {
                let a = stored.expose_secret().as_str().as_bytes();
                let b = provided.as_bytes();
                let min_len = a.len().min(b.len());
                let len_eq = Choice::from(u8::from(a.len() == b.len()));
                let ok: bool = (len_eq & a[..min_len].ct_eq(&b[..min_len])).into();
                if !ok {
                    tracing::warn!(ip = %peer_ip(&req), path = %path, "auth failure: invalid token");
                    return on_auth_failure(&state, ip).into_response();
                }
                // Successful auth — reset the failure counter for this IP.
                if let Some(addr) = ip {
                    state.auth_fail_counts.remove(&addr);
                }
            }
        }
    }

    next.run(req).await
}

/// Record an authentication failure and return the appropriate HTTP response.
///
/// - Increments the persistent per-IP failure counter.
/// - Consumes one token from the per-IP rate limiter.
/// - Returns HTTP 429 with exponential `Retry-After` when the rate limit fires.
/// - Returns HTTP 429 hard-lockout when total failures reach 20.
/// - Returns HTTP 401 otherwise.
fn on_auth_failure(state: &PlatformState, ip: Option<IpAddr>) -> impl IntoResponse {
    let Some(addr) = ip else {
        return unauthorized().into_response();
    };

    // Unbounded growth guard: if the map is already at capacity and this IP is new,
    // return a plain 401 rather than inserting. Known-bad IPs already in the map
    // continue to accumulate normally (they're already occupying a slot).
    if state.auth_fail_counts.len() >= MAX_TRACKED_AUTH_FAIL_IPS
        && !state.auth_fail_counts.contains_key(&addr)
    {
        return unauthorized().into_response();
    }

    // Increment total failure count.
    let new_count = {
        let mut entry = state.auth_fail_counts.entry(addr).or_insert(0);
        *entry = entry.saturating_add(1);
        *entry
    };

    // Hard lockout at 20+ total failures.
    if new_count >= 20 {
        tracing::warn!(%addr, failures = new_count, "auth: hard lockout threshold reached");
        return hard_lockout().into_response();
    }

    // Rate-limit check: >5 failures/min → exponential Retry-After.
    if state.auth_fail_limiter.check_key(&addr).is_err() {
        // Exponential backoff: 2^(bucket) seconds, where bucket = failures / 5.
        let secs = 2_u64.saturating_pow(new_count / 5);
        tracing::warn!(%addr, failures = new_count, retry_after = secs, "auth: rate limited");
        let retry_val = axum::http::HeaderValue::from_str(&secs.to_string())
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("60"));
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(axum::http::header::RETRY_AFTER, retry_val)],
            axum::Json(json!({
                "error": {
                    "code": "auth_rate_limited",
                    "message": "Too many authentication failures. Back off and retry.",
                    "status": 429
                }
            })),
        )
            .into_response();
    }

    unauthorized().into_response()
}

/// Extract the peer IP address from the request extensions.
fn peer_ip_addr(req: &Request<Body>) -> Option<IpAddr> {
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip())
}

/// Extract the peer IP from the request extensions, falling back to `"unknown"`.
///
/// Only called on error paths (401/403), so extraction is deferred until needed.
fn peer_ip(req: &Request<Body>) -> String {
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_owned())
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

/// HTTP 429 — IP is hard-locked out after 20 authentication failures.
///
/// Only cleared by a successful authentication from the same IP.
fn hard_lockout() -> impl IntoResponse {
    (
        StatusCode::TOO_MANY_REQUESTS,
        axum::Json(json!({
            "error": {
                "code": "auth_locked_out",
                "message": "This IP is locked out due to repeated authentication failures.",
                "status": 429
            }
        })),
    )
}

/// HTTP 401 — no or invalid token.
///
/// Includes `WWW-Authenticate` per RFC 7235 §3.1 so clients know the expected scheme.
fn unauthorized() -> impl IntoResponse {
    (
        StatusCode::UNAUTHORIZED,
        [(
            axum::http::header::WWW_AUTHENTICATE,
            axum::http::HeaderValue::from_static(r#"Bearer realm="lightarchitects""#),
        )],
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    /// Regression for NF-2: HTTP 401 must include RFC 7235 §3.1 `WWW-Authenticate` header.
    #[test]
    fn test_unauthorized_includes_www_authenticate() {
        let response = unauthorized().into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let header = response.headers().get(axum::http::header::WWW_AUTHENTICATE);
        assert!(
            header.is_some(),
            "HTTP 401 must include WWW-Authenticate header (RFC 7235 §3.1)"
        );
        assert_eq!(
            header.unwrap(),
            r#"Bearer realm="lightarchitects""#,
            "WWW-Authenticate must declare Bearer scheme with realm"
        );
    }
}
