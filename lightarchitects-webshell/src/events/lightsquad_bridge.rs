//! Bridge between the webshell HTTP layer and the lightsquad autonomous build
//! engine.
//!
//! When `POST /api/builds` arrives with `mode = "autonomous"` the handler
//! calls [`spawn_autonomous_build`], which:
//!
//! 1. Translates [`TaskSpec`] → `lightsquad::Task` (adds branch name, empty
//!    context tiers — the worker fills those in at runtime).
//! 2. Constructs a [`Program`] from the waves.
//! 3. Spawns `Program::run(worker_fn)` as a detached Tokio task.
//! 4. Broadcasts [`WebEvent`] variants on every state change so the frontend
//!    `AutonomousRun` and `DecisionLog` components update in real time.
//!
//! The `worker_fn` is selected at compile time:
//!
//! - `#[cfg(test)]` — `mock_worker`: writes a file + git-commits. Zero external
//!   dependencies; deterministic. Used by all integration tests.
//! - `#[cfg(not(test))]` — `cli_worker`: spawns `lightarchitects --bare -p
//!   <prompt>` in the task worktree. Requires the `lightarchitects` binary on
//!   `PATH`.

use std::path::{Path, PathBuf};

use tokio::sync::broadcast;
use tracing::{debug, warn};
use uuid::Uuid;

use lightarchitects::{
    agent::{CodingProviderError, OllamaCloudCodingProvider},
    ayin::{
        TraceOutcome, emit_span_background,
        span::{Actor, TraceContext},
        spawn_with_span_context,
    },
    lightsquad::{
        program::{Program, ProgramConfig},
        types::Task,
        wave_dispatcher::WorkerSpec,
    },
};

/// Emit a `LightSquad` AYIN span to disk (fire-and-forget) and return its ID.
///
/// The returned [`Uuid`] is the span's own disk-file ID — pass it as
/// `parent_id` to child spans so the AYIN Lineage Circuit can reconstruct
/// the full build tree from on-disk files.
pub(crate) fn emit_squad_span(
    action: &str,
    metadata: serde_json::Value,
    outcome: TraceOutcome,
    parent_id: Option<Uuid>,
    build_id: Uuid,
) -> Uuid {
    let ctx = TraceContext::new(Actor::new("lightsquad"), action)
        .outcome(outcome)
        .metadata(metadata)
        .session_id(&build_id.to_string());
    let span_id = ctx.span_id();
    let ctx = if let Some(pid) = parent_id {
        ctx.parent(pid)
    } else {
        ctx
    };
    emit_span_background(ctx);
    span_id
}

use crate::events::{
    WebEventV2,
    builds_handler::TaskSpec,
    decisions::DecisionsWriter,
    types::{ConductorTickEvent, MergeAgentStatusEvent, WebEvent, WorkerSlotGaugeEvent},
};

// ── Public entry point ────────────────────────────────────────────────────────

/// Context passed into the background task.
pub struct BridgeContext {
    /// Unique ID of the build session.
    pub build_id: Uuid,
    /// Human-readable build codename (used for branch prefixes).
    pub codename: String,
    /// Absolute path to the repository root.
    pub repo_root: PathBuf,
    /// Absolute path where per-task git worktrees are created.
    pub worktree_root: PathBuf,
    /// Name of the feature branch accumulating merged task results.
    pub feat_branch: String,
    /// Wave-partitioned task specs (waves are sequential; tasks within a wave run in parallel).
    pub waves: Vec<Vec<TaskSpec>>,
    /// SSE broadcast channel — every state change emits a [`WebEventV2`].
    pub event_tx: broadcast::Sender<WebEventV2>,
    /// HMAC-chained decision log writer for this build.
    pub decisions_writer: DecisionsWriter,
    /// When `true`, use the hermetic mock worker instead of the real CLI.
    /// Set from [`AppState::mock_workers`] — always `false` in production.
    pub mock_workers: bool,
    /// Shared HITL escalation queue — workers park here when `UserEscalation` fires.
    pub hitl_queue: crate::events::hitl_relay::HitlQueue,
}

/// Spawn the autonomous build as a detached Tokio task.
///
/// Returns a `JoinHandle` that the caller may store in `AppState` to cancel
/// or await the build. The handle resolves when all waves complete or the
/// first error halts execution.
pub fn spawn_autonomous_build(ctx: BridgeContext) -> tokio::task::JoinHandle<()> {
    spawn_with_span_context(run_build(ctx))
}

