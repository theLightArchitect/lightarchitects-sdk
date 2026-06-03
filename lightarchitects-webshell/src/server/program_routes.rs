//! Program supervision routes — A2A supervisor visibility (webshell-a2a-supervisor-visibility).
//!
//! Manages a single in-flight "program" — an ordered sequence of build codenames
//! whose A2A lifecycle events are visible in the `Supervision` screen.
//!
//! # Routes
//!
//! * `GET  /api/program/status` — current program state (unauthenticated 404 if idle).
//! * `POST /api/program/start`  — start a new program run (202 + `{id}`).
//! * `POST /api/program/cancel` — cancel the running program (204).
//!
//! # Security
//!
//! All codenames are validated against `[a-zA-Z0-9-]` (max 50 chars each, max 10
//! codenames per request) before any state is mutated — prevents path traversal
//! and injection into `payload_summary` strings fed to the SSE stream.
//!
//! Synthetic [`A2aEnvelopeEvent`]s use `sanitize_for_prompt`-level truncation
//! (≤200 grapheme clusters) applied inside [`emit_envelope`].
//!
//! # Original `program_manifest_handler`
//!
//! `GET /api/program-manifest` — serves the alpha readiness program manifest as
//! JSON.  Reads `~/lightarchitects/soul/helix/program/alpha-readiness/program_manifest.yaml`,
//! parses it as a dynamic `serde_yaml::Value`, and returns it as JSON. Returns
//! `404` when the file is absent and `500` on YAML parse errors.

use std::{sync::Arc, time::Duration};

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use lightarchitects::agent::{
    ClaudeCliProvider, IndirectInjectionShield,
    plan_to_waves::{PlanBuildSpec, PlanToWaves, PlanToWavesError},
};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    auth,
    events::{
        WebEvent, hitl_relay,
        types::{A2aEnvelopeEvent, A2aEnvelopeType, EventSource, IronclawHitlEscalationEvent},
    },
    server::AppState,
};

// ── Codename validation ────────────────────────────────────────────────────

const MAX_CODENAMES: usize = 10;
const MAX_CODENAME_LEN: usize = 50;

/// Returns `true` iff the codename contains only `[a-zA-Z0-9-]` and fits the
/// length bounds.  Rejects empty strings and path traversal characters.
fn is_valid_codename(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_CODENAME_LEN
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

// ── State types ────────────────────────────────────────────────────────────

/// Lifecycle state of the current program run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramState {
    /// No program has been started (or one was started but not yet persisted).
    Idle,
    /// A program is actively running the codename sequence.
    Running,
    /// All codenames completed successfully.
    Completed,
    /// The operator cancelled the run via `POST /api/program/cancel`.
    Cancelled,
}

/// In-memory state for one program run.
#[derive(Debug)]
pub struct ProgramRun {
    /// Unique identifier for this run — returned by `POST /api/program/start`.
    pub id: Uuid,
    /// Ordered list of build codenames to process.
    pub codenames: Vec<String>,
    /// 0-based index into `codenames` for the currently active build.
    pub current_idx: usize,
    /// Current lifecycle state.
    pub state: ProgramState,
    /// Token used to signal cancellation to the background task.
    pub cancel_token: CancellationToken,
    /// Whether this run skips per-wave HITL gates (operator-enabled via G6).
    pub auto_mode: bool,
}

/// Thread-safe program run slot — at most one program runs at a time.
pub type ProgramRunSlot = Arc<Mutex<Option<ProgramRun>>>;

/// Shared execution context threaded through the [`run_program`] helper chain.
///
/// Groups the four values needed at every level so individual functions stay
/// within the 7-argument limit enforced by `clippy::too_many_arguments`.
struct RunCtx<'a> {
    auto_mode: bool,
    run_id: Uuid,
    slot: &'a ProgramRunSlot,
    store: &'a crate::events::GlobalEventStore,
    hitl_queue: &'a hitl_relay::HitlQueue,
    cancel_token: &'a CancellationToken,
}

/// Creates an empty program run slot.
pub fn program_run_slot() -> ProgramRunSlot {
    Arc::new(Mutex::new(None))
}

// ── HTTP types ─────────────────────────────────────────────────────────────

