//! WebSocket control endpoint for agent sessions.
//!
//! `GET /api/builds/:id/agent/ws` — bidirectional control channel.
//!
//! ## Auth
//!
//! Browsers cannot set `Authorization` on `new WebSocket()`.  The token
//! travels in the `Sec-WebSocket-Protocol: bearer.<token>` sub-protocol
//! header (same pattern as the PTY WebSocket route).
//!
//! ## Message flow
//!
//! | Direction | Message | Purpose |
//! |-----------|---------|---------|
//! | Browser → Server | `{"action":"send_message","text":"..."}` | Start a new turn |
//! | Browser → Server | `{"action":"approve_permission","request_id":"..."}` | Approve a pending tool |
//! | Browser → Server | `{"action":"deny_permission","..."}` | Deny a pending tool |
//! | Browser → Server | `{"action":"interrupt"}` | Cancel in-flight turn |
//! | Browser → Server | `{"action":"steer","text":"..."}` | Append mid-turn instructions |
//! | Browser → Server | `{"action":"execute_plan"}` | Execute queued plan actions |
//! | Browser → Server | `{"action":"ping"}` | Keepalive |
//! | Server → Browser | `{"type":"ack","action":"..."}` | Control accepted |
//! | Server → Browser | `{"type":"reject","action":"...","reason":"..."}` | Control rejected |
//! | Server → Browser | `{"type":"permission_resolved","..."}` | Permission handled |
//! | Server → Browser | `{"type":"interrupted"}` | Turn was interrupted |
//! | Server → Browser | `{"type":"pong"}` | Ping reply |
//! | Server → Browser | `{"type":"server_error","message":"..."}` | Server-side error |
//!
//! ## Architecture
//!
//! Two concurrent tasks per connection:
//! 1. **Writer task** — owns the WebSocket sink. Receives from:
//!    - `event_rx` (SSE agent events forwarded here for convenience)
//!    - `outgoing_tx` (control responses from the reader task)
//! 2. **Reader task** — owns the WebSocket stream. Parses `ControlMessage`s,
//!    routes to the bridge, sends responses back via `outgoing_tx`.
//!
//! ## Fallback mode
//!
//! If the `lightarchitects-cli` binary does not support `--stream-events`,
//! the WebSocket handler falls back to spawning a single-shot `run` process
//! per `SendMessage` and emits the response back over the same WebSocket.

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::{
    extract::{Path, State, ws::WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};
use tracing::info;
use uuid::Uuid;

use crate::{auth, server::AppState};

use super::{
    bridge::run_fallback_turn,
    protocol::{ControlMessage, ControlResponse},
};

/// Maximum number of simultaneous agent WebSocket control connections.
pub const MAX_AGENT_WS: usize = 8;

/// Slot counter for agent WS connections (separate from PTY session cap).
static AGENT_WS_COUNT: AtomicUsize = AtomicUsize::new(0);

/// `GET /api/builds/:id/agent/ws` — WebSocket upgrade for agent control.
///
/// Returns 401 on auth failure, 503 when the cap is reached, or 101 on
/// successful upgrade.
pub async fn agent_ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    Path(build_id): Path<Uuid>,
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

    // Claim a slot (soft cap — degrades gracefully if exceeded).
    let current = AGENT_WS_COUNT.fetch_add(1, Ordering::AcqRel);
    if current >= MAX_AGENT_WS {
        AGENT_WS_COUNT.fetch_sub(1, Ordering::Relaxed);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("x-webshell-reason", "agent-ws-cap")],
        )
            .into_response();
    }

    ws.protocols([subproto.to_owned()])
        .on_upgrade(move |socket| async move {
            let (event_tx, control_tx) = super::ensure_agent_host(&session).await;

            handle_agent_socket(socket, build_id, session, event_tx, control_tx).await;
        })
}

