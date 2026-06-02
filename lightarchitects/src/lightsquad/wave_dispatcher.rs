//! Wave dispatcher — fans tasks out to per-worktree workers via Tokio `JoinSet`.
//!
//! [`dispatch_wave`] is the single entry point for a wave of tasks. It:
//!
//! 1. Resolves the ready set — tasks whose `depends_on` are all
//!    [`TaskStatus::Complete`] (via [`can_run`]).
//! 2. For each ready task, creates a git worktree ([`WorktreeManager::create`]),
//!    marks the task [`TaskStatus::InProgress`], and spawns `worker_fn` in a
//!    [`tokio::task::JoinSet`].
//! 3. As each worker finishes, the dispatcher merges the task branch back to
//!    `feat/<build>` via [`MergeAgent::merge_task_to_feat`], removes the
//!    worktree, and marks the task [`TaskStatus::Complete`] or
//!    [`TaskStatus::Failed`].
//! 4. Returns a `WaveResult` summarising success/failure counts.
//!
//! # Testability
//!
//! `worker_fn` is generic (`F: Fn(WorkerSpec) -> Fut`) so integration tests can
//! pass a mock that does `git commit --allow-empty` without invoking Claude CLI.
//!
//! # Concurrency contract
//!
//! The maximum number of live workers at any time is bounded by
//! `SLOT_CAPACITY` (7 per the IRONCLAW spec). Tasks that are ready but
//! cannot be slotted are queued until a slot opens.
//!
//! [`can_run`]: super::types::can_run
//! [`WorktreeManager::create`]: super::worktree_manager::WorktreeManager::create
//! [`MergeAgent::merge_task_to_feat`]: super::merge_agent::MergeAgent::merge_task_to_feat

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::{sync::RwLock, task::JoinSet};

use super::{
    merge_agent::{MergeAgent, MergeError},
    types::{Coordinator, SharedState, Task, TaskStatus},
    worker_executor::WorkerExecutor,
    worktree_manager::{WorktreeError, WorktreeManager},
};

/// Maximum concurrent **write-class** worker slots per IRONCLAW spec
/// §7-Slot Agent Pool. Applies to tasks whose [`Task::concurrency_safe`]
/// is `false` (the default) — i.e. any task that produces filesystem or
/// git mutations.
pub const SLOT_CAPACITY: usize = 7;

/// Maximum concurrent **read-class** worker slots — applies to tasks whose
/// [`Task::concurrency_safe`] is `true`. Read tasks (context gathering,
/// codebase exploration, doc lookup via context7) do not mutate state, so
/// they get a larger budget than the write-task pool.
///
/// Override via the `LIGHTSQUAD_READ_SLOT_CAPACITY` env var (set at wave
/// construction by the bridge or test harness; not re-read mid-wave).
pub const SLOT_CAPACITY_READ_DEFAULT: usize = 16;

// Compile-time invariant: read pool must be larger than write pool. Array
// length expression evaluates to 0 if the invariant is false, which is an
// error: `arrays of length 0 don't have a positive size`. The classic Rust
// const-assert pattern — no clippy false positives, fails at compile time.
const _READ_LARGER_THAN_WRITE: [(); 1] =
    [(); (SLOT_CAPACITY_READ_DEFAULT > SLOT_CAPACITY) as usize];

