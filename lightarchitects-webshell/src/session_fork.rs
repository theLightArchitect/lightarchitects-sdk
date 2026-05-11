//! `POST /api/session/fork` — hand a live webshell session off to a terminal.
//!
//! Given a `build_id`, looks up the build's copilot `session_id` and agent
//! kind, then spawns a native terminal running either `claude --resume <id>`
//! or `codex exec resume <id>`. The on-disk session JSONL is shared (Claude
//! at `~/.claude/projects/<hash>/<uuid>.jsonl`; Codex at
//! `~/.codex/sessions/<date>/rollout-*.jsonl`), so the terminal picks up
//! the exact same conversation — no snapshot, no migration.
//!
//! Platform support (Tier 1):
//! - **macOS**: spawns via `osascript` + `Terminal.app`. No extra deps.
//! - **Linux / Windows**: returns the resume command string in the response
//!   so the frontend can display a copy-paste banner. Keeping the spawn
//!   surface minimal is deliberate — cross-platform terminal spawning has
//!   enough edge cases (gnome-terminal vs konsole vs xterm vs wt.exe) that
//!   it warrants its own follow-up rather than bloating this action.
//!
//! Response shape:
//! ```json
//! {
//!   "launched": true,          // spawned OK on this platform
//!   "command": "claude --resume 9a8b...",
//!   "session_id": "9a8b...",
//!   "agent": "lightarchitects",
//!   "platform": "macos"
//! }
//! ```

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    auth,
    config::{AgentKind, AgentSession},
    server::AppState,
};

/// Request body for `POST /api/session/fork`.
#[derive(Debug, Deserialize)]
pub struct ForkRequest {
    /// Build whose copilot session should be handed off.
    pub build_id: Uuid,
}

/// Successful fork response.
#[derive(Debug, Serialize)]
pub struct ForkResponse {
    /// `true` when a native terminal was spawned; `false` when the frontend
    /// should show the `command` string for the user to paste manually.
    pub launched: bool,
    /// The exact resume command (e.g. `claude --resume <uuid>`).
    pub command: String,
    /// The session UUID being resumed (echoed for the frontend banner).
    pub session_id: String,
    /// Agent family of the handed-off session (`"lightarchitects"` / `"codex"`).
    pub agent: &'static str,
    /// Platform string (`"macos"` / `"linux"` / `"windows"` / `"unsupported"`).
    pub platform: &'static str,
}

/// `POST /api/session/fork` — fork a build's copilot session to a terminal.
///
/// Auth-gated. 404 when the build is unknown; 409 when the build has no
/// `session_id` yet (user hasn't sent a turn); 501 on unsupported platforms
/// (Linux/Windows) with the resume command returned in `command`.
#[allow(clippy::module_name_repetitions)]
pub async fn fork_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<ForkRequest>,
) -> impl IntoResponse {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !auth::validate_bearer(authz, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let Some(session) = state.builds.get(body.build_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "build_not_found" })),
        )
            .into_response();
    };

    // Read the copilot session id (populated after turn 1 or via
    // --resume-session). 409 when absent so the frontend can keep the
    // Fork button disabled instead of spawning an empty terminal.
    let session_id = {
        let guard = session.copilot_proc.lock().await;
        guard.as_ref().and_then(|p| p.session_id.clone())
    };
    let Some(session_id) = session_id else {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "no_session_yet",
                "hint": "send at least one chat turn before forking",
            })),
        )
            .into_response();
    };

    // Allowlist-validate session_id and cwd before embedding them in an
    // AppleScript string (H-92). Denylists are incomplete — AppleScript's
    // `do shell script` expands `$`, backticks, `;`, `|`, etc. Allowlists
    // make injection structurally impossible for these well-bounded inputs.
    //
    // session_id: UUID format — hex digits and hyphens only, max 36 chars.
    if !is_safe_fork_session_id(&session_id) {
        warn!(session_id = %session_id, "fork: rejected session_id with non-UUID chars");
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid_session_id" })),
        )
            .into_response();
    }

    // Build the resume command for the build's agent family.
    // Prefix with `cd <cwd>` so the terminal opens in the build's working
    // directory — critical because `claude --resume` derives the session
    // file path from the CWD's project hash.
    let cwd_str = session.cwd.to_string_lossy();
    // cwd: filesystem path — alphanumeric, `/`, `_`, `.`, `-`, space only,
    // max 512 chars. Rejects `$`, backticks, `;`, `|`, `&`, quotes, etc.
    if !is_safe_fork_cwd(&cwd_str) {
        warn!(cwd = %cwd_str, "fork: rejected cwd with non-allowlist chars");
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid_cwd" })),
        )
            .into_response();
    }
    let (agent_label, command) = match session.agent.kind() {
        AgentKind::Lightarchitects | AgentKind::LightarchitectsNative => (
            match session.agent {
                AgentSession::LightarchitectsNative(_) => "lightarchitects_native",
                _ => "lightarchitects",
            },
            format!("cd {cwd_str} && claude --resume {session_id}"),
        ),
        AgentKind::Codex => (
            "codex",
            format!("cd {cwd_str} && codex exec resume {session_id}"),
        ),
    };

    let (launched, platform) = spawn_terminal(&command);

    info!(
        build_id = %body.build_id,
        session_id = %session_id,
        agent = agent_label,
        platform,
        launched,
        "session forked to terminal"
    );

    (
        StatusCode::OK,
        Json(ForkResponse {
            launched,
            command,
            session_id,
            agent: agent_label,
            platform,
        }),
    )
        .into_response()
}

