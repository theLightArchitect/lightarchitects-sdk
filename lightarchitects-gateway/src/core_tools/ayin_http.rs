//! AYIN HTTP transport — bridges gateway actions to AYIN's HTTP endpoints.
//!
//! AYIN runs as a `LaunchAgent` HTTP server at `localhost:3742`, not as an MCP
//! subprocess. This module translates gateway `{action, params}` into HTTP
//! `GET` requests against the AYIN viewer API and wraps the response in the
//! standard MCP text-result envelope.
//!
//! # Supported actions
//!
//! | Gateway action | HTTP endpoint | Required params |
//! |----------------|---------------|-----------------|
//! | `sessions` | `GET /api/sessions` | *(none)* |
//! | `spans` | `GET /api/spans/:actor/:date` | `actor`, `date` |
//! | `conversations` | `GET /api/conversations/:date` | `date` |

use std::time::Duration;

use reqwest::Client;
use serde_json::Value;

use super::text_result;
use crate::error::GatewayError;

/// Base URL for the AYIN HTTP viewer (`LaunchAgent` service).
const AYIN_BASE_URL: &str = "http://127.0.0.1:3742";

/// HTTP request timeout for AYIN calls.
const AYIN_TIMEOUT: Duration = Duration::from_secs(10);

/// Dispatch an AYIN action to the appropriate HTTP endpoint.
///
/// # Errors
///
/// - [`GatewayError::MissingParam`] — a required parameter is absent.
/// - [`GatewayError::Internal`] — HTTP connection failure or non-2xx response.
/// - [`GatewayError::UnknownTool`] — action is not a known AYIN action.
pub async fn dispatch(action: &str, params: Value) -> Result<Value, GatewayError> {
    match action {
        "sessions" | "list_sessions" => get_sessions().await,
        "spans" | "get_spans" => get_spans(&params).await,
        "conversations" | "get_conversations" => get_conversations(&params).await,
        _ => Err(GatewayError::UnknownTool(format!("ayin:{action}"))),
    }
}

/// Check whether `action` is a gateway-routable AYIN action.
///
/// Uses the `AyinAction` enum from `lightarchitects-ayin` as the source of
/// truth — avoids maintaining a duplicate list of action names.
#[must_use]
pub fn is_ayin_action(action: &str) -> bool {
    action
        .parse::<lightarchitects::ayin::AyinAction>()
        .is_ok_and(|a| a.is_gateway_routable())
}

// ── URL segment validation ───────────────────────────────────────────────────

/// Validate that a user-supplied value is safe for interpolation into a URL path
/// segment.
///
/// Allows only alphanumeric characters, hyphens (`-`), underscores (`_`), and
/// dots (`.`). Rejects `/`, `..`, `?`, `#`, `%`, and any other characters that
/// could cause path traversal or endpoint injection.
///
/// # Errors
///
/// Returns [`GatewayError::InvalidParam`] with a descriptive message when the
/// value contains forbidden characters.
fn validate_url_segment(value: &str, param_name: &str) -> Result<(), GatewayError> {
    if value.is_empty() {
        return Err(GatewayError::InvalidParam(format!(
            "'{param_name}' must not be empty"
        )));
    }

    if value.contains("..") {
        return Err(GatewayError::InvalidParam(format!(
            "'{param_name}' contains path traversal sequence '..'"
        )));
    }

    let is_safe = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');

    if !is_safe {
        return Err(GatewayError::InvalidParam(format!(
            "'{param_name}' contains forbidden characters — \
             only alphanumeric, hyphens, underscores, and dots are allowed"
        )));
    }

    Ok(())
}

// ── Endpoint handlers ────────────────────────────────────────────────────────

/// `GET /api/sessions` — list all trace sessions.
async fn get_sessions() -> Result<Value, GatewayError> {
    let url = format!("{AYIN_BASE_URL}/api/sessions");
    let body = http_get(&url).await?;
    format_response(body)
}

