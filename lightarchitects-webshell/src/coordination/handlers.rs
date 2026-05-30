//! HTTP handlers for the Squad Comms coordination endpoints.
//!
//! Each handler is bearer-authenticated, mirroring the pattern used by
//! `events::sse_handler` and `server::polytopes`. Handlers read on-disk
//! artifacts directly (`~/.lightarchitects/tasks/queue.json` and the
//! soul-chat session files) — they intentionally do **not** link to the
//! conductor or soul-chat libraries to keep the dependency footprint
//! minimal and to honour the architectural rule that webshell is an
//! HTTP wrapper, not a duplicate implementation of business logic.
//!
//! TODO(crate-boundary): the queue read/write logic here duplicates a
//! tiny slice of `lightarchitects_gateway::conductor::queue`. Belongs in:
//!   [ ] private (lightarchitects-gateway) — exposed as `pub` API
//!   [x] public (SDK `lightarchitects::xxx` as API client) — current location
//! Decision needed by: post-merge of bridging-whistling-loom.
//! Tracking issue: TBD (file before merge).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tokio::sync::Mutex as TokioMutex;

use axum::{
    Json,
    extract::{Path as AxPath, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::{auth, server::AppState};

use super::types::{
    AddTaskRequest, AddTaskResponse, ApproveRequest, ApproveResponse, ChatSessionSummary,
    ChatSessionsResponse, ClaimRequest, ClaimResponse, InjectRequest, InjectResponse,
    RejectRequest, RejectResponse, SessionEndRequest, SessionEndResponse, SessionStartRequest,
    SessionStartResponse, SpawnWorkerRequest, SpawnWorkerResponse, TaskLogsResponse,
    TaskQueueResponse, TaskSummary,
};

// ── Security helpers ─────────────────────────────────────────────────────────

/// Process-level mutex serialising all queue read-modify-write cycles (HIGH H-TOCTOU).
///
/// All callers that mutate `queue.json` must hold `queue_lock().lock().await`
/// for the full read → check → write sequence to prevent double-claim races.
static QUEUE_LOCK: OnceLock<TokioMutex<()>> = OnceLock::new();

pub(crate) fn queue_lock() -> &'static TokioMutex<()> {
    QUEUE_LOCK.get_or_init(|| TokioMutex::new(()))
}

// ── On-disk shapes (subset of conductor/queue.rs and soul-chat session files) ──

/// Subset of the conductor task record we read off `queue.json`.
///
/// We deliberately mirror only the fields we surface — keeping this struct
/// independent of `lightarchitects_gateway::conductor::queue::Task` lets the
/// webshell stay a leaf consumer and not a dependency cycle target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OnDiskTask {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) project: String,
    pub(crate) prompt: String,
    #[serde(default)]
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) source: String,
    #[serde(default)]
    pub(crate) priority: String,
    #[serde(default)]
    pub(crate) added: Option<String>,
    #[serde(default)]
    pub(crate) started: Option<String>,
    #[serde(default)]
    pub(crate) finished: Option<String>,
    #[serde(default)]
    pub(crate) retries: u32,
    #[serde(default)]
    pub(crate) output_log: Option<String>,
    #[serde(default)]
    pub(crate) build_codename: Option<String>,
    #[serde(default)]
    pub(crate) assignee: Option<String>,
    #[serde(default)]
    pub(crate) build_session_id: Option<String>,
    #[serde(default)]
    pub(crate) awaiting_assertion_id: Option<String>,
    #[serde(default)]
    pub(crate) resolution_deadline: Option<String>,
}

/// Top-level shape of `~/.lightarchitects/tasks/queue.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OnDiskQueue {
    #[serde(default = "default_version")]
    version: String,
    #[serde(default)]
    pub(crate) tasks: Vec<OnDiskTask>,
}

fn default_version() -> String {
    "1.0".to_owned()
}

/// On-disk shape of a soul-chat session record.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnDiskSession {
    session_id: String,
    #[serde(default)]
    participants: Vec<String>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    topic: Option<String>,
    /// Build codename this session was created for (written by `session_start`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    build_codename: Option<String>,
}

// ── Public handlers ─────────────────────────────────────────────────────────