/// Response body for `GET /api/program/status`.
#[derive(Debug, Serialize)]
pub struct ProgramStatus {
    /// Unique run identifier, or `null` when no run has started.
    pub id: Option<Uuid>,
    /// All codenames in the program, in order.
    pub codenames: Vec<String>,
    /// The codename currently being processed, or `null` when idle/done.
    pub current: Option<String>,
    /// Current lifecycle state of the program.
    pub state: ProgramState,
}

/// Request body for `POST /api/program/start`.
#[derive(Debug, Deserialize)]
pub struct StartProgramRequest {
    /// Ordered list of build codenames to process (max [`MAX_CODENAMES`]).
    pub codenames: Vec<String>,
    /// When `true`, waves are dispatched without per-wave HITL confirmation.
    /// Requires the operator to have explicitly enabled Auto Mode in the UI
    /// (G6 gate — confirm-on-first-use, re-confirm after 1 h idle).
    /// Defaults to `false` for safe-by-default behaviour on older clients.
    #[serde(default)]
    pub auto_mode: bool,
}

/// A single task entry in a [`ProgramBuildSpec`] wave.
#[derive(Debug, Serialize)]
pub struct ProgramTaskSpec {
    /// Unique task id derived from `{codename}-wave{W}-task{T}`.
    pub id: String,
    /// Operator-legible task prompt with preamble (from [`PlanToWaves`]).
    pub prompt: String,
}

/// Per-codename wave/task matrix returned by `POST /api/program/plan`.
#[derive(Debug, Serialize)]
pub struct ProgramBuildSpec {
    /// Build codename this spec was generated for.
    pub codename: String,
    /// Wave list — outer vec = waves, inner vec = tasks per wave.
    pub waves: Vec<Vec<ProgramTaskSpec>>,
}

/// Request body for `POST /api/program/plan`.
#[derive(Debug, Deserialize)]
pub struct PlanProgramRequest {
    /// Ordered list of build codenames (max [`MAX_CODENAMES`]).
    pub codenames: Vec<String>,
}

/// Response body for `POST /api/program/plan`.
#[derive(Debug, Serialize)]
pub struct PlanProgramResponse {
    /// Wave/task matrix for each requested codename.
    pub builds: Vec<ProgramBuildSpec>,
    /// Non-fatal warnings (injection scan hits, empty phases, etc.).
    pub warnings: Vec<String>,
}

// ── Handlers ───────────────────────────────────────────────────────────────

/// `GET /api/program/status` — returns the current program state.
///
/// * `200` — program exists (idle, running, completed, or cancelled).
/// * `404` — no program has been started this session.
pub async fn program_status_handler(
    _: auth::AuthGuard,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let guard = state.program_run.lock().await;
    match guard.as_ref() {
        None => (StatusCode::NOT_FOUND, "no program started").into_response(),
        Some(run) => Json(ProgramStatus {
            id: Some(run.id),
            codenames: run.codenames.clone(),
            current: run.codenames.get(run.current_idx).cloned(),
            state: run.state.clone(),
        })
        .into_response(),
    }
}

/// `POST /api/program/start` — validates codenames and starts a new program run.
///
/// * `400` — empty codename list, invalid codename format, or too many codenames.
/// * `409` — a program is already running.
/// * `202` — program started; body: `{"id": "<uuid>"}`.
pub async fn start_program_handler(
    _: auth::AuthGuard,
    State(state): State<AppState>,
    Json(req): Json<StartProgramRequest>,
) -> impl IntoResponse {
    // Validate request.
    if req.codenames.is_empty() {
        return (StatusCode::BAD_REQUEST, "codenames must not be empty").into_response();
    }
    if req.codenames.len() > MAX_CODENAMES {
        return (
            StatusCode::BAD_REQUEST,
            format!("at most {MAX_CODENAMES} codenames per program"),
        )
            .into_response();
    }
    for cn in &req.codenames {
        if !is_valid_codename(cn) {
            return (
                StatusCode::BAD_REQUEST,
                format!("invalid codename '{cn}': only [a-zA-Z0-9-] allowed, max {MAX_CODENAME_LEN} chars"),
            )
                .into_response();
        }
    }

    let mut guard = state.program_run.lock().await;

    // Reject if already running.
    if let Some(run) = guard.as_ref() {
        if run.state == ProgramState::Running {
            return (StatusCode::CONFLICT, "a program is already running").into_response();
        }
    }

    let id = Uuid::new_v4();
    let cancel_token = CancellationToken::new();

    *guard = Some(ProgramRun {
        id,
        codenames: req.codenames.clone(),
        current_idx: 0,
        state: ProgramState::Running,
        cancel_token: cancel_token.clone(),
        auto_mode: req.auto_mode,
    });
    drop(guard); // release before spawning

    tokio::spawn(run_program(
        req.codenames,
        req.auto_mode,
        cancel_token,
        id,
        state.program_run.clone(),
        state.global_event_store.clone(),
        state.hitl_queue.clone(),
    ));

    (StatusCode::ACCEPTED, Json(serde_json::json!({ "id": id }))).into_response()
}

