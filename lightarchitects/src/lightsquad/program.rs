//! Build program — top-level orchestrator for a lightsquad autonomous build.
//!
//! [`Program`] owns the full lifecycle of one build: preflight, wave dispatch,
//! merge sequencing, and teardown. It is the public entry point that callers
//! construct and then drive with [`Program::run`].
//!
//! # Lifecycle
//!
//! ```text
//! Program::new(manifest, repo_root, worktree_root, feat_branch)
//!   └─ Program::run(worker_fn)
//!        ├─ preflight::disk()
//!        ├─ For each wave:
//!        │    ├─ dispatch_wave(ready_tasks, ..., worker_fn)  ← concurrent
//!        │    └─ on WaveError::TaskFailures → mark build Failed + return
//!        └─ mark build Complete
//! ```
//!
//! # Build status transitions
//!
//! `Pending → Running → [Gating →]* Complete | Failed`
//!
//! The `Gating` state is entered when the calling layer (e.g. the CLI gate
//! loop) inserts a pause between waves. `Program` itself does not implement
//! gating — it advances waves sequentially and exposes the build codename so
//! external gate logic can read/write `SharedState`.

use std::{future::Future, path::PathBuf};

use crate::lightsquad::{
    merge_agent::MergeAgent,
    types::{BuildStatus, Coordinator, Task, TaskStatus},
    wave_dispatcher::{self, WaveError, WaveResult, WorkerSpec},
    worktree_manager::WorktreeManager,
};

/// Configuration for a single autonomous build program.
#[derive(Debug, Clone)]
pub struct ProgramConfig {
    /// Human-readable codename for this build (e.g. `"ironclaw-spine"`).
    pub codename: String,
    /// Absolute path of the primary repository.
    pub repo_root: PathBuf,
    /// Directory under which per-task worktrees are created.
    ///
    /// Each task gets `<worktree_root>/<task_id>/`.
    pub worktree_root: PathBuf,
    /// Feature branch that workers merge back into (e.g. `"feat/ironclaw-spine"`).
    pub feat_branch: String,
    /// Ordered list of waves. Each inner `Vec` is dispatched concurrently;
    /// the outer ordering is sequential (wave N+1 starts only after wave N
    /// completes).
    pub waves: Vec<Vec<Task>>,
}

/// Top-level orchestrator for one lightsquad autonomous build.
pub struct Program {
    config: ProgramConfig,
    coordinator: Coordinator,
    worktree_manager: WorktreeManager,
    merge_agent: MergeAgent,
}

impl Program {
    /// Create a new [`Program`] from `config`.
    ///
    /// All sub-components share the same `ops_mutex` from the coordinator so
    /// no two ref-mutating git operations run concurrently.
    #[must_use]
    pub fn new(config: ProgramConfig) -> Self {
        let coordinator = Coordinator::new();
        let worktree_manager =
            WorktreeManager::new(coordinator.ops_mutex.clone(), config.repo_root.clone());
        let merge_agent = MergeAgent::new(coordinator.ops_mutex.clone(), config.repo_root.clone());
        Self {
            config,
            coordinator,
            worktree_manager,
            merge_agent,
        }
    }

    /// Return a clone of the coordinator handle (for external status polling).
    #[must_use]
    pub fn coordinator(&self) -> Coordinator {
        self.coordinator.clone()
    }

    /// Run the full build.
    ///
    /// `worker_fn` is called once per task with a [`WorkerSpec`] containing
    /// the task definition and its exclusive worktree path. It returns `Ok(())`
    /// on success or `Err(String)` with an error summary on failure.
    ///
    /// # Errors
    ///
    /// Returns the first [`WaveError`] that causes the build to halt.
    /// Build status in `SharedState` reflects the terminal state
    /// (`Complete` or `Failed`) before this function returns.
    pub async fn run<F, Fut>(&self, worker_fn: F) -> Result<BuildSummary, WaveError>
    where
        F: Fn(WorkerSpec) -> Fut + Clone + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        // Seed all task statuses as Pending.
        {
            let mut state = self.coordinator.state.write().await;
            for wave in &self.config.waves {
                for task in wave {
                    state.tasks.insert(task.id.clone(), TaskStatus::Pending);
                }
            }
            state
                .builds
                .insert(self.config.codename.clone(), BuildStatus::Running);
        }

        let mut total_succeeded: u32 = 0;
        let mut total_failed: u32 = 0;

        for (wave_idx, wave) in self.config.waves.iter().enumerate() {
            let result = wave_dispatcher::dispatch_wave(
                wave,
                &self.coordinator,
                &self.worktree_manager,
                &self.merge_agent,
                &self.config.feat_branch,
                &self.config.worktree_root,
                worker_fn.clone(),
            )
            .await;

            match result {
                Ok(WaveResult {
                    succeeded,
                    failed,
                    failed_ids,
                }) => {
                    total_succeeded += succeeded;
                    total_failed += failed;
                    if failed > 0 {
                        self.set_build_status(BuildStatus::Failed).await;
                        return Err(WaveError::TaskFailures(total_failed));
                    }
                    let _ = (wave_idx, failed_ids);
                }
                Err(WaveError::TaskFailures(n)) => {
                    total_failed += n;
                    self.set_build_status(BuildStatus::Failed).await;
                    return Err(WaveError::TaskFailures(total_failed));
                }
                Err(e) => {
                    self.set_build_status(BuildStatus::Failed).await;
                    return Err(e);
                }
            }
        }

        self.set_build_status(BuildStatus::Complete).await;
        Ok(BuildSummary {
            codename: self.config.codename.clone(),
            succeeded: total_succeeded,
            failed: total_failed,
        })
    }

    async fn set_build_status(&self, status: BuildStatus) {
        let mut state = self.coordinator.state.write().await;
        state.builds.insert(self.config.codename.clone(), status);
    }
}

/// Summary returned by [`Program::run`] on a successful build.
#[derive(Debug, Clone)]
pub struct BuildSummary {
    /// Build codename.
    pub codename: String,
    /// Total tasks that merged successfully.
    pub succeeded: u32,
    /// Total tasks that failed.
    pub failed: u32,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_config(waves: Vec<Vec<Task>>) -> ProgramConfig {
        ProgramConfig {
            codename: "test-build".to_owned(),
            repo_root: PathBuf::from("/tmp/test-repo"),
            worktree_root: PathBuf::from("/tmp/test-worktrees"),
            feat_branch: "feat/test-build".to_owned(),
            waves,
        }
    }

    #[test]
    fn program_new_does_not_panic() {
        let config = make_config(vec![]);
        let _ = Program::new(config);
    }

    #[test]
    fn build_summary_fields() {
        let s = BuildSummary {
            codename: "foo".to_owned(),
            succeeded: 3,
            failed: 0,
        };
        assert_eq!(s.codename, "foo");
        assert_eq!(s.succeeded, 3);
        assert_eq!(s.failed, 0);
    }

    #[tokio::test]
    async fn run_empty_waves_completes() {
        let config = make_config(vec![]);
        let program = Program::new(config);
        let result = program.run(|_spec| async { Ok(()) }).await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.succeeded, 0);
        assert_eq!(summary.failed, 0);
    }
}
