//! HTTP handlers for the standalone conversation API.
//!
//! All 5 routes are authenticated via [`auth::AuthGuard`] (Bearer **or**
//! `la_session` cookie).  No `buildId` is required — sessions are identified
//! only by the UUID minted at creation.
//!
//! ## Route map
//!
//! | Method | Path | Handler | Body limit |
//! |--------|------|---------|------------|
//! | POST | `/api/conversation` | [`create_conversation`] | 256 B |
//! | GET | `/api/conversation/{id}/stream` | [`stream_conversation`] | — |
//! | POST | `/api/conversation/{id}` | [`send_turn`] | 32 KB |
//! | POST | `/api/conversation/{id}/interrupt` | [`interrupt_conversation`] | 256 B |
//! | DELETE | `/api/conversation/{id}` | [`end_conversation`] | — |

use std::{
    convert::Infallible,
    sync::{Arc, atomic::Ordering},
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
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::warn;
use uuid::Uuid;

use crate::{
    auth,
    conversation::{
        session::{ConvSSEEvent, ConvSessionHandle},
        strategy_bridge,
    },
    server::AppState,
};

// ── Request / response shapes ─────────────────────────────────────────────────

/// Response body for `POST /api/conversation`.
#[derive(Serialize)]
pub struct CreateConversationResponse {
    /// Stable UUID for this session — pass to all subsequent endpoints.
    pub session_id: Uuid,
}

/// Request body for `POST /api/conversation/{id}`.
#[derive(Deserialize)]
pub struct TurnRequest {
    /// User message text (max 32 KB after JSON framing).
    pub message: String,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// `POST /api/conversation` — create a new standalone conversation session.
///
/// Returns the session UUID.  The caller should immediately open the SSE
/// stream via `GET /api/conversation/{id}/stream` to receive events.
///
/// Authenticated via [`auth::AuthGuard`] (Bearer **or** `la_session` cookie).
pub async fn create_conversation(
    _: auth::AuthGuard,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let handle = Arc::new(ConvSessionHandle::new());
    let session_id = handle.session_id;
    state.conversation_store.insert(session_id, handle);
    (
        StatusCode::CREATED,
        Json(CreateConversationResponse { session_id }),
    )
}

/// `GET /api/conversation/{id}/stream` — subscribe to the SSE event stream.
///
/// The stream stays open for the session lifetime.  Each turn produces a
/// sequence of events ending with `{"type":"done","turn_id":"..."}` or
/// `{"type":"error","message":"..."}`.
///
/// Authenticated via [`auth::AuthGuard`] (Bearer **or** `la_session` cookie).
pub async fn stream_conversation(
    _: auth::AuthGuard,
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Response {
    let Some(handle) = state.conversation_store.get(&session_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // Subscribe before returning — the Receiver is live from this point so no
    // events emitted between subscription and the first `.recv()` are lost.
    let rx = handle.event_tx.subscribe();
    let event_stream = stream::unfold(rx, drive_conv_stream);
    Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `POST /api/conversation/{id}` — dispatch a new turn.
///
/// Returns `202 Accepted` immediately; events arrive on the SSE stream.
/// Returns `409 Conflict` when a turn is already in progress.
///
/// Authenticated via [`auth::AuthGuard`] (Bearer **or** `la_session` cookie).
pub async fn send_turn(
    _: auth::AuthGuard,
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<TurnRequest>,
) -> impl IntoResponse {
    let Some(handle) = state.conversation_store.get(&session_id).map(|r| r.clone()) else {
        return StatusCode::NOT_FOUND;
    };

    {
        let inner = handle
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inner.active_run.is_some() {
            return StatusCode::CONFLICT;
        }
    }

    handle.touch();

    let litellm_config = Arc::clone(&state.litellm_config);
    let strategy = strategy_bridge::should_route_to_strategy(&body.message);

    if let Some(strategy_name) = strategy {
        let h = Arc::clone(&handle);
        let s = strategy_name.to_owned();
        tokio::spawn(async move {
            strategy_bridge::dispatch_conversation_strategy(h, &s);
        });
    } else {
        let h = Arc::clone(&handle);
        let msg = body.message.clone();
        tokio::spawn(async move {
            strategy_bridge::dispatch_native_turn(h, msg, litellm_config).await;
        });
    }

    StatusCode::ACCEPTED
}

/// `POST /api/conversation/{id}/interrupt` — signal the active turn to stop.
///
/// The interrupt flag is set immediately; the running turn will observe it at
/// the next `ConversationSession` iteration boundary and emit an
/// `{"type":"error","message":"Turn was interrupted"}` event.
///
/// Returns `200 OK` even when no turn is active (idempotent signal).
///
/// Authenticated via [`auth::AuthGuard`] (Bearer **or** `la_session` cookie).
pub async fn interrupt_conversation(
    _: auth::AuthGuard,
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let Some(handle) = state.conversation_store.get(&session_id) else {
        return StatusCode::NOT_FOUND;
    };
    handle.interrupt.store(true, Ordering::SeqCst);
    StatusCode::OK
}

/// `DELETE /api/conversation/{id}` — end a session and remove it from the store.
///
/// Returns `204 No Content` on success, `404 Not Found` if the session does
/// not exist (already ended or TTL-evicted).
///
/// Authenticated via [`auth::AuthGuard`] (Bearer **or** `la_session` cookie).
pub async fn end_conversation(
    _: auth::AuthGuard,
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.conversation_store.remove(&session_id) {
        Some(_) => StatusCode::NO_CONTENT,
        None => StatusCode::NOT_FOUND,
    }
}

// ── SSE stream driver ─────────────────────────────────────────────────────────

/// State-machine step for the conversation SSE stream.
///
/// Runs until the broadcast channel is closed (session ended or server
/// shutdown).  On lag, emits a synthetic `{"type":"lag","skipped":N}` event
/// following the same pattern as the global event SSE handler.
async fn drive_conv_stream(
    mut rx: broadcast::Receiver<ConvSSEEvent>,
) -> Option<(Result<Event, Infallible>, broadcast::Receiver<ConvSSEEvent>)> {
    loop {
        match rx.recv().await {
            Ok(event) => {
                let data = match serde_json::to_string(&event) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(error = %e, "failed to serialise ConvSSEEvent");
                        continue;
                    }
                };
                return Some((Ok(Event::default().data(data)), rx));
            }
            Err(RecvError::Lagged(n)) => {
                warn!(
                    skipped = n,
                    "conversation SSE subscriber lagged — events dropped"
                );
                let payload = format!(r#"{{"type":"lag","skipped":{n}}}"#);
                return Some((Ok(Event::default().data(payload)), rx));
            }
            Err(RecvError::Closed) => return None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn create_response_serializes_session_id() {
        let id = Uuid::new_v4();
        let resp = CreateConversationResponse { session_id: id };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(&id.to_string()));
        assert!(json.contains("session_id"));
    }
}