// ── Internal orchestration ────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
async fn run_build(ctx: BridgeContext) {
    let BridgeContext {
        build_id,
        codename,
        repo_root,
        worktree_root,
        feat_branch,
        waves,
        event_tx,
        decisions_writer,
        mock_workers,
        hitl_queue,
    } = ctx;

    // Translate TaskSpec → lightsquad::Task
    let mut ls_waves: Vec<Vec<Task>> = waves
        .into_iter()
        .map(|wave| {
            wave.into_iter()
                .map(|spec| Task {
                    branch: format!("task/{codename}/{}", spec.id),
                    depends_on: spec.depends_on,
                    file_ownership: spec.file_ownership,
                    concurrency_safe: spec.concurrency_safe,
                    context_tiers: vec![],
                    prompt: spec.prompt,
                    id: spec.id,
                })
                .collect()
        })
        .collect();

    // T4: critical-path scheduling — dispatch highest next-wave fan-out tasks first to
    // minimise inter-wave blocking time.
    sort_waves_by_fan_out(&mut ls_waves);
    let wave_count = ls_waves.len();

    // AYIN Phase 3: user.message root anchors the entire conductor turn in the
    // Lineage Circuit (gold ring).  All squad spans hang below it.
    let user_turn_span_id = {
        let ctx = TraceContext::new(Actor::new("user"), "user.message")
            .outcome(TraceOutcome::Continue)
            .metadata(serde_json::json!({"codename": &codename, "wave_count": wave_count}))
            .session_id(&build_id.to_string());
        let span_id = ctx.span_id();
        emit_span_background(ctx);
        span_id
    };

    // AYIN: build-root span parented to the user.message turn span.
    debug!(codename = %codename, wave_count, "squad: build started");
    let build_span_id = emit_squad_span(
        "squad.build.started",
        serde_json::json!({"codename": &codename, "wave_count": wave_count}),
        TraceOutcome::Continue,
        Some(user_turn_span_id),
        build_id,
    );

    // Ensure the feat branch exists — `MergeAgent::merge_task_to_feat` requires
    // `git checkout <feat_branch>` to succeed before the first task merge.
    // `-B` creates the branch if absent, resets it if already present (idempotent).
    if let Err(e) = tokio::process::Command::new("git")
        .args(["checkout", "-B", &feat_branch])
        .current_dir(&repo_root)
        .status()
        .await
    {
        let _ = decisions_writer.append(
            "L4",
            &format!("ESCALATION: could not create feat branch '{feat_branch}' — {e}"),
            Some("canon://agents-playbook#§15"),
        );
        return;
    }

    let config = ProgramConfig {
        codename: codename.clone(),
        repo_root,
        worktree_root,
        feat_branch,
        waves: ls_waves,
    };

    let program = Program::new(config);
    let dw = decisions_writer.clone();
    let tx = event_tx.clone();

    // Clone for move into worker_fn
    let tx_slot = tx.clone();
    let tx_merge = tx.clone();
    let dw_worker = dw.clone();

    let worker_fn = make_worker(
        build_id,
        build_span_id,
        tx_slot,
        tx_merge,
        dw_worker,
        mock_workers,
        hitl_queue,
    );

    // L1 decision: build started
    let _ = dw.append(
        "L1",
        &format!("Autonomous build '{codename}' started"),
        Some("canon://agents-playbook#§15"),
    );

    let outcome = match program.run(worker_fn).await {
        Ok(summary) => {
            let _ = dw.append(
                "L1",
                &format!(
                    "Build complete: {} succeeded, {} failed",
                    summary.succeeded, summary.failed
                ),
                None,
            );
            debug!(
                codename = %codename,
                succeeded = summary.succeeded,
                failed = summary.failed,
                "squad: build completed"
            );
            emit_squad_span(
                "squad.build.completed",
                serde_json::json!({
                    "codename": &codename,
                    "succeeded": summary.succeeded,
                    "failed": summary.failed,
                }),
                TraceOutcome::Continue,
                Some(build_span_id),
                build_id,
            );
            // Final conductor tick — queue_depth=0 signals completion
            let _ = tx.send(WebEventV2::from_event(
                WebEvent::ConductorTick(ConductorTickEvent {
                    build_id: build_id.to_string(),
                    tick_seq: u64::MAX,
                    queue_depth: 0,
                    active_workers: 0,
                }),
                Some(build_id),
            ));
            TraceOutcome::Continue
        }
        Err(e) => {
            let _ = dw.append(
                "L4",
                &format!("ESCALATION: build halted — {e:?}"),
                Some("canon://agents-playbook#§15"),
            );
            warn!(codename = %codename, error = %e, "squad: build failed");
            emit_squad_span(
                "squad.build.failed",
                serde_json::json!({"codename": &codename, "error": e.to_string()}),
                TraceOutcome::Error(e.to_string()),
                Some(build_span_id),
                build_id,
            );
            TraceOutcome::Error(e.to_string())
        }
    };

    // AYIN Phase 3: assistant.response leaf closes the turn (gold ring at outer
    // radius, paired with the user.message root above).
    {
        let ctx = TraceContext::new(Actor::new("claude"), "assistant.response")
            .outcome(outcome)
            .metadata(serde_json::json!({"codename": &codename}))
            .session_id(&build_id.to_string())
            .parent(user_turn_span_id);
        emit_span_background(ctx);
    }
}

// ── Worker selection ──────────────────────────────────────────────────────────

/// Build the per-task worker closure.
///
/// `use_mock = true` activates the hermetic mock path (write file + git commit)
/// instead of spawning the real `lightarchitects --bare` CLI. The flag is
/// captured by value and applies to every task in the closure's lifetime.
/// Captured context for a single worker invocation.
struct WorkerCtx {
    build_id: Uuid,
    /// AYIN disk-file ID of this task's `squad.task.started` span.
    /// Sub-spans (`ollama_call`, `cargo_check`, escalation) set this as parent.
    task_span_id: Uuid,
    wave_index: usize,
    tx_slot: broadcast::Sender<WebEventV2>,
    tx_merge: broadcast::Sender<WebEventV2>,
    dw: DecisionsWriter,
    hitl_queue: crate::events::hitl_relay::HitlQueue,
    use_mock: bool,
    depends_on: Vec<String>,
    /// Worktree-relative paths the worker may write. Empty = no restriction
    /// (legacy / interactive). Non-empty = strict subset check post-task.
    file_ownership: Vec<String>,
}

pub(crate) fn make_worker(
    build_id: Uuid,
    build_span_id: Uuid,
    tx_slot: broadcast::Sender<WebEventV2>,
    tx_merge: broadcast::Sender<WebEventV2>,
    dw: DecisionsWriter,
    use_mock: bool,
    hitl_queue: crate::events::hitl_relay::HitlQueue,
) -> impl Fn(
    WorkerSpec,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>
+ Clone
+ Send
+ 'static {
    move |spec: WorkerSpec| {
        let task_id = spec.task.id.clone();
        let wt = spec.worktree_path.clone();
        let prompt = spec.task.prompt.clone();
        let wave_index = spec.wave_index;
        let depends_on = spec.task.depends_on.clone();
        let file_ownership = spec.task.file_ownership.clone();
        // AYIN: per-task span; parent = root build span.
        debug!(task_id = %task_id, wave_index, "squad: task started");
        let task_span_id = emit_squad_span(
            "squad.task.started",
            serde_json::json!({"task_id": &task_id, "wave_index": wave_index}),
            TraceOutcome::Continue,
            Some(build_span_id),
            build_id,
        );
        let ctx = WorkerCtx {
            build_id,
            task_span_id,
            wave_index,
            tx_slot: tx_slot.clone(),
            tx_merge: tx_merge.clone(),
            dw: dw.clone(),
            hitl_queue: hitl_queue.clone(),
            use_mock,
            depends_on,
            file_ownership,
        };
        Box::pin(worker_body(task_id, wt, prompt, ctx))
    }
}