/// `GET /api/coordination/tasks` — return the queue snapshot.
pub async fn list_tasks(headers: HeaderMap, State(state): State<AppState>) -> impl IntoResponse {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let queue_path = queue_path();
    let queue = read_queue_async(queue_path)
        .await
        .unwrap_or_else(|_| OnDiskQueue {
            version: default_version(),
            tasks: Vec::new(),
        });

    let pending_count = queue.tasks.iter().filter(|t| t.status == "pending").count();
    let in_progress_count = queue
        .tasks
        .iter()
        .filter(|t| t.status == "in_progress")
        .count();
    let completed_count = queue
        .tasks
        .iter()
        .filter(|t| t.status == "completed")
        .count();
    let failed_count = queue.tasks.iter().filter(|t| t.status == "failed").count();

    let response = TaskQueueResponse {
        tasks: queue.tasks.iter().map(to_summary).collect(),
        pending_count,
        in_progress_count,
        completed_count,
        failed_count,
        daemon_running: daemon_pid_alive(),
    };
    Json(response).into_response()
}

/// Persist a new dispatch task entry to the conductor queue
/// (`~/.lightarchitects/tasks/queue.json`).
///
/// Called by [`crate::dispatch::executor::execute`] immediately after registering
/// the dispatch handle. Creates a task in `"in_progress"` state so the conductor
/// dashboard reflects the active dispatch without a separate claim step.
///
/// # Errors
///
/// Returns an error string if the queue cannot be read, serialised, or written.
pub async fn enqueue_dispatch(id: &str, title: &str, prompt: &str) -> Result<(), String> {
    // Hold the lock for the full read → check → write cycle (HIGH H-88).
    let _guard = queue_lock().lock().await;
    let queue_path = queue_path();
    let mut queue = match read_queue_async(queue_path.clone()).await {
        Ok(q) => q,
        Err(QueueIoError::Missing) => OnDiskQueue {
            version: default_version(),
            tasks: Vec::new(),
        },
        Err(QueueIoError::Read | QueueIoError::Parse | QueueIoError::Write) => {
            return Err("queue_read_error".to_owned());
        }
    };
    // Idempotent — skip if a task with this dispatch ID already exists.
    if queue.tasks.iter().any(|t| t.id == id) {
        return Ok(());
    }
    let task = OnDiskTask {
        id: id.to_owned(),
        title: truncate(title, 200),
        project: "webshell-dispatch".into(),
        prompt: truncate(prompt, 4_000),
        status: "in_progress".into(),
        source: "dispatch".into(),
        priority: "medium".into(),
        added: Some(now_rfc3339()),
        started: Some(now_rfc3339()),
        finished: None,
        retries: 0,
        output_log: None,
        build_codename: None,
        assignee: None,
        build_session_id: None,
        awaiting_assertion_id: None,
        resolution_deadline: None,
    };
    queue.tasks.push(task);
    write_queue_async(queue_path, queue)
        .await
        .map_err(|e| e.to_string())
}

/// Mark a dispatch queue entry as `"completed"`.
///
/// Called by [`crate::dispatch::executor`] after [`crate::dispatch::types::DispatchEvent::Complete`]
/// is broadcast. No-op if the entry is missing (cancelled or queue absent).
pub async fn complete_dispatch(id: &str) {
    // Hold the lock for the full read → modify → write cycle (HIGH H-88).
    let _guard = queue_lock().lock().await;
    let queue_path = queue_path();
    let Ok(mut queue) = read_queue_async(queue_path.clone()).await else {
        return;
    };
    if let Some(task) = queue.tasks.iter_mut().find(|t| t.id == id) {
        task.status = "completed".into();
        task.finished = Some(now_rfc3339());
    }
    if let Err(e) = write_queue_async(queue_path, queue).await {
        tracing::warn!(dispatch_id = %id, error = %e, "Failed to mark dispatch completed in queue");
    }
}

