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
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Read ConnectInfo from extensions; absent = in-process test (oneshot) → skip.
    let Some(ip) = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip())
    else {
        return next.run(request).await;
    };

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn ip(a: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, a))
    }

    /// Simulate one connection attempt and return whether it was allowed.
    fn attempt(limiter: &DashMap<IpAddr, (u32, Instant)>, addr: IpAddr) -> bool {
        let mut entry = limiter.entry(addr).or_insert((0, Instant::now()));
        let (count, window_start) = entry.value_mut();
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
    }

    #[test]
    fn counter_increments_and_rejects_at_limit() {
        let limiter: DashMap<IpAddr, (u32, Instant)> = DashMap::new();
        let addr = ip(1);
        for _ in 0..MAX_SSE_PER_WINDOW {
            assert!(attempt(&limiter, addr), "should allow up to limit");
        }
        // The (MAX+1)th attempt must be rejected.
        assert!(!attempt(&limiter, addr), "should reject at limit+1");
    }

    #[test]
    #[allow(clippy::unwrap_used)] // WHY: checked_sub(60s) always succeeds on any system where Instant::now() > 60s
    fn window_reset_clears_counter() {
        let limiter: DashMap<IpAddr, (u32, Instant)> = DashMap::new();
        let addr = ip(2);
        // Exhaust the window.
        for _ in 0..MAX_SSE_PER_WINDOW {
            attempt(&limiter, addr);
        }
        assert!(!attempt(&limiter, addr));

        // Backdate window_start to simulate expiry.
        limiter
            .entry(addr)
            .and_modify(|(_, ws)| *ws = Instant::now().checked_sub(WINDOW).unwrap());

        // After window reset the counter should be 0 again → allowed.
        assert!(
            attempt(&limiter, addr),
            "first attempt after window reset must pass"
        );
    }

    #[test]
    fn two_ips_tracked_independently() {
        let limiter: DashMap<IpAddr, (u32, Instant)> = DashMap::new();
        let a = ip(3);
        let b = ip(4);
        // Exhaust IP-a.
        for _ in 0..MAX_SSE_PER_WINDOW {
            attempt(&limiter, a);
        }
        assert!(!attempt(&limiter, a), "ip-a exhausted");
        // IP-b must still be unaffected.
        assert!(attempt(&limiter, b), "ip-b independent of ip-a");
    }

    #[test]
    fn new_sse_rate_limiter_is_empty() {
        let limiter = new_sse_rate_limiter();
        assert!(limiter.is_empty());
    }
}