/// Hydrate the task prompt with existing source context for Wave N+1 workers.
fn build_hydrated_prompt(prompt: &str, context: &str) -> String {
    if context.is_empty() {
        format!(
            "## Implementation guidance\n\n\
             Before writing code, verify you are ≥95% confident in the \
             correct implementation. If you are not, state what information \
             is missing in a `## Questions` section before the code blocks.\n\n\
             ---\n\n{prompt}"
        )
    } else {
        format!(
            "## Existing code context\n\n\
             The following files are already committed in this repository. \
             You MUST use their exact APIs — do not invent new enum \
             variants, function signatures, or struct fields that are not \
             shown below. The file names shown are the paths relative to \
             the repository root.\n\n\
             {context}\
             ## Implementation guidance\n\n\
             1. Read every file above before writing any code.\n\
             2. Identify each type, function, and error variant your task \
                will call.\n\
             3. If you are not ≥95% confident in the implementation, \
                add a `## Questions` section listing what is unclear — \
                the operator will resolve it before the next attempt.\n\
             4. Write only the file(s) your task specifies. Do not modify \
                the files shown in the context above unless the task \
                explicitly asks you to.\n\n\
             ---\n\n{prompt}"
        )
    }
}

/// Maximum fix-attempt iterations before escalating to HITL.
fn max_fix_attempts() -> u32 {
    std::env::var("LIGHTSQUAD_MAX_FIX_ATTEMPTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3)
}

/// Select the Ollama model for a task based on prompt complexity and dependency depth.
///
/// - Has dependencies AND long prompt (>800 chars) → heavyweight reasoning model
/// - Has dependencies OR medium prompt (400-800) → balanced model
/// - Short independent prompt → lightweight fast model
fn select_model(prompt: &str, depends_on: &[String]) -> &'static str {
    match (!depends_on.is_empty(), prompt.len()) {
        (true, c) if c > 800 => "qwen3-coder:480b-cloud",
        (true, _) | (_, 400..) => "kimi-k2.5:cloud",
        _ => "gemma4:31b-cloud",
    }
}

/// Run `cargo check --message-format=json` in `worktree` and return rendered
/// compile errors that are genuine bugs — filtering out expected wave-isolation
/// noise (E0583 / E0463 / cascading E0412, E0425, E0433).
///
/// Reads **stdout**: `--message-format=json` always emits diagnostics there;
/// stderr carries only progress lines.
async fn cargo_check_errors(worktree: &Path) -> Option<String> {
    let output = tokio::process::Command::new("cargo")
        .args(["check", "--message-format=json"])
        .current_dir(worktree)
        .output()
        .await
        .ok()?;
    if output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Pass 1: collect module names from E0583 so cascading name-resolution
    // errors on those same names can be suppressed in Pass 2.
    let absent: std::collections::HashSet<String> = stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["reason"] == "compiler-message")
        .filter(|v| v["message"]["code"]["code"] == "E0583")
        .filter_map(|v| {
            Some(
                v["message"]["message"]
                    .as_str()?
                    .split('`')
                    .nth(1)?
                    .to_owned(),
            )
        })
        .collect();

    // Pass 2: genuine errors only.
    // "aborting due to previous error(s)" has level="error" but code=null —
    // the `.is_object()` guard excludes it from the fix prompt.
    let errors: Vec<String> = stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["reason"] == "compiler-message")
        .filter(|v| v["message"]["code"].is_object())
        .filter(|v| v["message"]["level"] == "error")
        .filter(|v| {
            let code = v["message"]["code"]["code"].as_str().unwrap_or("");
            if matches!(code, "E0583" | "E0463") {
                return false;
            }
            if matches!(code, "E0412" | "E0425" | "E0433") {
                let msg = v["message"]["message"].as_str().unwrap_or("");
                return !absent.iter().any(|m| msg.contains(m.as_str()));
            }
            true
        })
        .filter_map(|v| v["message"]["rendered"].as_str().map(str::to_owned))
        .collect();

    if errors.is_empty() {
        None
    } else {
        Some(errors.join(""))
    }
}

/// Return the current HEAD commit SHA in `worktree`, or empty string on failure.
async fn git_head(worktree: &Path) -> Option<String> {
    let out = tokio::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(worktree)
        .output()
        .await
        .ok()?;
    if out.status.success() {
        let sha = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if sha.is_empty() { None } else { Some(sha) }
    } else {
        None
    }
}

/// Build a re-prompt showing the diff of what changed and the compiler errors.
///
/// When `pre_sha` is non-empty, runs `git diff pre_sha..HEAD -- src/` to show
/// only what changed rather than the entire file state on every retry. Falls
/// back to the full context view when the diff is empty or SHA is unavailable.
async fn build_fix_prompt(
    original_prompt: &str,
    errors: &str,
    worktree: &Path,
    pre_sha: &str,
) -> String {
    let changes = if pre_sha.is_empty() {
        read_src_context(worktree).await
    } else {
        let diff_out = tokio::process::Command::new("git")
            .args(["diff", pre_sha, "--unified=3", "--", "src/"])
            .current_dir(worktree)
            .output()
            .await
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        if diff_out.trim().is_empty() {
            read_src_context(worktree).await
        } else {
            format!("```diff\n{diff_out}```\n\n")
        }
    };
    format!(
        "The previous attempt produced compile errors. Fix them.\n\n\
         ## Compiler errors\n\n```\n{errors}```\n\n\
         ## What you changed\n\n{changes}\
         ## Your task (unchanged)\n\n{original_prompt}"
    )
}

/// Escalate a task to the HITL queue and wait for operator decision.
///
/// Returns `Ok(())` if the operator approves, `Err(String)` on rejection or
/// dropped channel.
#[allow(clippy::too_many_arguments)]
async fn escalate_to_hitl(
    task_id: &str,
    reason: String,
    build_id: Uuid,
    task_span_id: Uuid,
    wave_index: usize,
    hitl_queue: &crate::events::hitl_relay::HitlQueue,
    tx_slot: &broadcast::Sender<WebEventV2>,
    dw: &DecisionsWriter,
) -> Result<(), String> {
    warn!(task_id = %task_id, reason = %reason, wave_index, "squad: task escalated to HITL");
    emit_squad_span(
        "squad.task.escalation",
        serde_json::json!({"task_id": task_id, "reason": &reason}),
        TraceOutcome::Block,
        Some(task_span_id),
        build_id,
    );
    let (call_id, resolve_rx) = crate::events::hitl_relay::park(
        hitl_queue,
        build_id,
        task_id.to_owned(),
        reason.clone(),
        u32::try_from(wave_index).unwrap_or(0),
        1,
    );
    let _ = tx_slot.send(WebEventV2::from_event(
        WebEvent::Escalation(crate::events::types::EscalationEvent {
            build_id: build_id.to_string(),
            wave_index: u32::try_from(wave_index).unwrap_or(0),
            worker_slot: 1,
            reason: reason.clone(),
            call_id: call_id.to_string(),
        }),
        Some(build_id),
    ));
    let _ = dw.append(
        "L4",
        &format!("ESCALATION task '{task_id}': {reason} — awaiting operator (call_id={call_id})"),
        Some("canon://security-guardrails#§G-DENY"),
    );
    match resolve_rx.await {
        Ok(decision) if decision.approved => {
            let _ = dw.append(
                "L4",
                &format!(
                    "APPROVED by operator (call_id={call_id}): {}",
                    decision.operator_reason.as_deref().unwrap_or("no reason")
                ),
                None,
            );
            Ok(())
        }
        Ok(_) | Err(_) => Err(format!(
            "task '{task_id}' rejected by operator or HITL dropped (call_id={call_id})"
        )),
    }
}