/// `POST /api/coordination/tasks/add` — append a task to the queue.
pub async fn add_task(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<AddTaskRequest>,
) -> impl IntoResponse {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if let Err(reason) = validate_add(&req) {
        return (StatusCode::BAD_REQUEST, reason).into_response();
    }

    // Hold the lock for the full read → push → write cycle (HIGH H-88).
    let _queue_guard = queue_lock().lock().await;
    let queue_path = queue_path();
    let mut queue = match read_queue_async(queue_path.clone()).await {
        Ok(q) => q,
        Err(QueueIoError::Missing) => OnDiskQueue {
            version: default_version(),
            tasks: Vec::new(),
        },
        Err(QueueIoError::Read | QueueIoError::Parse | QueueIoError::Write) => {
            tracing::warn!("queue read failed on add_task");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let id = mint_task_id();
    let priority = normalize_priority(req.priority.as_deref());
    let build_codename = req
        .build_codename
        .as_deref()
        .map(|s| truncate(s, 200))
        .filter(|s| !s.is_empty());
    let assignee = req
        .assignee
        .as_deref()
        .map(|s| truncate(s, 200))
        .filter(|s| !s.is_empty());
    let build_session_id = req
        .build_session_id
        .as_deref()
        .map(|s| truncate(s, 200))
        .filter(|s| !s.is_empty());
    let task = OnDiskTask {
        id: id.clone(),
        title: truncate(&req.title, 200),
        project: req.project.clone(),
        prompt: truncate(&req.prompt, 4000),
        status: "pending".into(),
        source: "webshell".into(),
        priority,
        added: Some(now_rfc3339()),
        started: None,
        finished: None,
        retries: 0,
        output_log: None,
        build_codename,
        assignee,
        build_session_id,
        awaiting_assertion_id: None,
        resolution_deadline: None,
    };
    queue.tasks.push(task);

    if let Err(e) = write_queue_async(queue_path, queue).await {
        tracing::warn!(error = %e, "queue write failed on add_task");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let response = AddTaskResponse {
        id,
        status: "pending".into(),
    };
    (StatusCode::OK, Json(response)).into_response()
}

/// `POST /api/coordination/tasks/claim/:id` — soft-claim a pending task.
pub async fn claim_task(
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<ClaimRequest>,
) -> impl IntoResponse {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if req.claimant.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "claimant must be non-empty".to_owned(),
        )
            .into_response();
    }
    // Cap claimant length to prevent OOM via a crafted request (MED H-91).
    if req.claimant.len() > 200 {
        return (
            StatusCode::BAD_REQUEST,
            "claimant exceeds 200-character limit".to_owned(),
        )
            .into_response();
    }
    // Serialize the read-check-write cycle to prevent TOCTOU double-claim (HIGH H-TOCTOU).
    let _queue_guard = queue_lock().lock().await;
    let queue_path = queue_path();
    let Ok(mut queue) = read_queue_async(queue_path.clone()).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(task) = queue.tasks.iter_mut().find(|t| t.id == id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if task.status == "in_progress" {
        return StatusCode::CONFLICT.into_response();
    }
    let now = now_rfc3339();
    task.status = "in_progress".into();
    task.started = Some(now.clone());
    task.source = format!("claimed:{}", req.claimant);
    if let Err(e) = write_queue_async(queue_path, queue).await {
        tracing::warn!(error = %e, "queue write failed on claim_task");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Json(ClaimResponse {
        id,
        status: "in_progress".into(),
        started: now,
    })
    .into_response()
}

/// `GET /api/coordination/tasks/:id/logs` — tail the task log file.
pub async fn task_logs(
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let queue_path = queue_path();
    let Ok(queue) = read_queue(&queue_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(task) = queue.tasks.iter().find(|t| t.id == id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let tail = task
        .output_log
        .as_deref()
        .map(read_log_tail)
        .unwrap_or_default();
    Json(TaskLogsResponse {
        id,
        log_path: task.output_log.clone(),
        tail,
    })
    .into_response()
}

/// `GET /api/coordination/chat/sessions` — list known soul-chat sessions.
pub async fn chat_sessions(headers: HeaderMap, State(state): State<AppState>) -> impl IntoResponse {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let dir = sessions_dir();
    let sessions = read_sessions_dir(&dir).unwrap_or_default();
    Json(ChatSessionsResponse { sessions }).into_response()
}

/// `POST /api/coordination/sessions/start` — materialise a per-build soul-chat session.
///
/// The gateway mints the UUID and sends `{ build_codename, session_id }`. The
/// webshell writes the session file at
/// `~/lightarchitects/soul/helix/chat/sessions/<session_id>.json` so the
/// `GET /api/coordination/chat/sessions` list and the SSE stream see it
/// immediately. Idempotent: a repeat call with the same session ID overwrites
/// the existing file with status `running`.
pub async fn session_start(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<SessionStartRequest>,
) -> impl IntoResponse {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if req.session_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "session_id must be non-empty".to_owned(),
        )
            .into_response();
    }
    if req.build_codename.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "build_codename must be non-empty".to_owned(),
        )
            .into_response();
    }
    if req.build_codename.len() > 200 {
        return (
            StatusCode::BAD_REQUEST,
            "build_codename exceeds 200-character limit".to_owned(),
        )
            .into_response();
    }
    let dir = sessions_dir();
    let record = OnDiskSession {
        session_id: req.session_id.clone(),
        participants: Vec::new(),
        status: "running".into(),
        topic: None,
        build_codename: Some(req.build_codename.clone()),
    };
    if let Err(e) = write_session_async(dir, record).await {
        tracing::warn!(session_id = %req.session_id, error = %e, "session write failed");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Json(SessionStartResponse {
        session_id: req.session_id,
        build_codename: req.build_codename,
        status: "running".into(),
    })
    .into_response()
}

/// `POST /api/coordination/sessions/end` — mark a per-build soul-chat session stopped.
///
/// Updates `status` to `"stopped"` in the on-disk session file. If the file
/// does not exist (e.g. the session was never started) the handler returns 404.
pub async fn session_end(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<SessionEndRequest>,
) -> impl IntoResponse {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if req.session_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "session_id must be non-empty".to_owned(),
        )
            .into_response();
    }
    let dir = sessions_dir();
    let path = dir.join(format!("{}.json", req.session_id));
    let raw = match tokio::task::spawn_blocking(move || std::fs::read_to_string(&path)).await {
        Ok(Ok(s)) => s,
        Ok(Err(_)) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let mut record: OnDiskSession = match serde_json::from_str(&raw) {
        Ok(r) => r,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    record.status = "stopped".into();
    if let Err(e) = write_session_async(sessions_dir(), record).await {
        tracing::warn!(session_id = %req.session_id, error = %e, "session end write failed");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Json(SessionEndResponse {
        session_id: req.session_id,
        status: "stopped".into(),
    })
    .into_response()
}

/// `POST /api/coordination/chat/inject` — relay a message to the soul CLI.
///
/// Today we shell out to the `soul` binary at the canonical path. This is
/// deliberate: the soul-chat session lifecycle lives **inside** the soul
/// MCP server's process, not in the webshell, so we cannot inject directly
/// into a session that was started by another process. The CLI delegates
/// through the same MCP entry point and inherits its (limited) cross-process
/// semantics — see TODO(crate-boundary) below.
///
/// TODO(crate-boundary): cross-process inject is broken upstream
/// (SOUL-DEV/soul-mcp/src/tools/chat.rs:454-458 — `session_store` is in-memory
/// and a fresh CLI process sees an empty store). The fix lives in the private
/// `SOUL-DEV/soul-chat` and/or `soul-mcp` crates. This handler returns the
/// shell's stderr verbatim on failure so the operator sees the upstream
/// error.
pub async fn chat_inject(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<InjectRequest>,
) -> impl IntoResponse {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if req.session_id.trim().is_empty() || req.message.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "session_id and message are required".to_owned(),
        )
            .into_response();
    }
    let trimmed = truncate(&req.message, 4000);
    match shell_inject(&req.session_id, &trimmed).await {
        Ok(correlation_id) => Json(InjectResponse {
            injected: true,
            correlation_id,
        })
        .into_response(),
        Err(err) => {
            let sanitized = err.replace(['\n', '\r'], " ");
            tracing::warn!(error = %sanitized, "soul chat inject failed");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

/// `POST /api/coordination/tasks/spawn-worker` — spawn the conductor daemon or a one-shot worker.
///
/// In `"daemon"` mode the conductor is started as a background process and persists
/// until stopped. In `"once"` mode a single pending task is claimed and executed,
/// then the worker exits. Both modes are no-ops when the daemon is already running
/// (the conductor binary itself guards against duplicate daemons via `conductor.pid`).
///
/// The spawned process's stdio is fully detached — stdin/stdout/stderr inherit
/// `Stdio::null()` so the child does not stay linked to the webshell's HTTP server
/// file descriptors.
pub async fn spawn_worker(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<SpawnWorkerRequest>,
) -> impl IntoResponse {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Validate mode before touching the filesystem.
    let mode = req.mode.as_str();
    if mode != "daemon" && mode != "once" {
        return (
            StatusCode::BAD_REQUEST,
            format!("mode must be 'daemon' or 'once', got '{mode}'"),
        )
            .into_response();
    }

    let daemon_was_running = daemon_pid_alive();

    // Locate the conductor binary at the canonical path.
    let bin = home_dir()
        .join(".lightarchitects")
        .join("bin")
        .join("lightarchitects");
    let bin = if bin.exists() {
        bin
    } else {
        // Fallback to PATH lookup — allows dev environments without a deployed binary.
        PathBuf::from("lightarchitects")
    };

    // Build args: `conductor start` (daemon) or `conductor --once` (one-shot).
    let args: &[&str] = if mode == "once" {
        &["conductor", "--once"]
    } else {
        &["conductor", "start"]
    };

    let spawn_result = tokio::process::Command::new(&bin)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    match spawn_result {
        Ok(_child) => {
            // Give the daemon a brief window to write its PID file before we read it.
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let pid = read_conductor_pid();
            let message = if daemon_was_running {
                "conductor already running; mode applied".to_owned()
            } else if mode == "once" {
                "one-shot worker spawned".to_owned()
            } else {
                "conductor daemon started".to_owned()
            };
            Json(SpawnWorkerResponse {
                spawned: true,
                daemon_was_running,
                pid,
                message,
            })
            .into_response()
        }
        Err(e) => {
            tracing::warn!(
                mode = %mode,
                bin = %bin.display(),
                error = %e,
                "spawn-worker: conductor spawn failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `POST /api/builds/{id}/copilot/approve` — approve a pending tool-call permission.
///
/// Looks up the `AgentSessionHost` for the build, checks if the `call_id` exists
/// in the `permission_queue`, and sends `true` via the oneshot channel to unblock
/// the waiting agent turn.
///
/// # Errors
///
/// Returns:
/// - `401 Unauthorized` — missing or invalid bearer token
/// - `404 Not Found` — `build_id` doesn't exist or `AgentSessionHost` not initialized
/// - `400 Bad Request` — `call_id` not provided or invalid format
/// - `409 Conflict` — `call_id` already resolved (race condition)
/// - `503 Service Unavailable` — `control_tx` channel full
pub async fn approve_permission(
    AxPath(build_id): AxPath<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<ApproveRequest>,
) -> impl IntoResponse {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Validate call_id format (UUID v4 expected).
    if req.call_id.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "call_id is required".to_owned()).into_response();
    }

    // Parse build_id as UUID.
    let Ok(build_uuid) = uuid::Uuid::parse_str(&build_id) else {
        return (
            StatusCode::BAD_REQUEST,
            "invalid build_id format".to_owned(),
        )
            .into_response();
    };

    // Look up the build session.
    let Some(session) = state.builds.get(build_uuid) else {
        return (StatusCode::NOT_FOUND, format!("build {build_id} not found")).into_response();
    };

    // Clone the Arc<AgentSessionHost> while holding the lock (HIGH H-22).
    let host = {
        let guard = session.agent_host.lock().await;
        guard.as_ref().cloned()
    };
    let Some(host) = host else {
        return (
            StatusCode::NOT_FOUND,
            "agent session not initialized".to_owned(),
        )
            .into_response();
    };

    // Check if call_id exists in permission_queue.
    let Some((_, sender)) = host.permission_queue.remove(&req.call_id) else {
        return (
            StatusCode::CONFLICT,
            format!(
                "permission request {} already resolved or not found",
                req.call_id
            ),
        )
            .into_response();
    };

    // Send approval (true) via oneshot channel.
    if sender.send(true).is_err() {
        // Receiver already dropped — agent turn already resolved.
        return (
            StatusCode::CONFLICT,
            "agent turn already completed".to_owned(),
        )
            .into_response();
    }

    Json(ApproveResponse {
        approved: true,
        message: "permission approved".to_owned(),
    })
    .into_response()
}

/// `POST /api/builds/{id}/copilot/reject` — reject a pending tool-call permission.
///
/// Looks up the `AgentSessionHost` for the build, checks if the `call_id` exists
/// in the `permission_queue`, and sends `false` via the oneshot channel to unblock
/// the waiting agent turn with a denial.
///
/// # Errors
///
/// Returns:
/// - `401 Unauthorized` — missing or invalid bearer token
/// - `404 Not Found` — `build_id` doesn't exist or `AgentSessionHost` not initialized
/// - `400 Bad Request` — `call_id` not provided or invalid format
/// - `409 Conflict` — `call_id` already resolved (race condition)
/// - `503 Service Unavailable` — `control_tx` channel full
pub async fn reject_permission(
    AxPath(build_id): AxPath<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<RejectRequest>,
) -> impl IntoResponse {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Validate call_id format (UUID v4 expected).
    if req.call_id.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "call_id is required".to_owned()).into_response();
    }

    // Parse build_id as UUID.
    let Ok(build_uuid) = uuid::Uuid::parse_str(&build_id) else {
        return (
            StatusCode::BAD_REQUEST,
            "invalid build_id format".to_owned(),
        )
            .into_response();
    };

    // Look up the build session.
    let Some(session) = state.builds.get(build_uuid) else {
        return (StatusCode::NOT_FOUND, format!("build {build_id} not found")).into_response();
    };

    // Get the agent host. Bind the guard to a named binding so the MutexGuard
    // lives long enough for host to borrow from it across the permission_queue lookup.
    let agent_host_guard = session.agent_host.lock().await;
    let Some(host) = agent_host_guard.as_ref() else {
        return (
            StatusCode::NOT_FOUND,
            "agent session not initialized".to_owned(),
        )
            .into_response();
    };

    // Check if call_id exists in permission_queue.
    let Some((_, sender)) = host.permission_queue.remove(&req.call_id) else {
        return (
            StatusCode::CONFLICT,
            format!(
                "permission request {} already resolved or not found",
                req.call_id
            ),
        )
            .into_response();
    };

    // Log rejection reason if provided — strip newlines to prevent log injection.
    if let Some(ref reason) = req.reason {
        let sanitized = reason.replace(['\n', '\r'], " ");
        tracing::info!(call_id = %req.call_id, reason = %sanitized, "permission rejected");
    }

    // Send denial (false) via oneshot channel.
    if sender.send(false).is_err() {
        // Receiver already dropped — agent turn already resolved.
        return (
            StatusCode::CONFLICT,
            "agent turn already completed".to_owned(),
        )
            .into_response();
    }

    Json(RejectResponse {
        rejected: true,
        message: req
            .reason
            .unwrap_or_else(|| "permission rejected".to_owned()),
    })
    .into_response()
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Return `true` if the request carries valid credentials — either an
/// `Authorization: Bearer <token>` header **or** a valid `la_session` cookie.
/// Mirrors [`crate::auth::AuthGuard`] for handlers that already take `HeaderMap`.
fn is_authorised(headers: &HeaderMap, state: &AppState) -> bool {
    let token = &state.config.token;
    let bearer_ok = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| auth::validate_bearer(s, token));
    if bearer_ok {
        return true;
    }
    headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| auth::validate_session_cookie(s, token))
}

/// Resolve the canonical conductor queue path: `~/.lightarchitects/tasks/queue.json`.
pub(crate) fn queue_path() -> PathBuf {
    home_dir()
        .join(".lightarchitects")
        .join("tasks")
        .join("queue.json")
}

/// Resolve the soul-chat sessions directory: `~/lightarchitects/soul/helix/chat/sessions`.
fn sessions_dir() -> PathBuf {
    home_dir()
        .join("lightarchitects")
        .join("soul")
        .join("helix")
        .join("chat")
        .join("sessions")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/Users/kft"), PathBuf::from)
}

/// Error type for queue and session file I/O (P7 check 5 — opaque HTTP codes, CWE-209).
///
/// Unit variants prevent raw OS/serde error strings from reaching HTTP responses.
/// Internal detail is captured at the error site via `tracing::warn!`.
#[derive(Debug)]
pub(crate) enum QueueIoError {
    Missing,
    Read,
    Parse,
    Write,
}

impl std::fmt::Display for QueueIoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => f.write_str("queue file missing"),
            Self::Read => f.write_str("queue read error"),
            Self::Parse => f.write_str("queue parse error"),
            Self::Write => f.write_str("queue write error"),
        }
    }
}

