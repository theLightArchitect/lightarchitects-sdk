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
    if ALLOWED_BINARIES.contains(&name) {
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
    while let Ok(Some(line)) = lines.next_line().await {
        let mut reg = registry().lock().await;
        if let Some(e) = reg.entries.get_mut(&handle) {
            if e.buffer.len() < MAX_BUFFER_LINES {
                e.buffer.push(line + "\n");
            }
        }
    }
}

/// Drain a piped stderr into the process registry buffer.
async fn drain_stderr(handle: String, stream: tokio::process::ChildStderr) {
    let mut lines = tokio::io::BufReader::new(stream).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let mut reg = registry().lock().await;
        if let Some(e) = reg.entries.get_mut(&handle) {
            if e.buffer.len() < MAX_BUFFER_LINES {
                e.buffer.push(line + "\n");
            }
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

    let stdout_join = so.map(|s| tokio::spawn(drain_stdout(handle.clone(), s)));
    let stderr_join = se.map(|s| tokio::spawn(drain_stderr(handle.clone(), s)));

    // Race: explicit kill, timeout, or natural exit.
    let killed = tokio::select! {
        _ = kill_rx => true,
        () = tokio::time::sleep(timeout) => true,
        _ = child.wait() => false,
    };

    if killed {
        let _ = child.kill().await;
    }

    // Join drain tasks before marking complete — guarantees all output is buffered.