#[allow(clippy::too_many_lines)]
async fn worker_body(
    task_id: String,
    wt: PathBuf,
    prompt: String,
    ctx: WorkerCtx,
) -> Result<(), String> {
    let WorkerCtx {
        build_id,
        task_span_id,
        wave_index,
        tx_slot,
        tx_merge,
        dw,
        hitl_queue,
        use_mock,
        depends_on,
        file_ownership,
    } = ctx;

    let _ = tx_slot.send(WebEventV2::from_event(
        WebEvent::WorkerSlotGauge(WorkerSlotGaugeEvent {
            build_id: build_id.to_string(),
            wave_index: u32::try_from(wave_index).unwrap_or(0),
            active: 1,
            capacity: 7,
        }),
        Some(build_id),
    ));

    if use_mock {
        // Hermetic mock: write artifact + git commit (no CLI needed).
        tokio::fs::write(wt.join(format!("{task_id}.txt")), task_id.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        git_add_commit(&wt, &task_id)?;
        let _ = dw.append(
            "L2",
            &format!("Task '{task_id}' complete (mock worker)"),
            Some("canon://builders-cookbook#§66"),
        );
    } else {
        // Ollama Cloud coding worker — structured output + 4-gate validation.
        // Model is selected per-task based on prompt complexity and dependency depth.
        let model = select_model(&prompt, &depends_on);
        let auth_token = std::env::var("OLLAMA_API_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| secrecy::SecretString::new(s.into()));
        let provider = OllamaCloudCodingProvider::with_model(model, auth_token);
        let context = read_src_context(&wt).await;
        let hydrated_prompt = build_hydrated_prompt(&prompt, &context);

        // FixAgent loop: retry up to max_fix_attempts() on compile errors or no-change output.
        let max_attempts = max_fix_attempts();
        let mut current_prompt = hydrated_prompt.clone();
        let mut attempt = 0u32;

        loop {
            // Snapshot HEAD before execute so fix prompt shows a minimal diff.
            let pre_sha = git_head(&wt).await.unwrap_or_default();
            match provider.execute_task(&task_id, &current_prompt, &wt).await {
                Ok(outcome) => {
                    // Post-task ownership gate (agents-playbook §15.3.13 PoT-1):
                    // when file_ownership is declared, every file the worker
                    // wrote must be in the declared set. Out-of-scope writes
                    // escalate to HITL — never silently merged.
                    if !file_ownership.is_empty() {
                        if let Some(out_of_scope) =
                            files_out_of_scope(&outcome.files_written, &wt, &file_ownership)
                        {
                            let reason = format!(
                                "task '{task_id}' wrote files outside declared ownership: \
                                 {out_of_scope:?}; declared: {file_ownership:?}"
                            );
                            return escalate_to_hitl(
                                &task_id,
                                reason,
                                build_id,
                                task_span_id,
                                wave_index,
                                &hitl_queue,
                                &tx_slot,
                                &dw,
                            )
                            .await;
                        }
                    }
                    // Check for genuine compile errors (ignoring expected E0583/E0463).
                    if let Some(errors) = cargo_check_errors(&wt).await {
                        attempt += 1;
                        let _ = dw.append(
                            "L3",
                            &format!(
                                "Task '{task_id}' attempt {attempt}/{max_attempts}: \
                                 compile errors detected, retrying"
                            ),
                            Some("canon://builders-cookbook#§66"),
                        );
                        if attempt >= max_attempts {
                            let reason = format!(
                                "task '{task_id}' failed to compile after {max_attempts} \
                                 fix attempts:\n{errors}"
                            );
                            return escalate_to_hitl(
                                &task_id,
                                reason,
                                build_id,
                                task_span_id,
                                wave_index,
                                &hitl_queue,
                                &tx_slot,
                                &dw,
                            )
                            .await;
                        }
                        current_prompt = build_fix_prompt(&prompt, &errors, &wt, &pre_sha).await;
                        continue;
                    }
                    let _ = dw.append(
                        "L2",
                        &format!("Task '{task_id}' completed by OllamaCloud (model={model})"),
                        Some("canon://builders-cookbook#§66"),
                    );
                    break;
                }
                Err(CodingProviderError::NoChanges) => {
                    // LLM produced byte-identical output — count as an attempt.
                    attempt += 1;
                    let _ = dw.append(
                        "L3",
                        &format!(
                            "Task '{task_id}' attempt {attempt}/{max_attempts}: \
                             LLM produced no file changes, retrying"
                        ),
                        Some("canon://builders-cookbook#§66"),
                    );
                    if attempt >= max_attempts {
                        let reason = format!(
                            "task '{task_id}' produced no file changes after {max_attempts} attempts"
                        );
                        return escalate_to_hitl(
                            &task_id,
                            reason,
                            build_id,
                            task_span_id,
                            wave_index,
                            &hitl_queue,
                            &tx_slot,
                            &dw,
                        )
                        .await;
                    }
                    current_prompt = build_fix_prompt(
                        &prompt,
                        "The LLM produced no file changes (output was byte-identical \
                         to the current state). You must write at least one file.",
                        &wt,
                        &pre_sha,
                    )
                    .await;
                }
                Err(e) => {
                    // Security violations, validation failures → HITL.
                    return escalate_to_hitl(
                        &task_id,
                        e.to_string(),
                        build_id,
                        task_span_id,
                        wave_index,
                        &hitl_queue,
                        &tx_slot,
                        &dw,
                    )
                    .await;
                }
            }
        }
    }

    debug!(task_id = %task_id, wave_index, "squad: task completed");
    emit_squad_span(
        "squad.task.completed",
        serde_json::json!({"task_id": &task_id, "wave_index": wave_index}),
        TraceOutcome::Continue,
        Some(task_span_id),
        build_id,
    );

    let _ = tx_merge.send(WebEventV2::from_event(
        WebEvent::MergeAgentStatus(MergeAgentStatusEvent {
            build_id: build_id.to_string(),
            wave_index: u32::try_from(wave_index).unwrap_or(0),
            phase: "merged".to_owned(),
            commit_sha: None,
        }),
        Some(build_id),
    ));

    Ok(())
}

/// Collect all existing source files from `worktree` so downstream workers see
/// the exact API surface produced by prior-wave workers.
///
/// Returned string is empty when no prior code exists (Wave 1). When non-empty,
/// it is structured as a Markdown context block ready for prompt injection.
///
/// Covers:
/// - `Cargo.toml` — dependency and crate metadata
/// - `src/*.rs`  — all Rust source files (sorted; `lib.rs`/`main.rs` first)
/// - `tests/*.rs` — integration test files (so the worker knows expected APIs)
async fn read_src_context(worktree: &Path) -> String {
    read_src_context_with_budget(worktree, DEFAULT_CONTEXT_TOKEN_BUDGET).await
}

/// Default per-task source-context token budget.
///
/// At ≈4 bytes/token (the same conversion `OllamaCloudCodingProvider::execute_task`
/// uses), 8 000 tokens ≈ 32 KB of inlined source code — generous for Wave N+1
/// context while leaving room for the prompt, the system prompt, and the
/// response window even in 128 K-context models.
const DEFAULT_CONTEXT_TOKEN_BUDGET: usize = 8_000;

/// Budget-aware context assembler — files are packed greedily in priority order
/// (`Cargo.toml` > `src/lib.rs`/`src/main.rs` > `tests/*.rs` > others) until the
/// token budget is exhausted. Skipped files trigger a `… N more file(s) omitted`
/// marker so the model knows context was truncated rather than empty.
async fn read_src_context_with_budget(worktree: &Path, budget_tokens: usize) -> String {
    use std::fmt::Write as _;
    let mut sections: Vec<(String, String, String)> = Vec::new(); // (sort_key, path, content)

    // Cargo.toml — crate metadata + dependencies.
    let cargo = worktree.join("Cargo.toml");
    if let Ok(content) = tokio::fs::read_to_string(&cargo).await {
        if !content.is_empty() {
            sections.push(("0-Cargo.toml".to_owned(), "Cargo.toml".to_owned(), content));
        }
    }

    // src/ — library / binary source (lib.rs and main.rs sorted first).
    collect_rs_files(worktree, "src", &mut sections).await;

    // tests/ — integration tests (reveal expected public API signatures).
    collect_rs_files(worktree, "tests", &mut sections).await;

    if sections.is_empty() {
        return String::new();
    }

    sections.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = String::new();
    let mut tokens_used: usize = 0;
    let mut skipped: usize = 0;

    for (_, display_path, content) in &sections {
        let lang = if Path::new(display_path)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
        {
            "toml"
        } else {
            "rust"
        };
        // Markdown overhead: `### \`{path}\`\n` + ```` ```{lang}\n…``` ```` + `\n\n` ≈ 16 chars.
        let section_chars = display_path.len() + content.len() + lang.len() + 16;
        let section_tokens = section_chars / 4;
        if tokens_used.saturating_add(section_tokens) > budget_tokens {
            skipped += 1;
            continue;
        }
        let _ = write!(out, "### `{display_path}`\n```{lang}\n{content}```\n\n");
        tokens_used = tokens_used.saturating_add(section_tokens);
    }

    if skipped > 0 {
        let _ = write!(
            out,
            "### _<{skipped} more file(s) omitted to fit context budget>_\n\n"
        );
    }

    out
}

/// Bounded parallelism for file reads inside [`collect_rs_files`]. Picked to
/// be high enough to saturate typical SSD random-read throughput on a
/// medium-sized codebase walk while staying well below default `ulimit -n`
/// (256 on macOS by default). See `lightarchitects-sdk/CLAUDE.md` —
/// "Concurrency primitives".
const COLLECT_RS_READ_PARALLELISM: usize = 8;

/// Walk `<worktree>/<subdir>` recursively and append every `.rs` file to `out`.
///
/// Two phases:
///
/// 1. **Directory walk** (sequential — cheap, just `read_dir` calls)
///    builds a list of candidate file paths, skipping `target/` and hidden
///    directories.
/// 2. **Content reads** (parallel — bounded by [`COLLECT_RS_READ_PARALLELISM`])
///    fans out via `futures::stream::buffer_unordered`. This is the
///    bandwidth bottleneck for large source trees; parallelizing it
///    typically gives a 5–10× speedup over the sequential read loop the
///    earlier implementation used.
///
/// Ordering: read completion order is non-deterministic, but the caller
/// (`read_src_context`) sorts by the priority-prefixed key, so output
/// ordering is preserved. `lib.rs` / `main.rs` still sort to the top within
/// their subdir.
async fn collect_rs_files(worktree: &Path, subdir: &str, out: &mut Vec<(String, String, String)>) {
    use futures_util::stream::{self, StreamExt};

    // ── Phase 1: walk dirs, collect candidate paths ──────────────────
    // Tuple: (absolute path, repo-relative path, priority key as owned String).
    // We deliberately own priority instead of borrowing `&'static str` —
    // the latter trips a HRTB-inference cascade through the `make_worker`
    // closure's `FnOnce` bounds.
    let mut candidates: Vec<(PathBuf, String, String)> = Vec::new();
    let mut dirs = vec![worktree.join(subdir)];
    while let Some(dir) = dirs.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if path.is_dir() {
                if !name.starts_with('.') && name != "target" {
                    dirs.push(path);
                }
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("rs"))
            {
                let rel = path
                    .strip_prefix(worktree)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                // lib.rs / main.rs float to the top within their subdir.
                let priority = if name == "lib.rs" || name == "main.rs" {
                    "0".to_owned()
                } else {
                    "1".to_owned()
                };
                candidates.push((path, rel, priority));
            }
        }
    }

    // ── Phase 2: read file contents in parallel (bounded) ─────────────
    let mut results: Vec<(String, String, String)> = stream::iter(candidates)
        .map(|(path, rel, priority)| async move {
            tokio::fs::read_to_string(&path)
                .await
                .ok()
                .filter(|c| !c.is_empty())
                .map(|content| (format!("1-{priority}-{rel}"), rel, content))
        })
        .buffer_unordered(COLLECT_RS_READ_PARALLELISM)
        .filter_map(|opt| async move { opt })
        .collect()
        .await;

    out.append(&mut results);
}

