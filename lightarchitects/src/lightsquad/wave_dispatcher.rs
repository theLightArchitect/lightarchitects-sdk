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
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::{sync::RwLock, task::JoinSet};

use super::{
    merge_agent::{MergeAgent, MergeError},
    types::{Coordinator, SharedState, Task, TaskStatus},
    worktree_manager::{WorktreeError, WorktreeManager},
};

/// Maximum concurrent worker slots per IRONCLAW spec §7-Slot Agent Pool.
pub const SLOT_CAPACITY: usize = 7;

// ── Public types ──────────────────────────────────────────────────────────────

/// Bundle passed to a worker function for a single task.
#[derive(Debug, Clone)]
pub struct WorkerSpec {
    /// Task being executed.
    pub task: Task,
    /// Absolute path of the worktree the worker should operate in.
    pub worktree_path: PathBuf,
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
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Dispatch all ready tasks in `tasks` as a concurrent wave.
///
/// `worker_fn` receives a [`WorkerSpec`] and returns `Ok(())` on success or
/// `Err(String)` with an error summary on failure. The dispatcher merges
/// successful workers and removes all worktrees unconditionally.
///
/// Concurrency is bounded to [`SLOT_CAPACITY`] live workers.
///
/// # Errors
///
/// Returns [`WaveError::TaskFailures`] if any task fails. Returns
/// [`WaveError::Worktree`] or [`WaveError::Merge`] on infrastructure failures.
#[allow(clippy::too_many_lines)]
pub async fn dispatch_wave<F, Fut>(
    tasks: &[Task],
    coordinator: &Coordinator,
    worktree_manager: &WorktreeManager,
    merge_agent: &MergeAgent,
    feat_branch: &str,
    worktree_root: &Path,
    worker_fn: F,
) -> Result<WaveResult, WaveError>
where
    F: Fn(WorkerSpec) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = Result<(), String>> + Send + 'static,
{
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

    let mut join_set: JoinSet<TaskOutcome> = JoinSet::new();
    let mut slot_count: usize = 0;
    let mut task_index = 0;
    let mut result = WaveResult::default();

    // Draining loop: fill slots, process completions, refill.
    loop {
        // Fill available slots.
        while slot_count < SLOT_CAPACITY && task_index < ready.len() {
            let task = ready[task_index].clone();
            task_index += 1;

            let wt_branch = format!("task/build/{}", task.id);
            let wt_path = worktree_root.join(&task.id);

            // Create worktree (serialised through ops_mutex inside create).
            let handle = worktree_manager.create(&wt_branch, &wt_path).await?;

            // Mark InProgress.
            mark_task(&coordinator.state, &task.id, TaskStatus::InProgress).await;

            let spec = WorkerSpec {
                task: task.clone(),
                worktree_path: handle.path.clone(),
            };

            let worker = worker_fn.clone();
            let task_id = task.id.clone();
            let branch = handle.branch.clone();

            join_set.spawn(async move {
                let outcome = worker(spec).await;
                TaskOutcome {
                    task_id,
                    branch,
                    worktree_path: handle.path,
                    result: outcome,
                }
            });
            slot_count += 1;
        }

        // If nothing is running or queued, we are done.
        if join_set.is_empty() {
            break;
        }

        // Wait for the next worker to finish.
        let Some(join_result) = join_set.join_next().await else {
            break;
        };
        slot_count = slot_count.saturating_sub(1);

        let outcome = match join_result {
            Ok(o) => o,
            Err(e) => {
                // JoinError — task panicked. Treat as failure.
                result.failed += 1;
                result.failed_ids.push(format!("panic: {e}"));
                continue;
            }
        };

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
    result: Result<(), String>,
}

/// Write a task status transition to shared state.
async fn mark_task(state: &Arc<RwLock<SharedState>>, task_id: &str, status: TaskStatus) {
    let mut s = state.write().await;
    s.tasks.insert(task_id.to_owned(), status);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
}
