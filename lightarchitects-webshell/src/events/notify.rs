//! `POST /api/builds/:id/notify` — gateway → UI event forwarder.
//!
//! The `lightarchitects-gateway` MCP server's `ui.*` tools POST events here,
//! authenticated with the per-build `X-LA-Notify-Token` header (compared in
//! constant time). The handler parses the body as raw JSON and wraps it in
//! a [`WebEvent::GatewayNotify`] broadcast on the build's SSE channel.
//!
//! ## Trust domain
//!
//! This endpoint deliberately rejects the global webshell Bearer token.
//! Only the per-build notify token (32 random bytes, delivered to the
//! gateway via `LA_NOTIFY_TOKEN` env var on PTY spawn) is valid here.
//! This keeps browser-side bearer credentials out of the gateway's
//! trust domain — an XSS in the frontend cannot forge gateway events.
//!
//! ## Error map
//!
//! - `404 Not Found` — `build_id` not present in [`BuildRegistry`].
//! - `401 Unauthorized` — `X-LA-Notify-Token` absent or mismatched.
//! - `400 Bad Request` — body is not valid JSON.
//! - `200 OK` — event broadcast (even if zero subscribers; the broadcast
//!   channel behaviour is a deliberate feature, not an error).

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde_json::Value;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::{auth, events::WebEvent, server::AppState};

/// HTTP header carrying the hex-encoded 32-byte per-build notify token.
pub const NOTIFY_TOKEN_HEADER: &str = "x-la-notify-token";

/// Axum handler for `POST /api/builds/:id/notify`.
///
/// See the module docs for the error map. Returns `200 OK` on broadcast
/// success (even if there are currently no SSE subscribers).
pub async fn notify_handler(
    Path(build_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    // Look up the session — this also disambiguates "wrong token" from
    // "no such build": we return 404 first, preventing a token-oracle
    // side-channel on registry membership.
    let Some(session) = state.builds.get(build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // Extract the notify-token header. Missing header is a flat 401 so an
    // attacker cannot probe for the existence of the header name itself.
    //
    // SEC-5: log auth failures so silent probes are observable. Only safe
    // fields are logged — the raw token, the expected token, and the header
    // value never appear in logs, only the header length (which leaks
    // nothing useful to an attacker who already chose it).
    let Some(provided) = headers
        .get(NOTIFY_TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
    else {
        warn!(
            target: "auth",
            event = "notify_auth_failure",
            reason = "missing_header",
            build_id = %build_id,
            "rejected gateway notify with no X-LA-Notify-Token header",
        );
        return StatusCode::UNAUTHORIZED.into_response();
    };

    if !auth::validate_notify_token(provided, &session.notify_token) {
        warn!(
            target: "auth",
            event = "notify_auth_failure",
            reason = "invalid_token",
            build_id = %build_id,
            header_length = provided.len(),
            "rejected gateway notify with invalid X-LA-Notify-Token (token value not logged)",
        );
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Broadcast on the per-build channel. `send` returning `Err` just means
    // there are no live subscribers right now (e.g. no browser connected);
    // buffered gateway events for later subscribers would require the
    // channel to stay non-errored, but broadcast::Sender drops events when
    // no one is listening — acceptable UX (no browser → no UI update).
    let event = WebEvent::GatewayNotify { payload };
    match session.event_tx.send(event) {
        Ok(n) => {
            debug!(build_id = %build_id, subscribers = n, "gateway notify broadcast");
        }
        Err(_) => {
            debug!(build_id = %build_id, "gateway notify: no SSE subscribers");
        }
    }
    StatusCode::OK.into_response()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::config::{AgentSession, ClaudeBackend};
    use crate::session::BuildSession;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn anthropic_session() -> Arc<BuildSession> {
        Arc::new(BuildSession::new(
            PathBuf::from("/tmp"),
            AgentSession::Lightarchitects(ClaudeBackend::Anthropic),
        ))
    }

    #[test]
    fn notify_token_header_is_lowercase_for_axum() {
        // Axum normalises HTTP header names to lowercase; the constant must
        // match that form or `headers.get()` returns `None`.
        assert_eq!(NOTIFY_TOKEN_HEADER, NOTIFY_TOKEN_HEADER.to_lowercase());
    }

    #[test]
    fn gateway_notify_payload_preserves_body_shape() {
        // Sanity check: wrapping a payload in GatewayNotify keeps the body
        // reachable as `.payload` in the serialised JSON (contract with
        // the frontend's SSE dispatcher).
        let payload = serde_json::json!({"type": "focus_pillar", "pillar": "ARCH"});
        let event = WebEvent::GatewayNotify {
            payload: payload.clone(),
        };
        let serialised = serde_json::to_value(&event).unwrap();
        assert_eq!(serialised["type"], "gateway_notify");
        assert_eq!(serialised["payload"], payload);
    }

    #[tokio::test]
    async fn broadcast_on_session_event_tx_reaches_subscriber() {
        // Prove the mechanic the handler relies on: sending on event_tx
        // with a live subscriber delivers the event end-to-end.
        let session = anthropic_session();
        let mut rx = session.event_tx.subscribe();
        let event = WebEvent::GatewayNotify {
            payload: serde_json::json!({"type": "refresh_sitrep"}),
        };
        session.event_tx.send(event).unwrap();
        let received = rx.recv().await.unwrap();
        let json = serde_json::to_value(&received).unwrap();
        assert_eq!(json["type"], "gateway_notify");
        assert_eq!(json["payload"]["type"], "refresh_sitrep");
    }
}