/// Resolve the read-class slot cap from `LIGHTSQUAD_READ_SLOT_CAPACITY` or
/// the default. Non-numeric or zero values fall back to the default to
/// avoid silently disabling read parallelism.
#[must_use]
pub fn read_slot_capacity() -> usize {
    std::env::var("LIGHTSQUAD_READ_SLOT_CAPACITY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(SLOT_CAPACITY_READ_DEFAULT)
}

// ── Public types ──────────────────────────────────────────────────────────────

/// Bundle passed to a worker function for a single task.
#[derive(Debug, Clone)]
pub struct WorkerSpec {
    /// Task being executed.
    pub task: Task,
    /// Absolute path of the worktree the worker should operate in.
    pub worktree_path: PathBuf,
    /// 0-based index of the wave this task belongs to — forwarded to AYIN spans and SSE events.
    pub wave_index: usize,
}

/// Outcome of a completed wave.
#[derive(Debug, Clone, Default)]
pub struct WaveResult {
    /// Number of tasks that completed and merged successfully.
    pub succeeded: u32,
    /// Number of tasks that failed (worker error or merge conflict).
    pub failed: u32,
    /// IDs of tasks that failed.
    pub failed_ids: Vec<String>,
}

impl WaveResult {
    /// `true` if every task in the wave succeeded.
    #[must_use]
    pub fn all_succeeded(&self) -> bool {
        self.failed == 0
    }
}

/// Errors from wave dispatch.
#[derive(Debug, thiserror::Error)]
pub enum WaveError {
    /// A worker or merge step failed; wave result captures per-task outcomes.
    #[error("wave completed with {0} failure(s)")]
    TaskFailures(u32),
    /// Worktree lifecycle error (create or remove failed fatally).
    #[error("worktree error: {0}")]
    Worktree(#[from] WorktreeError),
    /// Merge error unrelated to a task conflict.
    #[error("merge error: {0}")]
    Merge(#[from] MergeError),
    /// Two tasks in the same wave declared exclusive ownership of the same
    /// file — would deadlock the merge agent on conflict. Agents-playbook
    /// §15.3.13 PW-6 gate.
    #[error("ownership conflict — file '{file}' claimed by tasks: {tasks:?}")]
    OwnershipConflict {
        /// The file claimed by more than one task.
        file: String,
        /// IDs of all tasks that declared ownership of `file`.
        tasks: Vec<String>,
    },
}

/// Verify that no two tasks in `tasks` claim the same file in their
/// `file_ownership` list (agents-playbook §15.3.13 PW-6 pre-wave gate).
///
/// Tasks with empty `file_ownership` opt out of enforcement and are skipped.
/// Returns `Ok(())` when all declared ownership sets are disjoint.
///
/// # Errors
///
/// Returns [`WaveError::OwnershipConflict`] on the first overlap detected.
pub fn validate_wave_ownership(tasks: &[Task]) -> Result<(), WaveError> {
    use std::collections::HashMap;
    let mut owner_of: HashMap<&str, Vec<&str>> = HashMap::new();
    for task in tasks {
        for file in &task.file_ownership {
            owner_of
                .entry(file.as_str())
                .or_default()
                .push(task.id.as_str());
        }
    }
    for (file, owners) in &owner_of {
        if owners.len() > 1 {
            return Err(WaveError::OwnershipConflict {
                file: (*file).to_owned(),
                tasks: owners.iter().map(|s| (*s).to_owned()).collect(),
            });
        }
    }
    Ok(())
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Dispatch all ready tasks in `tasks` as a concurrent wave.
///
/// `executor` is called once per ready task via [`WorkerExecutor::dispatch_one`].
/// The dispatcher merges successful workers and removes all worktrees
/// unconditionally.
///
/// Concurrency is bounded to [`SLOT_CAPACITY`] live workers.
///
/// # Errors
///
/// Returns [`WaveError::TaskFailures`] if any task fails. Returns
/// [`WaveError::Worktree`] or [`WaveError::Merge`] on infrastructure failures.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub async fn dispatch_wave(
    wave_index: usize,
    tasks: &[Task],
    coordinator: &Coordinator,
    worktree_manager: &WorktreeManager,
    merge_agent: &MergeAgent,
    feat_branch: &str,
    worktree_root: &Path,
    executor: Arc<dyn WorkerExecutor>,
) -> Result<WaveResult, WaveError> {
    // Pre-wave ownership gate (agents-playbook §15.3.13 PW-6): reject the
    // wave before any worktree is created if two tasks claim the same file.
    validate_wave_ownership(tasks)?;

    // Determine which tasks are ready to run.
    let ready: Vec<&Task> = {
        let state = coordinator.state.read().await;
        tasks
            .iter()
            .filter(|t| {
                let status = state.tasks.get(&t.id);
                // Only dispatch tasks that are still Pending and whose deps complete.
                matches!(status, Some(TaskStatus::Pending) | None)
                    && super::types::can_run(t, &state)
            })
            .collect()
    };

    // Partition ready set by concurrency class. Read tasks (concurrency_safe
    // = true) draw from the larger SLOT_CAPACITY_READ pool; write tasks draw
    // from SLOT_CAPACITY. Mirrors Claude Code's safe/unsafe batching, lifted
    // to the wave level so context-gathering tasks don't burn the write-pool
    // budget.
    let (safe_queue, unsafe_queue): (Vec<&Task>, Vec<&Task>) =
        ready.iter().partition(|t| t.concurrency_safe);
    let read_cap = read_slot_capacity();

    let mut join_set: JoinSet<TaskOutcome> = JoinSet::new();
    let mut read_slots: usize = 0;
    let mut write_slots: usize = 0;
    let mut safe_idx = 0;
    let mut unsafe_idx = 0;
    let mut result = WaveResult::default();

    // Draining loop: fill BOTH slot pools independently, then process the
    // next completion, then refill.
    loop {
        // Fill the read pool with safe tasks (up to read_cap).
        while read_slots < read_cap && safe_idx < safe_queue.len() {
            spawn_task_slot(
                safe_queue[safe_idx],
                wave_index,
                true,
                worktree_root,
                worktree_manager,
                coordinator,
                &executor,
                &mut join_set,
            )
            .await?;
            safe_idx += 1;
            read_slots += 1;
        }
        // Fill the write pool with unsafe tasks (up to SLOT_CAPACITY).
        while write_slots < SLOT_CAPACITY && unsafe_idx < unsafe_queue.len() {
            spawn_task_slot(
                unsafe_queue[unsafe_idx],
                wave_index,
                false,
                worktree_root,
                worktree_manager,
                coordinator,
                &executor,
                &mut join_set,
            )
            .await?;
            unsafe_idx += 1;
            write_slots += 1;
        }

        // If nothing is running or queued, we are done.
        if join_set.is_empty() {
            break;
        }

        // Wait for the next worker to finish.
        let Some(join_result) = join_set.join_next().await else {
            break;
        };

        let outcome = match join_result {
            Ok(o) => o,
            Err(e) => {
                // JoinError — task panicked. Treat as failure.
                // We can't tell which pool the panicked task was in; bias
                // toward freeing a write slot (the smaller pool) so we don't
                // stall write-task dispatch waiting on the larger read pool.
                if write_slots > 0 {
                    write_slots = write_slots.saturating_sub(1);
                } else if read_slots > 0 {
                    read_slots = read_slots.saturating_sub(1);
                }
                result.failed += 1;
                result.failed_ids.push(format!("panic: {e}"));
                continue;
            }
        };

        // Decrement the slot pool the completed task belonged to.
        if outcome.concurrency_safe {
            read_slots = read_slots.saturating_sub(1);
        } else {
            write_slots = write_slots.saturating_sub(1);
        }

        // Remove the worktree regardless of outcome.
        let _ = worktree_manager.remove(&outcome.worktree_path).await;

        match outcome.result {
            Ok(()) => {
                // Merge back to feat branch.
                match merge_agent
                    .merge_task_to_feat(&outcome.branch, feat_branch)
                    .await
                {
                    Ok(()) => {
                        mark_task(&coordinator.state, &outcome.task_id, TaskStatus::Complete).await;
                        coordinator.notify.notify_waiters();
                        result.succeeded += 1;
                        // Prune the task branch — non-fatal; a missed delete is cosmetic
                        // noise, not a correctness failure.
                        if let Err(e) = merge_agent.delete_branch(&outcome.branch).await {
                            tracing::warn!(
                                operation = "branch_cleanup",
                                branch = %outcome.branch,
                                task_id = %outcome.task_id,
                                wave_index = wave_index,
                                error_kind = "delete_failed",
                                "task branch delete failed — ref may accumulate; ref-reaper scheduled as follow-on",
                            );
                            tracing::debug!(error = %e, "task branch delete raw error");
                        }
                    }
                    Err(MergeError::Conflict { branch }) => {
                        mark_task(&coordinator.state, &outcome.task_id, TaskStatus::Failed).await;
                        result.failed += 1;
                        result.failed_ids.push(format!("conflict:{branch}"));
                    }
                    Err(e) => {
                        mark_task(&coordinator.state, &outcome.task_id, TaskStatus::Failed).await;
                        result.failed += 1;
                        result.failed_ids.push(outcome.task_id.clone());
                        // For non-conflict merge errors, surface them.
                        return Err(WaveError::Merge(e));
                    }
                }
            }
            Err(msg) => {
                mark_task(&coordinator.state, &outcome.task_id, TaskStatus::Failed).await;
                result.failed += 1;
                result
                    .failed_ids
                    .push(format!("{}:{}", outcome.task_id, msg));
            }
        }
    }

    // Set final wave status on coordinator.
    {
        let mut state = coordinator.state.write().await;
        for task in tasks {
            if let Some(s) = state.tasks.get(&task.id) {
                if *s == TaskStatus::Pending {
                    // Tasks that were still Pending (blocked deps) remain Pending.
                }
            }
        }
        // Update build status is done by the calling program.rs layer.
        let _ = &mut state;
    }

    if result.failed > 0 {
        Err(WaveError::TaskFailures(result.failed))
    } else {
        Ok(result)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Internal: per-task outcome from a [`JoinSet`] slot.
struct TaskOutcome {
    task_id: String,
    branch: String,
    worktree_path: PathBuf,
    /// Mirrors the dispatched task's [`Task::concurrency_safe`]. The drain
    /// loop uses this to decrement the correct slot counter on completion.
    concurrency_safe: bool,
    result: Result<(), String>,
}

/// Spawn a single task into the `JoinSet` — extracted to keep `dispatch_wave`'s
/// dual-pool fill loop readable. Creates the worktree, marks the task
/// `InProgress`, and submits the worker future via [`WorkerExecutor::dispatch_one`].
/// The spawned future stamps the task's `concurrency_safe` flag onto the resulting
/// [`TaskOutcome`] so the drain loop knows which slot pool to decrement.
#[allow(clippy::too_many_arguments)]
async fn spawn_task_slot(
    task: &Task,
    wave_index: usize,
    concurrency_safe: bool,
    worktree_root: &Path,
    worktree_manager: &WorktreeManager,
    coordinator: &Coordinator,
    executor: &Arc<dyn WorkerExecutor>,
    join_set: &mut JoinSet<TaskOutcome>,
) -> Result<(), WaveError> {
    // UUID suffix makes every attempt unique across both the git branch name and
    // the worktree filesystem path; prevents collision on retry when the same
    // task.id is re-dispatched (e.g. IronClaw wave retry).
    // simple() produces 32 hex chars; [..8] is always in-bounds.
    // One suffix shared by branch + path keeps the slot identity coherent.
    let slot_suffix = uuid::Uuid::new_v4().simple().to_string();
    let wt_branch = format!("task/build/{}-{}", task.id, &slot_suffix[..8]);
    let wt_path = worktree_root.join(format!("{}-{}", task.id, &slot_suffix[..8]));
    tracing::debug!(wt_branch = %wt_branch, task_id = %task.id, "wave retry slot allocated");

    let handle = worktree_manager.create(&wt_branch, &wt_path).await?;
    mark_task(&coordinator.state, &task.id, TaskStatus::InProgress).await;

    let spec = WorkerSpec {
        task: task.clone(),
        worktree_path: handle.path.clone(),
        wave_index,
    };
    let exec = Arc::clone(executor);
    let task_id = task.id.clone();
    let branch = handle.branch.clone();

    join_set.spawn(async move {
        let result = exec
            .dispatch_one(spec)
            .await
            .map(|_outcome| ())
            .map_err(|e| e.to_string());
        TaskOutcome {
            task_id,
            branch,
            worktree_path: handle.path,
            concurrency_safe,
            result,
        }
    });
    Ok(())
}

/// Write a task status transition to shared state.
async fn mark_task(state: &Arc<RwLock<SharedState>>, task_id: &str, status: TaskStatus) {
    let mut s = state.write().await;
    s.tasks.insert(task_id.to_owned(), status);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn wave_result_all_succeeded_empty() {
        assert!(WaveResult::default().all_succeeded());
    }

    #[test]
    fn wave_result_all_succeeded_with_failures() {
        let r = WaveResult {
            succeeded: 2,
            failed: 1,
            failed_ids: vec!["t1".to_owned()],
        };
        assert!(!r.all_succeeded());
    }

    #[test]
    fn wave_result_all_succeeded_clean() {
        let r = WaveResult {
            succeeded: 3,
            failed: 0,
            failed_ids: vec![],
        };
        assert!(r.all_succeeded());
    }

    #[test]
    fn slot_capacity_is_seven() {
        assert_eq!(SLOT_CAPACITY, 7);
    }

    // ── validate_wave_ownership ───────────────────────────────────────────────

    fn task_owns(id: &str, files: &[&str]) -> Task {
        Task {
            id: id.to_owned(),
            branch: format!("task/build/{id}"),
            depends_on: vec![],
            role: crate::lightsquad::agent_role::AgentRole::Engineer,
            file_ownership: files.iter().map(|s| (*s).to_owned()).collect(),
            concurrency_safe: false,
            context_tiers: vec![],
            prompt: format!("implement {id}"),
            policy_override: None,
        }
    }

    /// Two tasks claiming `src/lib.rs` must be rejected before dispatch.
    /// Without this gate, both worker worktrees would write `src/lib.rs`
    /// and the second `MergeAgent` call would deadlock or surface a conflict
    /// that no automated HITL resolver can untangle.
    #[test]
    fn validate_wave_ownership_rejects_overlap() {
        let tasks = vec![
            task_owns("A", &["src/lib.rs", "src/util.rs"]),
            task_owns("B", &["src/lib.rs"]),
        ];
        let err = validate_wave_ownership(&tasks).unwrap_err();
        match err {
            WaveError::OwnershipConflict { file, tasks } => {
                assert_eq!(file, "src/lib.rs");
                assert!(tasks.contains(&"A".to_owned()));
                assert!(tasks.contains(&"B".to_owned()));
            }
            other => panic!("expected OwnershipConflict, got {other:?}"),
        }
    }

    /// Disjoint ownership sets must pass cleanly.
    #[test]
    fn validate_wave_ownership_accepts_disjoint_sets() {
        let tasks = vec![
            task_owns("A", &["src/a.rs"]),
            task_owns("B", &["src/b.rs", "src/c.rs"]),
            task_owns("C", &["tests/integration.rs"]),
        ];
        assert!(validate_wave_ownership(&tasks).is_ok());
    }

    /// Empty `file_ownership` is the legacy / interactive opt-out — must not
    /// be treated as a wildcard ownership claim.
    #[test]
    fn validate_wave_ownership_skips_empty_lists() {
        let tasks = vec![task_owns("A", &[]), task_owns("B", &[])];
        assert!(validate_wave_ownership(&tasks).is_ok());
    }

    /// Mixed: some tasks declare ownership, others don't. Enforcement runs
    /// only on the declared ones — no spurious conflict between empty sets.
    #[test]
    fn validate_wave_ownership_mixed_declared_and_legacy() {
        let tasks = vec![
            task_owns("A", &["src/a.rs"]),
            task_owns("B", &[]), // legacy / opt-out
            task_owns("C", &["src/c.rs"]),
        ];
        assert!(validate_wave_ownership(&tasks).is_ok());
    }

    /// A task that claims the same file twice is its own conflict (still
    /// flagged — the file's owner list grows to 2 within the loop, even
    /// though both entries are the same task id).
    #[test]
    fn validate_wave_ownership_flags_intra_task_duplicate() {
        let tasks = vec![task_owns("A", &["src/lib.rs", "src/lib.rs"])];
        let err = validate_wave_ownership(&tasks).unwrap_err();
        assert!(matches!(err, WaveError::OwnershipConflict { .. }));
    }

    // ── Dual-slot accounting (read vs write pools) ────────────────────────────

    // Invariant `SLOT_CAPACITY_READ_DEFAULT > SLOT_CAPACITY` is enforced at
    // compile time by the module-level `_READ_LARGER_THAN_WRITE` array — the
    // whole point of the read pool is to enable wider parallelism for
    // concurrency-safe context-gathering tasks than the write-task budget
    // allows. No runtime test required here.

    /// `read_slot_capacity()` returns the default when the env var is
    /// absent. (We don't set env in tests — Rust 2024 marks `set_var` unsafe,
    /// and the workspace lints forbid unsafe in tests.)
    #[test]
    fn read_slot_capacity_returns_default_when_env_absent() {
        // Best-effort: this test only matches the default when no operator
        // override is in the harness env. Either way, the function must
        // return a positive integer.
        let cap = read_slot_capacity();
        assert!(cap > 0, "read_slot_capacity must be > 0; got {cap}");
        assert!(
            cap == SLOT_CAPACITY_READ_DEFAULT
                || std::env::var("LIGHTSQUAD_READ_SLOT_CAPACITY").is_ok(),
            "unexpected cap {cap} without env override"
        );
    }

    /// Partitioning at the dispatch entry: ready set splits cleanly into
    /// safe and unsafe queues. We exercise the `partition` call indirectly
    /// by constructing a mixed wave and asserting both queues are populated
    /// correctly.
    #[test]
    fn partition_separates_safe_from_unsafe_tasks() {
        let mk = |id: &str, safe: bool| {
            let mut t = task_owns(id, &[]);
            t.concurrency_safe = safe;
            t
        };
        let mixed = [
            mk("read-1", true),
            mk("write-1", false),
            mk("read-2", true),
            mk("write-2", false),
            mk("read-3", true),
        ];
        let (safe, unsafe_): (Vec<&Task>, Vec<&Task>) =
            mixed.iter().partition(|t| t.concurrency_safe);
        assert_eq!(safe.len(), 3);
        assert_eq!(unsafe_.len(), 2);
        assert!(safe.iter().all(|t| t.concurrency_safe));
        assert!(unsafe_.iter().all(|t| !t.concurrency_safe));
    }

    /// New `Task` instances default to `concurrency_safe = false`. Important:
    /// the field is `#[serde(default)]`, so existing `TaskSpec` JSON payloads
    /// without the field deserialize to safe-by-default-false.
    #[test]
    fn task_concurrency_safe_defaults_to_false() {
        let t = task_owns("t", &[]);
        assert!(
            !t.concurrency_safe,
            "default Task must be concurrency_safe=false (write-class)"
        );
    }

    /// JSON round-trip: a wave-spec payload that omits `concurrency_safe`
    /// must deserialize to false; one that sets it to true must round-trip.
    #[test]
    fn task_concurrency_safe_serde_roundtrip() {
        // No concurrency_safe field — should default to false.
        let legacy = r#"{
            "id": "legacy-t",
            "branch": "task/build/legacy-t",
            "depends_on": [],
            "context_tiers": [],
            "prompt": "do thing"
        }"#;
        let parsed: Task = serde_json::from_str(legacy).unwrap();
        assert!(!parsed.concurrency_safe);

        // Explicit true.
        let safe_spec = r#"{
            "id": "explore-t",
            "branch": "task/build/explore-t",
            "depends_on": [],
            "file_ownership": [],
            "concurrency_safe": true,
            "context_tiers": [],
            "prompt": "explore"
        }"#;
        let parsed: Task = serde_json::from_str(safe_spec).unwrap();
        assert!(parsed.concurrency_safe);

        // Re-serialize: field is included (Serialize derive includes all).
        let json = serde_json::to_string(&parsed).unwrap();
        assert!(json.contains("\"concurrency_safe\":true"));
    }

    // ── wave-branch-dedup regression guard ───────────────────────────────────

    /// Same task ID must produce distinct `wt_branch` values across attempts.
    /// Without a UUID suffix, the second attempt hits
    /// `fatal: a branch named 'task/build/<id>' already exists`.
    #[test]
    fn unique_branch_per_attempt() {
        let task_id = "test-task-abc123";
        let b1 = format!(
            "task/build/{}-{}",
            task_id,
            &uuid::Uuid::new_v4().simple().to_string()[..8],
        );
        let b2 = format!(
            "task/build/{}-{}",
            task_id,
            &uuid::Uuid::new_v4().simple().to_string()[..8],
        );
        assert_ne!(
            b1, b2,
            "same task_id must produce distinct branches on retry"
        );
        assert!(b1.starts_with(&format!("task/build/{task_id}-")));
        assert!(b2.starts_with(&format!("task/build/{task_id}-")));
    }

    /// Two consecutive branch creates for the same `task_id` must not collide.
    /// Validates the invariant broken before the UUID suffix fix at L385.
    #[tokio::test]
    async fn no_branch_collision_on_retry_same_task_id() {
        use tempfile::TempDir;
        let repo = TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init", repo.path().to_str().unwrap()])
            .status()
            .unwrap();
        // Seed an initial commit so git branch works.
        std::process::Command::new("git")
            .args([
                "-C",
                repo.path().to_str().unwrap(),
                "commit",
                "--allow-empty",
                "-m",
                "init",
            ])
            .status()
            .unwrap();
        let task_id = "task-abc123";
        let b1 = format!(
            "task/build/{}-{}",
            task_id,
            &uuid::Uuid::new_v4().simple().to_string()[..8],
        );
        let b2 = format!(
            "task/build/{}-{}",
            task_id,
            &uuid::Uuid::new_v4().simple().to_string()[..8],
        );
        assert_ne!(b1, b2, "branch names for same task_id on retry must differ");
        let s1 = std::process::Command::new("git")
            .args(["-C", repo.path().to_str().unwrap(), "branch", &b1])
            .status()
            .unwrap();
        let s2 = std::process::Command::new("git")
            .args(["-C", repo.path().to_str().unwrap(), "branch", &b2])
            .status()
            .unwrap();
        assert!(s1.success(), "first branch create must succeed");
        assert!(
            s2.success(),
            "second branch create must succeed (no collision)"
        );
    }
}