/// `POST /api/program/cancel` — cancels the running program.
///
/// * `204` — cancellation signal sent (or no program running — idempotent).
pub async fn cancel_program_handler(
    _: auth::AuthGuard,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let guard = state.program_run.lock().await;
    if let Some(run) = guard.as_ref() {
        if run.state == ProgramState::Running {
            run.cancel_token.cancel();
        }
    }
    StatusCode::NO_CONTENT
}

/// `POST /api/program/plan` — parse LASDLC plan files and emit a wave/task matrix.
///
/// Reads `~/.claude/plans/<codename>.md` for each requested codename, runs
/// [`PlanToWaves`] to produce operator-legible task prompts, and returns the
/// result without starting a program run.  Callers may use the response to
/// preview the generated plan or pass the `waves` directly to
/// `POST /api/program/start`.
///
/// * `400` — empty list, invalid codename format, or too many codenames.
/// * `404` — plan file not found for one of the requested codenames.
/// * `422` — plan file exists but fails LASDLC structural validation.
/// * `502` — canon gatekeeper provider error.
/// * `200` — wave/task matrix plus any non-fatal `warnings`.
#[allow(clippy::missing_errors_doc)]
pub async fn plan_program_handler(
    _: auth::AuthGuard,
    State(_state): State<AppState>,
    Json(req): Json<PlanProgramRequest>,
) -> impl IntoResponse {
    // Validate request.
    if req.codenames.is_empty() {
        return (StatusCode::BAD_REQUEST, "codenames must not be empty").into_response();
    }
    if req.codenames.len() > MAX_CODENAMES {
        return (
            StatusCode::BAD_REQUEST,
            format!("at most {MAX_CODENAMES} codenames per program"),
        )
            .into_response();
    }
    for cn in &req.codenames {
        if !is_valid_codename(cn) {
            return (
                StatusCode::BAD_REQUEST,
                format!(
                    "invalid codename '{cn}': only [a-zA-Z0-9-] allowed, max {MAX_CODENAME_LEN} chars"
                ),
            )
                .into_response();
        }
    }

    let plans_dir = match dirs_next::home_dir() {
        Some(h) => h.join(".claude/plans"),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "home directory unavailable",
            )
                .into_response();
        }
    };

    let shield = Arc::new(IndirectInjectionShield::new());
    let mut builds: Vec<ProgramBuildSpec> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for codename in &req.codenames {
        // Security: is_valid_codename() guarantees [a-zA-Z0-9-] only — no
        // path traversal possible.  The .md extension is hardcoded here.
        let plan_path = plans_dir.join(format!("{codename}.md"));

        let content = match tokio::fs::read_to_string(&plan_path).await {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return (
                    StatusCode::NOT_FOUND,
                    format!("plan not found for codename '{codename}'"),
                )
                    .into_response();
            }
            Err(_) => {
                return (StatusCode::BAD_GATEWAY, "plan file read error").into_response();
            }
        };

        let result = PlanToWaves::run(
            &content,
            codename,
            ClaudeCliProvider::default(),
            Arc::clone(&shield),
        )
        .await;

        let ptw_result = match result {
            Ok(r) => r,
            Err(PlanToWavesError::ParseError(_)) => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    format!("plan '{codename}' failed LASDLC structural validation"),
                )
                    .into_response();
            }
            Err(PlanToWavesError::GateError(_)) => {
                return (StatusCode::BAD_GATEWAY, "canon gatekeeper provider error")
                    .into_response();
            }
        };

        // Collect non-fatal gaps as warnings.
        warnings.extend(ptw_result.gaps);

        // Convert PlanBuildSpec → ProgramBuildSpec.
        builds.push(plan_build_spec_to_program(
            ptw_result
                .builds
                .into_iter()
                .next()
                .unwrap_or(PlanBuildSpec {
                    codename: codename.clone(),
                    waves: Vec::new(),
                }),
        ));
    }

    Json(PlanProgramResponse { builds, warnings }).into_response()
}

