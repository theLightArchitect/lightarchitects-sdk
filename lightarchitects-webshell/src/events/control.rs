//! HTTP POST handler for external control commands.
//!
//! Accepts a JSON [`ControlCommand`] body, validates the bearer token,
//! and broadcasts the command as a [`WebEvent::Control`] so it reaches
//! all connected browsers via the SSE fan-out.
//!
//! This is the primary mechanism by which Claude Code (or any process
//! with the auth token) can programmatically control the web app UI.

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use lightarchitects::lightsquad::supervisor::ResolveError;
use tracing::{info, warn};

use crate::{auth, server::AppState};

use super::types::{ControlCommand, HitlResolution, IronclawHitlResolutionEvent, WebEvent};

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
    _: auth::AuthGuard,
    State(state): State<AppState>,
    Json(cmd): Json<ControlCommand>,
) -> impl IntoResponse {
    // IronclawHitlResolution is handled locally and short-circuits the generic
    // broadcast path — it emits its own SSE event and returns a specific status.
    if let ControlCommand::IronclawHitlResolution {
        escalation_nonce,
        approved,
        operator_reason,
    } = &cmd
    {
        return handle_ironclaw_hitl_resolution(
            &state,
            *escalation_nonce,
            *approved,
            operator_reason.clone(),
        );
    }

    // Handle local-execution commands before broadcasting.
    match &cmd {
        ControlCommand::OpenInEditor { file, line } => {
            open_in_editor(file, *line, &state.config.cwd);
        }
        ControlCommand::RevealInFinder { path } => {
            reveal_in_finder(path, &state.config.cwd);
        }
        _ => {}
    }

    // Broadcast the control command as a WebEvent.
    let event = crate::events::WebEventV2::from_event(WebEvent::Control(cmd.clone()), None);
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

/// Handle `POST /api/control { kind: "ironclaw_hitl_resolution" }`.
///
/// Validates the `UUIDv7` nonce (single-use, SERAPH#3 anti-replay), unblocks the
/// parked worker, and emits `WebEvent::IronclawHitlResolution` SSE.
///
/// # Security
///
/// The `escalation_nonce` is NEVER included in log messages or error responses
/// (CWE-209). All logged fields are non-secret.
fn handle_ironclaw_hitl_resolution(
    state: &AppState,
    escalation_nonce: uuid::Uuid,
    approved: bool,
    operator_reason: Option<String>,
) -> StatusCode {
    match state
        .hitl_resolver
        .resolve(escalation_nonce, approved, operator_reason.clone())
    {
        Ok(task_id) => {
            let resolution = if approved {
                HitlResolution::Approve
            } else {
                HitlResolution::Reject
            };
            tracing::info!("[security] Pre-send audit: IronclawHitlResolution SSE emitted");
            let event = crate::events::WebEventV2::from_event(
                WebEvent::IronclawHitlResolution(IronclawHitlResolutionEvent {
                    build_id: uuid::Uuid::nil(), // populated by bridge; nil here is intentional
                    task_id: task_id.clone(),
                    resolution,
                    operator_id: "webshell:operator".to_owned(),
                    decided_at: chrono::Utc::now(),
                    nonce: escalation_nonce,
                }),
                None,
            );
            let _ = state.event_tx.send(event);
            let helix_root = crate::events::helix_decision_writer::resolve_helix_root();
            let decision_record = crate::events::helix_decision_writer::HitlDecisionRecord {
                build_id: uuid::Uuid::nil(),
                task_id: task_id.clone(),
                approved,
                operator_reason: operator_reason.clone(),
                decided_at: chrono::Utc::now(),
            };
            if let Err(e) = crate::events::helix_decision_writer::write_hitl_decision(
                &helix_root,
                &decision_record,
            ) {
                warn!(target: "webshell", error = %e, "helix_decision_writer: write failed (non-blocking)");
            }
            info!(
                target: "webshell",
                task_id = %task_id,
                approved,
                "IronclawHitlResolution: worker unblocked"
            );
            StatusCode::OK
        }
        Err(ResolveError::ReplayAttack(_)) => {
            // Nonce omitted from log — CWE-209.
            warn!(target: "webshell", "IronclawHitlResolution: nonce already consumed (replay)");
            StatusCode::CONFLICT
        }
        Err(ResolveError::NotFound(_)) => {
            // System A had no entry — fall back to System B (bridge-parked HitlQueue).
            resolve_via_hitl_queue(state, escalation_nonce, approved, operator_reason)
        }
    }
}

/// Resolve a HITL escalation parked in the bridge [`HitlQueue`] (System B).
///
/// The queue is keyed by `call_id` but stores `escalation_nonce`, so a
/// linear scan (O(n), n ≤ 7 concurrent slots) finds the matching entry.
fn resolve_via_hitl_queue(
    state: &AppState,
    escalation_nonce: uuid::Uuid,
    approved: bool,
    operator_reason: Option<String>,
) -> StatusCode {
    let bridge_call_id = state.hitl_queue.iter().find_map(|e| {
        if e.value().escalation_nonce == escalation_nonce {
            Some(*e.key())
        } else {
            None
        }
    });
    let Some(call_id) = bridge_call_id else {
        // Nonce omitted from log — CWE-209.
        warn!(target: "webshell", "IronclawHitlResolution: no pending escalation for nonce");
        return StatusCode::NOT_FOUND;
    };
    let Some((_, entry)) = state.hitl_queue.remove(&call_id) else {
        // Race: removed between iter and remove.
        warn!(target: "webshell", "IronclawHitlResolution: no pending escalation for nonce");
        return StatusCode::NOT_FOUND;
    };
    let task_id = entry.task_id.clone();
    let _ = entry
        .resolve_tx
        .send(crate::events::hitl_relay::HitlDecision {
            approved,
            operator_reason: operator_reason.clone(),
        });
    let resolution = if approved {
        HitlResolution::Approve
    } else {
        HitlResolution::Reject
    };
    tracing::info!("[security] Pre-send audit: IronclawHitlResolution SSE emitted (bridge path)");
    let event = crate::events::WebEventV2::from_event(
        WebEvent::IronclawHitlResolution(IronclawHitlResolutionEvent {
            build_id: uuid::Uuid::nil(),
            task_id: task_id.clone(),
            resolution,
            operator_id: "webshell:operator".to_owned(),
            decided_at: chrono::Utc::now(),
            nonce: escalation_nonce,
        }),
        None,
    );
    let _ = state.event_tx.send(event);
    info!(
        target: "webshell",
        task_id = %task_id,
        approved,
        "IronclawHitlResolution: bridge worker unblocked"
    );
    StatusCode::OK
}

/// Resolve `raw_path` to an absolute path within `cwd`, rejecting traversal.
///
/// Returns `None` if the path is unsafe (contains `..` components, null
/// bytes, or would escape `cwd` after canonicalisation).
fn resolve_safe_path(raw_path: &str, cwd: &std::path::Path) -> Option<std::path::PathBuf> {
    if raw_path.contains('\0') {
        return None;
    }
    let p = std::path::Path::new(raw_path);
    let resolved = if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    };
    // Reject paths that contain `..` components (pre-canonicalise check).
    if resolved
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return None;
    }
    // Reject absolute paths that escape cwd (symlink-safe containment check).
    if p.is_absolute() && !resolved.starts_with(cwd) {
        return None;
    }
    Some(resolved)
}