/// `GET /api/spans/:actor/:date` — load trace spans for a session.
async fn get_spans(params: &Value) -> Result<Value, GatewayError> {
    let actor = params
        .get("actor")
        .and_then(Value::as_str)
        .ok_or(GatewayError::MissingParam("actor"))?;
    let date = params
        .get("date")
        .and_then(Value::as_str)
        .ok_or(GatewayError::MissingParam("date"))?;

    validate_url_segment(actor, "actor")?;
    validate_url_segment(date, "date")?;

    let url = format!("{AYIN_BASE_URL}/api/spans/{actor}/{date}");
    let body = http_get(&url).await?;
    format_response(body)
}

/// `GET /api/conversations/:date` — load conversation traces for a date.
async fn get_conversations(params: &Value) -> Result<Value, GatewayError> {
    let date = params
        .get("date")
        .and_then(Value::as_str)
        .ok_or(GatewayError::MissingParam("date"))?;

    validate_url_segment(date, "date")?;

    let url = format!("{AYIN_BASE_URL}/api/conversations/{date}");
    let body = http_get(&url).await?;
    format_response(body)
}

// ── HTTP helpers ─────────────────────────────────────────────────────────────

/// Perform a `GET` request and return the parsed JSON body.
///
/// Connection errors produce a helpful message referencing the AYIN `LaunchAgent`.
/// Validates that the URL targets localhost before making the request.
async fn http_get(url: &str) -> Result<Value, GatewayError> {
    super::security::validate_local_url(url)?;
    let client = Client::builder()
        .timeout(AYIN_TIMEOUT)
        .build()
        .map_err(|e| GatewayError::Internal(format!("HTTP client build error: {e}")))?;

    let response = client.get(url).send().await.map_err(|e| {
        if e.is_connect() {
            GatewayError::Internal(format!(
                "Cannot connect to AYIN at {AYIN_BASE_URL} — is AYIN running? \
                 (launchctl kickstart -k gui/$(id -u)/io.lightarchitects.ayin)"
            ))
        } else if e.is_timeout() {
            GatewayError::Internal(format!("AYIN request timed out after {AYIN_TIMEOUT:?}"))
        } else {
            GatewayError::Internal(format!("AYIN HTTP request failed: {e}"))
        }
    })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(GatewayError::Internal(format!(
            "AYIN returned HTTP {status}: {body}"
        )));
    }

    response
        .json::<Value>()
        .await
        .map_err(|e| GatewayError::Internal(format!("AYIN response parse error: {e}")))
}

