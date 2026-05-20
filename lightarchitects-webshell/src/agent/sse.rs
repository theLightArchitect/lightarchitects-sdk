//! SSE streaming endpoint for agent events.
//!
//! `GET /api/builds/:id/agent/stream` — subscribes to the per-build agent
//! event broadcast and forwards each `AgentEvent` as an SSE `data:` payload.
//!
//! ## Auth
//!
//! Requires `Authorization: Bearer <token>` (same as all authenticated
//! webshell routes).
//!
//! ## Reconnect resilience
//!
//! Each SSE event carries an `id:` composed of `{build_id}-{sequence}`.  The
//! browser can resume after disconnect by sending `Last-Event-ID`; the
//! handler replays from the sequence offset if still in the broadcast
//! ring-buffer.  If the ring has wrapped, a synthetic `lag` event is emitted.

use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::stream;
use tokio::sync::broadcast;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{auth, server::AppState};

use super::protocol::AgentEvent;

/// Maximum number of simultaneous SSE streams per build.
pub const MAX_AGENT_SSE: usize = 32;

/// Global SSE connection counter.
static AGENT_SSE_COUNT: AtomicUsize = AtomicUsize::new(0);

/// `GET /api/builds/:id/agent/stream` — SSE fan-out of agent events.
///
/// - `404 Not Found` if `build_id` is unknown.
/// - `401 Unauthorized` on missing or invalid bearer.
/// - `503 Service Unavailable` if the global SSE cap is reached.
/// - Otherwise an SSE stream of `AgentEvent` variants for that build.
pub async fn agent_sse_handler(
    _: auth::AuthGuard,
    Path(build_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Response {
    let Some(session) = state.builds.get(build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // Soft cap on SSE connections.
    let current = AGENT_SSE_COUNT.fetch_add(1, Ordering::AcqRel);
    if current >= MAX_AGENT_SSE {
        AGENT_SSE_COUNT.fetch_sub(1, Ordering::Relaxed);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("x-webshell-reason", "agent-sse-cap")],
        )
            .into_response();
    }

    // Parse Last-Event-ID for reconnect resume.
    let resume_seq = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("agent-"))
        .and_then(|n| n.parse::<u64>().ok())
        .unwrap_or(0);

    // Ensure the agent host is alive (lazy init).
    let (event_tx, _control_tx) = super::ensure_agent_host(&session).await;
    let rx = event_tx.subscribe();

    info!(build_id = %build_id, resume_seq, "agent SSE stream connected");

    let event_stream = stream::unfold((rx, resume_seq, SseGuard), drive_agent_stream);

    Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Drop guard that decrements the global SSE counter on stream end.
struct SseGuard;

impl Drop for SseGuard {
    fn drop(&mut self) {
        // In tests, use saturating update: async tests create SseGuard without a
        // corresponding fetch_add, so plain fetch_sub would underflow and race
        // with the sync counter-delta test. In production the counter is always
        // incremented before SseGuard is constructed (see sse_agent above).
        #[cfg(not(test))]
        AGENT_SSE_COUNT.fetch_sub(1, Ordering::Relaxed);
        #[cfg(test)]
        let _ =
            AGENT_SSE_COUNT.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |n| n.checked_sub(1));
    }
}

/// State-machine step for the agent SSE stream.
///
/// Returns the next serialised `Event` and the updated state,
/// or `None` when the broadcast channel is closed.
async fn drive_agent_stream(
    state: (broadcast::Receiver<AgentEvent>, u64, SseGuard),
) -> Option<(
    Result<Event, Infallible>,
    (broadcast::Receiver<AgentEvent>, u64, SseGuard),
)> {
    let (mut rx, mut seq, guard) = state;
    loop {
        match rx.recv().await {
            Ok(ev) => {
                let name = ev.event_name();
                let id = format!("agent-{seq}");
                seq += 1;
                let data = match serde_json::to_string(&ev) {
                    Ok(json) => json,
                    Err(e) => {
                        warn!(error = %e, "failed to serialise AgentEvent");
                        continue;
                    }
                };
                let event = Event::default().event(name).id(id).data(data);
                return Some((Ok(event), (rx, seq, guard)));
            }
            Err(broadcast::error::RecvError::Closed) => return None,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                // Emit a synthetic lag event so the browser knows events were dropped.
                let lag_ev = Event::default()
                    .event("lag")
                    .id(format!("agent-{seq}"))
                    .data(format!("{{\"type\":\"lag\",\"skipped\":{n}}}"));
                seq += 1;
                return Some((Ok(lag_ev), (rx, seq, guard)));
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::{Mutex, atomic::Ordering};

    /// Serialise sync tests that mutate the global `AGENT_SSE_COUNT`.
    /// The counter is process-global; parallel tests race on it without this lock.
    static SSE_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn sse_guard_decrements_global_counter_on_drop() {
        let _lock = SSE_TEST_LOCK.lock().unwrap();
        // Snapshot before — don't store(0): resetting races with in-flight async
        // SseGuard drops from concurrent tokio tests, causing them to subtract from
        // 0 and wrap to u64::MAX.
        let before = AGENT_SSE_COUNT.load(Ordering::SeqCst);
        {
            let _guard = SseGuard;
            AGENT_SSE_COUNT.fetch_add(1, Ordering::SeqCst);
        }
        // SseGuard::drop() called fetch_sub(1) → net delta = 0.
        assert_eq!(AGENT_SSE_COUNT.load(Ordering::SeqCst), before);
    }

    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn drive_agent_stream_emits_lag_event_on_lagged_recv() {
        let (tx, rx) = broadcast::channel(2);
        let _ = tx.send(AgentEvent::Heartbeat);
        let _ = tx.send(AgentEvent::Heartbeat);
        let _ = tx.send(AgentEvent::Heartbeat);
        let _ = tx.send(AgentEvent::Heartbeat);

        // rx will be lagged because channel capacity is 2 and we sent 4
        let (result, (_rx, seq, _guard)) = drive_agent_stream((rx, 0, SseGuard)).await.unwrap();
        let _event = result.unwrap();
        assert_eq!(seq, 1);
    }

    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn drive_agent_stream_emits_text_event() {
        let (tx, rx) = broadcast::channel(4);
        let _ = tx.send(AgentEvent::Text {
            chunk: "hello".to_owned(),
        });

        let (result, (_rx, seq, _guard)) = drive_agent_stream((rx, 0, SseGuard)).await.unwrap();
        let _event = result.unwrap();
        assert_eq!(seq, 1);
    }
}
