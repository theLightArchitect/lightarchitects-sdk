//! HTTP route handlers for the Squad Dispatch module.
//!
//! All five handlers require `Authorization: Bearer <token>` — the same
//! middleware used by `/api/coordination/*` (HIGH H-5).
//!
//! # Endpoints
//!
//! - `POST /api/dispatch/classify` — classify task text without executing
//! - `POST /api/dispatch/execute`  — dispatch agents, return `DispatchId`
//! - `GET  /api/dispatch/status/:id` — SSE stream of [`DispatchEvent`]s
//! - `POST /api/dispatch/cancel/:id` — cancel an active dispatch
//! - `POST /api/dispatch/retry/:id/:agent` — retry a failed agent
//! - `POST /api/dispatch/:id/fs-approve` — approve a pending FS-mutation permission
//! - `POST /api/dispatch/:id/fs-reject`  — reject a pending FS-mutation permission
//! - `GET  /api/dispatch/:id/artifacts`          — list artifacts produced by a dispatch
//! - `GET  /api/dispatch/:id/artifacts/:name`    — fetch a single artifact by filename
//!
//! # Input validation (HIGH H-2)
//!
//! All `task` string fields pass through [`validate_task_input`] before any
//! downstream use:
//! - maximum 8 KB
//! - strips `\n`, `\r`, `\0`, `\x1b` (control characters)
//! - rejects non-UTF-8

use std::convert::Infallible;
use std::path::PathBuf;
use std::time::SystemTime;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures_util::stream;
use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};
use serde::Serialize;

use uuid::Uuid;

use crate::{
    auth,
    coordination::types::{FsApproveRequest, FsDecisionResponse, FsRejectRequest},
    server::AppState,
};

use super::{
    classifier, executor,
    types::{
        ClassifyRequest, DispatchError, DispatchEvent, DispatchId, DomainAgent, ExecuteRequest,
        ExecutionMode, RetryRequest, SanitizedTask,
    },
};

/// Maximum task input length in bytes (HIGH H-2).
const MAX_TASK_BYTES: usize = 8 * 1024;

/// Sequence counter for dispatch IDs (u16 overflow wraps — acceptable for
/// local dev tool; not a security property).
static DISPATCH_SEQ: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(1);

/// CALLSIGN table — deterministic rotation for squad dispatch IDs.
static CALLSIGNS: &[&str] = &[
    "ALPHA", "BRAVO", "CHARLIE", "DELTA", "ECHO", "FOXTROT", "GOLF", "HOTEL", "INDIA", "JULIET",
    "KILO", "LIMA", "MIKE", "NOVEMBER", "OSCAR", "PAPA", "QUEBEC", "ROMEO", "SIERRA", "TANGO",
];

/// Validate and sanitise a task input string (HIGH H-2).
///
/// - Checks UTF-8 well-formedness (already guaranteed by `Json<T>` deserialiser,
///   but verified explicitly for defence-in-depth).
/// - Caps length at [`MAX_TASK_BYTES`].
/// - Strips `\n`, `\r`, `\0`, and `\x1b` (ESC) to prevent SSE-frame injection
///   and terminal escape abuse.
///
/// # Errors
///
/// Returns [`DispatchError::InvalidInput`] with a human-readable reason.
pub fn validate_task_input(task: &str) -> Result<SanitizedTask, DispatchError> {
    if task.len() > MAX_TASK_BYTES {
        return Err(DispatchError::InvalidInput(format!(
            "task exceeds 8 KB limit ({} bytes)",
            task.len()
        )));
    }
    // Strip control characters.
    let sanitised: String = task
        .chars()
        .filter(|c| !matches!(*c, '\n' | '\r' | '\0' | '\x1b'))
        .collect();
    if sanitised.is_empty() && !task.is_empty() {
        return Err(DispatchError::InvalidInput(
            "task became empty after control-character sanitisation".to_owned(),
        ));
    }
    Ok(SanitizedTask(sanitised))
}

