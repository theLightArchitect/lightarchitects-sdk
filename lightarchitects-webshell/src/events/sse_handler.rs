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
//! The HMAC bearer token is stripped from every `JSON` payload before it
//! reaches the browser (defense against accidental self-embedding).  Full
//! PII redaction (paths, API keys) is added in the Phase-9 hardening pass.

use std::{convert::Infallible, sync::Arc};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::stream;
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::warn;
use uuid::Uuid;

use crate::{auth, events::WebEventV2, server::AppState};

/// `GET /api/events` — authenticates and returns an SSE stream.
///
/// Authenticated via [`auth::AuthGuard`] — accepts either
/// `Authorization: Bearer <token>` or a valid `la_session` cookie. The cookie
/// path is what makes browser `EventSource` work: SSE cannot set Authorization
/// headers, so cookies are its only durable auth channel.
pub async fn sse_handler(_: auth::AuthGuard, State(state): State<AppState>) -> Response {
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
/// - `401 Unauthorized` on missing or invalid credentials.
/// - Otherwise an SSE stream over [`WebEvent`]s for that build.
///
/// Authenticated via [`auth::AuthGuard`] (Bearer header **or** `la_session` cookie).
pub async fn sse_build_handler(
    _: auth::AuthGuard,
    Path(build_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Response {
    let Some(session) = state.builds.get(build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // Subscribe to this build's channel *before* returning — the stream's
    // first `.recv()` is lazy, but the `Receiver` itself is active from now,
    // so any event sent on `event_tx` after this line reaches this client.
    //
    // Also subscribe to the global AppState broadcast so helix_entry,
    // soul_promotion, ayin_span, etc. reach per-build listeners (Phase 10
    // fix — the UI uses the per-build stream for all SSE, so it must see
    // both channels).
    let token: Arc<str> = Arc::from(state.config.token.as_str());
    let session_rx = session.event_tx.subscribe();
    let global_rx = state.event_tx.subscribe();

    let event_stream = stream::unfold((session_rx, global_rx, token), drive_multiplex_stream);

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
    state: (broadcast::Receiver<WebEventV2>, Arc<str>),
) -> Option<(
    Result<Event, Infallible>,
    (broadcast::Receiver<WebEventV2>, Arc<str>),
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

/// Multiplexed state-machine step for the per-build SSE stream.
///
/// Fans in both the session-scoped channel (PTY notifications + gateway
/// notifies) and the global `AppState` broadcast (`helix_entry`, `soul_promotion`,
/// ayin spans) so the browser receives every event on one stream.
///
/// `tokio::select!` races both receivers; whichever resolves first emits the
/// next event. Closing the session channel is a hard exit; closing the global
/// channel is not — the global channel may outlive any single build.
#[allow(clippy::future_not_send)]
async fn drive_multiplex_stream(
    state: (
        broadcast::Receiver<WebEventV2>,
        broadcast::Receiver<WebEventV2>,
        Arc<str>,
    ),
) -> Option<(
    Result<Event, Infallible>,
    (
        broadcast::Receiver<WebEventV2>,
        broadcast::Receiver<WebEventV2>,
        Arc<str>,
    ),
)> {
    let (mut session_rx, mut global_rx, token) = state;
    loop {
        let event = tokio::select! {
            r = session_rx.recv() => r,
            r = global_rx.recv() => r,
        };
        match event {
            Ok(ev) => {
                let data = match serde_json::to_string(&ev) {
                    Ok(s) => redact(&s, &token),
                    Err(e) => {
                        warn!(error = %e, "failed to serialise WebEvent for SSE");
                        continue;
                    }
                };
                return Some((
                    Ok(Event::default().data(data)),
                    (session_rx, global_rx, token),
                ));
            }
            Err(RecvError::Lagged(n)) => {
                warn!(skipped = n, "per-build SSE lagged");
                let payload = format!(r#"{{"type":"lag","skipped":{n}}}"#);
                return Some((
                    Ok(Event::default().data(payload)),
                    (session_rx, global_rx, token),
                ));
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
