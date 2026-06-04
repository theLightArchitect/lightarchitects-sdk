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
//! - `kill_existing`: `bool` — when `true` AND the caller pinned a specific
//!   port AND that port is already serving a webshell, send SIGTERM to the
//!   listening PID and wait up to 3 s for the socket to free, then spawn a
//!   fresh instance. Enables the "always reclaim :8733" pattern without
//!   port-walking. Has no effect when `port` is left default (auto-scan
//!   already finds a free slot) or when the port is free. Default `false`.
//!
//! Response shape:
//! ```json
//! {
//!   "status": "running" | "started" | "reclaimed" | "reused",
//!   "port": 8733,
//!   "host_cmd": "claude",
//!   "url": "http://localhost:8733/#nonce=<uuid>",
//!   "token_available": true,
//!   "resumed_session": true,
//!   "session_mismatch": false,
//!   "kill_hint": null,
//!   "killed_predecessor": false
//! }
//! ```
//!
//! Status semantics:
//! - `started` — fresh spawn, no predecessor on the resolved port.
//! - `running` — caller hit an already-running webshell (no `session_id`, or
//!   `session_id` did not match anything we could probe).
//! - `reclaimed` — `kill_existing: true` successfully SIGTERM-ed a prior
//!   listener and a fresh instance was spawned in its place.
//! - `reused` — found a healthy webshell on a probed port whose
//!   `session_id` matches the caller's; minted a fresh nonce on the existing
//!   instance and returned its URL. **No new process was spawned.** This is
//!   the load-bearing path that enforces the "at most one webshell per
//!   session" invariant — every `/webshell` invocation after the first lands
//!   here.
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
const STARTUP_MAX_WAIT: Duration = Duration::from_secs(20);
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
    let kill_existing = params
        .get("kill_existing")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // Resolve (port, already_running):
    //   - explicit port  → use exactly, whatever its state
    //   - no session_id  → default port, whatever its state
    //   - session_id     → scan for free port starting at default, so a
    //                      LaunchAgent on 8733 doesn't block resume
    // Session-aware reuse path. The operator's invariant: at most one webshell
    // per Claude Code session. Before spawning anything, scan the standard port
    // range for an existing webshell already bound to this session_id (via
    // `--resume-session` from a prior `launch_webshell` call). If we find one,
    // skip the spawn entirely: mint a fresh nonce on the existing instance and
    // return its URL. This eliminates the port-walking + accumulation pattern.
    //
    // Only fires when:
    // - `session_id` is supplied (anonymous launches can't be reused),
    // - the gateway resolved a bearer token (otherwise `/api/session/current`
    //   would 401 and we'd skip every candidate).
    let pre_resolved_token = resolve_token();
    if let (Some(sid), Some(tok)) = (session_id.as_deref(), pre_resolved_token.as_deref()) {
        if let Some(existing_port) = find_existing_webshell_for_session(sid, tok).await {
            // Mint a fresh nonce on the existing webshell. The browser tab
            // bound to this session may still be open — reuse means the
            // operator can refresh it with the new URL and pick up where
            // they left off without a duplicate spawn.
            let url = match register_nonce(existing_port, tok).await {
                Some(nonce) => format!("http://localhost:{existing_port}/#nonce={nonce}"),
                None => format!("http://localhost:{existing_port}/"),
            };
            return Ok(text_result(serde_json::to_string_pretty(&json!({
                "status": "reused",
                "port": existing_port,
                "host_cmd": host_cmd,
                "url": url,
                "token_available": true,
                "resumed_session": true,
                "session_mismatch": false,
                "kill_hint": null,
                "dev_mode": false,
                "dev_mode_requested": dev_mode,
                "killed_predecessor": false,
            }))?));
        }
    }

    let (port, mut already_running) = if session_id.is_some() && !explicit_port {
        find_free_port_starting_at(requested_port).await
    } else {
        (requested_port, probe_health(requested_port).await)
    };

    // `kill_existing` reclaim path: caller pinned a port, something is
    // already there, and the caller explicitly wants us to take it back.
    // Only honored when `port` was explicit — auto-scan already finds a
    // free slot when port is left default, so the kill path stays opt-in
    // and never fires by accident.
    let killed_predecessor = if kill_existing && explicit_port && already_running {
        match kill_listener_on(port).await {
            Ok(()) => {
                // Re-probe after kill — listener may have released the socket.
                already_running = probe_health(port).await;
                if already_running {
                    warn!(
                        port,
                        "kill_existing requested but listener still present after SIGTERM \
                         + 3s wait — proceeding with session_mismatch handling"
                    );
                    false
                } else {
                    true
                }
            }
            Err(e) => {
                warn!(port, error = %e, "kill_existing: kill attempt failed");
                false
            }
        }
    } else {
        false
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
        if killed_predecessor {
            "reclaimed"
        } else {
            "started"
        }
    };

    // Reuse the pre-spawn-resolved token if we have it; otherwise resolve
    // now (the reuse-scan path only resolves when session_id is supplied).
    let token = pre_resolved_token.or_else(resolve_token);
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
        "killed_predecessor": killed_predecessor,
    }))?))
}