/// Build the `/api/dispatch/*` sub-router.
///
/// Registered under `AppState` — caller must call `.with_state(state)` on the
/// returned router (or nest it into a router that already has state).
pub fn dispatch_router() -> Router<AppState> {
    Router::new()
        .route("/api/dispatch/classify", post(classify_handler))
        .route("/api/dispatch/execute", post(execute_handler))
        .route("/api/dispatch/status/{id}", get(status_sse_handler))
        .route("/api/dispatch/cancel/{id}", post(cancel_handler))
        .route("/api/dispatch/retry/{id}/{agent}", post(retry_handler))
        .route("/api/dispatch/{id}/fs-approve", post(fs_approve_handler))
        .route("/api/dispatch/{id}/fs-reject", post(fs_reject_handler))
        .route("/api/dispatch/{id}/artifacts", get(artifacts_list_handler))
        .route(
            "/api/dispatch/{id}/artifacts/{name}",
            get(artifacts_fetch_handler),
        )
}

// ── Response helpers ──────────────────────────────────────────────────────────

/// Response body returned by `POST /api/dispatch/execute`.
#[derive(Serialize)]
struct ExecuteResponse {
    dispatch_id: String,
}

/// Response body for cancel and retry acknowledgements.
#[derive(Serialize)]
struct OkResponse {
    ok: bool,
}

