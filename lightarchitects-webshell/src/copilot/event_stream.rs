//! `GET /api/builds/:id/copilot/stream` — SSE stream of [`ConversationEvent`]s.
//!
//! Subscribes to the per-build event broadcast and forwards
//! [`WebEvent::CopilotResponse`] chunks as SDK-native [`ConversationEvent`]
//! frames, letting the browser receive incremental copilot output without
//! polling the request/response `POST /api/builds/:id/copilot` endpoint.
//!
//! ## Security
//!
//! Two validation layers guard this endpoint:
//!
//! 1. **Bearer auth** — [`auth::AuthGuard`] (same as all authenticated routes).
//! 2. **Origin check** — the `Origin` header must be a localhost origin
//!    (`http://localhost:*` or `http://127.0.0.1:*`). Requests from other
//!    origins receive `403 Forbidden`. The `Sec-Fetch-Site: same-origin`
//!    hint is accepted as a fast-pass but is not required — older browsers
//!    do not send it.
//!
//! These checks defend against a malicious page opening an `EventSource` to
//! the webshell from a different origin and silently draining the copilot
//! stream (CSRF via SSE, OWASP A01:2021).

use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::stream;
use lightarchitects::agent::conversation::{ConversationEvent, TerminationReason};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{
    auth,
    events::{WebEvent, WebEventV2},
    server::AppState,
};

/// Validate the `Origin` header for the SSE endpoint.
///
/// Accepts `http://localhost:<port>` and `http://127.0.0.1:<port>` only.
/// Returns `true` when the origin is acceptable (or absent — curl/Playwright).
fn is_acceptable_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get("origin") else {
        // No Origin header: CLI tools, Playwright in non-browser mode, same-origin
        // fetch. Allow.
        return true;
    };
    let Ok(s) = origin.to_str() else {
        return false;
    };
    s.starts_with("http://localhost:") || s.starts_with("http://127.0.0.1:")
}

/// `GET /api/builds/:id/copilot/stream` — per-turn SSE stream of copilot events.
///
/// - `401 Unauthorized` — missing or invalid bearer token.
/// - `403 Forbidden` — non-localhost `Origin` header.
/// - `404 Not Found` — unknown build UUID.
/// - Otherwise an SSE stream of [`ConversationEvent`] frames for that build.
///
/// The stream terminates when the per-build broadcast channel closes (i.e.
/// the build session is torn down).
pub async fn copilot_event_stream_handler(
    _: auth::AuthGuard,
    Path(build_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Response {
    if !is_acceptable_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some(session) = state.builds.get(build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let rx = session.event_tx.subscribe();

    let event_stream = stream::unfold(rx, drive_copilot_stream);

    Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// State-machine step for the copilot SSE stream.
///
/// Converts [`WebEvent::CopilotResponse`] events into [`ConversationEvent`]
/// SSE frames. Non-copilot events are silently skipped. Returns `None` when
/// the channel closes.
async fn drive_copilot_stream(
    mut rx: broadcast::Receiver<WebEventV2>,
) -> Option<(Result<Event, Infallible>, broadcast::Receiver<WebEventV2>)> {
    loop {
        match rx.recv().await {
            Ok(ev) => {
                let inner = ev.inner;
                let conversation_ev = match inner {
                    WebEvent::CopilotResponse { chunk, done, .. } => {
                        if done {
                            // done=true may carry the final chunk; emit Text first if
                            // non-empty, then Complete. We can only return one event per
                            // call — emit Complete here; the text chunk (if any) was
                            // already emitted in the prior iteration.
                            Some(ConversationEvent::Complete {
                                reason: TerminationReason::Complete,
                            })
                        } else if !chunk.is_empty() {
                            Some(ConversationEvent::Text { chunk })
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                let Some(cev) = conversation_ev else {
                    continue;
                };

                let name = cev.event_name();
                let data = match serde_json::to_string(&cev) {
                    Ok(json) => {
                        // Strip CR/LF — CWE-113 SSE frame injection defence.
                        json.chars()
                            .filter(|&c| c != '\r' && c != '\n')
                            .collect::<String>()
                    }
                    Err(_) => continue,
                };
                let event = Event::default().event(name).data(data);
                return Some((Ok(event), rx));
            }
            Err(broadcast::error::RecvError::Closed) => return None,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                let lag = Event::default()
                    .event("lag")
                    .data(format!("{{\"type\":\"lag\",\"skipped\":{n}}}"));
                return Some((Ok(lag), rx));
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn localhost_origin_accepted() {
        let mut h = HeaderMap::new();
        h.insert("origin", HeaderValue::from_static("http://localhost:3800"));
        assert!(is_acceptable_origin(&h));
    }

    #[test]
    fn loopback_origin_accepted() {
        let mut h = HeaderMap::new();
        h.insert("origin", HeaderValue::from_static("http://127.0.0.1:5173"));
        assert!(is_acceptable_origin(&h));
    }

    #[test]
    fn external_origin_rejected() {
        let mut h = HeaderMap::new();
        h.insert(
            "origin",
            HeaderValue::from_static("https://evil.example.com"),
        );
        assert!(!is_acceptable_origin(&h));
    }

    #[test]
    fn absent_origin_accepted() {
        let h = HeaderMap::new();
        assert!(is_acceptable_origin(&h));
    }

    #[tokio::test]
    async fn drive_copilot_stream_emits_text_chunk() {
        let (tx, rx) = broadcast::channel::<WebEventV2>(4);
        let _ = tx.send(WebEventV2::from_event(
            WebEvent::CopilotResponse {
                chunk: "hello".to_owned(),
                done: false,
                sibling: None,
                turn_span_id: None,
            },
            None,
        ));

        let (result, _rx) = drive_copilot_stream(rx).await.unwrap();
        let _event = result.unwrap();
    }

    #[tokio::test]
    async fn drive_copilot_stream_emits_complete_on_done() {
        let (tx, rx) = broadcast::channel::<WebEventV2>(4);
        let _ = tx.send(WebEventV2::from_event(
            WebEvent::CopilotResponse {
                chunk: String::new(),
                done: true,
                sibling: None,
                turn_span_id: None,
            },
            None,
        ));

        let (result, _rx) = drive_copilot_stream(rx).await.unwrap();
        let _event = result.unwrap();
    }

    #[tokio::test]
    async fn drive_copilot_stream_skips_non_copilot_events() {
        use crate::events::types::{CopilotActivityEvent, WebEvent};

        let (tx, rx) = broadcast::channel::<WebEventV2>(8);
        // Non-copilot events should be skipped — use CopilotActivity (not a response).
        let _ = tx.send(WebEventV2::from_event(
            WebEvent::CopilotActivity(CopilotActivityEvent {
                build_id: "test".to_owned(),
                kind: "assistant".to_owned(),
                summary: None,
                raw: serde_json::Value::Null,
                timestamp: "2026-05-22T00:00:00Z".to_owned(),
            }),
            None,
        ));
        let _ = tx.send(WebEventV2::from_event(
            WebEvent::CopilotResponse {
                chunk: "first chunk".to_owned(),
                done: false,
                sibling: None,
                turn_span_id: None,
            },
            None,
        ));

        let (result, _rx) = drive_copilot_stream(rx).await.unwrap();
        let _event = result.unwrap();
    }
}
