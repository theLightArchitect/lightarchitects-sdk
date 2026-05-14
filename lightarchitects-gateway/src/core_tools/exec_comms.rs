//! `exec.*` tool suite — process execution with cursor-paginated streaming output.
//!
//! # Security (T-1 — EEF Wave 2)
//!
//! All tools enforce T-1 command injection mitigation:
//! - Structured argv arrays only — `Command::new(&argv[0]).args(&argv[1..])`, never a shell string
//! - `argv[0]` restricted to an approved binary allowlist
//! - Per-argument metacharacter rejection: `;`, `|`, `&`, `$`, `` ` ``, `(`, `)`, `\n`, `\r`
//! - Rate limiting: 50 requests per 10-second sliding window
//!
//! # Tools
//!
//! | Name | Params | Returns |
//! |------|--------|---------|
//! | `exec.run_command` | `{argv, cwd, env?, timeout_ms?}` | `{pid, stream_handle}` |
//! | `exec.list_processes` | `{}` | `{processes}` |
//! | `exec.kill_process` | `{pid}` | `{killed, pid}` |
//! | `exec.get_output` | `{stream_handle, cursor}` | `{chunks, next_cursor, complete}` |

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use chrono::Utc;
use serde_json::{Value, json};
use tokio::io::AsyncBufReadExt;
use tokio::sync::Mutex;

use crate::config::expand_tilde;
use crate::error::GatewayError;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum concurrently tracked processes.
const MAX_REGISTRY_SIZE: usize = 100;
/// Per-process output buffer cap (lines). Older lines are kept; new lines are dropped.
const MAX_BUFFER_LINES: usize = 10_000;
/// Default kill timeout when none supplied (5 minutes).
const DEFAULT_TIMEOUT_MS: u64 = 300_000;
/// Rate limit: maximum requests per window.
const RATE_LIMIT_MAX: usize = 50;
/// Rate limit: sliding window duration.
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(10);
/// Lines returned per `exec.get_output` call.
const PAGE_SIZE: usize = 200;

/// Permitted binary names for `exec.run_command`.
///
/// Defense-in-depth beyond metacharacter rejection — only known toolchain
/// binaries may be invoked through the exec.* MCP surface.
const ALLOWED_BINARIES: &[&str] = &[
    "cargo",
    "cargo-nextest",
    "pnpm",
    "npx",
    "vitest",
    "playwright",
    "node",
    "rustfmt",
    "clippy-driver",
];

// ── Global singletons ─────────────────────────────────────────────────────────

static REGISTRY: OnceLock<Mutex<ExecRegistry>> = OnceLock::new();
static RATE_LIMITER: OnceLock<Mutex<SlidingWindowLimiter>> = OnceLock::new();

fn registry() -> &'static Mutex<ExecRegistry> {
    REGISTRY.get_or_init(|| Mutex::new(ExecRegistry::default()))
}

fn rate_limiter() -> &'static Mutex<SlidingWindowLimiter> {
    RATE_LIMITER
        .get_or_init(|| Mutex::new(SlidingWindowLimiter::new(RATE_LIMIT_MAX, RATE_LIMIT_WINDOW)))
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct ExecRegistry {
    entries: HashMap<String, ExecEntry>,
}

#[derive(Debug)]
struct ExecEntry {
    pid: u32,
    command_display: String,
    started_at: chrono::DateTime<Utc>,
    status: ExecStatus,
    buffer: Vec<String>,
    exit_code: Option<i32>,
    /// Oneshot sender — dropping it signals the drain task to kill the process.
    kill_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExecStatus {
    Running,
    Complete,
    Killed,
}

impl ExecStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Complete => "complete",
            Self::Killed => "killed",
        }
    }
}

/// Sliding-window rate limiter (not fair — drops excess, not queues them).
struct SlidingWindowLimiter {
    max_requests: usize,
    window: Duration,
    timestamps: Vec<Instant>,
}