// ── Auth helper ──────────────────────────────────────────────────────────────

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

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /api/dispatch/classify` — classify task text (no execution).
///
/// Bearer-authenticated (HIGH H-5).
///
/// # Rate limit
///
/// Per-IP ≤10 req/s is enforced at the infrastructure layer (future: tower
/// governor middleware — HIGH H-8).  This handler does not implement its own
/// rate-limiting so the constraint is visible in the spec.
#[tracing::instrument(skip(headers, state, req), fields(task_len = req.task.len()))]
async fn classify_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<ClassifyRequest>,
) -> Response {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let task = match validate_task_input(&req.task) {
        Ok(t) => t,
        Err(e) => {
            return (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()).into_response();
        }
    };
    let classification = classifier::classify(task.as_str());
    Json(classification).into_response()
}

/// `POST /api/dispatch/execute` — dispatch agents and start streaming.
///
/// Returns `{ dispatch_id: "SQD-ALPHA-01" }` on success.  The caller opens
/// `GET /api/dispatch/status/:id` to receive [`DispatchEvent`] SSE frames.
///
/// Bearer-authenticated (HIGH H-5).
#[tracing::instrument(skip(headers, state, req), fields(agent_count = req.agents.len(), dry = req.dry))]
async fn execute_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<ExecuteRequest>,
) -> Response {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Validate + sanitise task.
    let task = match validate_task_input(&req.task) {
        Ok(t) => t,
        Err(e) => {
            return (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()).into_response();
        }
    };

    // Deduplicate agent list and validate against enum variants.
    let agents = deduplicate_agents(req.agents);
    if agents.is_empty() {
        return (StatusCode::BAD_REQUEST, "No agents specified").into_response();
    }
    if agents.len() > executor::MAX_AGENTS_PER_DISPATCH {
        return (
            StatusCode::BAD_REQUEST,
            format!(
                "Too many agents (max {})",
                executor::MAX_AGENTS_PER_DISPATCH
            ),
        )
            .into_response();
    }

    let mode = match agents.len() {
        1 => ExecutionMode::Solo,
        _ => ExecutionMode::Squad,
    };

    // Mint a dispatch ID.
    let seq = DISPATCH_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dispatch_id = if mode == ExecutionMode::Squad {
        let callsign = CALLSIGNS[usize::from(seq) % CALLSIGNS.len()];
        match DispatchId::squad(callsign, seq) {
            Ok(id) => id,
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        let code = agents.first().map_or("UNK", |a| a.code());
        match DispatchId::solo(code, seq) {
            Ok(id) => id,
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    };

    let id_str = dispatch_id.to_string();

    match executor::execute(
        task,
        agents,
        mode,
        req.dry,
        dispatch_id,
        state.dispatch_registry,
        req.attachments,
        req.tool_config,
    )
    .await
    {
        Ok(()) => Json(ExecuteResponse {
            dispatch_id: id_str,
        })
        .into_response(),
        Err(DispatchError::ScopeRequired) => (
            StatusCode::FORBIDDEN,
            DispatchError::ScopeRequired.to_string(),
        )
            .into_response(),
        Err(DispatchError::AlreadyActive(ref id)) => (
            StatusCode::CONFLICT,
            format!("dispatch {id} already active"),
        )
            .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// `GET /api/dispatch/status/:id` — SSE stream of [`DispatchEvent`] frames.
///
/// Bearer-authenticated (HIGH H-5).
///
/// The SSE stream drives off a `broadcast::Receiver<DispatchEvent>`.  We
/// use `futures_util::stream::unfold` so we do not need the `tokio-stream`
/// crate (mirrors the pattern in `coordination::sse`).
#[tracing::instrument(skip(headers, state), fields(dispatch_id = %id_str))]
async fn status_sse_handler(
    headers: HeaderMap,
    Path(id_str): Path<String>,
    State(state): State<AppState>,
) -> Response {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Validate the ID string before constructing a DispatchId.
    if id_str.contains('\n') || id_str.contains('\r') {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let broadcast_rx = {
        let reg = state.dispatch_registry.lock().await;
        reg.broadcast_tx(&DispatchId::from_raw(id_str.clone()))
            .map(tokio::sync::broadcast::Sender::subscribe)
    };

    match broadcast_rx {
        None => StatusCode::NOT_FOUND.into_response(),
        Some(rx) => {
            // State is (receiver, done). When done=true the closure returns None,
            // ending the stream on the iteration after a terminal event is emitted.
            let sse_stream = stream::unfold((rx, false), |state| async move {
                let (mut receiver, done) = state;
                if done {
                    return None;
                }
                loop {
                    match receiver.recv().await {
                        Ok(event) => {
                            let is_terminal = matches!(
                                event,
                                DispatchEvent::Complete { .. } | DispatchEvent::Error { .. }
                            );
                            let data = serde_json::to_string(&event).ok()?;
                            let sse_event = Event::default().data(data);
                            return Some((
                                Ok::<Event, Infallible>(sse_event),
                                (receiver, is_terminal),
                            ));
                        }
                        // Lagged — skip missed frames, keep streaming.
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        // Sender dropped → dispatch complete, end stream.
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            return None;
                        }
                    }
                }
            });

            Sse::new(sse_stream)
                .keep_alive(KeepAlive::default())
                .into_response()
        }
    }
}

/// `POST /api/dispatch/cancel/:id` — cancel an active dispatch.
///
/// Bearer-authenticated (HIGH H-5).
#[tracing::instrument(skip(headers, state), fields(dispatch_id = %id_str))]
async fn cancel_handler(
    headers: HeaderMap,
    Path(id_str): Path<String>,
    State(state): State<AppState>,
) -> Response {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    if id_str.contains('\n') || id_str.contains('\r') {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let id = DispatchId::from_raw(id_str);
    match executor::cancel(&id, state.dispatch_registry).await {
        Ok(()) => Json(OkResponse { ok: true }).into_response(),
        Err(DispatchError::NotFound(_)) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// `POST /api/dispatch/retry/:id/:agent` — retry a failed agent.
///
/// Bearer-authenticated (HIGH H-5).
///
/// # Phase note
///
/// Retry logic is a stub in Phase 3 Wave 1 — full implementation in Wave 3 B2
/// once `TeamManager` is wired.
#[tracing::instrument(skip(headers, state, req), fields(dispatch_id = %id_str, agent = %agent_str))]
async fn retry_handler(
    headers: HeaderMap,
    Path((id_str, agent_str)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(req): Json<RetryRequest>,
) -> Response {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    if id_str.contains('\n') || id_str.contains('\r') {
        return StatusCode::BAD_REQUEST.into_response();
    }

    // Parse agent variant.
    let Some(_agent) = parse_agent(&agent_str) else {
        return (StatusCode::BAD_REQUEST, "Unknown agent").into_response();
    };

    // Validate optional task override.
    if let Some(ref task_override) = req.task {
        if let Err(e) = validate_task_input(task_override) {
            return (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()).into_response();
        }
    }

    let id = DispatchId::from_raw(id_str);
    {
        let registry = state.dispatch_registry.lock().await;
        if !registry.contains(&id) {
            return StatusCode::NOT_FOUND.into_response();
        }
    } // mutex guard drops here before response is built
    // TODO(team-manager): wire actual retry into TeamManager in Wave 3 B2.
    Json(OkResponse { ok: true }).into_response()
}

/// `POST /api/dispatch/:id/fs-approve` — approve a pending FS-mutation permission request.
///
/// `:id` is the build-session UUID (the `dispatch_id` field of the
/// `fs_mutation_pending` SSE event). The request body carries the `mutation_id`
/// that keys `AgentSessionHost::permission_queue`.
///
/// Bearer-authenticated (HIGH H-5). Returns 404 when the session or mutation
/// is not found. Returns 409 when the mutation was already resolved.
#[tracing::instrument(skip(headers, state, req), fields(build_id = %id_str))]
async fn fs_approve_handler(
    headers: HeaderMap,
    Path(id_str): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<FsApproveRequest>,
) -> Response {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    resolve_fs_permission(&id_str, &req.mutation_id, true, &state).await
}

/// `POST /api/dispatch/:id/fs-reject` — reject a pending FS-mutation permission request.
///
/// Same semantics as `fs-approve` but sends `false` to the oneshot channel,
/// causing the agent to receive a synthetic error for the blocked tool call.
#[tracing::instrument(skip(headers, state, req), fields(build_id = %id_str))]
async fn fs_reject_handler(
    headers: HeaderMap,
    Path(id_str): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<FsRejectRequest>,
) -> Response {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // reason is logged only — not forwarded over the oneshot (bool channel).
    if !req.reason.is_empty() {
        tracing::info!(mutation_id = %req.mutation_id, reason = %req.reason, "fs mutation rejected by operator");
    }
    resolve_fs_permission(&id_str, &req.mutation_id, false, &state).await
}

/// Shared resolution logic for approve + reject.
///
/// Finds the `AgentSessionHost` for `build_id_str`, removes `mutation_id` from
/// its `permission_queue`, and sends `approved` through the oneshot channel.
async fn resolve_fs_permission(
    build_id_str: &str,
    mutation_id: &str,
    approved: bool,
    state: &AppState,
) -> Response {
    // Parse the build-session UUID — the dispatch_id in SSE events is the build UUID.
    let Ok(build_uuid) = Uuid::parse_str(build_id_str) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    // Look up the BuildSession.
    let Some(session) = state.builds.get(build_uuid) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // Access the agent host (may be None if no agent activity yet).
    let guard = session.agent_host.lock().await;
    let Some(host) = guard.as_ref().cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    drop(guard); // release lock before send

    // Remove and resolve the oneshot sender (removes from queue atomically).
    let Some((_, sender)) = host.permission_queue.remove(mutation_id) else {
        // Already resolved or never registered.
        return StatusCode::CONFLICT.into_response();
    };

    // Unblock the waiting agent — error means receiver was dropped (turn cancelled).
    if sender.send(approved).is_err() {
        tracing::warn!(
            mutation_id,
            approved,
            "permission oneshot receiver dropped before resolve"
        );
    }

    Json(FsDecisionResponse {
        mutation_id: mutation_id.to_owned(),
        decision: if approved { "approved" } else { "rejected" }.to_owned(),
    })
    .into_response()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Deduplicate a list of `DomainAgent` values, preserving order.
fn deduplicate_agents(agents: Vec<DomainAgent>) -> Vec<DomainAgent> {
    let mut seen = std::collections::HashSet::new();
    agents.into_iter().filter(|a| seen.insert(*a)).collect()
}

/// Parse a domain agent from a URL path segment string.
fn parse_agent(s: &str) -> Option<DomainAgent> {
    match s {
        "engineer" => Some(DomainAgent::Engineer),
        "quality" => Some(DomainAgent::Quality),
        "security" => Some(DomainAgent::Security),
        "ops" => Some(DomainAgent::Ops),
        "researcher" => Some(DomainAgent::Researcher),
        "knowledge" => Some(DomainAgent::Knowledge),
        "testing" => Some(DomainAgent::Testing),
        "squad" => Some(DomainAgent::Squad),
        _ => None,
    }
}

// ── Artifact routes ───────────────────────────────────────────────────────────

/// Row returned by `GET /api/dispatch/:id/artifacts`.
#[derive(Serialize)]
struct ArtifactMeta {
    name: String,
    agent: String,
    size: u64,
    modified: String,
}

/// Validate a filename is safe to join under a base directory (CWE-22).
///
/// Rejects names containing path separators, `..`, or NUL bytes before
/// constructing the joined path.  After joining, ancestor-walks to the nearest
/// existing path component and asserts the canonicalized result is still under
/// the base directory (§63.P4).
fn safe_join(base: &std::path::Path, name: &str) -> Result<PathBuf, ()> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains('\0')
    {
        return Err(());
    }
    let candidate = base.join(name);
    // Ancestor-walk: find the nearest existing ancestor for canonicalize.
    let canon_base = {
        let mut p = base.to_path_buf();
        while !p.exists() {
            if !p.pop() {
                return Err(());
            }
        }
        p.canonicalize().map_err(|_| ())?
    };
    let canon_candidate = {
        let mut p = candidate.clone();
        while !p.exists() {
            if !p.pop() {
                return Err(());
            }
        }
        p.canonicalize().map_err(|_| ())?
    };
    if !canon_candidate.starts_with(&canon_base) {
        return Err(());
    }
    Ok(candidate)
}

/// ISO-8601 formatted mtime for display in the artifact list.
fn format_mtime(meta: &std::fs::Metadata) -> String {
    meta.modified()
        .ok()
        .and_then(|t| {
            t.duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs())
        })
        .map_or_else(
            || "unknown".to_owned(),
            |s| {
                let secs = s % 60;
                let mins = (s / 60) % 60;
                let hours = (s / 3600) % 24;
                let days_since_epoch = s / 86_400;
                // Simple ISO-8601 approximation — wall-clock display only (not TZ-aware).
                format!("{days_since_epoch}d {hours:02}:{mins:02}:{secs:02}Z")
            },
        )
}

/// Agent name inferred from artifact filename convention: `agent-<name>.md`
/// or `<name>-output.md`.  Falls back to the stem.
fn infer_agent(name: &str) -> String {
    let stem = name.trim_end_matches(".md");
    if let Some(s) = stem.strip_prefix("agent-") {
        return s.to_owned();
    }
    if let Some(s) = stem.strip_suffix("-output") {
        return s.to_owned();
    }
    stem.to_owned()
}

/// `GET /api/dispatch/:id/artifacts` — list artifacts in the dispatch scratch dir.
///
/// # Spans (dispatch.artifacts.list)
///
/// Emits one AYIN span per call with `file_count` metadata.  Credential values
/// are never included in span metadata — Security Guardrails §LLM01.
///
/// # Errors
///
/// Returns 404 if the scratch directory does not exist (`E_ARTIFACTS_DIR_MISSING`
/// is forwarded as the body so the UI can render a contextual message).
#[tracing::instrument(skip(headers, state), fields(dispatch_id = %id))]
async fn artifacts_list_handler(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let base = state.config.cwd.join(".tmp").join(format!("dispatch-{id}"));

    let span_start = std::time::Instant::now();

    let Ok(entries) = std::fs::read_dir(&base) else {
        let _ = TraceContext::new(Actor::new("webshell"), "dispatch.artifacts.list")
            .metadata(serde_json::json!({ "dispatch_id": id, "outcome": "dir_missing" }))
            .outcome(TraceOutcome::Block)
            .finish();
        return (StatusCode::NOT_FOUND, "E_ARTIFACTS_DIR_MISSING").into_response();
    };

    let mut rows: Vec<ArtifactMeta> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let meta = std::fs::metadata(&path);
            let (size, modified) = meta
                .as_ref()
                .map(|m| (m.len(), format_mtime(m)))
                .unwrap_or((0, "unknown".to_owned()));
            let agent = infer_agent(&name);
            rows.push(ArtifactMeta {
                name,
                agent,
                size,
                modified,
            });
        }
    }
    rows.sort_by(|a, b| a.name.cmp(&b.name));

    let elapsed_ms = u64::try_from(span_start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let _ = TraceContext::new(Actor::new("webshell"), "dispatch.artifacts.list")
        .metadata(serde_json::json!({
            "dispatch_id": id,
            "file_count": rows.len(),
            "elapsed_ms": elapsed_ms,
        }))
        .outcome(TraceOutcome::Continue)
        .finish();

    Json(rows).into_response()
}

/// `GET /api/dispatch/:id/artifacts/:name` — fetch a single artifact file.
///
/// # Security
///
/// Filename is validated by [`safe_join`] (CWE-22, §63.P4).  Returns 400 on
/// any path-traversal attempt.
///
/// # Spans (dispatch.artifacts.preview)
///
/// Emits one AYIN span per call with `file_size` (never file contents).
#[tracing::instrument(skip(headers, state), fields(dispatch_id = %id, file_name = %name))]
async fn artifacts_fetch_handler(
    headers: HeaderMap,
    Path((id, name)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Response {
    if !is_authorised(&headers, &state) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let base = state.config.cwd.join(".tmp").join(format!("dispatch-{id}"));

    let Ok(path) = safe_join(&base, &name) else {
        return (StatusCode::BAD_REQUEST, "invalid artifact name").into_response();
    };

    let content = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(_) => {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let _ = TraceContext::new(Actor::new("webshell"), "dispatch.artifacts.preview")
        .metadata(serde_json::json!({
            "dispatch_id": id,
            "file_size": content.len(),
        }))
        .outcome(TraceOutcome::Continue)
        .finish();

    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        Bytes::from(content),
    )
        .into_response()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn validate_task_input_accepts_normal() {
        let result = validate_task_input("refactor the auth module");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_task_input_rejects_oversize() {
        let large = "x".repeat(MAX_TASK_BYTES + 1);
        let result = validate_task_input(&large);
        assert!(matches!(result, Err(DispatchError::InvalidInput(_))));
    }

    #[test]
    fn validate_task_input_strips_control_chars() {
        let result = validate_task_input("refactor\n\rauth\0\x1b").unwrap();
        assert_eq!(result.as_str(), "refactorauth");
    }

    #[test]
    fn validate_task_input_accepts_8kb_boundary() {
        let boundary = "x".repeat(MAX_TASK_BYTES);
        let result = validate_task_input(&boundary);
        assert!(result.is_ok());
    }

    #[test]
    fn deduplicate_agents_preserves_order() {
        let agents = vec![
            DomainAgent::Engineer,
            DomainAgent::Testing,
            DomainAgent::Engineer,
        ];
        let deduped = deduplicate_agents(agents);
        assert_eq!(deduped, vec![DomainAgent::Engineer, DomainAgent::Testing]);
    }

    #[test]
    fn parse_agent_all_variants() {
        assert_eq!(parse_agent("engineer"), Some(DomainAgent::Engineer));
        assert_eq!(parse_agent("security"), Some(DomainAgent::Security));
        assert_eq!(parse_agent("unknown"), None);
    }

    // ── boundary_sanitization_verified (Phase 3 exit criterion) ──────────────

    #[test]
    fn validate_task_input_rejects_only_control_chars() {
        // Input that becomes empty after stripping — must error, not silently pass.
        let result = validate_task_input("\n\r\0\x1b");
        assert!(matches!(result, Err(DispatchError::InvalidInput(_))));
    }

    #[test]
    fn validate_task_input_preserves_unicode_multibyte() {
        // Non-ASCII UTF-8 (emoji + CJK) must survive unmodified.
        let input = "Audit 建設 🔒 system";
        let result = validate_task_input(input).unwrap();
        assert_eq!(result.as_str(), input);
    }

    #[test]
    fn validate_task_input_strips_each_control_individually() {
        assert_eq!(validate_task_input("a\nb").unwrap().as_str(), "ab"); // \n
        assert_eq!(validate_task_input("a\rb").unwrap().as_str(), "ab"); // \r
        assert_eq!(validate_task_input("a\0b").unwrap().as_str(), "ab"); // NUL
        assert_eq!(validate_task_input("a\x1bb").unwrap().as_str(), "ab"); // ESC
    }

    #[test]
    fn validate_task_input_rejects_8kb_plus_one() {
        // Exact boundary + 1 must fail; the exact boundary passes (covered above).
        let over = "x".repeat(MAX_TASK_BYTES + 1);
        let result = validate_task_input(&over);
        assert!(matches!(result, Err(DispatchError::InvalidInput(_))));
        if let Err(DispatchError::InvalidInput(msg)) = result {
            assert!(msg.contains("8 KB"), "error should mention limit: {msg}");
        }
    }

    // ── dispatch_routes_authed (Phase 3 exit criterion) ───────────────────────
    //
    // Integration tests: every POST /api/dispatch/* endpoint must return 401
    // when the Authorization header is absent.  Uses tower::ServiceExt::oneshot
    // to drive the router in-process without binding a TCP port.

    fn make_test_state() -> crate::server::AppState {
        use crate::config::{AgentSession, Config, TokenSource};
        use std::ffi::OsString;
        use std::path::PathBuf;
        crate::server::AppState::for_test(
            Config {
                port: 0,
                host_cmd: OsString::from("bash"),
                cwd: PathBuf::from("/tmp"),
                token: "test-bearer-abc".to_owned(),
                token_source: TokenSource::EnvVar,
                agent: AgentSession::default(),
                claude_agent_template: None,
                container_mode: crate::container::ContainerMode::Auto,
                dev_mode: false,
                max_context_prompts: 50,
                litellm: crate::config::LiteLLMConfig::default(),
                hermes_mcp: crate::config::HermesMcpConfig::default(),
                resume_session_id: None,
            },
            crate::container::DockerCapability::Unavailable,
        )
    }

    #[tokio::test]
    async fn dispatch_classify_rejects_unauthenticated() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;
        let app = dispatch_router().with_state(make_test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/api/dispatch/classify")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"task":"audit code"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn dispatch_execute_rejects_unauthenticated() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;
        let app = dispatch_router().with_state(make_test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/api/dispatch/execute")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"task":"refactor auth","agents":["engineer"],"mode":"solo","dry":true}"#,
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn dispatch_cancel_rejects_unauthenticated() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;
        let app = dispatch_router().with_state(make_test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/api/dispatch/cancel/SQD-ALPHA-01")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn dispatch_retry_rejects_unauthenticated() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;
        let app = dispatch_router().with_state(make_test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/api/dispatch/retry/SQD-ALPHA-01/engineer")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn dispatch_classify_accepts_valid_bearer() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;
        let app = dispatch_router().with_state(make_test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/api/dispatch/classify")
            .header("content-type", "application/json")
            .header("authorization", "Bearer test-bearer-abc")
            .body(Body::from(r#"{"task":"audit code"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // 200 confirms auth passed; classification returned.
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ── security_agent_scoped (Phase 3 exit criterion) ────────────────────────
    //
    // Property: every request that includes the Security domain agent MUST go
    // through EngagementScope synthesis + validation (HIGH H-7).  Verified at
    // the route layer: a valid authenticated execute request with agents=[security]
    // must succeed, confirming the scope path ran without error.

    #[tokio::test]
    async fn dispatch_execute_security_scope_enforced() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;
        let app = dispatch_router().with_state(make_test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/api/dispatch/execute")
            .header("content-type", "application/json")
            .header("authorization", "Bearer test-bearer-abc")
            // dry=true: no filesystem writes; mode=solo; agents=[security].
            .body(Body::from(
                r#"{"task":"audit security surface","agents":["security"],"mode":"solo","dry":true}"#,
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // 200 OK confirms EngagementScope.synthesise() + validate() succeeded (H-7).
        // Any scope failure would 403; any auth failure would 401.
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
