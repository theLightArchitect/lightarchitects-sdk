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

use std::{path::PathBuf, sync::Arc};

use uuid::Uuid;

use crate::lightsquad::{
    merge_agent::MergeAgent,
    types::{BuildStatus, Coordinator, Task, TaskStatus},
    wave_dispatcher::{self, WaveError, WaveResult},
    worker_executor::WorkerExecutor,
    worktree_manager::WorktreeManager,
};

/// Webshell endpoint config for §3.5 `IMPLEMENTATION_COMPLETE` attestations.
///
/// When present in [`ProgramConfig`], a fire-and-forget POST is sent to
/// `POST /api/builds/{build_id}/attestation` after every successful wave.
/// Failures are logged but never propagate — monitoring must not block builds.
#[derive(Debug, Clone)]
pub struct AttestationConfig {
    /// Webshell base URL, e.g. `"http://127.0.0.1:8733"`.
    pub webshell_url: String,
    /// Build session UUID — identifies the SSE channel the webshell broadcasts on.
    pub build_id: Uuid,
    /// Global bearer token for `Authorization: Bearer <token>`.
    pub bearer_token: String,
    /// Absolute path of the primary repository — used to resolve the post-merge
    /// HEAD SHA for wave-boundary attestations.
    pub repo_root: PathBuf,
    /// Feature branch name — used to resolve `refs/heads/<feat_branch>` HEAD.
    pub feat_branch: String,
}

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
    /// Executor used for every task in every wave.
    ///
    /// Typically [`InProcessExecutor`] (Phase 3) or [`ContainerExecutor`] (Phase 4+).
    /// Set at construction time; shared across all waves via `Arc::clone`.
    ///
    /// [`InProcessExecutor`]: super::worker_executor::InProcessExecutor
    /// [`ContainerExecutor`]: super::worker_executor::ContainerExecutor
    pub executor: Arc<dyn WorkerExecutor>,
    /// If set, a §3.5 `IMPLEMENTATION_COMPLETE` attestation is fire-and-forget
    /// sent to the webshell after each successful wave. Errors are logged but
    /// never propagate — attestation is a monitoring side-channel, not a gate.
    pub attestation: Option<AttestationConfig>,
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

    /// Run the full build using the executor from [`ProgramConfig::executor`].
    ///
    /// Each task receives a [`WorkerSpec`] via [`WorkerExecutor::dispatch_one`].
    /// The executor is `Arc`-cloned per wave so concurrent waves share the same
    /// underlying executor instance.
    ///
    /// # Errors
    ///
    /// Returns the first [`WaveError`] that causes the build to halt.
    /// Build status in `SharedState` reflects the terminal state
    /// (`Complete` or `Failed`) before this function returns.
    pub async fn run(&self) -> Result<BuildSummary, WaveError> {
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
                wave_idx,
                wave,
                &self.coordinator,
                &self.worktree_manager,
                &self.merge_agent,
                &self.config.feat_branch,
                &self.config.worktree_root,
                Arc::clone(&self.config.executor),
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
                    let _ = failed_ids;
                    // Fire-and-forget §3.5 attestation — errors never block the build.
                    if let Some(ref cfg) = self.config.attestation {
                        // Wave index is bounded by plan size (always < 100).
                        #[allow(clippy::cast_possible_truncation)]
                        post_wave_attestation(cfg, wave_idx as u32, succeeded).await;
                    }
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

/// POST a §3.5 `IMPLEMENTATION_COMPLETE` attestation to the webshell.
///
/// Fire-and-forget: errors are logged at WARN but never propagate.
/// The 5-second timeout prevents stalling if the webshell is unreachable.
async fn post_wave_attestation(cfg: &AttestationConfig, wave: u32, tasks_succeeded: u32) {
    // Resolve the post-merge HEAD on the feat branch — reflects the actual merge commit.
    let commit_sha = tokio::process::Command::new("git")
        .args(["rev-parse", &format!("refs/heads/{}", cfg.feat_branch)])
        .current_dir(&cfg.repo_root)
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_owned(), |s| s.trim().to_owned());

    let url = format!(
        "{}/api/builds/{}/attestation",
        cfg.webshell_url, cfg.build_id
    );
    let body = serde_json::json!({
        "wave": wave,
        "task_id": format!("wave-{wave}-boundary"),
        "agent_id": "lightsquad/wave-dispatcher",
        "commit_sha": commit_sha,
        "gates_passed": [],
        "gates_skipped": [],
        "ayin_spans_dropped_total": 0_u64,
        "trust_boundary": "unverified_pre_2.10",
        "confidence": 1.0_f32,
    });
    let _ = tasks_succeeded; // available for richer payloads in a future BUILD
    match reqwest::Client::new()
        .post(&url)
        .bearer_auth(&cfg.bearer_token)
        .json(&body)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => {
            tracing::info!(
                build_id = %cfg.build_id,
                wave,
                "§3.5 attestation broadcast"
            );
        }
        Ok(r) => {
            tracing::warn!(
                build_id = %cfg.build_id,
                wave,
                status = %r.status(),
                "§3.5 attestation POST non-2xx"
            );
        }
        Err(e) => {
            tracing::warn!(
                build_id = %cfg.build_id,
                wave,
                error = %e,
                "§3.5 attestation POST failed"
            );
        }
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
        use crate::lightsquad::worker_executor::InProcessExecutor;
        ProgramConfig {
            codename: "test-build".to_owned(),
            repo_root: PathBuf::from("/tmp/test-repo"),
            worktree_root: PathBuf::from("/tmp/test-worktrees"),
            feat_branch: "feat/test-build".to_owned(),
            waves,
            executor: Arc::new(InProcessExecutor::new(|_spec| async { Ok(()) })),
            attestation: None,
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
        let result = program.run().await;
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.succeeded, 0);
        assert_eq!(summary.failed, 0);
    }
}
