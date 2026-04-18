//! Gateway UI tools — POST events to the Platform GUI's notify endpoint.
//!
//! Feature-gated by three environment variables set by the webshell PTY
//! spawner: `LA_GUI_URL`, `LA_BUILD_ID`, and `LA_NOTIFY_TOKEN`. When any is
//! unset, tools return a success response with `{"degraded": true, ...}` so
//! headless Claude sessions (outside the Platform GUI) don't break.
//!
//! The gateway posts to `{LA_GUI_URL}/api/builds/{LA_BUILD_ID}/notify` with
//! an `X-LA-Notify-Token` header; webshell validates it (constant-time) and
//! broadcasts the event to browser SSE subscribers.
//!
//! # Supported actions
//!
//! | Action | Required params | GUI effect |
//! |--------|-----------------|------------|
//! | `ui_set_active_build` | `build_id` | Switch focused build |
//! | `ui_focus_pillar` | `pillar` | Highlight pillar in conductor |
//! | `ui_flag_finding` | `severity`, `message` | Append to findings list |
//! | `ui_refresh_sitrep` | *(none)* | Nudge UI to re-fetch SITREP |
//! | `ui_update_conductor` | *(optional `queue`)* | Update conductor display |
//! | `ui_notify` | `message` | Generic notification toast |

use std::time::Duration;

use reqwest::Client;
use serde_json::{Value, json};

use super::security::validate_local_url;
use super::text_result;
use crate::error::GatewayError;

/// HTTP request timeout for Platform GUI notify calls.
const UI_TIMEOUT: Duration = Duration::from_secs(5);

/// Environment variable set by webshell: base URL of the Platform GUI.
const ENV_GUI_URL: &str = "LA_GUI_URL";

/// Environment variable set by webshell: per-build UUID.
const ENV_BUILD_ID: &str = "LA_BUILD_ID";

/// Environment variable set by webshell: per-build notify token.
const ENV_NOTIFY_TOKEN: &str = "LA_NOTIFY_TOKEN";

/// All UI action names routed through this module.
const UI_ACTIONS: &[&str] = &[
    "ui_set_active_build",
    "ui_focus_pillar",
    "ui_flag_finding",
    "ui_refresh_sitrep",
    "ui_update_conductor",
    "ui_notify",
];

/// Return true if `action` is a UI action routed via this module.
#[must_use]
pub fn is_ui_action(action: &str) -> bool {
    UI_ACTIONS.contains(&action)
}

/// Dispatch a UI action to the Platform GUI notify endpoint.
///
/// # Errors
///
/// - [`GatewayError::MissingParam`] — a required parameter is absent.
/// - [`GatewayError::InvalidParam`] — the `build_id` env var contains unsafe characters.
/// - [`GatewayError::Internal`] — HTTP transport failure, SSRF guard rejection,
///   or non-2xx response from the Platform GUI.
pub async fn dispatch(action: &str, params: Value) -> Result<Value, GatewayError> {
    let Some(ctx) = GuiContext::from_env() else {
        return degraded_response(action);
    };

    let event = build_event(action, &params)?;
    ctx.post(&event).await?;

    Ok(text_result(serde_json::to_string(&json!({
        "ok": true,
        "action": action,
        "event_type": event["type"],
        "build_id": ctx.build_id,
    }))?))
}

/// Build the success-but-no-op response for when GUI env vars are unset.
fn degraded_response(action: &str) -> Result<Value, GatewayError> {
    let reason = format!(
        "UI feature disabled: {ENV_GUI_URL}, {ENV_BUILD_ID}, and {ENV_NOTIFY_TOKEN} \
         must all be set (spawn Claude via the Platform GUI webshell to enable)"
    );
    Ok(text_result(serde_json::to_string(&json!({
        "degraded": true,
        "reason": reason,
        "action": action,
    }))?))
}

/// Runtime context for a Platform GUI notify call — all three env vars present.
struct GuiContext {
    url: String,
    build_id: String,
    token: String,
}

impl GuiContext {
    /// Resolve from environment. Returns `None` if any of the three required
    /// variables is missing — callers should treat this as silent degradation.
    fn from_env() -> Option<Self> {
        let url = std::env::var(ENV_GUI_URL).ok()?;
        let build_id = std::env::var(ENV_BUILD_ID).ok()?;
        let token = std::env::var(ENV_NOTIFY_TOKEN).ok()?;
        if url.is_empty() || build_id.is_empty() || token.is_empty() {
            return None;
        }
        Some(Self {
            url,
            build_id,
            token,
        })
    }