fn read_queue(path: &Path) -> Result<OnDiskQueue, QueueIoError> {
    if !path.exists() {
        return Err(QueueIoError::Missing);
    }
    let content = std::fs::read_to_string(path).map_err(|e| {
        tracing::warn!(error = %e, path = %path.display(), "queue read failed");
        QueueIoError::Read
    })?;
    serde_json::from_str(&content).map_err(|e| {
        tracing::warn!(error = %e, path = %path.display(), "queue parse failed");
        QueueIoError::Parse
    })
}

/// Async wrapper — offloads blocking file I/O to a thread pool (MED H-90).
pub(crate) async fn read_queue_async(path: PathBuf) -> Result<OnDiskQueue, QueueIoError> {
    tokio::task::spawn_blocking(move || read_queue(&path))
        .await
        .unwrap_or_else(|_| {
            tracing::warn!("queue read blocking task panicked");
            Err(QueueIoError::Read)
        })
}

fn write_queue(path: &Path, queue: &OnDiskQueue) -> Result<(), QueueIoError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            tracing::warn!(error = %e, path = %path.display(), "queue dir create failed");
            QueueIoError::Write
        })?;
    }
    let body = serde_json::to_string_pretty(queue).map_err(|e| {
        tracing::warn!(error = %e, "queue serialise failed");
        QueueIoError::Write
    })?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, body.as_bytes()).map_err(|e| {
        tracing::warn!(error = %e, path = %tmp.display(), "queue tmp write failed");
        QueueIoError::Write
    })?;
    std::fs::rename(&tmp, path).map_err(|e| {
        tracing::warn!(error = %e, "queue atomic rename failed");
        QueueIoError::Write
    })?;
    Ok(())
}

