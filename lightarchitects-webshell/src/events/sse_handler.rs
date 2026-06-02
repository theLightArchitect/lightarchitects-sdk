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
    extract::{Path, RawQuery, State},
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

use lightarchitects::lightsquad::agent_role::AgentRole;

use crate::{
    auth,
    events::{TopicFilter, WebEventV2},
    server::AppState,
};

/// State tuple for the multiplexed per-build SSE stream.
type MultiplexState = (
    broadcast::Receiver<WebEventV2>,
    broadcast::Receiver<WebEventV2>,
    Arc<str>,
    Option<TopicFilter>,
    Option<AgentRole>,
);

/// Parse `?role=<role>` from a raw query string.
///
/// Returns `None` when the parameter is absent or the value is not a valid
/// [`AgentRole`]. Invalid roles are silently ignored — same policy as
/// `topic_from_raw`: the connection stays open, unfiltered, rather than
/// returning 400 on a long-lived SSE stream.
fn role_from_raw(raw: Option<&str>) -> Option<AgentRole> {
    raw.and_then(|q| {
        q.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            if k == "role" { Some(v) } else { None }
        })
    })
    .and_then(|v| v.parse::<AgentRole>().ok())
}

/// Parse `?topic=<pattern>` from a raw query string without external deps.
///
/// Returns `None` when the parameter is absent or the pattern is invalid.
/// Invalid patterns are silently ignored — the connection continues unfiltered
/// rather than returning an error, because SSE streams are long-lived and a
/// bad pattern would silently break the connection if we returned 400.
///
/// Handles common percent-encoded characters in the pattern (`%2A` → `*`,
/// `%3E` → `>`). Full percent-decoding is not required because topic pattern
/// characters (`.`, `*`, `>`, alphanumerics) are valid query-string chars.
fn topic_from_raw(raw: Option<&str>) -> Option<TopicFilter> {
    raw.and_then(|q| {
        q.split('&')
            .find_map(|pair| {
                let (k, v) = pair.split_once('=')?;
                if k == "topic" { Some(v) } else { None }
            })
            .map(|v| {
                // Minimal decode for the two percent-encoded chars that appear
                // in NATS-style topic patterns when set via URLSearchParams.
                v.replace("%2A", "*")
                    .replace("%3E", ">")
                    .replace("%2a", "*")
                    .replace("%3e", ">")
            })
            .and_then(|v| TopicFilter::parse(&v).ok())
    })
}