/// Spawn a native terminal running `command`. Returns `(launched, platform)`.
#[cfg(target_os = "macos")]
fn spawn_terminal(command: &str) -> (bool, &'static str) {
    // AppleScript: tell Terminal to `do script` opens a new window with
    // the command running inside a login shell. Escaping: embed the
    // command as a quoted AppleScript string, escaping any internal
    // double-quotes. Session UUIDs are hex+hyphens, so escaping here is
    // defensive rather than load-bearing.
    let escaped = command.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("tell application \"Terminal\" to do script \"{escaped}\"");
    match std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_child) => {
            // Truly detached: we neither wait on nor kill the child.
            // `std::process::Child::drop` (unlike `tokio::process::Child`)
            // does NOT send SIGKILL — the kernel keeps osascript running
            // to completion (typically ~100ms after it fires the
            // AppleEvent) and reaps it as a short-lived zombie. Waiting
            // here would block the async handler for seconds the first
            // time Terminal.app boots.
            (true, "macos")
        }
        Err(e) => {
            warn!(error = %e, "osascript spawn failed");
            (false, "macos")
        }
    }
}

#[cfg(target_os = "linux")]
fn spawn_terminal(_command: &str) -> (bool, &'static str) {
    // Linux: many terminal emulators, no canonical launcher. Return the
    // command string so the frontend can render a copy-paste banner.
    // A future iteration can try gnome-terminal / konsole / xterm in
    // priority order.
    (false, "linux")
}

#[cfg(target_os = "windows")]
fn spawn_terminal(_command: &str) -> (bool, &'static str) {
    // Windows Terminal (`wt.exe`) would be the canonical target, but
    // testing that lives outside Tier 1. Return the command string.
    (false, "windows")
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn spawn_terminal(_command: &str) -> (bool, &'static str) {
    (false, "unsupported")
}

/// Allowlist validator for session IDs embedded in `AppleScript` strings.
///
/// Accepts UUID format: hex digits (`[0-9a-fA-F]`) and hyphens, max 36 chars.
/// This makes shell/AppleScript injection via `session_id` structurally impossible.
fn is_safe_fork_session_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Allowlist validator for working-directory paths embedded in `AppleScript` strings.
///
/// Accepts filesystem path chars: alphanumeric, `/`, `_`, `.`, `-`, and space.
/// Rejects `$`, backticks, `;`, `|`, `&`, `>`, `<`, `(`, `)`, quotes, etc.
/// Max 512 chars prevents runaway buffer use.
fn is_safe_fork_cwd(cwd: &str) -> bool {
    !cwd.is_empty()
        && cwd.len() <= 512
        && cwd
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '.' | '-' | ' '))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn session_id_allowlist_accepts_uuid() {
        assert!(is_safe_fork_session_id(
            "9a8b7c6d-5e4f-3a2b-1c0d-e9f8a7b6c5d4"
        ));
    }

    #[test]
    fn session_id_allowlist_rejects_dollar() {
        assert!(!is_safe_fork_session_id("$(rm -rf ~)"));
    }

    #[test]
    fn session_id_allowlist_rejects_backtick() {
        assert!(!is_safe_fork_session_id("`id`"));
    }

    #[test]
    fn session_id_allowlist_rejects_overlong() {
        assert!(!is_safe_fork_session_id(&"a".repeat(37)));
    }

    #[test]
    fn cwd_allowlist_accepts_normal_path() {
        assert!(is_safe_fork_cwd("/Users/kft/Projects/my-project"));
    }

    #[test]
    fn cwd_allowlist_rejects_semicolon() {
        assert!(!is_safe_fork_cwd("/tmp; rm -rf ~"));
    }

    #[test]
    fn cwd_allowlist_rejects_pipe() {
        assert!(!is_safe_fork_cwd("/tmp | cat /etc/passwd"));
    }

    #[test]
    fn cwd_allowlist_rejects_dollar() {
        assert!(!is_safe_fork_cwd("/tmp/$HOME"));
    }

    #[test]
    fn cwd_allowlist_rejects_overlong() {
        assert!(!is_safe_fork_cwd(&"/a".repeat(257)));
    }

    #[test]
    fn fork_response_serializes_with_expected_fields() {
        let resp = ForkResponse {
            launched: true,
            command: "claude --resume abc-123".to_owned(),
            session_id: "abc-123".to_owned(),
            agent: "lightarchitects",
            platform: "macos",
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""launched":true"#));
        assert!(json.contains(r#""command":"claude --resume abc-123""#));
        assert!(json.contains(r#""agent":"lightarchitects""#));
    }
}