impl SlidingWindowLimiter {
    fn new(max_requests: usize, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            timestamps: Vec::new(),
        }
    }

    /// Returns `true` and records the instant if within the rate limit.
    fn try_acquire(&mut self) -> bool {
        let now = Instant::now();
        self.timestamps
            .retain(|t| now.duration_since(*t) < self.window);
        if self.timestamps.len() >= self.max_requests {
            return false;
        }
        self.timestamps.push(now);
        true
    }
}

// ── T-1 validation helpers ────────────────────────────────────────────────────

fn validate_binary(argv0: &str) -> Result<(), GatewayError> {
    let name = std::path::Path::new(argv0)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if ALLOWED_BINARIES.iter().any(|&b| b == name) {
        return Ok(());
    }
    Err(GatewayError::Subprocess(format!(
        "T-1: binary '{name}' not in exec allowlist; permitted: {}",
        ALLOWED_BINARIES.join(", ")
    )))
}

fn validate_arg(entry: &str) -> Result<(), GatewayError> {
    const METACHARACTERS: &[char] = &[';', '|', '&', '$', '`', '(', ')', '\n', '\r'];
    if let Some(bad) = METACHARACTERS.iter().find(|&&c| entry.contains(c)) {
        return Err(GatewayError::Subprocess(format!(
            "T-1: argv entry contains disallowed metacharacter '{bad}'"
        )));
    }
    Ok(())
}

fn validate_argv(argv: &[String]) -> Result<(), GatewayError> {
    if argv.is_empty() {
        return Err(GatewayError::MissingParam("argv must be non-empty"));
    }
    validate_binary(&argv[0])?;
    for arg in argv {
        validate_arg(arg)?;
    }
    Ok(())
}

fn parse_argv(params: &Value) -> Result<Vec<String>, GatewayError> {
    params["argv"]
        .as_array()
        .ok_or(GatewayError::MissingParam("argv"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or(GatewayError::MissingParam("argv entries must be strings"))
        })
        .collect()
}

fn parse_extra_env(params: &Value) -> HashMap<String, String> {
    params["env"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

// ── Background drain helpers ──────────────────────────────────────────────────

/// Drain a piped stdout into the process registry buffer.
async fn drain_stdout(handle: String, stream: tokio::process::ChildStdout) {
    let mut lines = tokio::io::BufReader::new(stream).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let mut reg = registry().lock().await;
                if let Some(e) = reg.entries.get_mut(&handle) {
                    if e.buffer.len() < MAX_BUFFER_LINES {
                        e.buffer.push(line + "\n");
                    }
                }
            }
            _ => break,
        }
    }
}

/// Drain a piped stderr into the process registry buffer.
async fn drain_stderr(handle: String, stream: tokio::process::ChildStderr) {
    let mut lines = tokio::io::BufReader::new(stream).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let mut reg = registry().lock().await;
                if let Some(e) = reg.entries.get_mut(&handle) {
                    if e.buffer.len() < MAX_BUFFER_LINES {
                        e.buffer.push(line + "\n");
                    }
                }
            }
            _ => break,
        }
    }
}

/// Drive a spawned child to completion, recording output and exit status.
///
/// Races kill signal, timeout, and natural process exit. Joins drain tasks before
/// recording `ExecStatus::Complete` so all buffered output is available.
async fn drain_and_wait(
    handle: String,
    mut child: tokio::process::Child,
    timeout: Duration,
    kill_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let so = child.stdout.take();
    let se = child.stderr.take();

    let h_so = so.map(|s| tokio::spawn(drain_stdout(handle.clone(), s)));
    let h_se = se.map(|s| tokio::spawn(drain_stderr(handle.clone(), s)));

    // Race: explicit kill, timeout, or natural exit.
    let killed = tokio::select! {
        _ = kill_rx => true,
        _ = tokio::time::sleep(timeout) => true,
        _ = child.wait() => false,
    };

    if killed {
        let _ = child.kill().await;
    }

    // Join drain tasks before marking complete — guarantees all output is buffered.
    if let Some(t) = h_so {
        let _ = t.await;
    }
    if let Some(t) = h_se {
        let _ = t.await;
    }

    let exit_code = child.wait().await.ok().and_then(|s| s.code());

    let mut reg = registry().lock().await;
    if let Some(e) = reg.entries.get_mut(&handle) {
        e.status = if killed {
            ExecStatus::Killed
        } else {
            ExecStatus::Complete
        };
        e.exit_code = exit_code;
        e.kill_tx = None;
    }
}