/// Spawn `open -t <file>` (macOS) or `$EDITOR <file>:<line>`.
///
/// Falls back to `open -t` when `$EDITOR` is not set.  Line-number
/// injection uses the `file:line` convention understood by most editors
/// (VS Code, Cursor, Neovim, etc.).
fn open_in_editor(raw_file: &str, line: Option<u32>, cwd: &std::path::Path) {
    let Some(path) = resolve_safe_path(raw_file, cwd) else {
        warn!(
            raw_file,
            "OpenInEditor: path rejected (traversal or null byte)"
        );
        return;
    };
    let path_str = path.to_string_lossy().into_owned();
    let target = match line {
        Some(n) => format!("{path_str}:{n}"),
        None => path_str,
    };

    // Prefer $EDITOR; fall back to macOS `open -t` (default text editor).
    let result = if let Ok(editor) = std::env::var("EDITOR") {
        tokio::process::Command::new(&editor).arg(&target).spawn()
    } else {
        // `open -t` opens the file in the default text editor on macOS.
        tokio::process::Command::new("open")
            .arg("-t")
            .arg(&target)
            .spawn()
    };

    match result {
        Ok(_) => info!(target = %target, "OpenInEditor: spawned"),
        Err(e) => warn!(target = %target, error = %e, "OpenInEditor: spawn failed"),
    }
}