fn git_add_commit(worktree: &PathBuf, _task_id: &str) -> Result<(), String> {
    let run = |args: &'static [&'static str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(worktree)
            .output();
        match output {
            Ok(o) if o.status.success() => Ok(()),
            Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
            Err(e) => Err(e.to_string()),
        }
    };
    run(&["add", "-A"])?;
    run(&["commit", "--allow-empty", "-m", "task complete"])
}

// ── SSE payload structs re-exported for bridge use ───────────────────────────
// (Already defined in types.rs — referenced via use above.)

/// Return the subset of `written` paths that are NOT within `declared` (the
/// task's `file_ownership`).
///
/// Both lists are normalised to worktree-relative POSIX-style paths before
/// comparison. Returns `None` if all writes are in scope.
///
/// This is the agents-playbook §15.3.13 PoT-1 check — every successful
/// `execute_task` must pass through it when `declared` is non-empty.
pub(crate) fn files_out_of_scope(
    written: &[PathBuf],
    worktree_root: &Path,
    declared: &[String],
) -> Option<Vec<String>> {
    let normalize = |s: &str| s.replace('\\', "/");
    let declared_set: std::collections::HashSet<String> =
        declared.iter().map(|s| normalize(s)).collect();

    let mut out_of_scope: Vec<String> = Vec::new();
    for abs in written {
        let rel = abs
            .strip_prefix(worktree_root)
            .unwrap_or(abs)
            .to_string_lossy()
            .into_owned();
        let rel = normalize(&rel);
        if !declared_set.contains(&rel) {
            out_of_scope.push(rel);
        }
    }
    if out_of_scope.is_empty() {
        None
    } else {
        Some(out_of_scope)
    }
}