/// Convert [`PlanBuildSpec`] (library type) into [`ProgramBuildSpec`] (route type).
fn plan_build_spec_to_program(spec: PlanBuildSpec) -> ProgramBuildSpec {
    let waves = spec
        .waves
        .into_iter()
        .enumerate()
        .map(|(wave_idx, tasks)| {
            tasks
                .into_iter()
                .enumerate()
                .map(|(task_idx, prompt)| ProgramTaskSpec {
                    id: format!(
                        "{}-wave{}-task{}",
                        spec.codename,
                        wave_idx + 1,
                        task_idx + 1
                    ),
                    prompt,
                })
                .collect()
        })
        .collect();
    ProgramBuildSpec {
        codename: spec.codename,
        waves,
    }
}

// ── Background task ────────────────────────────────────────────────────────

/// Emits a single [`A2aEnvelopeEvent`] into the global event store.
fn emit_envelope(
    store: &crate::events::GlobalEventStore,
    codename: &str,
    task_id: &str,
    phase: u32,
    wave: u32,
    envelope_type: A2aEnvelopeType,
    summary: &str,
) {
    // Truncate to ≤200 grapheme clusters (mirrors sanitize_for_prompt CAT-6).
    let payload_summary = unicode_segmentation::UnicodeSegmentation::graphemes(summary, true)
        .take(200)
        .collect::<String>();

    store.push(
        EventSource::GateRunner {
            gate_id: format!("program-{codename}"),
        },
        WebEvent::A2aEnvelope(A2aEnvelopeEvent {
            codename: codename.to_owned(),
            task_id: task_id.to_owned(),
            phase,
            wave,
            envelope_type,
            payload_summary,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }),
    );
}

/// Sets the program run state to [`ProgramState::Cancelled`].
async fn set_cancelled(slot: &ProgramRunSlot) {
    let mut g = slot.lock().await;
    if let Some(run) = g.as_mut() {
        run.state = ProgramState::Cancelled;
    }
}

/// Background task: parses each LASDLC plan via [`PlanToWaves`], walks the
/// resulting wave/task matrix, and emits real A2A envelopes.
///
/// When `auto_mode` is `false` each wave is preceded by a HITL gate (see
/// [`gate_wave`]).  Cancellation or rejection terminates the run immediately.
///
/// # Security
/// `escalation_nonce` is embedded in the SSE payload for resolution but must
/// never appear in tracing macros or HTTP error bodies (CWE-209).
async fn run_program(
    codenames: Vec<String>,
    auto_mode: bool,
    cancel_token: CancellationToken,
    run_id: Uuid,
    slot: ProgramRunSlot,
    store: crate::events::GlobalEventStore,
    hitl_queue: hitl_relay::HitlQueue,
) {
    let Some(home) = dirs_next::home_dir() else {
        set_cancelled(&slot).await;
        return;
    };
    let plans_dir = home.join(".claude/plans");
    let shield = Arc::new(IndirectInjectionShield::new());
    let ctx = RunCtx {
        auto_mode,
        run_id,
        slot: &slot,
        store: &store,
        hitl_queue: &hitl_queue,
        cancel_token: &cancel_token,
    };

    for (idx, codename) in codenames.iter().enumerate() {
        {
            let mut g = slot.lock().await;
            if let Some(run) = g.as_mut() {
                run.current_idx = idx;
            }
        }
        if cancel_token.is_cancelled() {
            set_cancelled(&slot).await;
            return;
        }
        if process_codename(codename, &ctx, &plans_dir, &shield)
            .await
            .is_err()
        {
            return;
        }
    }

    let mut g = slot.lock().await;
    if let Some(run) = g.as_mut() {
        run.state = ProgramState::Completed;
    }
}