    /// POST the event JSON to the GUI's per-build notify endpoint.
    async fn post(&self, event: &Value) -> Result<(), GatewayError> {
        validate_build_id(&self.build_id)?;

        let base = self.url.trim_end_matches('/');
        let notify_url = format!("{base}/api/builds/{}/notify", self.build_id);
        validate_local_url(&notify_url)?;

        let client = Client::builder()
            .timeout(UI_TIMEOUT)
            .build()
            .map_err(|e| GatewayError::Internal(format!("HTTP client build error: {e}")))?;

        let response = client
            .post(&notify_url)
            .header("X-LA-Notify-Token", &self.token)
            .header("Content-Type", "application/json")
            .json(event)
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    GatewayError::Internal(format!(
                        "Cannot connect to Platform GUI at {base} — is webshell running?"
                    ))
                } else if e.is_timeout() {
                    GatewayError::Internal(format!(
                        "Platform GUI request timed out after {UI_TIMEOUT:?}"
                    ))
                } else {
                    GatewayError::Internal(format!("Platform GUI HTTP request failed: {e}"))
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(GatewayError::Internal(format!(
                "Platform GUI returned HTTP {status}: {body}"
            )));
        }

        Ok(())
    }
}

/// Validate that `build_id` is safe for URL-path interpolation.
///
/// Allows alphanumerics, hyphens, and underscores — matches UUID format and
/// rejects `/`, `..`, `?`, and other path-traversal characters.
fn validate_build_id(build_id: &str) -> Result<(), GatewayError> {
    if build_id.is_empty() {
        return Err(GatewayError::InvalidParam(
            "LA_BUILD_ID must not be empty".to_owned(),
        ));
    }
    if build_id.contains("..") {
        return Err(GatewayError::InvalidParam(
            "LA_BUILD_ID contains path traversal sequence '..'".to_owned(),
        ));
    }
    let is_safe = build_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !is_safe {
        return Err(GatewayError::InvalidParam(
            "LA_BUILD_ID contains forbidden characters — \
             only alphanumerics, hyphens, and underscores are allowed"
                .to_owned(),
        ));
    }
    Ok(())
}

/// Translate an `action` + params into the JSON event payload posted to the GUI.
fn build_event(action: &str, params: &Value) -> Result<Value, GatewayError> {
    match action {
        "ui_set_active_build" => {
            let build_id = required_str(params, "build_id")?;
            Ok(json!({"type": "set_active_build", "build_id": build_id}))
        }
        "ui_focus_pillar" => {
            let pillar = required_str(params, "pillar")?;
            Ok(json!({"type": "focus_pillar", "pillar": pillar}))
        }
        "ui_flag_finding" => {
            let severity = required_str(params, "severity")?;
            let message = required_str(params, "message")?;
            let file = params.get("file").and_then(Value::as_str);
            Ok(json!({
                "type": "flag_finding",
                "severity": severity,
                "message": message,
                "file": file,
            }))
        }
        "ui_refresh_sitrep" => Ok(json!({"type": "refresh_sitrep"})),
        "ui_update_conductor" => {
            let queue = params.get("queue").cloned();
            Ok(json!({"type": "update_conductor", "queue": queue}))
        }
        "ui_notify" => {
            let level = params
                .get("level")
                .and_then(Value::as_str)
                .unwrap_or("info");
            let message = required_str(params, "message")?;
            Ok(json!({"type": "notify", "level": level, "message": message}))
        }
        _ => Err(GatewayError::UnknownTool(format!("ui:{action}"))),
    }
}

/// Extract a required string parameter.
fn required_str<'a>(params: &'a Value, key: &'static str) -> Result<&'a str, GatewayError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or(GatewayError::MissingParam(key))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unsafe_code)]
mod tests {
    use super::*;

    // `is_ui_action` — membership check

    #[test]
    fn all_six_actions_recognized() {
        for action in UI_ACTIONS {
            assert!(is_ui_action(action), "'{action}' should be a UI action");
        }
    }

    #[test]
    fn non_ui_action_rejected() {
        assert!(!is_ui_action("read"));
        assert!(!is_ui_action("guard"));
        assert!(!is_ui_action("ui_unknown"));
        assert!(!is_ui_action(""));
    }

    // `build_event` — payload construction

    #[test]
    fn event_set_active_build() {
        let ev = build_event("ui_set_active_build", &json!({"build_id": "abc"})).unwrap();
        assert_eq!(ev["type"], "set_active_build");
        assert_eq!(ev["build_id"], "abc");
    }

    #[test]
    fn event_focus_pillar() {
        let ev = build_event("ui_focus_pillar", &json!({"pillar": "ARCH"})).unwrap();
        assert_eq!(ev["type"], "focus_pillar");
        assert_eq!(ev["pillar"], "ARCH");
    }

    #[test]
    fn event_flag_finding_with_file() {
        let ev = build_event(
            "ui_flag_finding",
            &json!({"severity": "high", "message": "bug", "file": "src/main.rs"}),
        )
        .unwrap();
        assert_eq!(ev["type"], "flag_finding");
        assert_eq!(ev["severity"], "high");
        assert_eq!(ev["message"], "bug");
        assert_eq!(ev["file"], "src/main.rs");
    }