/// Sort each wave's tasks in descending order of how many next-wave tasks
/// depend on them (T4 critical-path scheduling).
///
/// Tasks that unblock the most successors run first, minimising the window
/// during which next-wave slots sit idle waiting for their last dependency.
pub(crate) fn sort_waves_by_fan_out(waves: &mut [Vec<Task>]) {
    use std::collections::HashMap;
    for wave_i in 0..waves.len() {
        if wave_i + 1 < waves.len() {
            let next_wave = &waves[wave_i + 1];
            let fan_out: HashMap<String, usize> = waves[wave_i]
                .iter()
                .map(|t| {
                    let count = next_wave
                        .iter()
                        .filter(|nt| nt.depends_on.contains(&t.id))
                        .count();
                    (t.id.clone(), count)
                })
                .collect();
            waves[wave_i].sort_by(|a, b| {
                fan_out
                    .get(&b.id)
                    .unwrap_or(&0)
                    .cmp(fan_out.get(&a.id).unwrap_or(&0))
            });
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use lightarchitects::lightsquad::types::Task;

    fn task(id: &str, deps: &[&str]) -> Task {
        Task {
            id: id.to_owned(),
            branch: format!("task/build/{id}"),
            depends_on: deps.iter().map(|d| (*d).to_owned()).collect(),
            file_ownership: vec![],
            concurrency_safe: false,
            context_tiers: vec![],
            prompt: format!("implement {id}"),
        }
    }

    // ── T4: sort_waves_by_fan_out ─────────────────────────────────────────────

    /// Diamond DAG: wave-0 has two tasks; only one of them is depended on by
    /// both wave-1 tasks. That task should sort first.
    ///
    /// ```
    ///   A (fan-out 2)   B (fan-out 0)
    ///       ↓         ↗
    ///    [C, D] both depend on A
    /// ```
    #[test]
    fn fan_out_sort_diamond_dag() {
        let mut waves = vec![
            vec![task("B", &[]), task("A", &[])], // B listed first; A should move up
            vec![task("C", &["A"]), task("D", &["A"])],
        ];
        sort_waves_by_fan_out(&mut waves);
        assert_eq!(waves[0][0].id, "A", "A unblocks 2 tasks — must sort first");
        assert_eq!(waves[0][1].id, "B");
    }

    /// Funnel DAG: wave-0 has three tasks with different fan-out counts.
    /// Ordering should be strictly descending by fan-out.
    ///
    /// ```
    ///   C(fan=0)  B(fan=1)  A(fan=3)
    ///                ↓    ↗↓↘
    ///           [X,Y,Z] all depend on A; [W] depends on B
    /// ```
    #[test]
    fn fan_out_sort_funnel_dag() {
        let mut waves = vec![
            // Initial order: C, B, A
            vec![task("C", &[]), task("B", &[]), task("A", &[])],
            vec![
                task("X", &["A"]),
                task("Y", &["A"]),
                task("Z", &["A"]),
                task("W", &["B"]),
            ],
        ];
        sort_waves_by_fan_out(&mut waves);
        let ids: Vec<&str> = waves[0].iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids[0], "A", "A fan-out=3 must be first");
        assert_eq!(ids[1], "B", "B fan-out=1 must be second");
        assert_eq!(ids[2], "C", "C fan-out=0 must be last");
    }

    /// Flat wave (no cross-wave deps): sort must be stable — order preserved
    /// when all fan-outs are equal (all zero).
    #[test]
    fn fan_out_sort_flat_no_deps() {
        let original_ids = vec!["X", "Y", "Z"];
        let mut waves = vec![
            original_ids
                .iter()
                .map(|id| task(id, &[]))
                .collect::<Vec<_>>(),
            vec![task("P", &[]), task("Q", &[])], // no dependency on wave-0 tasks
        ];
        sort_waves_by_fan_out(&mut waves);
        // All fan-outs are 0 — sort_by is stable, order must not change
        let ids: Vec<&str> = waves[0].iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, original_ids);
    }

    /// Single-wave build: `sort_waves_by_fan_out` must be a no-op (no next wave).
    #[test]
    fn fan_out_sort_single_wave_noop() {
        let mut waves = vec![vec![task("A", &[]), task("B", &[])]];
        sort_waves_by_fan_out(&mut waves);
        // Wave unchanged
        let ids: Vec<&str> = waves[0].iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["A", "B"]);
    }

    /// Three-wave chain: sort applies independently at each wave boundary.
    ///
    /// ```
    ///   Wave 0:  B(f=1)  A(f=2)    → after sort: A, B
    ///   Wave 1:  X(f=0)  Y(f=0) Z(f=1) → after sort: Z, X, Y (or Z, Y, X — stable)
    ///   Wave 2:  M depends on Z
    /// ```
    #[test]
    fn fan_out_sort_three_wave_chain() {
        let mut waves = vec![
            vec![task("B", &[]), task("A", &[])],
            vec![task("X", &["A"]), task("Y", &["A"]), task("Z", &["A", "B"])],
            vec![task("M", &["Z"])],
        ];
        sort_waves_by_fan_out(&mut waves);
        // Wave 0: A has fan-out 3 (X, Y, Z all depend on A), B has fan-out 1 (Z depends on B)
        assert_eq!(waves[0][0].id, "A");
        assert_eq!(waves[0][1].id, "B");
        // Wave 1: Z has fan-out 1 (M depends on Z); X and Y have 0
        assert_eq!(waves[1][0].id, "Z");
    }

    // ── T7: collect_rs_files ──────────────────────────────────────────────────

    /// Creates the following tree inside `dir`:
    ///
    /// ```
    /// src/
    ///   lib.rs          (non-empty)
    ///   utils.rs        (non-empty)
    ///   empty.rs        (empty — must be skipped)
    ///   submod/
    ///     handler.rs    (non-empty)
    ///   target/
    ///     build.rs      (non-empty — must be skipped: inside target/)
    ///   .hidden/
    ///     secret.rs     (non-empty — must be skipped: inside hidden dir)
    /// ```
    async fn setup_src_tree(dir: &std::path::Path) {
        tokio::fs::create_dir_all(dir.join("src/submod"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(dir.join("src/target"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(dir.join("src/.hidden"))
            .await
            .unwrap();

        tokio::fs::write(dir.join("src/lib.rs"), b"pub fn lib() {}")
            .await
            .unwrap();
        tokio::fs::write(dir.join("src/utils.rs"), b"pub fn util() {}")
            .await
            .unwrap();
        tokio::fs::write(dir.join("src/empty.rs"), b"")
            .await
            .unwrap();
        tokio::fs::write(dir.join("src/submod/handler.rs"), b"pub fn handle() {}")
            .await
            .unwrap();
        tokio::fs::write(dir.join("src/target/build.rs"), b"fn target_code() {}")
            .await
            .unwrap();
        tokio::fs::write(dir.join("src/.hidden/secret.rs"), b"fn secret() {}")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn collect_rs_files_skips_target_and_hidden() {
        let tmp = tempfile::tempdir().unwrap();
        setup_src_tree(tmp.path()).await;

        let mut out: Vec<(String, String, String)> = Vec::new();
        collect_rs_files(tmp.path(), "src", &mut out).await;

        let paths: Vec<&str> = out.iter().map(|(_, p, _)| p.as_str()).collect();
        // Must find the 3 legitimate files
        assert!(
            paths.iter().any(|p| p.ends_with("lib.rs")),
            "lib.rs missing: {paths:?}"
        );
        assert!(
            paths.iter().any(|p| p.ends_with("utils.rs")),
            "utils.rs missing: {paths:?}"
        );
        assert!(
            paths.iter().any(|p| p.ends_with("handler.rs")),
            "handler.rs missing: {paths:?}"
        );
        // Must NOT find skipped files
        assert!(
            !paths.iter().any(|p| p.contains("target")),
            "target/ leaked: {paths:?}"
        );
        assert!(
            !paths.iter().any(|p| p.contains(".hidden")),
            ".hidden/ leaked: {paths:?}"
        );
        assert!(
            !paths.iter().any(|p| p.ends_with("empty.rs")),
            "empty.rs must be skipped: {paths:?}"
        );
        assert_eq!(out.len(), 3, "expected exactly 3 files, got: {paths:?}");
    }

    #[tokio::test]
    async fn collect_rs_files_lib_rs_sorts_first() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(tmp.path().join("src"))
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("src/alpha.rs"), b"fn a() {}")
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("src/lib.rs"), b"pub mod alpha;")
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("src/zeta.rs"), b"fn z() {}")
            .await
            .unwrap();

        let mut out: Vec<(String, String, String)> = Vec::new();
        collect_rs_files(tmp.path(), "src", &mut out).await;
        out.sort_by(|a, b| a.0.cmp(&b.0));

        let first_name = std::path::Path::new(&out[0].1)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(
            first_name, "lib.rs",
            "lib.rs must sort first; got: {}",
            out[0].1
        );
    }

    #[tokio::test]
    async fn collect_rs_files_missing_subdir_is_graceful() {
        let tmp = tempfile::tempdir().unwrap();
        // No "src" subdir at all — should return empty without panicking.
        let mut out: Vec<(String, String, String)> = Vec::new();
        collect_rs_files(tmp.path(), "src", &mut out).await;
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn collect_rs_files_deeply_nested() {
        let tmp = tempfile::tempdir().unwrap();
        // Three levels deep: src/a/b/deep.rs
        tokio::fs::create_dir_all(tmp.path().join("src/a/b"))
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("src/a/b/deep.rs"), b"fn deep() {}")
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("src/a/mid.rs"), b"fn mid() {}")
            .await
            .unwrap();

        let mut out: Vec<(String, String, String)> = Vec::new();
        collect_rs_files(tmp.path(), "src", &mut out).await;

        assert_eq!(out.len(), 2);
        let paths: Vec<&str> = out.iter().map(|(_, p, _)| p.as_str()).collect();
        assert!(paths.iter().any(|p| p.ends_with("deep.rs")));
        assert!(paths.iter().any(|p| p.ends_with("mid.rs")));
    }

    // ── read_src_context integration ──────────────────────────────────────────

    #[tokio::test]
    async fn read_src_context_produces_markdown_blocks() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(tmp.path().join("src"))
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("Cargo.toml"), b"[package]\nname = \"test\"")
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("src/lib.rs"), b"pub fn hello() {}")
            .await
            .unwrap();

        let ctx = read_src_context(tmp.path()).await;
        assert!(!ctx.is_empty(), "context must not be empty");
        assert!(
            ctx.contains("### `Cargo.toml`"),
            "Cargo.toml section missing"
        );
        assert!(
            ctx.contains("```toml"),
            "Cargo.toml must use toml code fence"
        );
        assert!(ctx.contains("### `src/lib.rs`"), "lib.rs section missing");
        assert!(ctx.contains("```rust"), "lib.rs must use rust code fence");
    }

    #[tokio::test]
    async fn read_src_context_empty_dir_returns_empty_string() {
        let tmp = tempfile::tempdir().unwrap();
        // No Cargo.toml, no src/ — empty worktree (Wave 1 cold start).
        let ctx = read_src_context(tmp.path()).await;
        assert!(ctx.is_empty(), "empty worktree must yield empty string");
    }

    #[tokio::test]
    async fn read_src_context_cargo_toml_sorts_before_src() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(tmp.path().join("src"))
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("Cargo.toml"), b"[package]\nname=\"x\"")
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("src/main.rs"), b"fn main() {}")
            .await
            .unwrap();

        let ctx = read_src_context(tmp.path()).await;
        let cargo_pos = ctx.find("### `Cargo.toml`").unwrap();
        let main_pos = ctx.find("### `src/main.rs`").unwrap();
        assert!(
            cargo_pos < main_pos,
            "Cargo.toml must appear before src/main.rs"
        );
    }

    // ── Token budget enforcement ──────────────────────────────────────────────

    /// When the budget is exceeded by the available source code, low-priority
    /// files must be skipped and a truncation marker must be emitted so the
    /// model knows context was clipped (and won't hallucinate the missing
    /// surface as "this surface doesn't exist").
    #[tokio::test]
    async fn read_src_context_truncates_when_over_budget() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(tmp.path().join("src"))
            .await
            .unwrap();
        // Write 5 files of 800 bytes each = 4000 bytes ≈ 1000 tokens total.
        // With a 200-token budget, only the first 1–2 files should fit.
        let big_content = "a".repeat(800);
        for i in 0..5 {
            tokio::fs::write(
                tmp.path().join(format!("src/file_{i:02}.rs")),
                big_content.as_bytes(),
            )
            .await
            .unwrap();
        }

        let ctx = read_src_context_with_budget(tmp.path(), 200).await;
        assert!(
            ctx.contains("more file(s) omitted to fit context budget"),
            "truncation marker missing in: {ctx}"
        );
        // Token-budget cap is advisory upper bound — actual emitted output
        // must be smaller than the full unbounded version (5 × ~800 = 4000+).
        assert!(
            ctx.len() < 4000,
            "expected truncated output, got {} bytes",
            ctx.len()
        );
    }

    /// Under a tight budget, priority-ranked files (Cargo.toml, lib.rs) must
    /// fit before alphabetically-earlier-but-lower-priority files.
    #[tokio::test]
    async fn read_src_context_prioritizes_lib_rs_under_budget() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(tmp.path().join("src"))
            .await
            .unwrap();
        let content = "a".repeat(400); // ≈100 tokens per file
        // alpha.rs is alphabetically earlier than lib.rs but lower priority.
        tokio::fs::write(tmp.path().join("src/alpha.rs"), content.as_bytes())
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("src/lib.rs"), content.as_bytes())
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("src/zeta.rs"), content.as_bytes())
            .await
            .unwrap();

        // Budget allows ~1.5 sections worth — lib.rs must be the one that fits.
        let ctx = read_src_context_with_budget(tmp.path(), 150).await;
        assert!(
            ctx.contains("### `src/lib.rs`"),
            "lib.rs (highest src/ priority) must fit; got: {ctx}"
        );
    }

    // ── files_out_of_scope (PoT-1 post-task gate) ────────────────────────────

    /// Worker stays in scope when all writes are inside the declared
    /// ownership set — `files_out_of_scope` returns `None`.
    #[test]
    fn files_out_of_scope_clean_when_in_scope() {
        let wt = std::path::PathBuf::from("/tmp/wt-test");
        let written = vec![wt.join("src/lib.rs"), wt.join("src/util.rs")];
        let declared = vec!["src/lib.rs".to_owned(), "src/util.rs".to_owned()];
        assert!(files_out_of_scope(&written, &wt, &declared).is_none());
    }

    /// Worker writes one file outside its declared set → returned in the
    /// out-of-scope list. The in-scope file is NOT listed.
    #[test]
    fn files_out_of_scope_flags_escapee() {
        let wt = std::path::PathBuf::from("/tmp/wt-test");
        let written = vec![
            wt.join("src/lib.rs"),      // in scope
            wt.join("src/escapee.rs"),  // not declared
            wt.join("tests/sneaky.rs"), // not declared
        ];
        let declared = vec!["src/lib.rs".to_owned()];
        let out = files_out_of_scope(&written, &wt, &declared).unwrap();
        assert_eq!(out.len(), 2);
        assert!(out.contains(&"src/escapee.rs".to_owned()));
        assert!(out.contains(&"tests/sneaky.rs".to_owned()));
        assert!(!out.contains(&"src/lib.rs".to_owned()));
    }

    /// Empty declared list short-circuits at the caller — but if reached,
    /// every written file is "out of scope" by definition. The dispatcher
    /// guards this with `!file_ownership.is_empty()` before calling.
    #[test]
    fn files_out_of_scope_empty_declared_means_all_escape() {
        let wt = std::path::PathBuf::from("/tmp/wt-test");
        let written = vec![wt.join("src/lib.rs")];
        let declared: Vec<String> = vec![];
        let out = files_out_of_scope(&written, &wt, &declared).unwrap();
        assert_eq!(out, vec!["src/lib.rs"]);
    }

    /// Path normalisation: absolute writes outside the worktree root pass
    /// through unchanged. The PoT-1 invariant is that the validator
    /// `OllamaResponseValidator` already rejects out-of-worktree writes
    /// before they reach here — but if they did, they'd surface as escapes.
    #[test]
    fn files_out_of_scope_handles_absolute_outside_worktree() {
        let wt = std::path::PathBuf::from("/tmp/wt-test");
        let written = vec![std::path::PathBuf::from("/etc/passwd")];
        let declared = vec!["src/lib.rs".to_owned()];
        let out = files_out_of_scope(&written, &wt, &declared).unwrap();
        // strip_prefix fails → falls back to the full path string
        assert!(out[0].contains("passwd"));
    }

    /// When all files fit within the budget, no truncation marker is emitted.
    #[tokio::test]
    async fn read_src_context_no_marker_when_within_budget() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(tmp.path().join("src"))
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("src/lib.rs"), b"pub fn a() {}")
            .await
            .unwrap();

        // Default 8000 token budget is enormous vs. ~3 tokens of actual content.
        let ctx = read_src_context(tmp.path()).await;
        assert!(
            !ctx.contains("omitted to fit"),
            "truncation marker must NOT be emitted when context fits: {ctx}"
        );
    }
}