async fn handle_agent_socket(
    socket: axum::extract::ws::WebSocket,
    build_id: Uuid,
    session: std::sync::Arc<crate::session::BuildSession>,
    event_tx: tokio::sync::broadcast::Sender<super::protocol::AgentEvent>,
    control_tx: mpsc::Sender<ControlMessage>,
) {
    let (mut ws_sink, mut ws_stream) = socket.split();
    let mut event_rx = event_tx.subscribe();

    // Channel for the reader task to send responses to the writer task.
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<axum::extract::ws::Message>(256);

    // ── Writer task ──────────────────────────────────────────────────────
    let writer_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                // Forward SSE agent events into the WebSocket.
                ev = event_rx.recv() => {
                    let ev = match ev {
                        Ok(ev) => ev,
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            let lag = ControlResponse::ServerError {
                                message: format!("event lag: skipped {n}"),
                            };
                            let _ = ws_sink.send(axum::extract::ws::Message::Text(
                                serde_json::to_string(&lag).unwrap_or_default().into(),
                            )).await;
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    };
                    let Ok(json) = serde_json::to_string(&ev) else {
                        continue;
                    };
                    if ws_sink
                        .send(axum::extract::ws::Message::Text(json.into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                // Forward control responses from the reader task.
                Some(msg) = outgoing_rx.recv() => {
                    if ws_sink.send(msg).await.is_err() {
                        break;
                    }
                }
                else => break,
            }
        }
    });

    // ── Reader task ──────────────────────────────────────────────────────
    drive_reader(
        &mut ws_stream,
        &session,
        &event_tx,
        &control_tx,
        &outgoing_tx,
    )
    .await;

    writer_task.abort();
    AGENT_WS_COUNT.fetch_sub(1, Ordering::Relaxed);
    info!(build_id = %build_id, "agent WebSocket closed");
}

/// Bridge health check: returns `true` only if the stored child handle
/// exists and the OS process is still alive.
async fn bridge_is_alive(host: &super::AgentSessionHost) -> bool {
    let mut guard = host.child.lock().await;
    let Some(child) = guard.as_mut() else {
        return false;
    };
    // try_wait returns Ok(None) while the process is still running.
    matches!(child.try_wait(), Ok(None))
}

async fn drive_reader(
    ws_stream: &mut futures_util::stream::SplitStream<axum::extract::ws::WebSocket>,
    session: &std::sync::Arc<crate::session::BuildSession>,
    event_tx: &tokio::sync::broadcast::Sender<super::protocol::AgentEvent>,
    control_tx: &mpsc::Sender<ControlMessage>,
    outgoing_tx: &mpsc::Sender<axum::extract::ws::Message>,
) {
    while let Some(Ok(msg)) = ws_stream.next().await {
        let axum::extract::ws::Message::Text(text) = msg else {
            continue;
        };
        let ctrl: ControlMessage = match serde_json::from_str(&text) {
            Ok(c) => c,
            Err(e) => {
                let resp = ControlResponse::Reject {
                    action: "unknown".to_owned(),
                    reason: format!("parse error: {e}"),
                };
                let _ = outgoing_tx
                    .send(axum::extract::ws::Message::Text(
                        serde_json::to_string(&resp).unwrap_or_default().into(),
                    ))
                    .await;
                continue;
            }
        };

        let action_name = match &ctrl {
            ControlMessage::SendMessage { .. } => "send_message",
            ControlMessage::ApprovePermission { .. } => "approve_permission",
            ControlMessage::DenyPermission { .. } => "deny_permission",
            ControlMessage::Interrupt => "interrupt",
            ControlMessage::Steer { .. } => "steer",
            ControlMessage::ExecutePlan => "execute_plan",
            ControlMessage::Ping => "ping",
        };

        // Handle ping locally.
        if matches!(ctrl, ControlMessage::Ping) {
            let resp = ControlResponse::Pong;
            let _ = outgoing_tx
                .send(axum::extract::ws::Message::Text(
                    serde_json::to_string(&resp).unwrap_or_default().into(),
                ))
                .await;
            continue;
        }

        // Determine whether the streaming bridge is alive.
        let has_bridge = {
            let guard = session.agent_host.lock().await;
            match guard.as_ref() {
                Some(host) => bridge_is_alive(host).await,
                None => false,
            }
        };

        if !has_bridge {
            if let ControlMessage::SendMessage { text } = &ctrl {
                handle_fallback(session, event_tx, outgoing_tx, action_name, text).await;
                continue;
            }
        }

        // Streaming mode: forward to bridge control channel.
        match control_tx.try_send(ctrl) {
            Ok(()) => {
                let resp = ControlResponse::Ack {
                    action: action_name.to_owned(),
                };
                let _ = outgoing_tx
                    .send(axum::extract::ws::Message::Text(
                        serde_json::to_string(&resp).unwrap_or_default().into(),
                    ))
                    .await;
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                let resp = ControlResponse::Reject {
                    action: action_name.to_owned(),
                    reason: "bridge control queue full".to_owned(),
                };
                let _ = outgoing_tx
                    .send(axum::extract::ws::Message::Text(
                        serde_json::to_string(&resp).unwrap_or_default().into(),
                    ))
                    .await;
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                let resp = ControlResponse::Reject {
                    action: action_name.to_owned(),
                    reason: "bridge control channel closed".to_owned(),
                };
                let _ = outgoing_tx
                    .send(axum::extract::ws::Message::Text(
                        serde_json::to_string(&resp).unwrap_or_default().into(),
                    ))
                    .await;
            }
        }
    }
}

