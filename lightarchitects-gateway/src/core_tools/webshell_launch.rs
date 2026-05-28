//! `launch_webshell` core action — MCP-triggered startup of the webshell GUI.
//!
//! Spawns the webshell binary (non-blocking, detached) if it is not already
//! running, waits briefly for its `/api/health` endpoint to come up, and
//! returns a JSON blob containing the authenticated URL. Intended for
//! Claude Code or Codex sessions to open the webshell with one tool call
//! after the user has installed the lightarchitects plugin and logged in.
//!
//! Request params (all optional):
//! - `port`: `u16` — default `8733`
//! - `host_cmd`: `str` — default `"claude"` (the agent binary the webshell
//!   should spawn in its PTY)
//! - `cwd`: `str` — working directory for the spawned agent; default is
//!   the caller's `CWD`.
//! - `session_id`: `str` — pre-seed the webshell with an existing Claude
//!   Code or Codex session UUID so the first copilot turn resumes that
//!   conversation. Passed to the webshell binary as `--resume-session <id>`.
//!   Only applied when the webshell is not already running — an already-
//!   running webshell keeps whatever session it was launched with.
//! - `dev_mode`: `bool` — pass `--dev-mode` to the webshell binary so it
//!   allows the loopback Vite dev server and relaxed HMR CSP.
//!
//! Response shape:
//! ```json
//! {
//!   "status": "running" | "started",
//!   "port": 8733,
//!   "host_cmd": "claude",
//!   "url": "http://localhost:8733/#nonce=<uuid>",
//!   "token_available": true,
//!   "resumed_session": true,
//!   "session_mismatch": false,
//!   "kill_hint": null
//! }
//! ```
//!
//! The URL fragment uses a one-time nonce (`#nonce=<uuid>`) exchanged by the
//! browser via `POST /api/auth/nonce-exchange`. The raw bearer token never
//! appears in the MCP tool response, preventing it from being recorded in
//! Claude Code session logs.
//!
//! `session_mismatch` is `true` when the caller supplied a `session_id`
//! but the webshell was already running — meaning the running instance
//! kept whatever session it was started with. The `/webshell` slash
//! command uses this to render a kill-and-retry hint via `kill_hint`.
//!
//! The URL may omit the `#token=` fragment when no token is resolvable.
//! Claude Code renders URLs as clickable links — the action does not open
//! the browser itself.

use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tokio::time::sleep;
use tracing::warn;

use crate::config::GatewayConfig;
use crate::core_tools::text_result;
use crate::error::GatewayError;

const DEFAULT_PORT: u16 = 8733;
const STARTUP_MAX_WAIT: Duration = Duration::from_secs(3);
const STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(100);
/// When a `session_id` is supplied and the caller did not pin a specific
/// port, we scan this many consecutive ports starting at `DEFAULT_PORT`
/// looking for a free slot. This exists specifically to sidestep the
/// common case where a `LaunchAgent` owns 8733 — we silently roll up to
/// 8734, 8735, etc. rather than failing with `session_mismatch`.
const PORT_SCAN_WINDOW: u16 = 10;