/// Read plan + run [`PlanToWaves`] + gate and dispatch each wave.
///
/// Returns `Ok(())` on success, `Err(())` when cancelled (slot already set to
/// [`ProgramState::Cancelled`]).
async fn process_codename(
    codename: &str,
    ctx: &RunCtx<'_>,
    plans_dir: &std::path::Path,
    shield: &Arc<IndirectInjectionShield>,
) -> Result<(), ()> {
    // is_valid_codename() already enforced in start_program_handler.
    let Ok(content) = tokio::fs::read_to_string(&plans_dir.join(format!("{codename}.md"))).await
    else {
        emit_envelope(
            ctx.store,
            codename,
            &format!("{codename}-wave0-task0"),
            0,
            0,
            A2aEnvelopeType::TaskEscalated,
            "plan file unavailable",
        );
        set_cancelled(ctx.slot).await;
        return Err(());
    };

    let Ok(ptw_result) = PlanToWaves::run(
        &content,
        codename,
        ClaudeCliProvider::default(),
        Arc::clone(shield),
    )
    .await
    else {
        emit_envelope(
            ctx.store,
            codename,
            &format!("{codename}-wave0-task0"),
            0,
            0,
            A2aEnvelopeType::TaskEscalated,
            "plan processing failed",
        );
        set_cancelled(ctx.slot).await;
        return Err(());
    };

    let build_spec = ptw_result
        .builds
        .into_iter()
        .next()
        .unwrap_or_else(|| PlanBuildSpec {
            codename: codename.to_owned(),
            waves: Vec::new(),
        });

    for (wave_idx, wave_tasks) in build_spec.waves.iter().enumerate() {
        let wave_num = u32::try_from(wave_idx + 1).unwrap_or(u32::MAX);
        if ctx.cancel_token.is_cancelled() {
            set_cancelled(ctx.slot).await;
            return Err(());
        }
        if !ctx.auto_mode {
            gate_wave(codename, wave_num, wave_idx, wave_tasks.len(), ctx).await?;
        }
        dispatch_wave(
            codename,
            wave_num,
            wave_tasks,
            ctx.cancel_token,
            ctx.slot,
            ctx.store,
        )
        .await?;
    }
    Ok(())
}

/// Park wave `wave_num` for operator HITL approval before dispatch.
///
/// Emits [`IronclawHitlEscalationEvent`] over SSE and awaits the decision.
/// Returns `Ok(())` if approved, `Err(())` on rejection or cancellation (slot
/// already set to [`ProgramState::Cancelled`]).
///
/// # Security
/// `escalation_nonce` must never appear in tracing macros or HTTP error bodies.
async fn gate_wave(
    codename: &str,
    wave_num: u32,
    wave_idx: usize,
    wave_tasks_len: usize,
    ctx: &RunCtx<'_>,
) -> Result<(), ()> {
    let gate_task_id = format!("{codename}-wave{wave_num}-gate");
    let (_call_id, escalation_nonce, resolve_rx) = hitl_relay::park(
        ctx.hitl_queue,
        ctx.run_id,
        gate_task_id.clone(),
        format!("Approve wave {wave_num} of '{codename}'?"),
        u32::try_from(wave_idx).unwrap_or(u32::MAX),
        1,
    );

    // Nonce goes in the SSE payload (transport plane) so the browser can send
    // it back on resolution — must never appear in logs or error bodies.
    ctx.store.push(
        EventSource::GateRunner {
            gate_id: format!("program-{codename}"),
        },
        WebEvent::IronclawHitlEscalation(IronclawHitlEscalationEvent {
            build_id: ctx.run_id,
            task_id: gate_task_id.clone(),
            decision_topic: format!("Wave {wave_num} dispatch — '{codename}'"),
            layer_failed: 4,
            escalation_question: format!(
                "Approve dispatch of wave {wave_num} ({wave_tasks_len} tasks) for build '{codename}'?"
            ),
            deadline: None,
            traceparent: None,
            nonce: escalation_nonce,
        }),
    );

    // Fail-closed: dropped channel or run cancellation = rejected.
    let approved = tokio::select! {
        res = resolve_rx => res.is_ok_and(|d| d.approved),
        () = ctx.cancel_token.cancelled() => false,
    };

    if approved {
        return Ok(());
    }

    emit_envelope(
        ctx.store,
        codename,
        &gate_task_id,
        0,
        wave_num,
        A2aEnvelopeType::TaskEscalated,
        "wave rejected or run cancelled",
    );
    set_cancelled(ctx.slot).await;
    Err(())
}