/// Wrap a JSON value in the standard MCP text-result envelope.
fn format_response(body: Value) -> Result<Value, GatewayError> {
    let pretty = serde_json::to_string_pretty(&body).map_err(GatewayError::Json)?;
    Ok(text_result(pretty))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn is_ayin_action_recognises_public_actions() {
        assert!(is_ayin_action("sessions"));
        assert!(is_ayin_action("spans"));
        assert!(is_ayin_action("conversations"));
    }

    #[test]
    fn is_ayin_action_rejects_internal_actions() {
        assert!(!is_ayin_action("dashboard"));
        assert!(!is_ayin_action("vendor"));
    }

    #[test]
    fn is_ayin_action_rejects_unknown_strings() {
        assert!(!is_ayin_action("guard"));
        assert!(!is_ayin_action("helix"));
        assert!(!is_ayin_action("frobnicate"));
    }

    #[tokio::test]
    async fn dispatch_unknown_action_returns_error() {
        let err = dispatch("nonexistent", json!({})).await.unwrap_err();
        assert!(
            matches!(err, GatewayError::UnknownTool(ref s) if s.contains("ayin:nonexistent")),
            "expected UnknownTool, got {err:?}"
        );
    }

    #[tokio::test]
    async fn dispatch_sessions_returns_error_when_offline() {
        // AYIN may not be running in the test environment — verify we get a
        // clear error message, not a panic.
        let result = dispatch("sessions", json!({})).await;
        match result {
            Ok(_) => {} // AYIN is running locally — that is fine
            Err(GatewayError::Internal(msg)) => {
                assert!(
                    msg.contains("AYIN") || msg.contains("connect"),
                    "error should reference AYIN, got: {msg}"
                );
            }
            Err(other) => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_spans_requires_actor_and_date() {
        let err = dispatch("spans", json!({})).await.unwrap_err();
        assert!(
            matches!(err, GatewayError::MissingParam("actor")),
            "expected MissingParam(actor), got {err:?}"
        );

        let err = dispatch("spans", json!({"actor": "claude"}))
            .await
            .unwrap_err();
        assert!(
            matches!(err, GatewayError::MissingParam("date")),
            "expected MissingParam(date), got {err:?}"
        );
    }

    #[tokio::test]
    async fn dispatch_conversations_requires_date() {
        let err = dispatch("conversations", json!({})).await.unwrap_err();
        assert!(
            matches!(err, GatewayError::MissingParam("date")),
            "expected MissingParam(date), got {err:?}"
        );
    }

    // ── URL segment validation ──────────────────────────────────────────

    #[test]
    fn validate_url_segment_accepts_safe_values() {
        assert!(validate_url_segment("claude", "actor").is_ok());
        assert!(validate_url_segment("2026-03-30", "date").is_ok());
        assert!(validate_url_segment("eva_session.1", "actor").is_ok());
        assert!(validate_url_segment("corso-build-42", "actor").is_ok());
    }

    #[test]
    fn validate_url_segment_rejects_path_traversal() {
        let err = validate_url_segment("../../admin", "actor").unwrap_err();
        assert!(
            matches!(err, GatewayError::InvalidParam(ref s) if s.contains("path traversal")),
            "expected InvalidParam with traversal message, got {err:?}"
        );
    }

    #[test]
    fn validate_url_segment_rejects_slash() {
        let err = validate_url_segment("foo/bar", "actor").unwrap_err();
        assert!(
            matches!(err, GatewayError::InvalidParam(ref s) if s.contains("forbidden")),
            "expected InvalidParam with forbidden message, got {err:?}"
        );
    }

    #[test]
    fn validate_url_segment_rejects_query_and_fragment() {
        let err = validate_url_segment("date?admin=true", "date").unwrap_err();
        assert!(
            matches!(err, GatewayError::InvalidParam(_)),
            "expected InvalidParam, got {err:?}"
        );

        let err = validate_url_segment("date#frag", "date").unwrap_err();
        assert!(
            matches!(err, GatewayError::InvalidParam(_)),
            "expected InvalidParam, got {err:?}"
        );
    }

    #[test]
    fn validate_url_segment_rejects_percent_encoding() {
        let err = validate_url_segment("..%2f..%2fadmin", "actor").unwrap_err();
        assert!(
            matches!(err, GatewayError::InvalidParam(_)),
            "expected InvalidParam, got {err:?}"
        );
    }

    #[test]
    fn validate_url_segment_rejects_empty() {
        let err = validate_url_segment("", "actor").unwrap_err();
        assert!(
            matches!(err, GatewayError::InvalidParam(ref s) if s.contains("empty")),
            "expected InvalidParam with empty message, got {err:?}"
        );
    }

    #[tokio::test]
    async fn dispatch_spans_rejects_traversal_in_actor() {
        let err = dispatch(
            "spans",
            json!({"actor": "../../admin", "date": "2026-03-30"}),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, GatewayError::InvalidParam(ref s) if s.contains("actor")),
            "expected InvalidParam for actor, got {err:?}"
        );
    }

    #[tokio::test]
    async fn dispatch_spans_rejects_traversal_in_date() {
        let err = dispatch("spans", json!({"actor": "claude", "date": "../secrets"}))
            .await
            .unwrap_err();
        assert!(
            matches!(err, GatewayError::InvalidParam(ref s) if s.contains("date")),
            "expected InvalidParam for date, got {err:?}"
        );
    }

    #[tokio::test]
    async fn dispatch_conversations_rejects_traversal_in_date() {
        let err = dispatch("conversations", json!({"date": "../../admin"}))
            .await
            .unwrap_err();
        assert!(
            matches!(err, GatewayError::InvalidParam(ref s) if s.contains("date")),
            "expected InvalidParam for date, got {err:?}"
        );
    }
}
