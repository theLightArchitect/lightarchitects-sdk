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
    let dw_worker = dw.clone();

    let worker_fn = make_worker(
        build_id,
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
/// Captured context for a single worker invocation.
struct WorkerCtx {
    build_id: Uuid,
    tx_slot: broadcast::Sender<WebEventV2>,
    tx_merge: broadcast::Sender<WebEventV2>,
    dw: DecisionsWriter,
    hitl_queue: crate::events::hitl_relay::HitlQueue,
    use_mock: bool,
}

fn make_worker(
    build_id: Uuid,
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
        let ctx = WorkerCtx {
            build_id,
            tx_slot: tx_slot.clone(),
            tx_merge: tx_merge.clone(),
            dw: dw.clone(),
            hitl_queue: hitl_queue.clone(),
            use_mock,
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

async fn worker_body(
    task_id: String,
    wt: PathBuf,
    prompt: String,
    ctx: WorkerCtx,
) -> Result<(), String> {
    let WorkerCtx {
        build_id,
        tx_slot,
        tx_merge,
        dw,
        hitl_queue,
        use_mock,
    } = ctx;

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
        let context = read_src_context(&wt).await;
        let hydrated_prompt = build_hydrated_prompt(&prompt, &context);

        match provider.execute_task(&task_id, &hydrated_prompt, &wt).await {
            Ok(_) => {
                let _ = dw.append(
                    "L2",
                    &format!("Task '{task_id}' completed by OllamaCloud"),
                    Some("canon://builders-cookbook#§66"),
                );
            }
            Err(e) => {
                // Security violations and validation errors escalate via HITL.
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
                let _ = dw.append("L4", &format!("ESCALATION task '{task_id}': {reason} — awaiting operator (call_id={call_id})"), Some("canon://security-guardrails#§G-DENY"));

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
    for (_, display_path, content) in &sections {
        let lang = if Path::new(display_path)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
        {
            "toml"
        } else {
            "rust"
        };
        let _ = write!(out, "### `{display_path}`\n```{lang}\n{content}```\n\n");
    }
    out
}

/// Walk `<worktree>/<subdir>` and append every `.rs` file to `out`.
///
/// `lib.rs` and `main.rs` are sorted before all other entries so the module
/// map is visible at the top of any listing.
async fn collect_rs_files(worktree: &Path, subdir: &str, out: &mut Vec<(String, String, String)>) {
    let dir = worktree.join(subdir);
    let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("rs"))
        {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(content) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        if content.is_empty() {
            continue;
        }
        // Ensure lib.rs / main.rs float to the top within their subdir.
        let priority = if name == "lib.rs" || name == "main.rs" {
            "0"
        } else {
            "1"
        };
        let sort_key = format!("1-{subdir}-{priority}-{name}");
        let display_path = format!("{subdir}/{name}");
        out.push((sort_key, display_path, content));
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