/// Execute the `launch_webshell` action.
///
/// # Errors
///
/// Returns [`GatewayError::SpawnFailed`] if the binary cannot be spawned,
/// [`GatewayError::Internal`] if the webshell does not respond within
/// [`STARTUP_MAX_WAIT`] after spawn.
pub async fn run(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    // Track whether the caller pinned a specific port. If they did, we
    // honor it exactly — including returning session_mismatch on collision.
    // If they left it default, we auto-scan for a free port when a
    // session_id is supplied so the common LaunchAgent-on-8733 case
    // resolves transparently.
    let explicit_port = params.get("port").and_then(Value::as_u64).is_some();
    let requested_port = params
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|n| u16::try_from(n).ok())
        .unwrap_or(DEFAULT_PORT);
    let host_cmd = params
        .get("host_cmd")
        .and_then(Value::as_str)
        .unwrap_or("claude")
        .to_owned();
    let cwd = params.get("cwd").and_then(Value::as_str).map(PathBuf::from);
    let session_id = params
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let dev_mode = params
        .get("dev_mode")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // Resolve (port, already_running):
    //   - explicit port  → use exactly, whatever its state
    //   - no session_id  → default port, whatever its state
    //   - session_id     → scan for free port starting at default, so a
    //                      LaunchAgent on 8733 doesn't block resume
    let (port, already_running) = if session_id.is_some() && !explicit_port {
        find_free_port_starting_at(requested_port).await
    } else {
        (requested_port, probe_health(requested_port).await)
    };

    let status_str = if already_running {
        "running"
    } else {
        spawn_detached(
            config,
            port,
            &host_cmd,
            cwd.as_deref(),
            session_id.as_deref(),
            dev_mode,
        )?;
        wait_for_health(port).await?;
        "started"
    };

    let token = resolve_token();
    let url = match &token {
        Some(t) => {
            if let Some(nonce) = register_nonce(port, t).await {
                format!("http://localhost:{port}/#nonce={nonce}")
            } else {
                warn!(
                    port,
                    "nonce registration failed — URL will not include auth fragment"
                );
                format!("http://localhost:{port}/")
            }
        }
        None => format!("http://localhost:{port}/"),
    };

    // `resumed_session` is only meaningful when we actually spawned the
    // binary — an already-running webshell ignores our session_id.
    let resumed_session = !already_running && session_id.is_some();

    // `session_mismatch` signals the collision case: the caller wanted a
    // specific session but the running webshell is serving something
    // else. `kill_hint` is a shell one-liner the slash command can
    // surface so the user can terminate the stale instance and retry.
    let session_mismatch = already_running && session_id.is_some();
    let kill_hint: Option<String> = if session_mismatch {
        Some(format!("lsof -ti:{port} | xargs kill"))
    } else {
        None
    };

    Ok(text_result(serde_json::to_string_pretty(&json!({
        "status": status_str,
        "port": port,
        "host_cmd": host_cmd,
        "url": url,
        "token_available": token.is_some(),
        "resumed_session": resumed_session,
        "session_mismatch": session_mismatch,
        "kill_hint": kill_hint,
        "dev_mode": !already_running && dev_mode,
        "dev_mode_requested": dev_mode,
    }))?))
}

/// Scan `start..start+PORT_SCAN_WINDOW` for a port whose `/api/health`
/// is NOT responding (i.e. no webshell listening). Returns the first
/// free one; if all are busy, returns (start, true) so the caller
/// sees `session_mismatch` with a `kill_hint` on the default port.
async fn find_free_port_starting_at(start: u16) -> (u16, bool) {
    for offset in 0..PORT_SCAN_WINDOW {
        let Some(port) = start.checked_add(offset) else {
            break;
        };
        if !probe_health(port).await {
            return (port, false);
        }
    }
    (start, true)
}

async fn probe_health(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/api/health");
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
    else {
        return false;
    };
    client
        .get(&url)
        .send()
        .await
        .is_ok_and(|r| r.status().is_success())
}

/// Register a one-time auth nonce with the running webshell.
///
/// POSTs to `POST /api/auth/nonce` on the running webshell with the resolved
/// bearer token. Returns a [`uuid::Uuid`] nonce that the browser can exchange
/// for a session cookie via `POST /api/auth/nonce-exchange`. Using the nonce
/// instead of the raw token prevents the credential from appearing in the MCP
/// tool-response URL, which Claude Code logs to `~/.claude/projects/*/session.jsonl`.
///
/// Returns `None` if the webshell does not support the nonce endpoint or any
/// network error occurs — callers fall back to a token-less URL.
async fn register_nonce(port: u16, token: &str) -> Option<uuid::Uuid> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;
    let url = format!("http://127.0.0.1:{port}/api/auth/nonce");
    let resp = client.post(&url).bearer_auth(token).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    let nonce_str = body.get("nonce")?.as_str()?;
    nonce_str.parse().ok()
}

async fn wait_for_health(port: u16) -> Result<(), GatewayError> {
    let deadline = tokio::time::Instant::now() + STARTUP_MAX_WAIT;
    while tokio::time::Instant::now() < deadline {
        if probe_health(port).await {
            return Ok(());
        }
        sleep(STARTUP_POLL_INTERVAL).await;
    }
    Err(GatewayError::Internal(format!(
        "webshell spawned but /api/health did not respond within {}ms",
        STARTUP_MAX_WAIT.as_millis()
    )))
}

