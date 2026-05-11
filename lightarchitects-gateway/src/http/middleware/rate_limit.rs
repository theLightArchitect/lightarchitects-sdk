//! Path-aware per-IP rate-limit middleware.
//!
//! Five tiers, keyed by request path:
//! - `/v1/platform/health`          — exempt (liveness probe; no quota consumed)
//! - `POST /v1/admin/skills/upload` — skills limiter (≤1 req/sec — SERAPH F-MEDIUM-3)
//! - `/v1/admin/*`                  — write limiter (10 req/min)
//! - `/v1/platform/helix*`, `/v1/vault/*` — helix limiter (20 req/min)
//! - all other `/v1/platform/*`     — read limiter (100 req/min)
//!
//! Clients that exceed their quota receive HTTP 429 with `Retry-After: 60`.

use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{Request, Response, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;

use super::super::state::PlatformState;

/// Reject requests that exceed the per-IP quota for their path tier.
pub async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<PlatformState>>,
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let path = req.uri().path();

    // Health probe — bypass all rate limits.
    if path == "/v1/platform/health" {
        return next.run(req).await;
    }

    let ip = addr.ip();
    // Skills upload is checked before the general admin tier — SERAPH F-MEDIUM-3 requires
    // ≤1 req/sec, which is tighter than the 10 req/min write_limiter burst window.
    let result = if path == "/v1/admin/skills/upload" {
        state.skills_limiter.check_key(&ip)
    } else if path.starts_with("/v1/admin/") {
        state.write_limiter.check_key(&ip)
    } else if path.starts_with("/v1/platform/helix") || path.starts_with("/v1/vault/") {
        state.helix_limiter.check_key(&ip)
    } else {
        state.read_limiter.check_key(&ip)
    };

    match result {
        Ok(()) => next.run(req).await,
        Err(_not_until) => (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "60")],
            axum::Json(json!({
                "error": {
                    "code": "rate_limited",
                    "message": "Too many requests. Retry after 60 seconds.",
                    "status": 429
                }
            })),
        )
            .into_response(),
    }
}