/// Emit `TaskStart` / `TaskComplete` for each task, then `WaveComplete`.
///
/// Returns `Err(())` if cancelled mid-wave (slot already set).
async fn dispatch_wave(
    codename: &str,
    wave_num: u32,
    wave_tasks: &[String],
    cancel_token: &CancellationToken,
    slot: &ProgramRunSlot,
    store: &crate::events::GlobalEventStore,
) -> Result<(), ()> {
    for (task_idx, task_prompt) in wave_tasks.iter().enumerate() {
        let task_num = task_idx + 1;
        let task_id = format!("{codename}-wave{wave_num}-task{task_num}");

        if cancel_token.is_cancelled() {
            set_cancelled(slot).await;
            return Err(());
        }

        emit_envelope(
            store,
            codename,
            &task_id,
            0,
            wave_num,
            A2aEnvelopeType::TaskStart,
            task_prompt,
        );

        // Interruptible pause simulating task hand-off latency.
        tokio::select! {
            () = tokio::time::sleep(Duration::from_millis(200)) => {}
            () = cancel_token.cancelled() => {
                emit_envelope(store, codename, &task_id, 0, wave_num, A2aEnvelopeType::TaskEscalated, "task cancelled");
                set_cancelled(slot).await;
                return Err(());
            }
        }

        emit_envelope(
            store,
            codename,
            &task_id,
            0,
            wave_num,
            A2aEnvelopeType::TaskComplete { success: true },
            &format!("task {task_num} dispatched"),
        );
    }

    emit_envelope(
        store,
        codename,
        &format!("{codename}-wave{wave_num}"),
        0,
        wave_num,
        A2aEnvelopeType::WaveComplete,
        &format!("wave {wave_num} complete ({} tasks)", wave_tasks.len()),
    );
    Ok(())
}

// ── Original manifest handler ──────────────────────────────────────────────

