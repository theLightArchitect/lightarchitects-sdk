//! HTTP handlers for `exec.*` process-execution tools (EEF Wave 2, E2 gate).
//!
//! Provides four REST endpoints:
//!   * `POST /api/exec/run`          — spawn a T-1 validated subprocess
//!   * `GET  /api/exec/output/{h}`   — cursor-based output polling
//!   * `GET  /api/exec/processes`    — list active/completed processes
//!   * `POST /api/exec/kill`         — terminate a process by pid
//!
//! **T-1 (command injection) mitigations**:
//!   * Structured `argv` arrays only — no shell string expansion.
//!   * Binary allowlist — only pre-approved executables may be spawned.
//!   * Metacharacter rejection on every argument.
//!   * Rate limit: 50 spawn requests per 10-second sliding window.
//!
//! Output is buffered in a global `OnceLock<Mutex<ExecRegistry>>` capped at
//! 1 MB per handle (rolling drop of oldest lines).

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{Mutex, oneshot};
use uuid::Uuid;

use crate::real_data::is_authed_pub;
use crate::server::AppState;

// ---------------------------------------------------------------------------
// T-1 allowlist + validation
// ---------------------------------------------------------------------------

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

const SHELL_METACHARACTERS: &[char] = &[
    '&', '|', ';', '`', '$', '(', ')', '{', '}', '<', '>', '\n', '\r',
];

const MAX_BUFFER_BYTES: usize = 1024 * 1024; // 1 MB per handle
const RATE_LIMIT_MAX: usize = 50;
const RATE_LIMIT_WINDOW_SECS: u64 = 10;

fn validate_binary(argv0: &str) -> Result<(), &'static str> {
    let base = std::path::Path::new(argv0)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(argv0);
    if ALLOWED_BINARIES.contains(&base) {
        Ok(())
    } else {
        Err("binary not in exec allowlist")
    }
}

fn validate_arg(arg: &str) -> Result<(), &'static str> {
    if arg.chars().any(|c| SHELL_METACHARACTERS.contains(&c)) {
        Err("argument contains shell metacharacter")
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rate limiter
// ---------------------------------------------------------------------------

struct SlidingWindowLimiter {
    timestamps: Vec<Instant>,
}

impl SlidingWindowLimiter {
    fn new() -> Self {
        Self {
            timestamps: Vec::new(),
        }
    }

    fn check_and_record(&mut self) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(RATE_LIMIT_WINDOW_SECS);
        self.timestamps.retain(|t| now.duration_since(*t) < window);
        if self.timestamps.len() >= RATE_LIMIT_MAX {
            return false;
        }
        self.timestamps.push(now);
        true
    }
}

static RATE_LIMITER: OnceLock<Mutex<SlidingWindowLimiter>> = OnceLock::new();

fn rate_limiter() -> &'static Mutex<SlidingWindowLimiter> {
    RATE_LIMITER.get_or_init(|| Mutex::new(SlidingWindowLimiter::new()))
}

// ---------------------------------------------------------------------------
// Process registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ExecStatus {
    Running,
    Complete,
    Killed,
}

/// A single buffered output line from a running or completed process.
#[derive(Debug, Clone, Serialize)]
pub struct OutputLine {
    /// Monotonically increasing line sequence number within the handle.
    pub seq: u64,
    /// `"stdout"` or `"stderr"`.
    pub stream: String,
    /// Raw line content (may contain ANSI codes).
    pub line: String,
}

struct ExecEntry {
    pid: u32,
    command_display: String,
    started_at: chrono::DateTime<Utc>,
    status: ExecStatus,
    buffer: Vec<OutputLine>,
    buffer_bytes: usize,
    next_seq: u64,
    exit_code: Option<i32>,
    kill_tx: Option<oneshot::Sender<()>>,
}

struct ExecRegistry {
    entries: HashMap<String, ExecEntry>,
}

impl ExecRegistry {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

static REGISTRY: OnceLock<Mutex<ExecRegistry>> = OnceLock::new();

fn registry() -> &'static Mutex<ExecRegistry> {
    REGISTRY.get_or_init(|| Mutex::new(ExecRegistry::new()))
}

async fn append_line(handle: &str, stream: &str, line: String) {
    let mut reg = registry().lock().await;
    let Some(entry) = reg.entries.get_mut(handle) else {
        return;
    };
    let seq = entry.next_seq;
    entry.next_seq += 1;
    let bytes = line.len() + stream.len() + 16;
    // Rolling 1 MB cap — drop oldest lines when full.
    while entry.buffer_bytes + bytes > MAX_BUFFER_BYTES && !entry.buffer.is_empty() {
        let removed = entry.buffer.remove(0);
        entry.buffer_bytes = entry
            .buffer_bytes
            .saturating_sub(removed.line.len() + removed.stream.len() + 16);
    }
    entry.buffer_bytes += bytes;
    entry.buffer.push(OutputLine {
        seq,
        stream: stream.to_owned(),
        line,
    });
}

async fn drain_stdout(handle: String, stream: tokio::process::ChildStdout) {
    let mut lines = BufReader::new(stream).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        append_line(&handle, "stdout", line).await;
    }
}

async fn drain_stderr(handle: String, stream: tokio::process::ChildStderr) {
    let mut lines = BufReader::new(stream).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        append_line(&handle, "stderr", line).await;
    }
}

