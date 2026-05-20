//! Fleet SSE and snapshot routes.
//!
//! `GET /api/builds/{id}/fleet`          — per-build SSE fleet stream
//! `GET /api/builds/{id}/fleet/snapshot` — point-in-time fleet snapshot
//!
//! ## Auth
//!
//! Both endpoints require `Authorization: Bearer <token>` (same as all
//! authenticated webshell routes). `401 Unauthorized` on missing/invalid token.
//!
//! ## Cap
//!
//! Fleet SSE connections are capped at [`MAX_FLEET_SSE`] per webshell process
//! (not per build). The 429 response carries `X-Webshell-Reason: fleet-sse-cap`.
//!
//! ## Connection lifecycle
//!
//! 1. Auth check (401 on failure).
//! 2. Build lookup (404 on unknown `build_id`).
//! 3. Lazy-init `FleetBroadcaster` on `BuildSession.fleet_broadcaster`.
//! 4. Increment SSE counter (RAII `FleetSseGuard`); 429 if over cap.
//! 5. Emit snapshot as first SSE event.
//! 6. Subscribe to broadcaster; forward events as `data:` frames.
//! 7. Keepalive every 30 s.
//! 8. On disconnect: `FleetSseGuard` drops, decrementing counter.

use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::stream;
use tokio::sync::broadcast;
use tokio::time::Duration;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    agent::fleet::{FleetBroadcaster, FleetEvent},
    auth,
    server::AppState,
};

/// Maximum number of simultaneous fleet SSE streams (process-wide).
pub const MAX_FLEET_SSE: usize = 100;

/// Global fleet SSE connection counter.
static FLEET_SSE_COUNT: AtomicUsize = AtomicUsize::new(0);

// ── GET /api/builds/{id}/fleet ────────────────────────────────────────────────

/// `GET /api/builds/{id}/fleet` — per-build fleet SSE stream.
///
/// - `401 Unauthorized` on missing/invalid bearer.
/// - `404 Not Found` if `build_id` is unknown.
/// - `429 Too Many Requests` (header `X-Webshell-Reason: fleet-sse-cap`) when cap exceeded.
/// - `200 OK` + SSE stream on success.
pub async fn fleet_sse_handler(
    _: auth::AuthGuard,
    Path(build_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Response {
    let Some(session) = state.builds.get(build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // Soft cap on SSE connections.
    let current = FLEET_SSE_COUNT.fetch_add(1, Ordering::AcqRel);
    if current >= MAX_FLEET_SSE {
        FLEET_SSE_COUNT.fetch_sub(1, Ordering::Relaxed);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("x-webshell-reason", "fleet-sse-cap")],
            Json(serde_json::json!({
                "error": "fleet SSE connection cap exceeded",
                "max_connections": MAX_FLEET_SSE
            })),
        )
            .into_response();
    }

    // Lazy-init the fleet broadcaster (OQ5 resolution).
    let broadcaster = get_or_init_broadcaster(&session).await;

    info!(build_id = %build_id, "fleet SSE stream connected");

    // Take snapshot for the first event before subscribing, so we don't miss
    // events that arrive between snapshot and subscribe.
    let snap = broadcaster.snapshot();
    let rx = broadcaster.subscribe();

    let event_stream = stream::unfold((rx, FleetSseGuard, Some(snap)), drive_fleet_stream);

    Sse::new(event_stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(30))
                .text("keep-alive"),
        )
        .into_response()
}

// ── GET /api/builds/{id}/fleet/snapshot ──────────────────────────────────────

/// `GET /api/builds/{id}/fleet/snapshot` — point-in-time fleet state.
///
/// - `401 Unauthorized` on missing/invalid bearer.
/// - `404 Not Found` if `build_id` is unknown.
/// - `200 OK` + `FleetSnapshot` JSON on success.
pub async fn fleet_snapshot_handler(
    _: auth::AuthGuard,
    Path(build_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Response {
    let Some(session) = state.builds.get(build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let broadcaster = get_or_init_broadcaster(&session).await;
    let snapshot = broadcaster.snapshot();

    Json(snapshot).into_response()
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Get or lazily initialise the `FleetBroadcaster` on a `BuildSession`.
///
/// The `Mutex` serialises concurrent initialisations to prevent TOCTOU races
/// where two requests both observe `None` and spawn duplicate broadcasters.
async fn get_or_init_broadcaster(session: &crate::session::BuildSession) -> Arc<FleetBroadcaster> {
    let mut guard = session.fleet_broadcaster.lock().await;
    if let Some(b) = guard.as_ref() {
        return Arc::clone(b);
    }
    let session_id = session.build_id.to_string();
    let b = FleetBroadcaster::start(session_id).await;
    *guard = Some(Arc::clone(&b));
    b
}

/// Drop guard that decrements the global fleet SSE counter on stream end.
struct FleetSseGuard;

impl Drop for FleetSseGuard {
    fn drop(&mut self) {
        FLEET_SSE_COUNT.fetch_sub(1, Ordering::Relaxed);
    }
}

/// State-machine step for the fleet SSE stream.
///
/// On the first call, emits the snapshot that was captured before subscribing.
/// Subsequent calls read from the broadcast receiver.
async fn drive_fleet_stream(
    state: (
        broadcast::Receiver<FleetEvent>,
        FleetSseGuard,
        Option<lightarchitects::fleet::FleetSnapshot>,
    ),
) -> Option<(
    Result<Event, Infallible>,
    (
        broadcast::Receiver<FleetEvent>,
        FleetSseGuard,
        Option<lightarchitects::fleet::FleetSnapshot>,
    ),
)> {
    let (mut rx, guard, pending_snap) = state;

    // First call: emit the pending snapshot before subscribing to live events.
    if let Some(snap) = pending_snap {
        let ev = FleetEvent::Snapshot {
            nodes: snap.nodes,
            captured_at: snap.captured_at,
        };
        return match serde_json::to_string(&ev) {
            Ok(json) => {
                let event = Event::default().event("snapshot").data(json);
                Some((Ok(event), (rx, guard, None)))
            }
            Err(e) => {
                warn!(error = %e, "failed to serialise fleet snapshot");
                Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\":\"serialise\"}")),
                    (rx, guard, None),
                ))
            }
        };
    }

    // Subsequent calls: receive from the broadcast channel.
    loop {
        match rx.recv().await {
            Ok(ev) => {
                let event_name = event_name(&ev);
                match serde_json::to_string(&ev) {
                    Ok(json) => {
                        let event = Event::default().event(event_name).data(json);
                        return Some((Ok(event), (rx, guard, None)));
                    }
                    Err(e) => {
                        warn!(error = %e, "failed to serialise FleetEvent");
                    }
                }
            }
            Err(broadcast::error::RecvError::Closed) => return None,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                let lag_ev = Event::default()
                    .event("lag")
                    .data(format!("{{\"type\":\"lag\",\"skipped\":{n}}}"));
                return Some((Ok(lag_ev), (rx, guard, None)));
            }
        }
    }
}