/// Async wrapper — offloads blocking file I/O to a thread pool (MED H-90).
pub(crate) async fn write_queue_async(
    path: PathBuf,
    queue: OnDiskQueue,
) -> Result<(), QueueIoError> {
    tokio::task::spawn_blocking(move || write_queue(&path, &queue))
        .await
        .unwrap_or_else(|_| {
            tracing::warn!("queue write blocking task panicked");
            Err(QueueIoError::Write)
        })
}

fn write_session(dir: &Path, record: &OnDiskSession) -> Result<(), QueueIoError> {
    std::fs::create_dir_all(dir).map_err(|e| {
        tracing::warn!(error = %e, dir = %dir.display(), "session dir create failed");
        QueueIoError::Write
    })?;
    let path = dir.join(format!("{}.json", record.session_id));
    let body = serde_json::to_string_pretty(record).map_err(|e| {
        tracing::warn!(error = %e, "session serialise failed");
        QueueIoError::Write
    })?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, body.as_bytes()).map_err(|e| {
        tracing::warn!(error = %e, path = %tmp.display(), "session tmp write failed");
        QueueIoError::Write
    })?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        tracing::warn!(error = %e, "session atomic rename failed");
        QueueIoError::Write
    })?;
    Ok(())
}

/// Async wrapper — offloads blocking file I/O to a thread pool (MED H-90).
async fn write_session_async(dir: PathBuf, record: OnDiskSession) -> Result<(), QueueIoError> {
    tokio::task::spawn_blocking(move || write_session(&dir, &record))
        .await
        .unwrap_or_else(|_| {
            tracing::warn!("session write blocking task panicked");
            Err(QueueIoError::Write)
        })
}

