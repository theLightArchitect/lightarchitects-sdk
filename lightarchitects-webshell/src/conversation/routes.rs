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
//! | GET | `/api/conversation/recent` | [`list_recent_conversations`] | — |
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

/// Entry in the `GET /api/conversation/recent` response.
#[derive(Serialize)]
struct RecentSessionEntry {
    session_id: Uuid,
    title: String,
    turn_count: usize,
    /// Seconds elapsed since session creation (monotonic, not wall-clock).
    ago_secs: u64,
}

/// Query parameters for `GET /api/conversation/recent`.
#[derive(Deserialize)]
pub struct RecentQuery {
    #[serde(default = "default_recent_limit")]
    limit: usize,
}

fn default_recent_limit() -> usize {
    20
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

/// `GET /api/conversation/recent` — list active sessions ordered by recency.
///
/// Iterates the in-memory session store (O(n), bounded by TTL=3600s eviction).
/// Returns at most `limit` entries (default 20, cap 100), sorted with the most
/// recently created session first.
///
/// Authenticated via [`auth::AuthGuard`] (Bearer **or** `la_session` cookie).
pub async fn list_recent_conversations(
    _: auth::AuthGuard,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<RecentQuery>,
) -> impl IntoResponse {
    let limit = params.limit.min(100);
    let mut entries: Vec<RecentSessionEntry> = state
        .conversation_store
        .iter()
        .map(|entry| {
            let h = entry.value();
            let inner = h
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            RecentSessionEntry {
                session_id: h.session_id,
                title: inner
                    .title
                    .clone()
                    .unwrap_or_else(|| format!("{} turns", inner.turn_count)),
                turn_count: inner.turn_count,
                ago_secs: h.created_at.elapsed().as_secs(),
            }
        })
        .collect();
    entries.sort_by_key(|e| e.ago_secs);
    entries.truncate(limit);
    Json(entries)
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

/// Request body for `POST /api/conversation/{id}/resume`.
#[derive(Deserialize)]
pub struct ResumeRequest {
    /// Single-use nonce issued in the `hitl_pause` SSE event.
    pub nonce: String,
}

/// `POST /api/conversation/{id}/resume` — release a parked HITL turn.
///
/// The nonce must match an entry in the session's [`ResumeRegistry`]; expired,
/// mismatched, or already-used nonces return `404 Not Found`.
///
/// Returns `200 OK` on success; the parked turn unblocks and resumes SSE
/// emission. Returns `404` when the session or nonce is not found.
///
/// Authenticated via [`auth::AuthGuard`] (Bearer **or** `la_session` cookie).
pub async fn resume_conversation(
    _: auth::AuthGuard,
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<ResumeRequest>,
) -> impl IntoResponse {
    let Some(handle) = state.conversation_store.get(&session_id) else {
        return StatusCode::NOT_FOUND;
    };
    match handle
        .resume_registry
        .take(&body.nonce, &session_id.to_string())
    {
        Some(_) => StatusCode::OK,
        None => StatusCode::NOT_FOUND,
    }
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
                let lag = ConvSSEEvent::Lag { skipped: n };
                let payload = serde_json::to_string(&lag)
                    .unwrap_or_else(|_| format!(r#"{{"type":"lag","skipped":{n}}}"#));
                return Some((Ok(Event::default().data(payload)), rx));
            }
            Err(RecvError::Closed) => return None,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines
)]
mod tests {
    use std::{ffi::OsString, path::PathBuf};

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt as _;

    use super::*;
    use crate::server::build_app;

    const TEST_TOKEN: &str = "conv-test-token-xyz";

    fn test_state() -> crate::server::AppState {
        crate::server::AppState::for_test(
            crate::config::Config {
                port: 0,
                host_cmd: OsString::from("bash"),
                cwd: PathBuf::from("/tmp"),
                token: TEST_TOKEN.to_owned(),
                token_source: crate::config::TokenSource::EnvVar,
                agent: crate::config::AgentSession::default(),
                claude_agent_template: None,
                container_mode: crate::container::ContainerMode::Auto,
                dev_mode: false,
                max_context_prompts: 50,
                litellm: crate::config::LiteLLMConfig::default(),
                hermes_mcp: crate::config::HermesMcpConfig::default(),
                resume_session_id: None,
            },
            crate::container::DockerCapability::Unavailable,
        )
    }

    #[test]
    fn create_response_serializes_session_id() {
        let id = Uuid::new_v4();
        let resp = CreateConversationResponse { session_id: id };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(&id.to_string()));
        assert!(json.contains("session_id"));
    }

    #[tokio::test]
    async fn create_conversation_returns_201_with_session_id() {
        let app = build_app(test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/api/conversation")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from("{}"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json["session_id"].as_str().is_some(),
            "session_id must be present"
        );
        // Verify it parses as a valid UUID
        let id_str = json["session_id"].as_str().unwrap();
        assert!(
            uuid::Uuid::parse_str(id_str).is_ok(),
            "session_id must be a valid UUID"
        );
    }

    #[tokio::test]
    async fn create_conversation_requires_auth() {
        let app = build_app(test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/api/conversation")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn stream_unknown_session_returns_404() {
        let app = build_app(test_state());
        let unknown_id = Uuid::new_v4();
        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/conversation/{unknown_id}/stream"))
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn send_turn_unknown_session_returns_404() {
        let app = build_app(test_state());
        let unknown_id = Uuid::new_v4();
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/conversation/{unknown_id}"))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(r#"{"message":"hello"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn end_conversation_lifecycle() {
        // Create → verify 201 → delete → verify 204 → delete again → verify 404
        let state = test_state();
        let app = build_app(state);

        // Step 1: create
        let create_req = Request::builder()
            .method("POST")
            .uri("/api/conversation")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from("{}"))
            .unwrap();
        let create_resp = app.clone().oneshot(create_req).await.unwrap();
        assert_eq!(create_resp.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(create_resp.into_body(), 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let session_id = json["session_id"].as_str().unwrap().to_owned();

        // Step 2: delete → 204
        let del_req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/conversation/{session_id}"))
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::empty())
            .unwrap();
        let del_resp = app.clone().oneshot(del_req).await.unwrap();
        assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

        // Step 3: delete again → 404 (idempotent check — already removed)
        let repeat_req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/conversation/{session_id}"))
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::empty())
            .unwrap();
        let repeat_resp = app.oneshot(repeat_req).await.unwrap();
        assert_eq!(repeat_resp.status(), StatusCode::NOT_FOUND);
    }
}