/// Serves the alpha program manifest as JSON.
///
/// * `200` — manifest parsed and returned as JSON.
/// * `404` — `program_manifest.yaml` not found on disk.
/// * `500` — YAML parse error.
pub async fn program_manifest_handler(
    _: auth::AuthGuard,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    let path = dirs_next::home_dir()
        .map(|h| h.join("lightarchitects/soul/helix/program/alpha-readiness/program_manifest.yaml"))
        .unwrap_or_default();

    match tokio::fs::read_to_string(&path).await {
        Ok(yaml) => match serde_yaml::from_str::<Value>(&yaml) {
            Ok(val) => Json(val).into_response(),
            Err(e) => {
                tracing::warn!("program_manifest parse error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "program manifest parse error",
                )
                    .into_response()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            (StatusCode::NOT_FOUND, "program_manifest.yaml not found").into_response()
        }
        Err(e) => {
            tracing::warn!("program_manifest read error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "program manifest read error",
            )
                .into_response()
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use std::{ffi::OsString, path::PathBuf};
    use tower::ServiceExt;

    use crate::{
        config::{Config, TokenSource},
        container::{ContainerMode, DockerCapability},
        server::AppState,
    };

    fn test_state() -> AppState {
        let config = Config {
            port: 0,
            host_cmd: OsString::from("bash"),
            cwd: PathBuf::from("/tmp"),
            token: "test-token".to_owned(),
            token_source: TokenSource::EnvVar,
            agent: crate::config::AgentSession::default(),
            claude_agent_template: None,
            container_mode: ContainerMode::Auto,
            dev_mode: false,
            max_context_prompts: 50,
            litellm: crate::config::LiteLLMConfig::default(),
            hermes_mcp: crate::config::HermesMcpConfig::default(),
        };
        AppState::for_test(config, DockerCapability::Unavailable)
    }

    fn auth_get(uri: &str) -> Request<Body> {
        Request::builder()
            .method("GET")
            .uri(uri)
            .header("authorization", "Bearer test-token")
            .body(Body::empty())
            .unwrap()
    }

    fn auth_post(uri: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("authorization", "Bearer test-token")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    }

    fn auth_post_empty(uri: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("authorization", "Bearer test-token")
            .body(Body::empty())
            .unwrap()
    }

    // ── status ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn status_returns_404_when_idle() {
        let app = crate::server::build_app(test_state());
        let resp = app.oneshot(auth_get("/api/program/status")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── start — validation ───────────────────────────────────────────────────

    #[tokio::test]
    async fn start_rejects_empty_codenames() {
        let app = crate::server::build_app(test_state());
        let resp = app
            .oneshot(auth_post(
                "/api/program/start",
                serde_json::json!({ "codenames": [] }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn start_rejects_too_many_codenames() {
        let codenames: Vec<String> = (0..=MAX_CODENAMES).map(|i| format!("build-{i}")).collect();
        let app = crate::server::build_app(test_state());
        let resp = app
            .oneshot(auth_post(
                "/api/program/start",
                serde_json::json!({ "codenames": codenames }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn start_rejects_codename_with_slash() {
        let app = crate::server::build_app(test_state());
        let resp = app
            .oneshot(auth_post(
                "/api/program/start",
                serde_json::json!({ "codenames": ["../../etc/passwd"] }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn start_rejects_codename_too_long() {
        let long: String = "a".repeat(MAX_CODENAME_LEN + 1);
        let app = crate::server::build_app(test_state());
        let resp = app
            .oneshot(auth_post(
                "/api/program/start",
                serde_json::json!({ "codenames": [long] }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn start_rejects_codename_with_spaces() {
        let app = crate::server::build_app(test_state());
        let resp = app
            .oneshot(auth_post(
                "/api/program/start",
                serde_json::json!({ "codenames": ["bad name"] }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── start — acceptance ───────────────────────────────────────────────────

    #[tokio::test]
    async fn start_returns_202_with_id() {
        let app = crate::server::build_app(test_state());
        let resp = app
            .oneshot(auth_post(
                "/api/program/start",
                serde_json::json!({ "codenames": ["webshell-a2a-supervisor-visibility"] }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["id"].is_string(), "response must contain uuid id");
    }

    #[tokio::test]
    async fn start_then_status_shows_running() {
        let state = test_state();
        let app = crate::server::build_app(state.clone());
        app.oneshot(auth_post(
            "/api/program/start",
            serde_json::json!({ "codenames": ["my-build"] }),
        ))
        .await
        .unwrap();

        let app2 = crate::server::build_app(state);
        let resp = app2.oneshot(auth_get("/api/program/status")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["state"], "running");
    }

    #[tokio::test]
    async fn start_conflict_when_already_running() {
        let state = test_state();
        let app = crate::server::build_app(state.clone());
        app.oneshot(auth_post(
            "/api/program/start",
            serde_json::json!({ "codenames": ["first-build"] }),
        ))
        .await
        .unwrap();

        let app2 = crate::server::build_app(state);
        let resp = app2
            .oneshot(auth_post(
                "/api/program/start",
                serde_json::json!({ "codenames": ["second-build"] }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    // ── cancel ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn cancel_returns_204_when_nothing_running() {
        let app = crate::server::build_app(test_state());
        let resp = app
            .oneshot(auth_post_empty("/api/program/cancel"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn cancel_after_start_returns_204() {
        let state = test_state();
        let app = crate::server::build_app(state.clone());
        app.oneshot(auth_post(
            "/api/program/start",
            serde_json::json!({ "codenames": ["my-build"] }),
        ))
        .await
        .unwrap();

        let app2 = crate::server::build_app(state);
        let resp = app2
            .oneshot(auth_post_empty("/api/program/cancel"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    // ── is_valid_codename unit tests ─────────────────────────────────────────

    #[test]
    fn valid_codenames_pass() {
        assert!(is_valid_codename("my-build"));
        assert!(is_valid_codename("Build123"));
        assert!(is_valid_codename("a"));
        assert!(is_valid_codename(&"x".repeat(MAX_CODENAME_LEN)));
    }

    #[test]
    fn invalid_codenames_fail() {
        assert!(!is_valid_codename(""));
        assert!(!is_valid_codename("has space"));
        assert!(!is_valid_codename("../../etc"));
        assert!(!is_valid_codename("has/slash"));
        assert!(!is_valid_codename("has.dot"));
        assert!(!is_valid_codename(&"x".repeat(MAX_CODENAME_LEN + 1)));
    }
}