/// RAII guard that resets `fallback_in_flight` on drop.
struct FallbackGuard {
    host: std::sync::Arc<super::AgentSessionHost>,
}

impl Drop for FallbackGuard {
    fn drop(&mut self) {
        self.host.fallback_in_flight.store(false, Ordering::Release);
    }
}

async fn handle_fallback(
    session: &std::sync::Arc<crate::session::BuildSession>,
    event_tx: &tokio::sync::broadcast::Sender<super::protocol::AgentEvent>,
    outgoing_tx: &mpsc::Sender<axum::extract::ws::Message>,
    action_name: &str,
    text: &str,
) {
    let guard = session.agent_host.lock().await;
    let Some(host) = guard.as_ref().map(std::sync::Arc::clone) else {
        let resp = ControlResponse::Reject {
            action: action_name.to_owned(),
            reason: "agent host not initialised".to_owned(),
        };
        let _ = outgoing_tx.try_send(axum::extract::ws::Message::Text(
            serde_json::to_string(&resp).unwrap_or_default().into(),
        ));
        return;
    };
    let should_run = host
        .fallback_in_flight
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok();
    if !should_run {
        let resp = ControlResponse::Reject {
            action: action_name.to_owned(),
            reason: "fallback turn already in flight".to_owned(),
        };
        let _ = outgoing_tx.try_send(axum::extract::ws::Message::Text(
            serde_json::to_string(&resp).unwrap_or_default().into(),
        ));
        return;
    }
    drop(guard);

    let guard = FallbackGuard { host };
    let event_tx = event_tx.clone();
    let session_ref = session.clone();
    let text = text.to_owned();
    tokio::spawn(async move {
        run_fallback_turn(&session_ref, event_tx, &text).await;
        drop(guard);
    });
    let resp = ControlResponse::Ack {
        action: action_name.to_owned(),
    };
    let _ = outgoing_tx.try_send(axum::extract::ws::Message::Text(
        serde_json::to_string(&resp).unwrap_or_default().into(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentSessionHost;
    use std::sync::atomic::Ordering;

    #[test]
    fn fallback_guard_resets_flag_on_drop() {
        let (host, _rx) = AgentSessionHost::new();
        let arc = std::sync::Arc::new(host);

        arc.fallback_in_flight.store(true, Ordering::Relaxed);
        assert!(arc.fallback_in_flight.load(Ordering::Relaxed));

        {
            let _guard = FallbackGuard {
                host: std::sync::Arc::clone(&arc),
            };
        }

        assert!(!arc.fallback_in_flight.load(Ordering::Relaxed));
    }
}
