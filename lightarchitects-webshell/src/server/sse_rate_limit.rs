//! Per-IP SSE connection rate limiter (CWE-770).
//!
//! Limits SSE *connection attempts* to [`MAX_SSE_PER_WINDOW`] per IP per
//! 60-second window.  Keyed by the remote IP extracted from
//! [`axum::extract::ConnectInfo`]; returns 429 with a `Retry-After` header
//! when the budget is exceeded.
//!
//! Applied via `.layer(sse_rate_layer(state))` on individual SSE routes.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;

use crate::server::AppState;

/// Maximum SSE connection attempts per IP per [`WINDOW`].
const MAX_SSE_PER_WINDOW: u32 = 10;
const WINDOW: Duration = Duration::from_secs(60);

/// Per-IP connection count within the current window.
///
/// Value is `(count, window_start)`.  Eviction is lazy (stale entries are
/// reset on the next connection from that IP) — safe for the single-operator
/// deployment model where the map stays small.
pub type SseRateLimiter = Arc<DashMap<IpAddr, (u32, Instant)>>;

/// Construct a new [`SseRateLimiter`].
pub fn new_sse_rate_limiter() -> SseRateLimiter {
    Arc::new(DashMap::new())
}

/// Axum middleware function — checks and increments the rate counter.
///
/// On excess: returns `429 Too Many Requests` with `Retry-After: 60`.
/// On success: passes the request through unchanged.
pub async fn sse_rate_limit_middleware(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let ip = addr.ip();

    let allowed = {
        let mut entry = state
            .sse_rate_limiter
            .entry(ip)
            .or_insert((0, Instant::now()));
        let (count, window_start) = entry.value_mut();

        // Reset window if expired.
        if window_start.elapsed() >= WINDOW {
            *count = 0;
            *window_start = Instant::now();
        }

        if *count < MAX_SSE_PER_WINDOW {
            *count += 1;
            true
        } else {
            false
        }
    };

    if allowed {
        next.run(request).await
    } else {
        let mut resp = StatusCode::TOO_MANY_REQUESTS.into_response();
        resp.headers_mut().insert(
            axum::http::header::RETRY_AFTER,
            HeaderValue::from_static("60"),
        );
        resp
    }
}
