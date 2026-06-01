//! Worker executor abstraction — in-process vs Docker container dispatch.
//!
//! [`WorkerExecutor`] is the trait that [`crate::lightsquad::wave_dispatcher`]
//! calls once per task.  Two concrete implementations are provided:
//!
//! - [`InProcessExecutor`] — wraps the existing closure-based worker path.
//!   Selected when `docker_capable == DockerCapability::Absent` or when the
//!   `LA_WORKER_MODE=in-process` env var is set.
//!
//! - [`ContainerExecutor`] — runs each worker task inside a PolicyStore-enforced
//!   Docker container.  **Phase 3 stub**: policy + semaphore bookkeeping is wired;
//!   the actual `docker run` call is deferred to Phase 4 (webshell
//!   `container/worker_runner.rs`).  Callers receive
//!   [`WorkerError::NotImplemented`] until Phase 4 wires the spawn function.

use std::{future::Future, path::PathBuf, pin::Pin, sync::Arc};

use tokio::sync::Semaphore;

use crate::container_spawn::SpawnPolicy;

use super::wave_dispatcher::WorkerSpec;

// ── Return-type alias ──────────────────────────────────────────────────────────

/// Heap-allocated, `Send` future — the erased return type for
/// [`InProcessExecutor`]'s inner closure.
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

// ── Public types ───────────────────────────────────────────────────────────────

/// Summary produced by a successful worker execution.
#[derive(Debug, Default)]
pub struct WorkerOutcome {
    /// Files written by the worker, relative to the task worktree root.
    pub files_written: Vec<PathBuf>,
    /// Last 1 KiB of worker stdout/stderr for diagnostic logging.
    pub stdout_excerpt: String,
}

/// Errors from [`WorkerExecutor::dispatch_one`].
#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    /// The per-task policy override would loosen a resource limit or the
    /// effective policy is invalid.
    #[error("policy error: {0}")]
    Policy(#[from] crate::container_spawn::SpawnError),
    /// Docker I/O error (image pull failure, socket unavailable, etc.).
    #[error("docker I/O error: {0}")]
    Docker(#[source] std::io::Error),
    /// Worker exceeded the per-task wall-clock timeout.
    #[error("worker timed out")]
    Timeout,
    /// Worker exited with a non-zero code.
    #[error("worker exited with code {0}")]
    NonZeroExit(i32),
    /// The executor has not been fully wired yet (Phase 3 stub signal).
    #[error("executor not yet implemented — Phase 4 required")]
    NotImplemented,
}

/// Whether a [`WorkerExecutor`] dispatches tasks in-process or in containers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorMode {
    /// Tasks execute as Tokio async tasks inside the gateway process.
    InProcess,
    /// Tasks execute inside isolated Docker containers via `PolicyStore` rules.
    Container,
}

// ── Trait ──────────────────────────────────────────────────────────────────────

/// Execute a single [`WorkerSpec`] and return a [`WorkerOutcome`].
///
/// Implementations must be `Send + Sync` so they can be wrapped in
/// `Arc<dyn WorkerExecutor>` and shared across Tokio tasks.
#[async_trait::async_trait]
pub trait WorkerExecutor: Send + Sync + std::fmt::Debug {
    /// Dispatch the task described by `spec` and await its completion.
    ///
    /// # Errors
    ///
    /// Returns a [`WorkerError`] if the task fails to start, exceeds its
    /// wall-clock timeout, or exits with a non-zero code.
    async fn dispatch_one(&self, spec: WorkerSpec) -> Result<WorkerOutcome, WorkerError>;

    /// Report which dispatch strategy this executor uses.
    fn mode(&self) -> ExecutorMode;
}

// ── InProcessExecutor ──────────────────────────────────────────────────────────

/// Wraps the existing closure-based `worker_fn` path as a [`WorkerExecutor`].
///
/// This preserves behavioural parity with the pre-trait code path while
/// allowing `dispatch_wave` to accept `Arc<dyn WorkerExecutor>`.
pub struct InProcessExecutor {
    worker_fn: Box<dyn Fn(WorkerSpec) -> BoxFuture<Result<(), String>> + Send + Sync>,
}

impl InProcessExecutor {
    /// Wrap an arbitrary `F: Fn(WorkerSpec) -> Fut` as an [`InProcessExecutor`].
    pub fn new<F, Fut>(f: F) -> Self
    where
        F: Fn(WorkerSpec) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        Self {
            worker_fn: Box::new(move |spec| Box::pin(f(spec))),
        }
    }
}