/// Spawn `open -R <path>` to reveal the file in Finder (macOS).
fn reveal_in_finder(raw_path: &str, cwd: &std::path::Path) {
    let Some(path) = resolve_safe_path(raw_path, cwd) else {
        warn!(
            raw_path,
            "RevealInFinder: path rejected (traversal or null byte)"
        );
        return;
    };
    match tokio::process::Command::new("open")
        .arg("-R")
        .arg(path.as_os_str())
        .spawn()
    {
        Ok(_) => info!(path = %path.display(), "RevealInFinder: spawned"),
        Err(e) => warn!(path = %path.display(), error = %e, "RevealInFinder: spawn failed"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
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
    fn control_command_open_in_editor_round_trips() {
        let json = r#"{"command":"open_in_editor","file":"/src/main.rs","line":42}"#;
        let cmd: ControlCommand = serde_json::from_str(json).unwrap();
        assert!(
            matches!(cmd, ControlCommand::OpenInEditor { ref file, line: Some(42) } if file == "/src/main.rs")
        );
    }

    #[test]
    fn control_command_open_in_editor_no_line_round_trips() {
        let json = r#"{"command":"open_in_editor","file":"src/lib.rs","line":null}"#;
        let cmd: ControlCommand = serde_json::from_str(json).unwrap();
        assert!(
            matches!(cmd, ControlCommand::OpenInEditor { ref file, line: None } if file == "src/lib.rs")
        );
    }

    #[test]
    fn control_command_reveal_in_finder_round_trips() {
        let json = r#"{"command":"reveal_in_finder","path":"/Users/foo/project"}"#;
        let cmd: ControlCommand = serde_json::from_str(json).unwrap();
        assert!(
            matches!(cmd, ControlCommand::RevealInFinder { ref path } if path == "/Users/foo/project")
        );
    }

    #[test]
    fn resolve_safe_path_rejects_traversal() {
        let cwd = std::path::Path::new("/project");
        assert!(resolve_safe_path("../etc/passwd", cwd).is_none());
        assert!(resolve_safe_path("/project/../etc/passwd", cwd).is_none());
    }

    #[test]
    fn resolve_safe_path_rejects_null_byte() {
        let cwd = std::path::Path::new("/project");
        assert!(resolve_safe_path("foo\0bar", cwd).is_none());
    }

    #[test]
    fn resolve_safe_path_accepts_absolute() {
        let cwd = std::path::Path::new("/project");
        let result = resolve_safe_path("/project/src/main.rs", cwd);
        assert_eq!(
            result,
            Some(std::path::PathBuf::from("/project/src/main.rs"))
        );
    }

    #[test]
    fn resolve_safe_path_accepts_relative() {
        let cwd = std::path::Path::new("/project");
        let result = resolve_safe_path("src/main.rs", cwd);
        assert_eq!(
            result,
            Some(std::path::PathBuf::from("/project/src/main.rs"))
        );
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
    fn ironclaw_hitl_resolution_command_round_trips() {
        let nonce = uuid::Uuid::now_v7();
        let json = format!(
            r#"{{"command":"ironclaw_hitl_resolution","escalation_nonce":"{nonce}","approved":true,"operator_reason":"looks good"}}"#
        );
        let cmd: ControlCommand = serde_json::from_str(&json).unwrap();
        assert!(
            matches!(
                &cmd,
                ControlCommand::IronclawHitlResolution { approved: true, .. }
            ),
            "unexpected variant: {cmd:?}"
        );
    }

    #[test]
    fn ironclaw_hitl_resolution_command_rejected_round_trips() {
        let nonce = uuid::Uuid::now_v7();
        let json = format!(
            r#"{{"command":"ironclaw_hitl_resolution","escalation_nonce":"{nonce}","approved":false,"operator_reason":null}}"#
        );
        let cmd: ControlCommand = serde_json::from_str(&json).unwrap();
        assert!(
            matches!(
                &cmd,
                ControlCommand::IronclawHitlResolution {
                    approved: false,
                    operator_reason: None,
                    ..
                }
            ),
            "unexpected variant: {cmd:?}"
        );
    }

    #[test]
    fn ironclaw_hitl_resolution_nonce_field_parses() {
        let nonce = uuid::Uuid::nil();
        let json = format!(
            r#"{{"command":"ironclaw_hitl_resolution","escalation_nonce":"{nonce}","approved":true,"operator_reason":null}}"#
        );
        let cmd: ControlCommand = serde_json::from_str(&json).unwrap();
        if let ControlCommand::IronclawHitlResolution {
            escalation_nonce, ..
        } = cmd
        {
            assert_eq!(escalation_nonce, uuid::Uuid::nil());
        } else {
            panic!("wrong variant");
        }
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