    #[test]
    fn event_flag_finding_without_file() {
        let ev = build_event(
            "ui_flag_finding",
            &json!({"severity": "low", "message": "nit"}),
        )
        .unwrap();
        assert!(ev["file"].is_null());
    }

    #[test]
    fn event_refresh_sitrep_empty() {
        let ev = build_event("ui_refresh_sitrep", &json!({})).unwrap();
        assert_eq!(ev["type"], "refresh_sitrep");
    }

    #[test]
    fn event_update_conductor_with_queue() {
        let ev = build_event(
            "ui_update_conductor",
            &json!({"queue": [{"id": 1, "task": "scan"}]}),
        )
        .unwrap();
        assert_eq!(ev["type"], "update_conductor");
        assert!(ev["queue"].is_array());
    }

    #[test]
    fn event_notify_default_level_is_info() {
        let ev = build_event("ui_notify", &json!({"message": "hi"})).unwrap();
        assert_eq!(ev["level"], "info");
        assert_eq!(ev["message"], "hi");
    }

    #[test]
    fn event_notify_with_explicit_level() {
        let ev = build_event("ui_notify", &json!({"level": "warn", "message": "careful"})).unwrap();
        assert_eq!(ev["level"], "warn");
    }

    #[test]
    fn missing_required_param_errors() {
        let err = build_event("ui_focus_pillar", &json!({})).unwrap_err();
        assert!(matches!(err, GatewayError::MissingParam("pillar")));
    }

    #[test]
    fn unknown_action_errors() {
        let err = build_event("ui_bogus", &json!({})).unwrap_err();
        assert!(matches!(err, GatewayError::UnknownTool(_)));
    }

    // `validate_build_id` — URL-segment safety

    #[test]
    fn valid_uuid_accepted() {
        assert!(validate_build_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn empty_rejected() {
        assert!(validate_build_id("").is_err());
    }

    #[test]
    fn path_traversal_rejected() {
        assert!(validate_build_id("../secrets").is_err());
    }

    #[test]
    fn slash_rejected() {
        assert!(validate_build_id("a/b").is_err());
    }

    #[test]
    fn special_chars_rejected() {
        assert!(validate_build_id("a?b").is_err());
        assert!(validate_build_id("a b").is_err());
        assert!(validate_build_id("a%2f").is_err());
    }

    // `GuiContext::from_env` — env-var gating

    #[test]
    fn context_none_when_all_unset() {
        // Note: env is process-wide; we scope by clearing + restoring.
        let backup = (
            std::env::var(ENV_GUI_URL).ok(),
            std::env::var(ENV_BUILD_ID).ok(),
            std::env::var(ENV_NOTIFY_TOKEN).ok(),
        );
        // SAFETY: Rust's env APIs are !Send for removes; tests run single-threaded.
        unsafe {
            std::env::remove_var(ENV_GUI_URL);
            std::env::remove_var(ENV_BUILD_ID);
            std::env::remove_var(ENV_NOTIFY_TOKEN);
        }
        assert!(GuiContext::from_env().is_none());
        // Restore
        unsafe {
            if let Some(v) = backup.0 {
                std::env::set_var(ENV_GUI_URL, v);
            }
            if let Some(v) = backup.1 {
                std::env::set_var(ENV_BUILD_ID, v);
            }
            if let Some(v) = backup.2 {
                std::env::set_var(ENV_NOTIFY_TOKEN, v);
            }
        }
    }

    // `degraded_response` — silent-degradation envelope

    #[tokio::test]
    async fn degraded_response_shape() {
        let result = degraded_response("ui_focus_pillar").unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"degraded\":true"));
        assert!(text.contains("ui_focus_pillar"));
        assert!(text.contains(ENV_GUI_URL));
    }

    // `dispatch` — silent degradation via env absence

    #[tokio::test]
    async fn dispatch_degrades_silently_when_env_unset() {
        // Ensure env vars are unset for this test
        let backup = (
            std::env::var(ENV_GUI_URL).ok(),
            std::env::var(ENV_BUILD_ID).ok(),
            std::env::var(ENV_NOTIFY_TOKEN).ok(),
        );
        unsafe {
            std::env::remove_var(ENV_GUI_URL);
            std::env::remove_var(ENV_BUILD_ID);
            std::env::remove_var(ENV_NOTIFY_TOKEN);
        }
        let result = dispatch("ui_refresh_sitrep", json!({})).await.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"degraded\":true"));
        // Restore
        unsafe {
            if let Some(v) = backup.0 {
                std::env::set_var(ENV_GUI_URL, v);
            }
            if let Some(v) = backup.1 {
                std::env::set_var(ENV_BUILD_ID, v);
            }
            if let Some(v) = backup.2 {
                std::env::set_var(ENV_NOTIFY_TOKEN, v);
            }
        }
    }
}
