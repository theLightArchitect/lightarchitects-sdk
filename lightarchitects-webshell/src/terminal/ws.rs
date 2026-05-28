//! Axum WebSocket handler for `GET /api/terminal/ws`.
//!
//! ## Auth
//!
//! Browsers cannot set `Authorization` on `new WebSocket()`. The token
//! therefore travels in the `Sec-WebSocket-Protocol: bearer.<token>`
//! sub-protocol header. We reject with 401 **before** calling
//! [`WebSocketUpgrade::on_upgrade`] — the client sees a plain HTTP 401,
//! not a failed handshake.
//!
//! ## Concurrency cap
//!
//! At most [`MAX_SESSIONS`] concurrent PTY sessions are allowed. A 5th
//! connection receives 503 with `X-WebShell-Reason: session-cap`.
//! The slot is reserved atomically on the HTTP request and released via
//! [`SessionGuard`] when the async session task completes.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    extract::{Path, State, ws::WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use uuid::Uuid;

use crate::{auth, server::AppState, terminal::session};
use secrecy::ExposeSecret;

/// Maximum number of simultaneous PTY sessions.
pub const MAX_SESSIONS: usize = 4;

/// RAII guard that decrements the session count when dropped.
///
/// Constructed by [`try_claim_session`] after an atomic slot reservation.
/// Passed to [`session::run_session`] so the slot is returned to the pool
/// exactly when the session task exits — even on panic (via `Drop`).
pub struct SessionGuard(Arc<AtomicUsize>);

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}

/// Axum handler for `GET /api/terminal/ws`.
///
/// Returns 401 on auth failure, 503 when the session cap is reached,
/// or an HTTP 101 Switching Protocols upgrade response on success.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Response {
    // Extract the Sec-WebSocket-Protocol header (case-insensitive per RFC 6455).
    let subproto = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !auth::validate_ws_headers(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Atomically claim a session slot; reject if at capacity.
    let Some(guard) = try_claim_session(&state.session_count) else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("x-webshell-reason", "session-cap")],
        )
            .into_response();
    };

    let config = Arc::clone(&state.config);

    // Auth and cap checks pass — perform the WebSocket upgrade. Echo the
    // subprotocol only for bearer-subprotocol auth; cookie-authenticated
    // browsers do not send one.
    // The session task owns `guard`; dropping it decrements the count.
    let upgrade = if subproto.is_empty() {
        ws
    } else {
        ws.protocols([subproto.to_owned()])
    };

    upgrade.on_upgrade(move |socket| async move {
        let pepper = Arc::clone(&state.turnlog_pepper);
        let session_id = uuid::Uuid::new_v4().to_string();
        let cwd = config.cwd.clone();
        let host_cmd_str = config.host_cmd.clone().into_string().unwrap_or_default();
        let turnlog = if pepper.expose_secret().is_empty() {
            None
        } else {
            let tl = crate::turnlog::WebshellTurnLog::open(
                session_id,
                cwd,
                &host_cmd_str,
                &pepper,
                Some(state.event_tx.clone()),
                state.soul_store.clone(),
            )
            .await
            .ok()
            .flatten();
            // Phase 19c.2 — attach hot-reload policy when available.
            tl.map(|t| {
                if let Some(policy) = state.promotion_policy.clone() {
                    t.with_policy(policy)
                } else {
                    t
                }
            })
        };
        session::run_session(socket, config, None, guard).await;
        if let Some(tl) = turnlog {
            tl.close(lightarchitects::turnlog::EndReason::Complete)
                .await;
        }
    })
}

/// Axum handler for `GET /api/builds/:id/terminal/ws` (Phase C).
///
/// Same auth + concurrency contract as [`ws_handler`], but the PTY is bound
/// to a specific build session in the registry. The session's env vars
/// (`LA_BUILD_ID`, `LA_NOTIFY_TOKEN`, `LA_GUI_URL`, optional `ANTHROPIC_*`)
/// and CLI arguments (`--agent`, `--add-dir`, `-n …`, model/prompt/tools
/// overrides) are threaded through on spawn.
///
/// Status codes:
/// - `401 Unauthorized` — missing/invalid `Sec-WebSocket-Protocol: bearer.…`.
/// - `404 Not Found` — `:id` is not in [`crate::session::BuildRegistry`].
/// - `503 Service Unavailable` — global PTY session cap reached
///   (see [`MAX_SESSIONS`]; the global and per-build routes share the cap).
/// - `101 Switching Protocols` — WebSocket upgrade on success.
pub async fn ws_build_handler(
    Path(build_id): Path<Uuid>,
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Response {
    let subproto = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !auth::validate_ws_subprotocol(subproto, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let Some(session) = state.builds.get(build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(guard) = try_claim_session(&state.session_count) else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("x-webshell-reason", "session-cap")],
        )
            .into_response();
    };

    let config = Arc::clone(&state.config);
    // Echo the subprotocol back (RFC 6455 §4.1).
    ws.protocols([subproto.to_owned()])
        .on_upgrade(move |socket| async move {
            session::run_session(socket, config, Some(session), guard).await;
        })
}

/// Attempts to atomically claim one session slot.
///
/// Returns [`Some(SessionGuard)`] if the reservation succeeded (slot count
/// was below [`MAX_SESSIONS`] before the increment) or [`None`] if the cap
/// was already reached.
pub fn try_claim_session(count: &Arc<AtomicUsize>) -> Option<SessionGuard> {
    // Fetch-then-check: increment unconditionally, roll back on overshoot.
    // This correctly serialises concurrent requests because the fetch_add is
    // atomic — at most one caller observes `prev < MAX_SESSIONS` for a given
    // available slot.
    let prev = count.fetch_add(1, Ordering::AcqRel);
    if prev >= MAX_SESSIONS {
        count.fetch_sub(1, Ordering::Release);
        None
    } else {
        Some(SessionGuard(Arc::clone(count)))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn make_count(n: usize) -> Arc<AtomicUsize> {
        Arc::new(AtomicUsize::new(n))
    }

    #[test]
    fn claim_succeeds_when_below_cap() {
        let count = make_count(0);
        let guard = try_claim_session(&count);
        assert!(guard.is_some());
        assert_eq!(count.load(Ordering::Acquire), 1);
    }

    #[test]
    fn claim_fails_at_cap() {
        let count = make_count(MAX_SESSIONS);
        let guard = try_claim_session(&count);
        assert!(guard.is_none());
        // Roll-back must restore the original value.
        assert_eq!(count.load(Ordering::Acquire), MAX_SESSIONS);
    }

    #[test]
    fn claim_fills_last_slot() {
        let count = make_count(MAX_SESSIONS - 1);
        let guard = try_claim_session(&count);
        assert!(guard.is_some());
        assert_eq!(count.load(Ordering::Acquire), MAX_SESSIONS);
    }

    #[test]
    fn guard_drop_decrements_count() {
        let count = make_count(0);
        let guard = try_claim_session(&count).unwrap();
        assert_eq!(count.load(Ordering::Acquire), 1);
        drop(guard);
        assert_eq!(count.load(Ordering::Acquire), 0);
    }

    #[test]
    fn guard_drop_does_not_underflow_at_zero() {
        // Dropping two guards from independent counts must not cross-pollute.
        let count_a = make_count(0);
        let count_b = make_count(0);
        let g_a = try_claim_session(&count_a).unwrap();
        let g_b = try_claim_session(&count_b).unwrap();
        drop(g_a);
        drop(g_b);
        assert_eq!(count_a.load(Ordering::Acquire), 0);
        assert_eq!(count_b.load(Ordering::Acquire), 0);
    }
}
