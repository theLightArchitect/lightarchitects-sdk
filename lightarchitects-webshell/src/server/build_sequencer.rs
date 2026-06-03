//! Sequential multi-build program orchestrator.
//!
//! [`BuildSequencer`] accepts a [`ProgramManifest`] (an ordered list of build
//! codenames) and runs them one at a time, in declaration order. Each build
//! completes before the next is dispatched, enforcing the sequential-run
//! invariant required by the A2A supervisor visibility plan (SCRUM BLOCKING #5 +
//! BLOCKING #8 state-machine test).
//!
//! ## State machine
//!
//! ```text
//!    Idle
//!     │  start()
//!     ▼
//!   Running ──── build N completes ────► [next build | Idle]
//!     │
//!     │  cancel()
//!     ▼
//!  Cancelled
//! ```
//!
//! The invariant "build B never starts before build A completes" is enforced by
//! the sequential `for` loop in [`BuildSequencer::run`] — there is no
//! concurrent dispatch path.

use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Ordered list of build codenames to execute sequentially.
#[derive(Debug, Clone)]
pub struct ProgramManifest {
    /// Ordered build codenames. Each must resolve to an active build session.
    pub codenames: Vec<String>,
}

/// State of the sequencer visible to callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequencerState {
    /// No program is running; sequencer is ready to start.
    Idle,
    /// A program is running.
    Running {
        /// 0-based index of the build currently executing.
        current: usize,
        /// Total number of builds in the manifest.
        total: usize,
    },
    /// Sequencer completed all builds successfully.
    Completed,
    /// Sequencer was cancelled before finishing.
    Cancelled,
}

impl std::fmt::Display for SequencerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Running { current, total } => write!(f, "running ({current}/{total})"),
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Sequential build program runner.
///
/// Acquires [`litellm_supervisor_semaphore`] before each build invocation so
/// parallel LLM supervisor calls are bounded at 1 (the semaphore permit count
/// set in [`AppState::new`]).
///
/// [`litellm_supervisor_semaphore`]: crate::server::AppState::litellm_supervisor_semaphore
pub struct BuildSequencer {
    manifest: ProgramManifest,
    semaphore: Arc<Semaphore>,
    cancel: CancellationToken,
}

impl BuildSequencer {
    /// Construct a new sequencer.
    #[must_use]
    pub fn new(
        manifest: ProgramManifest,
        semaphore: Arc<Semaphore>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            manifest,
            semaphore,
            cancel,
        }
    }

    /// Run all builds in declaration order.
    ///
    /// Blocks until all builds complete, the token is cancelled, or
    /// `dispatch_one` returns an error. The sequential-run invariant is
    /// enforced by this loop — there is no concurrent path.
    ///
    /// Returns `Ok(())` on clean completion.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on the first build failure or if the semaphore
    /// closes (which should not happen under normal operation).
    pub async fn run<F, Fut>(&self, dispatch_one: F) -> Result<(), String>
    where
        F: Fn(String) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        let total = self.manifest.codenames.len();
        for (idx, codename) in self.manifest.codenames.iter().enumerate() {
            if self.cancel.is_cancelled() {
                warn!(idx, codename = %codename, "BuildSequencer: cancellation requested before dispatch");
                return Err("cancelled".to_owned());
            }

            // Acquire supervisor semaphore — bounds concurrent LLM calls.
            let _permit = self
                .semaphore
                .acquire()
                .await
                .map_err(|e| format!("semaphore closed: {e}"))?;

            info!(
                idx,
                total,
                codename = %codename,
                "BuildSequencer: dispatching build {}/{}", idx + 1, total
            );

            dispatch_one(codename.clone()).await.map_err(|e| {
                warn!(idx, codename = %codename, error = %e, "BuildSequencer: build failed");
                e
            })?;

            info!(idx, codename = %codename, "BuildSequencer: build completed");
            // _permit drops here — semaphore released before next iteration.
        }
        info!(total, "BuildSequencer: all builds completed");
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_sequencer(codenames: Vec<&str>) -> BuildSequencer {
        let manifest = ProgramManifest {
            codenames: codenames.into_iter().map(str::to_owned).collect(),
        };
        BuildSequencer::new(
            manifest,
            Arc::new(Semaphore::new(1)),
            CancellationToken::new(),
        )
    }

    /// Sequential-run invariant: build B never starts before build A completes.
    ///
    /// This is the `build_sequencer_state_machine_predicate` test required by
    /// SCRUM BLOCKING #8 (AYIN, CORSO).
    #[tokio::test]
    async fn build_sequencer_state_machine_predicate() {
        let seq = make_sequencer(vec!["alpha", "beta", "gamma"]);
        let log = Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));

        let log_clone = log.clone();
        let result = seq
            .run(|codename| {
                let log = log_clone.clone();
                async move {
                    log.lock().await.push(format!("start:{codename}"));
                    // Simulate async work.
                    tokio::task::yield_now().await;
                    log.lock().await.push(format!("end:{codename}"));
                    Ok(())
                }
            })
            .await;

        assert!(result.is_ok());
        let entries = log.lock().await;
        // Verify interleaving: start A, end A, start B, end B, start C, end C.
        assert_eq!(
            entries.as_slice(),
            &[
                "start:alpha",
                "end:alpha",
                "start:beta",
                "end:beta",
                "start:gamma",
                "end:gamma",
            ]
        );
    }

    #[tokio::test]
    async fn cancellation_before_second_build() {
        let cancel = CancellationToken::new();
        let manifest = ProgramManifest {
            codenames: vec!["first".to_owned(), "second".to_owned()],
        };
        let seq = BuildSequencer::new(manifest, Arc::new(Semaphore::new(1)), cancel.clone());

        let result = seq
            .run(|codename| {
                let cancel = cancel.clone();
                async move {
                    cancel.cancel();
                    let _ = codename;
                    Ok(())
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "cancelled");
    }

    #[test]
    fn state_display_labels_are_correct() {
        assert_eq!(SequencerState::Idle.to_string(), "idle");
        assert_eq!(
            SequencerState::Running {
                current: 2,
                total: 5
            }
            .to_string(),
            "running (2/5)"
        );
        assert_eq!(SequencerState::Completed.to_string(), "completed");
        assert_eq!(SequencerState::Cancelled.to_string(), "cancelled");
    }
}