/// `GET /api/events` — authenticates and returns an SSE stream.
///
/// Authenticated via [`auth::AuthGuard`] — accepts either
/// `Authorization: Bearer <token>` or a valid `la_session` cookie. The cookie
/// path is what makes browser `EventSource` work: SSE cannot set Authorization
/// headers, so cookies are its only durable auth channel.
pub async fn sse_handler(
    _: auth::AuthGuard,
    RawQuery(raw): RawQuery,
    State(state): State<AppState>,
) -> Response {
    let token: Arc<str> = Arc::from(state.config.token.as_str());
    let filter = topic_from_raw(raw.as_deref());
    let role = role_from_raw(raw.as_deref());
    let rx = state.event_tx.subscribe();

    let event_stream = stream::unfold((rx, token, filter, role), drive_stream);

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
    RawQuery(raw): RawQuery,
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
    let filter = topic_from_raw(raw.as_deref());
    let role = role_from_raw(raw.as_deref());
    let session_rx = session.event_tx.subscribe();
    let global_rx = state.event_tx.subscribe();

    let event_stream = stream::unfold(
        (session_rx, global_rx, token, filter, role),
        drive_multiplex_stream,
    );

    Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// State-machine step for the SSE stream.
///
/// Returns the next serialised [`Event`] and the updated state, or `None`
/// when the broadcast channel is closed.  On lag, emits a synthetic
/// `{"type":"lag","skipped":N}` event and continues.
///
/// When `filter` is `Some`, events whose `topic` does not match are skipped
/// silently. When `role` is `Some`, events whose `agent_id` does not match
/// the role string are skipped silently. Both filters may be active together.
async fn drive_stream(
    state: (
        broadcast::Receiver<WebEventV2>,
        Arc<str>,
        Option<TopicFilter>,
        Option<AgentRole>,
    ),
) -> Option<(
    Result<Event, Infallible>,
    (
        broadcast::Receiver<WebEventV2>,
        Arc<str>,
        Option<TopicFilter>,
        Option<AgentRole>,
    ),
)> {
    let (mut rx, token, filter, role) = state;
    loop {
        match rx.recv().await {
            Ok(event) => {
                if let Some(f) = &filter {
                    if !f.matches(&event.topic) {
                        continue;
                    }
                }
                if let Some(r) = &role {
                    if event.agent_id != r.to_string() {
                        continue;
                    }
                }
                let data = match serde_json::to_string(&event) {
                    Ok(s) => redact(&s, &token),
                    Err(e) => {
                        warn!(error = %e, "failed to serialise WebEvent for SSE");
                        continue;
                    }
                };
                return Some((Ok(Event::default().data(data)), (rx, token, filter, role)));
            }
            Err(RecvError::Lagged(n)) => {
                warn!(skipped = n, "SSE subscriber lagged — events dropped");
                let payload = format!(r#"{{"type":"lag","skipped":{n}}}"#);
                return Some((
                    Ok(Event::default().data(payload)),
                    (rx, token, filter, role),
                ));
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
///
/// When `filter` is `Some`, events whose `topic` does not match are skipped
/// silently. When `role` is `Some`, events whose `agent_id` does not match
/// the role string are skipped silently.
#[allow(clippy::future_not_send)]
async fn drive_multiplex_stream(
    state: MultiplexState,
) -> Option<(Result<Event, Infallible>, MultiplexState)> {
    let (mut session_rx, mut global_rx, token, filter, role) = state;
    loop {
        let event = tokio::select! {
            r = session_rx.recv() => r,
            r = global_rx.recv() => r,
        };
        match event {
            Ok(ev) => {
                if let Some(f) = &filter {
                    if !f.matches(&ev.topic) {
                        continue;
                    }
                }
                if let Some(r) = &role {
                    if ev.agent_id != r.to_string() {
                        continue;
                    }
                }
                let data = match serde_json::to_string(&ev) {
                    Ok(s) => redact(&s, &token),
                    Err(e) => {
                        warn!(error = %e, "failed to serialise WebEvent for SSE");
                        continue;
                    }
                };
                return Some((
                    Ok(Event::default().data(data)),
                    (session_rx, global_rx, token, filter, role),
                ));
            }
            Err(RecvError::Lagged(n)) => {
                warn!(skipped = n, "per-build SSE lagged");
                let payload = format!(r#"{{"type":"lag","skipped":{n}}}"#);
                return Some((
                    Ok(Event::default().data(payload)),
                    (session_rx, global_rx, token, filter, role),
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

/// Multi-secret variant of [`redact`].
///
/// Used by `drive_native_sse` to redact both the session bearer token AND the
/// Ollama Cloud API key from native SSE stream chunks before the bytes cross
/// the wire to the operator's browser.  Closes merge-gate finding GAP-3
/// (SSE body bypass of `sse_handler::redact()` path) from
/// webshell-la-native-backend.
///
/// Empty strings in `secrets` are skipped to support
/// `Option<SecretString>::map(...)` patterns at the call site.
pub(crate) fn redact_secrets(input: &str, secrets: &[&str]) -> String {
    let mut out = input.to_owned();
    for s in secrets {
        if s.is_empty() {
            continue;
        }
        if out.contains(*s) {
            out = out.replace(*s, "[REDACTED]");
        }
    }
    out
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

    // ── redact_secrets (multi-secret variant, Phase-10 GAP-3 close) ──

    #[test]
    fn redact_secrets_handles_multiple_distinct_secrets() {
        let body = r#"{"err":"auth failed: token=sess-abc-123 key=bearer-xyz-456 stack..."}"#;
        let result = redact_secrets(body, &["sess-abc-123", "bearer-xyz-456"]);
        assert!(
            !result.contains("sess-abc-123"),
            "session token must be redacted: {result}"
        );
        assert!(
            !result.contains("bearer-xyz-456"),
            "API key must be redacted: {result}"
        );
        assert_eq!(
            result.matches("[REDACTED]").count(),
            2,
            "both secrets must be replaced: {result}"
        );
    }

    #[test]
    fn redact_secrets_skips_empty_strings() {
        // Models Option<SecretString>::map(...).as_deref() → "" → still safe.
        let body = r#"{"text":"hello world"}"#;
        let result = redact_secrets(body, &["", ""]);
        assert_eq!(result, body, "empty secrets must not modify input");
    }

    #[test]
    fn redact_secrets_with_empty_list_is_identity() {
        let body = r#"{"chunk":"streaming text"}"#;
        let result = redact_secrets(body, &[]);
        assert_eq!(result, body);
    }

    #[test]
    fn redact_secrets_preserves_ordinary_error_text() {
        // Positive regression: non-secret content passes through unchanged.
        // Phase-2 Risk R6 (sanitizer hides real error message from operator).
        let body = r#"{"type":"error","message":"provider error: unknown model"}"#;
        let result = redact_secrets(body, &["sess-abc", "bearer-xyz"]);
        assert_eq!(
            result, body,
            "ordinary error text must pass through verbatim: {result}"
        );
    }

    #[test]
    fn redact_secrets_redacts_both_when_one_substring_of_other() {
        // Edge case: la_native_api_key might share a prefix with session token.
        // Ensure both still redacted independently.
        let body = "prefix-12345 prefix-12345-extra";
        let result = redact_secrets(body, &["prefix-12345-extra", "prefix-12345"]);
        // The longer one was replaced first, then the shorter one — both gone.
        assert!(!result.contains("prefix-12345"), "got: {result}");
    }
}