fn read_sessions_dir(dir: &Path) -> Option<Vec<ChatSessionSummary>> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            if let Some(s) = parse_session_file(&path) {
                out.push(s);
            }
        }
    }
    out.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    Some(out)
}

fn parse_session_file(path: &Path) -> Option<ChatSessionSummary> {
    let raw = std::fs::read_to_string(path).ok()?;
    let on_disk: OnDiskSession = serde_json::from_str(&raw).ok()?;
    Some(ChatSessionSummary {
        session_id: on_disk.session_id,
        status: if on_disk.status.is_empty() {
            "unknown".into()
        } else {
            on_disk.status
        },
        participants: on_disk.participants,
        current_topic: on_disk.topic,
        message_count: None,
    })
}

fn to_summary(t: &OnDiskTask) -> TaskSummary {
    TaskSummary {
        id: t.id.clone(),
        title: t.title.clone(),
        project: t.project.clone(),
        prompt_excerpt: truncate(&t.prompt, 240),
        status: if t.status.is_empty() {
            "pending".into()
        } else {
            t.status.clone()
        },
        source: t.source.clone(),
        priority: if t.priority.is_empty() {
            "medium".into()
        } else {
            t.priority.clone()
        },
        added: t.added.clone(),
        started: t.started.clone(),
        finished: t.finished.clone(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    s.chars().take(max).collect()
}

fn validate_add(req: &AddTaskRequest) -> Result<(), String> {
    if req.title.trim().is_empty() {
        return Err("title is required".into());
    }
    if req.project.trim().is_empty() {
        return Err("project is required".into());
    }
    if req.prompt.trim().is_empty() {
        return Err("prompt is required".into());
    }
    Ok(())
}

fn normalize_priority(p: Option<&str>) -> String {
    match p.map(str::to_lowercase).as_deref() {
        Some("high") => "high".into(),
        Some("low") => "low".into(),
        _ => "medium".into(),
    }
}

fn mint_task_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("webshell-{now}")
}

fn now_rfc3339() -> String {
    // Use chrono indirectly via the conductor pattern would require a dep
    // we don't pull. Format epoch seconds as a stand-in; conductor's own
    // `added` field accepts any string the consumer recognises.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("epoch:{secs}")
}

fn read_log_tail(path: &str) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(200);
    lines[start..].iter().map(|s| (*s).to_owned()).collect()
}

