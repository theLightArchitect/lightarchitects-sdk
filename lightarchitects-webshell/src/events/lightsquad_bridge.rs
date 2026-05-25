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

use std::path::PathBuf;

use tokio::sync::broadcast;
use uuid::Uuid;

use lightarchitects::{
    agent::OllamaCloudCodingProvider,
    lightsquad::{
        program::{Program, ProgramConfig},
        types::Task,
        wave_dispatcher::WorkerSpec,
    },
};

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
    tokio::spawn(run_build(ctx))
}

// ── Internal orchestration ────────────────────────────────────────────────────

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
    let ls_waves: Vec<Vec<Task>> = waves
        .into_iter()
        .map(|wave| {
            wave.into_iter()
                .map(|spec| Task {
                    branch: format!("task/{codename}/{}", spec.id),
                    depends_on: spec.depends_on,
                    context_tiers: vec![],
                    prompt: spec.prompt,
                    id: spec.id,
                })
                .collect()
        })
        .collect();

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
    let tx_fix = tx.clone();
    let build_id_worker = build_id;
    let codename_worker = codename.clone();
    let dw_worker = dw.clone();

    let worker_fn = make_worker(
        build_id_worker,
        codename_worker,
        tx_slot,
        tx_merge,
        tx_fix,
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

    match program.run(worker_fn).await {
        Ok(summary) => {
            let _ = dw.append(
                "L1",
                &format!(
                    "Build complete: {} succeeded, {} failed",
                    summary.succeeded, summary.failed
                ),
                None,
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
        }
        Err(e) => {
            let _ = dw.append(
                "L4",
                &format!("ESCALATION: build halted — {e:?}"),
                Some("canon://agents-playbook#§15"),
            );
        }
    }
}

// ── Worker selection ──────────────────────────────────────────────────────────

/// Build the per-task worker closure.
///
/// `use_mock = true` activates the hermetic mock path (write file + git commit)
/// instead of spawning the real `lightarchitects --bare` CLI. The flag is
/// captured by value and applies to every task in the closure's lifetime.
#[allow(clippy::too_many_arguments)]
fn make_worker(
    build_id: Uuid,
    _codename: String,
    tx_slot: broadcast::Sender<WebEventV2>,
    tx_merge: broadcast::Sender<WebEventV2>,
    _tx_fix: broadcast::Sender<WebEventV2>,
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
        let build_id = build_id;
        let tx_slot = tx_slot.clone();
        let tx_merge = tx_merge.clone();
        let dw = dw.clone();
        let hitl_queue = hitl_queue.clone();
        let prompt = spec.task.prompt.clone();
        Box::pin(async move {
            let task_id = spec.task.id.clone();
            let wt = spec.worktree_path.clone();

            let _ = tx_slot.send(WebEventV2::from_event(
                WebEvent::WorkerSlotGauge(WorkerSlotGaugeEvent {
                    build_id: build_id.to_string(),
                    wave_index: 0,
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
                let provider = OllamaCloudCodingProvider::default_coding()
                    .map_err(|e| format!("provider init failed: {e}"))?;

                match provider.execute_task(&task_id, &prompt, &wt).await {
                    Ok(_) => {
                        let _ = dw.append(
                            "L2",
                            &format!("Task '{task_id}' completed by OllamaCloud"),
                            Some("canon://builders-cookbook#§66"),
                        );
                    }
                    Err(e) => {
                        // Security violations and validation errors escalate to the
                        // operator via HITL rather than halting the build silently.
                        let reason = e.to_string();
                        let (call_id, resolve_rx) = crate::events::hitl_relay::park(
                            &hitl_queue,
                            build_id,
                            task_id.clone(),
                            reason.clone(),
                            0, // wave_index — TODO: thread wave index through WorkerSpec
                            1, // worker_slot — TODO: thread slot number through WorkerSpec
                        );

                        let _ = tx_slot.send(WebEventV2::from_event(
                            WebEvent::Escalation(crate::events::types::EscalationEvent {
                                build_id: build_id.to_string(),
                                wave_index: 0,
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

                        // Await operator decision — block this worker slot.
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
                            }
                            Ok(_) | Err(_) => {
                                return Err(format!(
                                    "task '{task_id}' rejected by operator or HITL dropped (call_id={call_id})"
                                ));
                            }
                        }
                    }
                }
            }

            let _ = tx_merge.send(WebEventV2::from_event(
                WebEvent::MergeAgentStatus(MergeAgentStatusEvent {
                    build_id: build_id.to_string(),
                    wave_index: 0,
                    phase: "merged".to_owned(),
                    commit_sha: None,
                }),
                Some(build_id),
            ));

            Ok(())
        })
    }
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