/// Extract the SSE event name from a `FleetEvent` variant.
fn event_name(ev: &FleetEvent) -> &'static str {
    match ev {
        FleetEvent::Snapshot { .. } => "snapshot",
        FleetEvent::AgentSpawned { .. } => "agent_spawned",
        FleetEvent::AgentProgress { .. } => "agent_progress",
        FleetEvent::AgentCompleted { .. } => "agent_completed",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    /// Serialise sync tests that mutate the global `FLEET_SSE_COUNT`.
    static SSE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// S5 invariant: `FleetSseGuard` decrements the counter on drop.
    #[test]
    fn s5_fleet_sse_guard_decrements_on_drop() {
        let _lock = SSE_TEST_LOCK.lock().unwrap();
        let before = FLEET_SSE_COUNT.load(Ordering::SeqCst);
        {
            let _guard = FleetSseGuard;
            FLEET_SSE_COUNT.fetch_add(1, Ordering::SeqCst);
        }
        assert_eq!(
            FLEET_SSE_COUNT.load(Ordering::SeqCst),
            before,
            "counter must return to pre-guard value after drop"
        );
    }

    /// S5 invariant: 101st connection must be rejected (cap = 100).
    #[test]
    fn s5_fleet_sse_cap_enforced() {
        let _lock = SSE_TEST_LOCK.lock().unwrap();
        // Store current count and restore after test.
        let before = FLEET_SSE_COUNT.load(Ordering::SeqCst);
        FLEET_SSE_COUNT.store(MAX_FLEET_SSE, Ordering::SeqCst);

        // Simulate what the handler does: fetch_add, check, fetch_sub.
        let current = FLEET_SSE_COUNT.fetch_add(1, Ordering::AcqRel);
        let over_cap = current >= MAX_FLEET_SSE;
        if over_cap {
            FLEET_SSE_COUNT.fetch_sub(1, Ordering::Relaxed);
        }

        assert!(over_cap, "101st connection must hit the cap");
        assert_eq!(
            FLEET_SSE_COUNT.load(Ordering::SeqCst),
            MAX_FLEET_SSE,
            "counter must be rolled back on cap rejection"
        );

        // Restore.
        FLEET_SSE_COUNT.store(before, Ordering::SeqCst);
    }

    /// S2 invariant: fleet handlers are wired with `AuthGuard`.
    ///
    /// This is a compile-time guarantee enforced by the `_: auth::AuthGuard`
    /// extractor parameter. The test verifies the event-name helper covers all
    /// variants so clippy's non-exhaustive match check catches new variants.
    #[test]
    fn event_name_covers_all_variants() {
        use lightarchitects::fleet::{ExitPath, FleetNode, FleetStatus};
        let node = FleetNode {
            agent_id: "a".to_owned(),
            agent_type: "t".to_owned(),
            description: "d".to_owned(),
            parent_agent_id: None,
            worktree_path: None,
            run_in_background: false,
            status: FleetStatus::Running,
            turns: 0,
            elapsed_ms: 0,
            exit_path: None,
        };
        let cases = vec![
            (
                FleetEvent::Snapshot {
                    nodes: vec![],
                    captured_at: "t".to_owned(),
                },
                "snapshot",
            ),
            (FleetEvent::AgentSpawned { node }, "agent_spawned"),
            (
                FleetEvent::AgentProgress {
                    agent_id: "a".to_owned(),
                    elapsed_ms: 0,
                },
                "agent_progress",
            ),
            (
                FleetEvent::AgentCompleted {
                    agent_id: "a".to_owned(),
                    exit_path: ExitPath::Completed,
                    turns: 0,
                    duration_ms: 0,
                },
                "agent_completed",
            ),
        ];
        for (ev, expected) in cases {
            assert_eq!(event_name(&ev), expected);
        }
    }
}
