//! `GET /api/events` — SSE fan-out to connected browsers.
//!
//! Streams internal [`WebEvent`]s as standard SSE `data:` payloads.
//!
//! ## Auth
//!
//! Requires `Authorization: Bearer <token>`.  Returns 401 on missing or
//! invalid credentials (same token as the PTY WebSocket route).
//!
//! ## Backpressure and lag
//!
//! Each browser connection holds a [`broadcast::Receiver`] slot.  If a
//! slow client falls behind by more than [`crate::events::EVENT_CHANNEL_BUF`]
//! events, the receiver receives [`RecvError::Lagged`].  Rather than closing
//! the stream, we emit a synthetic `{"type":"lag","skipped":N}` event so the
//! browser can display a "N events dropped" indicator.
//!
//! ## Redaction
//!
//! The HMAC bearer token is stripped from every JSON payload before it
//! reaches the browser (defense against accidental self-embedding).  Full
//! PII redaction (paths, API keys) is added in the Phase-9 hardening pass.

use std::{convert::Infallible, sync::Arc};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::stream;
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::warn;
use uuid::Uuid;

use crate::{auth, events::WebEvent, server::AppState};

/// `GET /api/events` — authenticates and returns an SSE stream.
///
/// Responds 401 when `Authorization: Bearer <token>` is missing or invalid.
pub async fn sse_handler(headers: HeaderMap, State(state): State<AppState>) -> Response {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !auth::validate_bearer(authz, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let token: Arc<str> = Arc::from(state.config.token.as_str());
    let rx = state.event_tx.subscribe();

    let event_stream = stream::unfold((rx, token), drive_stream);

    Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `GET /api/builds/:id/events` — per-build SSE fan-out (Phase C).
///
/// Mirrors [`sse_handler`] but subscribes to the per-build
/// `BuildSession::event_tx` instead of the global channel. Authenticates
/// with the same global Bearer token (the browser already holds it from
/// the hash-fragment handshake), then looks up the build by UUID.
///
/// - `404 Not Found` if `build_id` is unknown.
/// - `401 Unauthorized` on missing or invalid bearer.
/// - Otherwise an SSE stream over [`WebEvent`]s for that build.
pub async fn sse_build_handler(
    Path(build_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Response {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !auth::validate_bearer(authz, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let Some(session) = state.builds.get(build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // Subscribe to this build's channel *before* returning — the stream's
    // first `.recv()` is lazy, but the `Receiver` itself is active from now,
    // so any event sent on `event_tx` after this line reaches this client.
    let token: Arc<str> = Arc::from(state.config.token.as_str());
    let rx = session.event_tx.subscribe();

    let event_stream = stream::unfold((rx, token), drive_stream);

    Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// State-machine step for the SSE stream.
///
/// Returns the next serialised [`Event`] and the updated `(rx, token)` state,
/// or `None` when the broadcast channel is closed.  On lag, emits a synthetic
/// `{"type":"lag","skipped":N}` event and continues.
async fn drive_stream(
    state: (broadcast::Receiver<WebEvent>, Arc<str>),
) -> Option<(
    Result<Event, Infallible>,
    (broadcast::Receiver<WebEvent>, Arc<str>),
)> {
    let (mut rx, token) = state;
    loop {
        match rx.recv().await {
            Ok(event) => {
                let data = match serde_json::to_string(&event) {
                    Ok(s) => redact(&s, &token),
                    Err(e) => {
                        warn!(error = %e, "failed to serialise WebEvent for SSE");
                        continue;
                    }
                };
                return Some((Ok(Event::default().data(data)), (rx, token)));
            }
            Err(RecvError::Lagged(n)) => {
                warn!(skipped = n, "SSE subscriber lagged — events dropped");
                let payload = format!(r#"{{"type":"lag","skipped":{n}}}"#);
                return Some((Ok(Event::default().data(payload)), (rx, token)));
            }
            Err(RecvError::Closed) => return None,
        }
    }
}

/// Replaces occurrences of `token` in `json` with `[REDACTED]`.
///
/// Prevents the HMAC bearer token from appearing in SSE payloads if it is
/// accidentally embedded in a span's metadata field.
fn redact(json: &str, token: &str) -> String {
    if token.is_empty() {
        return json.to_owned();
    }
    json.replace(token, "[REDACTED]")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn redact_replaces_token_in_payload() {
        let json = r#"{"action":"tool.call","token":"secret123"}"#;
        let result = redact(json, "secret123");
        assert!(
            !result.contains("secret123"),
            "token must be redacted: {result}"
        );
        assert!(result.contains("[REDACTED]"), "{result}");
    }

    #[test]
    fn redact_empty_token_returns_input_unchanged() {
        let json = r#"{"action":"tool.call"}"#;
        let result = redact(json, "");
        assert_eq!(result, json);
    }

    #[test]
    fn redact_no_occurrence_returns_input_unchanged() {
        let json = r#"{"action":"tool.call","actor":"soul"}"#;
        let result = redact(json, "secret999");
        assert_eq!(result, json);
    }

    #[test]
    fn redact_replaces_all_occurrences() {
        let json = r#"{"a":"secret","b":"secret"}"#;
        let result = redact(json, "secret");
        assert_eq!(result.matches("[REDACTED]").count(), 2);
    }
}