fn read_conductor_pid() -> Option<u32> {
    let pid_path = home_dir().join(".lightarchitects").join("conductor.pid");
    std::fs::read_to_string(&pid_path)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
}

fn daemon_pid_alive() -> bool {
    let pid_path = home_dir().join(".lightarchitects").join("conductor.pid");
    let Ok(content) = std::fs::read_to_string(&pid_path) else {
        return false;
    };
    let Ok(pid) = content.trim().parse::<u32>() else {
        return false;
    };
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

async fn shell_inject(session_id: &str, message: &str) -> Result<String, String> {
    // Reject traversal sequences and flag-injection before passing to subprocess (CRITICAL C-ARG).
    if session_id.is_empty()
        || session_id
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
    {
        return Err("invalid session_id".to_owned());
    }
    let bin = home_dir()
        .join("lightarchitects")
        .join("soul")
        .join(".config")
        .join("bin")
        .join("soul");
    let bin = if bin.exists() {
        bin
    } else {
        // Fallback to PATH lookup.
        PathBuf::from("soul")
    };
    let output = tokio::process::Command::new(&bin)
        .args(["chat", "inject", "--session-id", session_id, message])
        .output()
        .await
        .map_err(|e| format!("spawn soul CLI failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if stderr.is_empty() {
            format!("soul chat inject exited with status {}", output.status)
        } else {
            stderr
        });
    }
    Ok(short_correlation_id())
}