impl std::fmt::Debug for InProcessExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InProcessExecutor").finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl WorkerExecutor for InProcessExecutor {
    async fn dispatch_one(&self, spec: WorkerSpec) -> Result<WorkerOutcome, WorkerError> {
        (self.worker_fn)(spec)
            .await
            .map(|()| WorkerOutcome::default())
            .map_err(|_| WorkerError::NonZeroExit(1))
    }

    fn mode(&self) -> ExecutorMode {
        ExecutorMode::InProcess
    }
}

// ── ContainerExecutor ─────────────────────────────────────────────────────────

/// Dispatches tasks inside PolicyStore-enforced Docker containers.
///
/// **Phase 3 stub**: policy loading and semaphore bookkeeping are wired; the
/// actual `docker run` invocation is deferred to Phase 4 where
/// `container/worker_runner.rs` in the webshell provides the spawn function.
///
/// Until [`WorkerError::NotImplemented`] is replaced by Phase 4 wiring,
/// all [`dispatch_one`][`WorkerExecutor::dispatch_one`] calls return an error.
/// The `LA_WORKER_MODE=in-process` env var falls back to [`InProcessExecutor`]
/// at the construction site (see `lightsquad_bridge::make_executor`).
pub struct ContainerExecutor {
    /// Shared system-wide spawn policy.  A single `ArcSwap` load per call
    /// (M10 idiom — snapshot once at entry, use throughout).
    pub policy: Arc<dyn SpawnPolicy>,
    /// Shared semaphore capping concurrent PTY + `WorkerTask` containers
    /// (deliberate — a busy autonomous build reduces PTY availability).
    pub semaphore: Arc<Semaphore>,
}