async fn drain_and_wait(
    handle: String,
    mut child: tokio::process::Child,
    timeout: Duration,
    kill_rx: oneshot::Receiver<()>,
) {
    let so = child.stdout.take();
    let se = child.stderr.take();
    let stdout_task = so.map(|s| tokio::spawn(drain_stdout(handle.clone(), s)));
    let stderr_task = se.map(|s| tokio::spawn(drain_stderr(handle.clone(), s)));

    let killed = tokio::select! {
        _ = kill_rx => true,
        () = tokio::time::sleep(timeout) => true,
        _ = child.wait() => false,
    };

    if killed {
        let _ = child.kill().await;
    }
    if let Some(task) = stdout_task {
        let _ = task.await;
    }
    if let Some(task) = stderr_task {
        let _ = task.await;
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

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Request body for `POST /api/exec/run`.
#[derive(Deserialize)]
pub struct RunRequest {
    /// Structured argv — `argv[0]` must be in the binary allowlist.
    argv: Vec<String>,
    /// Working directory for the spawned process.
    cwd: Option<String>,
    /// Optional extra environment variables merged into the process env.
    env: Option<HashMap<String, String>>,
    /// Process timeout in milliseconds (default 5 minutes).
    timeout_ms: Option<u64>,
}

/// Request body for `POST /api/exec/kill`.
#[derive(Deserialize)]
pub struct KillRequest {
    /// OS process ID returned by a previous `POST /api/exec/run`.
    pid: u32,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /api/exec/run` — validate argv, spawn subprocess, return handle.
pub async fn run_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<RunRequest>,
) -> impl IntoResponse {
    if !is_authed_pub(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if body.argv.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "argv is empty"})),
        )
            .into_response();
    }
    // T-1: validate binary + all arguments.
    if let Err(e) = validate_binary(&body.argv[0]) {
        return (StatusCode::FORBIDDEN, Json(json!({"error": e}))).into_response();
    }
    for arg in body.argv.iter().skip(1) {
        if let Err(e) = validate_arg(arg) {
            return (StatusCode::FORBIDDEN, Json(json!({"error": e}))).into_response();
        }
    }
    // Rate limit.
    if !rate_limiter().lock().await.check_and_record() {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "rate limit exceeded"})),
        )
            .into_response();
    }

    let working_dir = body.cwd.as_deref().unwrap_or(".").to_owned();
    let timeout = Duration::from_millis(body.timeout_ms.unwrap_or(300_000));
    let handle = Uuid::new_v4().to_string();
    let command_display = body.argv.join(" ");

    let mut process_cmd = tokio::process::Command::new(&body.argv[0]);
    process_cmd
        .args(&body.argv[1..])
        .current_dir(&working_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    if let Some(env) = &body.env {
        process_cmd.envs(env);
    }

    let child = match process_cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "spawn_failed", "detail": e.to_string()})),
            )
                .into_response();
        }
    };

    let pid = child.id().unwrap_or(0);
    let (kill_tx, kill_rx) = oneshot::channel::<()>();

    {
        let mut reg = registry().lock().await;
        reg.entries.insert(
            handle.clone(),
            ExecEntry {
                pid,
                command_display: command_display.clone(),
                started_at: Utc::now(),
                status: ExecStatus::Running,
                buffer: Vec::new(),
                buffer_bytes: 0,
                next_seq: 0,
                exit_code: None,
                kill_tx: Some(kill_tx),
            },
        );
    }

    tokio::spawn(drain_and_wait(handle.clone(), child, timeout, kill_rx));

    (
        StatusCode::OK,
        Json(json!({"handle": handle, "pid": pid, "command": command_display})),
    )
        .into_response()
}

/// `GET /api/exec/output/{handle}?cursor=N` — return lines from seq `cursor` onwards.
#[allow(clippy::implicit_hasher)] // axum Query extractor requires HashMap, not generic over BuildHasher
pub async fn output_handler(
    Path(handle): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if !is_authed_pub(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let cursor: u64 = params
        .get("cursor")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let reg = registry().lock().await;
    let Some(entry) = reg.entries.get(&handle) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "handle_not_found"})),
        )
            .into_response();
    };

    let chunks: Vec<Value> = entry
        .buffer
        .iter()
        .filter(|l| l.seq >= cursor)
        .map(|l| json!({"seq": l.seq, "stream": l.stream, "line": l.line}))
        .collect();

    let next_cursor = chunks
        .last()
        .map_or(cursor, |c| c["seq"].as_u64().unwrap_or(cursor) + 1);
    let complete = entry.status != ExecStatus::Running;

    (
        StatusCode::OK,
        Json(json!({
            "chunks": chunks,
            "next_cursor": next_cursor,
            "complete": complete,
            "exit_code": entry.exit_code,
            "killed": entry.status == ExecStatus::Killed,
        })),
    )
        .into_response()
}

/// `GET /api/exec/processes` — list all tracked processes.
pub async fn processes_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authed_pub(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let reg = registry().lock().await;
    let processes: Vec<Value> = reg
        .entries
        .iter()
        .map(|(h, e)| {
            json!({
                "handle": h,
                "pid": e.pid,
                "command": e.command_display,
                "started_at": e.started_at.to_rfc3339(),
                "status": e.status,
                "exit_code": e.exit_code,
            })
        })
        .collect();
    (StatusCode::OK, Json(json!({"processes": processes}))).into_response()
}

/// `POST /api/exec/kill` — send kill signal to a process by pid.
pub async fn kill_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<KillRequest>,
) -> impl IntoResponse {
    if !is_authed_pub(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let kill_tx = {
        let mut reg = registry().lock().await;
        reg.entries
            .values_mut()
            .find(|e| e.pid == body.pid)
            .and_then(|e| e.kill_tx.take())
    };
    match kill_tx {
        Some(tx) => {
            let _ = tx.send(());
            (
                StatusCode::OK,
                Json(json!({"killed": true, "pid": body.pid})),
            )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "no active process with that pid"})),
        )
            .into_response(),
    }
}