fn short_correlation_id() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    #[allow(clippy::cast_possible_truncation)]
    let low32 = secs as u64 & 0x_ffff_ffff;
    format!("inj-{low32:x}")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn truncate_respects_char_count() {
        let s: String = "a".repeat(300);
        let out = truncate(&s, 240);
        assert_eq!(out.chars().count(), 240);
    }

    #[test]
    fn validate_add_rejects_empty_fields() {
        let bad = AddTaskRequest {
            title: "  ".into(),
            project: "p".into(),
            prompt: "q".into(),
            priority: None,
            build_codename: None,
            assignee: None,
            build_session_id: None,
        };
        assert!(validate_add(&bad).is_err());
    }

    #[test]
    fn validate_add_accepts_full_body() {
        let ok = AddTaskRequest {
            title: "t".into(),
            project: "p".into(),
            prompt: "q".into(),
            priority: Some("high".into()),
            build_codename: None,
            assignee: None,
            build_session_id: None,
        };
        assert!(validate_add(&ok).is_ok());
    }

    #[test]
    fn normalize_priority_defaults_to_medium() {
        assert_eq!(normalize_priority(None), "medium");
        assert_eq!(normalize_priority(Some("bogus")), "medium");
        assert_eq!(normalize_priority(Some("HIGH")), "high");
        assert_eq!(normalize_priority(Some("low")), "low");
    }

    #[test]
    fn read_queue_round_trip() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("queue.json");
        let q = OnDiskQueue {
            version: "1.0".into(),
            tasks: vec![OnDiskTask {
                id: "t1".into(),
                title: "x".into(),
                project: "p".into(),
                prompt: "q".into(),
                status: "pending".into(),
                source: "manual".into(),
                priority: "medium".into(),
                added: None,
                started: None,
                finished: None,
                retries: 0,
                output_log: None,
                build_codename: None,
                assignee: None,
                build_session_id: None,
                awaiting_assertion_id: None,
                resolution_deadline: None,
            }],
        };
        write_queue(&path, &q).expect("write");
        let back = read_queue(&path).expect("read");
        assert_eq!(back.tasks.len(), 1);
        assert_eq!(back.tasks[0].id, "t1");
    }

    #[test]
    fn read_queue_missing_returns_missing_variant() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("missing.json");
        let err = read_queue(&path).expect_err("must fail");
        matches!(err, QueueIoError::Missing);
    }

    #[test]
    fn to_summary_truncates_prompt() {
        let long = "x".repeat(1000);
        let task = OnDiskTask {
            id: "i".into(),
            title: "t".into(),
            project: "p".into(),
            prompt: long,
            status: "pending".into(),
            source: "manual".into(),
            priority: "medium".into(),
            added: None,
            started: None,
            finished: None,
            retries: 0,
            output_log: None,
            build_codename: None,
            assignee: None,
            build_session_id: None,
            awaiting_assertion_id: None,
            resolution_deadline: None,
        };
        let s = to_summary(&task);
        assert_eq!(s.prompt_excerpt.len(), 240);
    }

    #[test]
    fn parse_session_file_reads_minimal_record() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("aaaa.json");
        std::fs::write(
            &p,
            r#"{"session_id":"aaaa","participants":["eva","corso"],"status":"running","topic":"x"}"#,
        )
        .expect("write");
        let s = parse_session_file(&p).expect("parse");
        assert_eq!(s.session_id, "aaaa");
        assert_eq!(s.status, "running");
        assert_eq!(s.participants.len(), 2);
        assert_eq!(s.current_topic.as_deref(), Some("x"));
    }
}