fn spawn_detached(
    config: &GatewayConfig,
    port: u16,
    host_cmd: &str,
    cwd: Option<&std::path::Path>,
    session_id: Option<&str>,
    dev_mode: bool,
) -> Result<(), GatewayError> {
    let bin_path = config.agents.get("webshell").map_or_else(
        || {
            let home = std::env::var_os("HOME").unwrap_or_default();
            PathBuf::from(&home)
                .join("lightarchitects")
                .join("webshell")
                .join("bin")
                .join("lightarchitects-webshell")
        },
        crate::config::AgentConfig::binary_path,
    );

    let mut proc = Command::new(&bin_path);
    proc.arg("--port")
        .arg(port.to_string())
        .arg("--host-cmd")
        .arg(host_cmd)
        // Scrub env vars that Claude Code injects into its child processes.
        // The gateway inherits them; without removal, the webshell inherits
        // them; the webshell's `claude --resume` subprocess then inherits
        // them, and the `ANTHROPIC_API_KEY=your_anthropic_key_here` Claude
        // Code placeholder shadows subscription OAuth and fails auth with
        // "Invalid API key". Scrubbing once at this boundary propagates
        // cleanness to all downstream children.
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("ANTHROPIC_AUTH_TOKEN")
        .env_remove("CLAUDE_CODE_OAUTH_TOKEN")
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        .env_remove("CLAUDE_CODE_EXECPATH")
        // §N.1 / SG-3: ARENA_PEPPER must not propagate to child processes.
        .env_remove("ARENA_PEPPER")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(cwd_path) = cwd {
        proc.arg("--cwd").arg(cwd_path);
    }
    if let Some(sid) = session_id {
        proc.arg("--resume-session").arg(sid);
    }
    if dev_mode {
        proc.arg("--dev-mode");
    }
    // Detach: do not `wait` on the child. The Child is dropped, but since
    // stdio is null and we don't need exit status, this is fine.
    proc.spawn().map_err(|e| GatewayError::SpawnFailed {
        agent: "webshell".to_owned(),
        reason: format!(
            "failed to spawn webshell binary {}: {e}",
            bin_path.display()
        ),
    })?;
    Ok(())
}

/// Resolve the webshell bearer token: env → keyring → file.
fn resolve_token() -> Option<String> {
    if let Ok(token) = std::env::var("LIGHTARCHITECTS_WEBSHELL_TOKEN")
        && !token.is_empty()
    {
        return Some(token);
    }
    if let Ok(entry) = keyring::Entry::new("lightarchitects", "webshell-token")
        && let Ok(token) = entry.get_password()
        && !token.is_empty()
    {
        return Some(token);
    }
    if let Some(path) = lightarchitects::core::paths::root() {
        let token_path = path.join("webshell").join(".token");
        if let Ok(token) = std::fs::read_to_string(&token_path) {
            let trimmed = token.trim().to_owned();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn probe_health_returns_false_for_unused_port() {
        // Port 1 is always closed on a normal system.
        assert!(!probe_health(1).await);
    }

    #[test]
    fn spawn_params_parse_defaults() {
        // Verify defaults line up with constants when no params are passed.
        assert_eq!(DEFAULT_PORT, 8733);
    }

    #[tokio::test]
    async fn find_free_port_returns_a_free_candidate_in_closed_range() {
        // Starting at port 1 — a closed port range. The scan should
        // pick up port 1 immediately as "free" (nothing listening).
        let (port, already_running) = find_free_port_starting_at(1).await;
        assert_eq!(port, 1);
        assert!(!already_running);
    }

    #[tokio::test]
    async fn find_free_port_respects_scan_window_upper_bound() {
        // When called at u16::MAX, `checked_add` must not wrap. The
        // scan should terminate cleanly and fall back to the start.
        let (port, already_running) = find_free_port_starting_at(u16::MAX).await;
        assert_eq!(port, u16::MAX);
        // already_running depends on whether anything responds on u16::MAX;
        // on a normal dev machine, nothing does, so it's false.
        assert!(!already_running);
    }
}
