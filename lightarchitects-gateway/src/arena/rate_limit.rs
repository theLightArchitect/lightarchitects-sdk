//! Sliding-window rate limiter for `Arena` API gateway.
//!
//! In-memory, per-key rate limiting using a weighted sliding window algorithm.
//! O(1) memory per key. Periodic eviction prevents unbounded growth.
//! Returns 429 with `Retry-After` and standard rate limit headers.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use serde_json::json;

use super::AppState;
use super::auth::AuthContext;

/// Maximum number of tracked keys before eviction sweep.
const MAX_TRACKED_KEYS: usize = 100_000;

/// Sliding window counter for a single key.
#[derive(Debug, Clone)]
struct WindowCounter {
    current_count: u32,
    previous_count: u32,
    window_start: Instant,
}

/// In-memory rate limiter state.
///
/// Uses `DashMap` instead of `RwLock<HashMap>` — each key maps to an independent
/// shard lock, so concurrent requests for different keys don't block each other.
#[derive(Debug)]
pub struct RateLimiter {
    counters: DashMap<String, WindowCounter>,
    window: Duration,
    default_limit: u32,
}

impl RateLimiter {
    /// Create a rate limiter with the given window duration and default limit.
    #[must_use]
    pub fn new(window_secs: u64, default_limit: u32) -> Arc<Self> {
        Arc::new(Self {
            counters: DashMap::new(),
            window: Duration::from_secs(window_secs),
            default_limit,
        })
    }

    /// Check and record a request. Returns `(allowed, limit, remaining, retry_after)`.
    ///
    /// Synchronous — `DashMap` provides sharded interior mutability without async.
    fn check(&self, key: &str, limit: u32) -> (bool, u32, u32, u64) {
        let now = Instant::now();

        // Lazy eviction: sweep stale entries when map exceeds threshold
        if self.counters.len() > MAX_TRACKED_KEYS {
            let stale_threshold = self.window.saturating_mul(2);
            self.counters
                .retain(|_, c| now.duration_since(c.window_start) < stale_threshold);
        }

        let mut counter = self
            .counters
            .entry(key.to_owned())
            .or_insert(WindowCounter {
                current_count: 0,
                previous_count: 0,
                window_start: now,
            });

        let elapsed = now.duration_since(counter.window_start);

        if elapsed >= self.window {
            counter.previous_count = counter.current_count;
            counter.current_count = 0;
            counter.window_start = now;
        }

        let elapsed_fraction = elapsed.as_secs_f64() / self.window.as_secs_f64();
        let overlap = 1.0 - elapsed_fraction.min(1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let effective =
            counter.current_count + (f64::from(counter.previous_count) * overlap) as u32;

        if effective >= limit {
            let retry_after = self.window.as_secs().saturating_sub(elapsed.as_secs());
            return (false, limit, 0, retry_after.max(1));
        }

        counter.current_count = counter.current_count.saturating_add(1);
        let remaining = limit.saturating_sub(effective.saturating_add(1));
        (true, limit, remaining, 0)
    }
}

/// Rate limit middleware — uses `key_hash` from `AuthContext` (not prefix).
pub async fn rate_limit_middleware(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let limiter = &state.rate_limiter;
    let auth_ctx = request.extensions().get::<AuthContext>().cloned();

    let Some(ctx) = auth_ctx else {
        return next.run(request).await;
    };

    let limit = if ctx.rate_limit > 0 {
        ctx.rate_limit
    } else {
        limiter.default_limit
    };

    // Rate limit on key_hash (not prefix) — unique per key, survives rotation
    let (allowed, max, remaining, retry_after) = limiter.check(&ctx.key_hash, limit);

    if !allowed {
        tracing::warn!(
            key_prefix = %ctx.key_prefix,
            limit = max,
            retry_after = retry_after,
            "Rate limit exceeded"
        );
        return rate_limit_response(max, retry_after);
    }

    let mut response = next.run(request).await;
    add_rate_limit_headers(&mut response, max, remaining);
    response
}

/// Build a 429 response with rate limit headers.
fn rate_limit_response(max: u32, retry_after: u64) -> Response {
    let body = json!({
        "error": {
            "code": "rate_limit_exceeded",
            "message": "Too many requests",
            "status": 429,
            "retry_after": retry_after
        }
    });
    let mut response = (StatusCode::TOO_MANY_REQUESTS, axum::Json(body)).into_response();
    let headers = response.headers_mut();
    if let Ok(v) = max.to_string().parse() {
        headers.insert("x-ratelimit-limit", v);
    }
    if let Ok(v) = "0".parse() {
        headers.insert("x-ratelimit-remaining", v);
    }
    if let Ok(v) = retry_after.to_string().parse() {
        headers.insert("retry-after", v);
    }
    response
}

/// Add rate limit headers to a successful response.
fn add_rate_limit_headers(response: &mut Response, max: u32, remaining: u32) {
    let headers = response.headers_mut();
    if let Ok(v) = max.to_string().parse() {
        headers.insert("x-ratelimit-limit", v);
    }
    if let Ok(v) = remaining.to_string().parse() {
        headers.insert("x-ratelimit-remaining", v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(60, 10);
        for _ in 0..10 {
            let (allowed, _, _, _) = limiter.check("test-key", 10);
            assert!(allowed);
        }
    }

    #[test]
    fn test_rate_limiter_blocks_over_limit() {
        let limiter = RateLimiter::new(60, 5);
        for _ in 0..5 {
            let (allowed, _, _, _) = limiter.check("test-key", 5);
            assert!(allowed);
        }
        let (allowed, _, remaining, retry_after) = limiter.check("test-key", 5);
        assert!(!allowed);
        assert_eq!(remaining, 0);
        assert!(retry_after > 0);
    }

    #[test]
    fn test_rate_limiter_independent_keys() {
        let limiter = RateLimiter::new(60, 2);
        let (allowed, _, _, _) = limiter.check("key-a", 2);
        assert!(allowed);
        let (allowed, _, _, _) = limiter.check("key-a", 2);
        assert!(allowed);
        let (allowed, _, _, _) = limiter.check("key-a", 2);
        assert!(!allowed);
        let (allowed, _, _, _) = limiter.check("key-b", 2);
        assert!(allowed);
    }

    #[test]
    fn test_rate_limiter_returns_remaining() {
        let limiter = RateLimiter::new(60, 10);
        let (_, limit, remaining, _) = limiter.check("test", 10);
        assert_eq!(limit, 10);
        assert_eq!(remaining, 9);
    }
}