// ── exec.run_command ──────────────────────────────────────────────────────────

/// Spawn a process with structured argv and stream output into the registry.
///
/// Returns `{pid, stream_handle}`. Poll output with `exec.get_output`.
///
/// # Errors
///
/// Returns [`GatewayError`] on invalid params, T-1 rejection, rate limit,
/// registry capacity, or spawn failure.
pub async fn run_run_command(params: Value) -> Result<Value, GatewayError> {
    {
        let mut rl = rate_limiter().lock().await;
        if !rl.try_acquire() {
            return Err(GatewayError::Subprocess(
                "exec rate limit exceeded (50 req / 10s)".to_owned(),
            ));
        }
    }

    let argv = parse_argv(&params)?;
    validate_argv(&argv)?;

    let cwd_str = params["cwd"]
        .as_str()
        .ok_or(GatewayError::MissingParam("cwd"))?;
    let cwd = expand_tilde(cwd_str);
    if !cwd.is_dir() {
        return Err(GatewayError::File(format!(
            "cwd does not exist or is not a directory: {}",
            cwd.display()
        )));
    }

    let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(DEFAULT_TIMEOUT_MS);
    let timeout = Duration::from_millis(timeout_ms);
    let extra_env = parse_extra_env(&params);

    let handle = uuid::Uuid::new_v4().to_string();
    let command_display = argv.join(" ");
    let started_at = Utc::now();

    let mut cmd = tokio::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    cmd.current_dir(&cwd);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    for (k, v) in &extra_env {
        cmd.env(k, v);
    }

    let child = cmd
        .spawn()
        .map_err(|e| GatewayError::Subprocess(format!("spawn failed: {e}")))?;
    let pid = child.id().unwrap_or(0);

    let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();

    {
        let mut reg = registry().lock().await;
        if reg.entries.len() >= MAX_REGISTRY_SIZE {
            return Err(GatewayError::Subprocess(format!(
                "exec registry full ({MAX_REGISTRY_SIZE}); kill old processes first"
            )));
        }
        reg.entries.insert(
            handle.clone(),
            ExecEntry {
                pid,
                command_display,
                started_at,
                status: ExecStatus::Running,
                buffer: Vec::new(),
                exit_code: None,
                kill_tx: Some(kill_tx),
            },
        );
    }

    let task_handle = handle.clone();
    tokio::spawn(async move {
        drain_and_wait(task_handle, child, timeout, kill_rx).await;
    });

    Ok(json!({ "pid": pid, "stream_handle": handle }))
}

// ── exec.list_processes ───────────────────────────────────────────────────────

/// Return all tracked processes with status, command, and buffer metrics.
///
/// # Errors
///
/// Never fails; `Result` signature matches the dispatch interface.
pub async fn run_list_processes(_params: Value) -> Result<Value, GatewayError> {
    let reg = registry().lock().await;
    let processes: Vec<Value> = reg
        .entries
        .iter()
        .map(|(handle, e)| {
            json!({
                "stream_handle": handle,
                "pid": e.pid,
                "command": e.command_display,
                "started_at": e.started_at.to_rfc3339(),
                "status": e.status.as_str(),
                "output_lines": e.buffer.len(),
                "exit_code": e.exit_code,
            })
        })
        .collect();
    Ok(json!({ "processes": processes }))
}

// ── exec.kill_process ─────────────────────────────────────────────────────────