impl std::fmt::Debug for ContainerExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainerExecutor")
            .field("semaphore_available", &self.semaphore.available_permits())
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl WorkerExecutor for ContainerExecutor {
    /// Phase 3 stub: validates that the per-task policy override only tightens
    /// the effective policy, acquires a semaphore permit to reserve capacity,
    /// then returns [`WorkerError::NotImplemented`].
    ///
    /// Phase 4 replaces the `NotImplemented` return with a real `docker run`
    /// invocation via `container/worker_runner::spawn_worker_container`.
    async fn dispatch_one(&self, spec: WorkerSpec) -> Result<WorkerOutcome, WorkerError> {
        // Snapshot effective policy once (M10).
        let effective = self.policy.effective_policy();

        // If the task requests a policy override, validate it can only tighten.
        if let Some(ref override_req) = spec.task.policy_override {
            use crate::container_spawn::{IsoMode, NetworkPolicy};

            // Validate iso_mode: overrides must be equal or stricter.
            if let Some(ref_iso) = override_req.iso_mode {
                let iso_rank = |m: IsoMode| match m {
                    IsoMode::Standard => 0u8,
                    IsoMode::Hardened => 1,
                    IsoMode::Airgapped => 2,
                };
                if iso_rank(ref_iso) < iso_rank(effective.iso_mode) {
                    return Err(WorkerError::Policy(
                        crate::container_spawn::SpawnError::PolicyTighteningViolation(format!(
                            "per-task IsoMode {ref_iso:?} is less strict than system IsoMode {:?}",
                            effective.iso_mode
                        )),
                    ));
                }
            }

            // Validate network: overrides must be equal or stricter.
            if let Some(ref_net) = override_req.network {
                let net_rank = |n: NetworkPolicy| match n {
                    NetworkPolicy::Bridge | NetworkPolicy::Host => 0u8, // Host is not stricter than Bridge
                    NetworkPolicy::None => 2,
                    NetworkPolicy::Balanced => 1,
                };
                if net_rank(ref_net) < net_rank(effective.network) {
                    return Err(WorkerError::Policy(
                        crate::container_spawn::SpawnError::PolicyTighteningViolation(format!(
                            "per-task NetworkPolicy {ref_net:?} is less strict than system NetworkPolicy {:?}",
                            effective.network
                        )),
                    ));
                }
            }
        }

        // Reserve a container slot before spawning.
        let _permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| WorkerError::NotImplemented)?;

        // Phase 3 stub — actual docker run deferred to Phase 4 webshell wiring.
        // The semaphore permit is released on drop here.
        drop(effective);
        Err(WorkerError::NotImplemented)
    }

    fn mode(&self) -> ExecutorMode {
        ExecutorMode::Container
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::container_spawn::{
        ContainerPolicy, IsoMode, NetworkPolicy, SpawnError, SpawnPolicy,
    };
    use crate::lightsquad::types::TaskPolicyOverride;

    /// Minimal `SpawnPolicy` that returns a fixed `ContainerPolicy`.
    struct FixedPolicy(Arc<ContainerPolicy>);

    impl FixedPolicy {
        fn new(iso_mode: IsoMode, network: NetworkPolicy) -> Self {
            Self(Arc::new(ContainerPolicy {
                iso_mode,
                network,
                ..ContainerPolicy::default()
            }))
        }
    }

    impl SpawnPolicy for FixedPolicy {
        fn effective_policy(&self) -> Arc<ContainerPolicy> {
            Arc::clone(&self.0)
        }

        fn tighten_for_build(
            &self,
            _override_policy: &ContainerPolicy,
        ) -> Result<Arc<ContainerPolicy>, SpawnError> {
            Ok(Arc::clone(&self.0))
        }

        fn update_system_policy(&self, _new: ContainerPolicy) -> Result<(), SpawnError> {
            Ok(())
        }
    }

    fn make_spec(
        policy_override: Option<crate::lightsquad::types::TaskPolicyOverride>,
    ) -> WorkerSpec {
        use crate::lightsquad::types::Task;
        WorkerSpec {
            task: Task {
                id: "t1".to_owned(),
                branch: "task/t1".to_owned(),
                depends_on: vec![],
                file_ownership: vec![],
                concurrency_safe: false,
                context_tiers: vec![],
                prompt: "test".to_owned(),
                policy_override,
            },
            wave_index: 0,
            worktree_path: std::path::PathBuf::from("/tmp/t1"),
        }
    }

    #[tokio::test]
    async fn in_process_executor_success() {
        let exec = InProcessExecutor::new(|_spec| async { Ok(()) });
        let result = exec.dispatch_one(make_spec(None)).await;
        assert!(result.is_ok());
        assert_eq!(exec.mode(), ExecutorMode::InProcess);
    }

    #[tokio::test]
    async fn in_process_executor_failure_maps_to_nonzero_exit() {
        let exec = InProcessExecutor::new(|_spec| async { Err("worker error".to_owned()) });
        let result = exec.dispatch_one(make_spec(None)).await;
        assert!(matches!(result, Err(WorkerError::NonZeroExit(1))));
    }

    #[tokio::test]
    async fn container_executor_no_override_returns_not_implemented() {
        let exec = ContainerExecutor {
            policy: Arc::new(FixedPolicy::new(IsoMode::Standard, NetworkPolicy::Bridge)),
            semaphore: Arc::new(Semaphore::new(4)),
        };
        let result = exec.dispatch_one(make_spec(None)).await;
        assert!(matches!(result, Err(WorkerError::NotImplemented)));
        assert_eq!(exec.mode(), ExecutorMode::Container);
    }

    #[tokio::test]
    async fn container_executor_tightening_override_accepted() {
        let exec = ContainerExecutor {
            policy: Arc::new(FixedPolicy::new(IsoMode::Standard, NetworkPolicy::Bridge)),
            semaphore: Arc::new(Semaphore::new(4)),
        };
        // Hardened > Standard — tightening is allowed.
        let ov = TaskPolicyOverride {
            iso_mode: Some(IsoMode::Hardened),
            ..TaskPolicyOverride::default()
        };
        let result = exec.dispatch_one(make_spec(Some(ov))).await;
        assert!(matches!(result, Err(WorkerError::NotImplemented)));
    }

    #[tokio::test]
    async fn container_executor_loosening_iso_rejected() {
        let exec = ContainerExecutor {
            policy: Arc::new(FixedPolicy::new(IsoMode::Hardened, NetworkPolicy::Bridge)),
            semaphore: Arc::new(Semaphore::new(4)),
        };
        // Standard < Hardened — loosening must be rejected.
        let ov = TaskPolicyOverride {
            iso_mode: Some(IsoMode::Standard),
            ..TaskPolicyOverride::default()
        };
        let result = exec.dispatch_one(make_spec(Some(ov))).await;
        assert!(matches!(result, Err(WorkerError::Policy(_))));
    }

    #[tokio::test]
    async fn container_executor_semaphore_exhausted_returns_error() {
        let exec = ContainerExecutor {
            policy: Arc::new(FixedPolicy::new(IsoMode::Standard, NetworkPolicy::Bridge)),
            semaphore: Arc::new(Semaphore::new(0)),
        };
        // acquire_owned on a closed semaphore returns NotImplemented.
        let sem = Arc::clone(&exec.semaphore);
        sem.close();
        let result = exec.dispatch_one(make_spec(None)).await;
        assert!(matches!(result, Err(WorkerError::NotImplemented)));
    }
}