/// Reclaim a port by SIGTERM-ing whatever is listening on it.
///
/// Uses `lsof -ti:<port> -sTCP:LISTEN` to find the listening PID, then sends
/// SIGTERM and waits up to 3 s for the socket to release. Returns `Ok(())`
/// when the kill was dispatched (even if the process is still draining —
/// caller re-probes after to determine final state).
///
/// Failure modes — all return `Err`:
/// - `lsof` is missing from `$PATH` (extremely unusual on macOS)
/// - `lsof` returned non-zero (no listener — caller should not have called
///   us, but we report it cleanly anyway)
/// - The PID could not be parsed as a positive integer
/// - `kill` failed (process already gone, permission denied, etc.)
///
/// Why `lsof` instead of `nix::sys::socket` enumeration: `lsof` is available
/// on every macOS+Linux dev box without adding a dep, and the cost of a
/// subprocess is negligible compared to the 3 s drain wait we're about to do.
async fn kill_listener_on(port: u16) -> Result<(), GatewayError> {
    let out = tokio::process::Command::new("lsof")
        .args(["-ti", &format!(":{port}"), "-sTCP:LISTEN"])
        .output()
        .await
        .map_err(|e| GatewayError::Internal(format!("lsof spawn failed: {e}")))?;
    if !out.status.success() {
        return Err(GatewayError::Internal(format!(
            "lsof on :{port} returned non-success — no listener?"
        )));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Take only the first PID — there should be exactly one LISTEN socket
    // per port, but defend against multi-line output (e.g., IPv4 + IPv6).
    let Some(pid_str) = stdout
        .lines()
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return Err(GatewayError::Internal(format!(
            "lsof on :{port} returned empty output"
        )));
    };
    let pid: i32 = pid_str
        .parse()
        .map_err(|e| GatewayError::Internal(format!("lsof PID parse failed ({pid_str:?}): {e}")))?;
    let kill_out = tokio::process::Command::new("/bin/kill")
        .args(["-TERM", &pid.to_string()])
        .output()
        .await
        .map_err(|e| GatewayError::Internal(format!("kill spawn failed: {e}")))?;
    if !kill_out.status.success() {
        return Err(GatewayError::Internal(format!(
            "kill -TERM {pid} failed: {}",
            String::from_utf8_lossy(&kill_out.stderr).trim()
        )));
    }
    // Wait up to 3 s for the socket to release. Poll every 100 ms so we
    // return as soon as the predecessor exits — fast path is ~200-400 ms.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        if !probe_health(port).await {
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
    }
    // Timed out — caller re-probes and decides what to do.
    Ok(())
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

/// Scan the standard webshell port range for an existing instance bound to
/// `session_id`. Returns the first matching port, or `None` if no match.
///
/// Uses the gateway's resolved bearer token to call
/// `GET /api/session/current` on every healthy webshell — webshells that
/// don't expose this endpoint (pre-`resume_session_id` binaries) return 404
/// and are skipped. The endpoint is auth-gated, so the bearer token is
/// required to even read the response.
///
/// Concurrency: all candidate ports are probed in parallel via `tokio::join!`
/// fan-out so the worst-case wall-clock cost is one HTTP timeout, not 10.
async fn find_existing_webshell_for_session(session_id: &str, token: &str) -> Option<u16> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .ok()?;

    // Build the candidate port list — the same range `find_free_port_starting_at`
    // walks, so we cover every port a prior `launch_webshell` could have used.
    let candidates: Vec<u16> = (0..PORT_SCAN_WINDOW)
        .filter_map(|offset| DEFAULT_PORT.checked_add(offset))
        .collect();

    // Fan-out each probe; return the first port whose session_id matches.
    let futures = candidates.into_iter().map(|port| {
        let client = client.clone();
        let token = token.to_owned();
        let session_id = session_id.to_owned();
        async move {
            let url = format!("http://127.0.0.1:{port}/api/session/current");
            let resp = client.get(&url).bearer_auth(&token).send().await.ok()?;
            if !resp.status().is_success() {
                return None;
            }
            let body: serde_json::Value = resp.json().await.ok()?;
            let remote_sid = body.get("session_id")?.as_str()?;
            if remote_sid == session_id {
                Some(port)
            } else {
                None
            }
        }
    });
    let results = futures_util::future::join_all(futures).await;
    results.into_iter().flatten().next()
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
                .join(".lightarchitects")
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
    // Pass `--resume-session <id>` so the spawned webshell stores the session
    // identifier in its config and exposes it via `GET /api/session/current`.
    // This is the keystone of the reuse path: subsequent `launch_webshell`
    // calls probe the standard port range and reuse this instance instead of
    // spawning a duplicate. Webshell binaries that don't yet recognize this
    // arg will exit silently — `make deploy` of the matching webshell version
    // is a prerequisite for the reuse path to work end-to-end.
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