/// Signal a tracked process to terminate by sending the kill signal via its channel.
///
/// # Errors
///
/// Returns [`GatewayError`] if the process is not tracked or already complete.
pub async fn run_kill_process(params: Value) -> Result<Value, GatewayError> {
    let pid = params["pid"]
        .as_u64()
        .ok_or(GatewayError::MissingParam("pid"))? as u32;

    let kill_tx = {
        let mut reg = registry().lock().await;
        reg.entries
            .values_mut()
            .find(|e| e.pid == pid)
            .and_then(|e| e.kill_tx.take())
    };

    match kill_tx {
        Some(tx) => {
            let _ = tx.send(());
            Ok(json!({ "killed": true, "pid": pid }))
        }
        None => Err(GatewayError::Subprocess(format!(
            "no active process with pid={pid} (may have already completed or been killed)"
        ))),
    }
}

// ── exec.get_output ───────────────────────────────────────────────────────────

/// Retrieve buffered output lines since `cursor`.
///
/// Returns up to `PAGE_SIZE` lines. `next_cursor` advances past the returned
/// chunk. When `complete=true` and `next_cursor == total_lines`, all output
/// has been consumed.
///
/// # Errors
///
/// Returns [`GatewayError`] if the handle is not found in the registry.
pub async fn run_get_output(params: Value) -> Result<Value, GatewayError> {
    let handle = params["stream_handle"]
        .as_str()
        .ok_or(GatewayError::MissingParam("stream_handle"))?;
    let cursor = params["cursor"].as_u64().unwrap_or(0) as usize;

    let reg = registry().lock().await;
    let e = reg.entries.get(handle).ok_or_else(|| {
        GatewayError::Subprocess(format!("stream_handle '{handle}' not found in registry"))
    })?;

    let total = e.buffer.len();
    let start = cursor.min(total);
    let end = (start + PAGE_SIZE).min(total);
    let chunks: Vec<&str> = e.buffer[start..end].iter().map(String::as_str).collect();
    let complete = matches!(e.status, ExecStatus::Complete | ExecStatus::Killed) && end == total;

    Ok(json!({
        "chunks": chunks,
        "next_cursor": end,
        "complete": complete,
        "status": e.status.as_str(),
        "exit_code": e.exit_code,
        "total_lines": total,
    }))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validate_binary_permits_allowlist() {
        assert!(validate_binary("cargo").is_ok());
        assert!(validate_binary("/usr/local/bin/cargo").is_ok());
        assert!(validate_binary("pnpm").is_ok());
    }

    #[test]
    fn validate_binary_blocks_shells() {
        assert!(validate_binary("sh").is_err());
        assert!(validate_binary("bash").is_err());
        assert!(validate_binary("/bin/sh").is_err());
    }

    #[test]
    fn validate_arg_rejects_metacharacters() {
        assert!(validate_arg("test; rm -rf /").is_err());
        assert!(validate_arg("arg|pipe").is_err());
        assert!(validate_arg("$HOME").is_err());
        assert!(validate_arg("`id`").is_err());
        assert!(validate_arg("echo\nhello").is_err());
    }

    #[test]
    fn validate_arg_permits_safe_flags() {
        assert!(validate_arg("--format=json").is_ok());
        assert!(validate_arg("--message-format").is_ok());
        assert!(validate_arg("libtest-json").is_ok());
        assert!(validate_arg("--all-features").is_ok());
        assert!(validate_arg("test_module::my_test").is_ok());
    }

    #[test]
    fn validate_argv_empty_is_rejected() {
        assert!(validate_argv(&[]).is_err());
    }

    #[tokio::test]
    async fn rate_limiter_blocks_at_limit() {
        let mut rl = SlidingWindowLimiter::new(3, Duration::from_secs(60));
        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
        assert!(!rl.try_acquire()); // 4th is denied
    }

    #[tokio::test]
    async fn get_output_unknown_handle_errors() {
        let result = run_get_output(json!({
            "stream_handle": "no-such-handle-abc123",
            "cursor": 0
        }))
        .await;
        assert!(result.is_err());
    }
}
