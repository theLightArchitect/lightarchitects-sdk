//! HTTP POST handler for external control commands.
//!
//! Accepts a JSON [`ControlCommand`] body, validates the bearer token,
//! and broadcasts the command as a [`WebEvent::Control`] so it reaches
//! all connected browsers via the SSE fan-out.
//!
//! This is the primary mechanism by which Claude Code (or any process
//! with the auth token) can programmatically control the web app UI.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use tracing::info;

use crate::{auth, server::AppState};

use super::types::{ControlCommand, WebEvent};

/// `POST /api/control` — accepts a control command and broadcasts it.
///
/// The caller must provide a valid `Authorization: Bearer <token>` header.
/// The body must be a valid JSON [`ControlCommand`] (tagged enum with
/// `"command"` discriminant).
///
/// # Response codes
///
/// - `200` — command accepted and broadcast.
/// - `400` — body is not valid JSON or does not match a known command.
/// - `401` — missing or invalid `Authorization` header.
pub async fn control_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(cmd): Json<ControlCommand>,
) -> impl IntoResponse {
    // Validate bearer token.
    let Some(authz) = headers.get("authorization") else {
        return StatusCode::UNAUTHORIZED;
    };
    let Ok(authz_str) = authz.to_str() else {
        return StatusCode::UNAUTHORIZED;
    };
    if !auth::validate_bearer(authz_str, &state.config.token) {
        return StatusCode::UNAUTHORIZED;
    }

    // Broadcast the control command as a WebEvent.
    let event = WebEvent::Control(cmd.clone());
    let receiver_count = state
        .event_tx
        .send(event)
        .map_or(0, |_| state.event_tx.receiver_count());
    info!(
        target: "webshell",
        command = ?cmd,
        receivers = receiver_count,
        "Control command broadcast"
    );

    StatusCode::OK
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn control_command_focus_panel_round_trips() {
        let json = r#"{"command":"focus_panel","panel":"helix"}"#;
        let cmd: ControlCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, ControlCommand::FocusPanel { ref panel } if panel == "helix"));
    }

    #[test]
    fn control_command_resize_panels_round_trips() {
        let json = r#"{"command":"resize_panels","terminal":60,"helix":40}"#;
        let cmd: ControlCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(
            cmd,
            ControlCommand::ResizePanels {
                terminal: 60,
                helix: 40
            }
        ));
    }

    #[test]
    fn control_command_set_helix_zoom_round_trips() {
        let json = r#"{"command":"set_helix_zoom","level":5.0}"#;
        let cmd: ControlCommand = serde_json::from_str(json).unwrap();
        assert!(
            matches!(cmd, ControlCommand::SetHelixZoom { level } if (level - 5.0).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn control_command_set_panel_visibility_round_trips() {
        let json = r#"{"command":"set_panel_visibility","panel":"terminal","visible":false}"#;
        let cmd: ControlCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(
            cmd,
            ControlCommand::SetPanelVisibility {
                ref panel,
                visible: false
            } if panel == "terminal"
        ));
    }

    #[test]
    fn control_command_notify_round_trips() {
        let json = r#"{"command":"notify","message":"hello","level":"info"}"#;
        let cmd: ControlCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(
            cmd,
            ControlCommand::Notify {
                ref message,
                ref level
            } if message == "hello" && level == "info"
        ));
    }

    #[test]
    fn control_command_unknown_command_is_error() {
        let json = r#"{"command":"unknown","panel":"helix"}"#;
        assert!(serde_json::from_str::<ControlCommand>(json).is_err());
    }

    #[test]
    fn control_command_missing_field_is_error() {
        let json = r#"{"command":"focus_panel"}"#;
        assert!(serde_json::from_str::<ControlCommand>(json).is_err());
    }

    #[test]
    fn web_event_control_serialises_type_tag() {
        let cmd = ControlCommand::FocusPanel {
            panel: "helix".to_owned(),
        };
        let event = WebEvent::Control(cmd);
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            json.contains(r#""type":"control""#),
            "missing type tag: {json}"
        );
        assert!(
            json.contains(r#""command":"focus_panel""#),
            "missing command tag: {json}"
        );
    }
}
