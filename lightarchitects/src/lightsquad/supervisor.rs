//! Supervisor — long-running Tokio task monitoring the `ironclaw-hitl` channel.
//!
//! The Supervisor is the single authority that runs [`DecisionPipeline::evaluate`]
//! on every worker escalation and appends the verdict to the HMAC-chained decision
//! log. Workers communicate via a bounded `tokio::sync::mpsc` channel, embedding a
//! `oneshot` sender so the Supervisor can return the verdict asynchronously without
//! a separate reply channel.
//!
//! # Channel protocol
//!
//! ```text
//! Worker                        Supervisor
//!   │                                │
//!   │─ HitlEscalation { ctx, tx } ──►│
//!   │                                │── evaluate(ctx) ──► PipelineResult
//!   │                                │── chain.append(entry)
//!   │◄─────── tx.send(result) ───────│
//! ```
//!
//! # Phase 2 scope
//!
//! - Channel type aliases and `HitlEscalation` event type.
//! - `Supervisor::new` + `Supervisor::run` (Tokio spawn).
//! - Decision appended to [`HashChain`] after every evaluation.
//! - `poll_tick` span emitted for AYIN D8 compression benchmark (Phase 6).
//!
//! Phase 4 adds:
//! - `IronclawHitlEscalationEvent` SSE serialisation (nonce + traceparent).
//! - `HitlResolution` HTTP response from the browser operator.
//! - Real `PlatformClient` for Layers 1–3 canon/Northstar resolution.

use std::path::PathBuf;

use chrono::Utc;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, instrument, warn};

use crate::lightsquad::{
    decision_pipeline::{DecisionContext, DecisionPipeline, PipelineResult},
    decisions::hash_chain::{ChainError, DecisionEntry, DecisionLayer, HashChain},
    program::ProgramConfig,
};

// ─── Channel types ────────────────────────────────────────────────────────────

/// Capacity of the `ironclaw-hitl` channel.
///
/// Bounded at 64 so a misbehaving worker pool cannot accumulate unbounded
/// pending escalations. Workers block on `send` when the channel is full,
/// which back-pressures the wave dispatcher.
pub const HITL_CHANNEL_CAPACITY: usize = 64;

/// Sender half of the `ironclaw-hitl` channel.
pub type IronclawHitlTx = mpsc::Sender<HitlEscalation>;

/// Receiver half of the `ironclaw-hitl` channel.
pub type IronclawHitlRx = mpsc::Receiver<HitlEscalation>;

/// Create an `ironclaw-hitl` channel pair.
#[must_use]
pub fn hitl_channel() -> (IronclawHitlTx, IronclawHitlRx) {
    mpsc::channel(HITL_CHANNEL_CAPACITY)
}

// ─── HitlEscalation ──────────────────────────────────────────────────────────

/// An escalation event sent from a worker to the Supervisor via the `ironclaw-hitl` channel.
///
/// The worker suspends until the Supervisor replies via `respond`. The oneshot
/// sender is `Option` so the Supervisor can send the verdict exactly once and
/// detect if the worker has already dropped the receiver (timed out / cancelled).
pub struct HitlEscalation {
    /// Task that originated this escalation.
    pub task_id: String,
    /// The decision context to be evaluated by the pipeline.
    pub context: DecisionContext,
    /// W3C `traceparent` header value propagated from the worker's span.
    ///
    /// Phase 4 uses this to continue the AYIN trace through the SSE round-trip.
    /// Phase 2: carried as-is for logging; no span stitching yet.
    pub traceparent: Option<String>,
    /// One-shot sender the Supervisor uses to return the pipeline verdict.
    pub respond: oneshot::Sender<PipelineResult>,
}

impl std::fmt::Debug for HitlEscalation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HitlEscalation")
            .field("task_id", &self.task_id)
            .field("context_description", &self.context.description)
            .field("traceparent", &self.traceparent)
            .finish_non_exhaustive()
    }
}

// ─── SupervisorConfig ─────────────────────────────────────────────────────────

/// Configuration for a [`Supervisor`] instance.
#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    /// Build codename — used in log spans and the decision-log file name.
    pub codename: String,
    /// Directory where `decisions-<codename>.ndjson` is written.
    pub decisions_dir: PathBuf,
    /// 32-byte HMAC key for the decision chain.
    ///
    /// Callers derive this from the per-wave HKDF subkey (see `lightsquad::hmac`).
    /// For tests, any fixed 32-byte value is acceptable.
    pub chain_key: [u8; 32],
}

impl SupervisorConfig {
    /// Derive a `SupervisorConfig` from a [`ProgramConfig`].
    ///
    /// The decision log is written to `<worktree_root>/decisions/`.
    #[must_use]
    pub fn from_program(config: &ProgramConfig, chain_key: [u8; 32]) -> Self {
        Self {
            codename: config.codename.clone(),
            decisions_dir: config.worktree_root.join("decisions"),
            chain_key,
        }
    }
}

// ─── Supervisor ───────────────────────────────────────────────────────────────

/// Long-running Tokio task that evaluates `ironclaw-hitl` escalations.
///
/// Spawn with [`Supervisor::run`], which moves `self` into a `tokio::task::spawn`
/// call and returns a `JoinHandle`. The task exits cleanly when the `IronclawHitlTx`
/// half of the channel is dropped (all workers have finished or the wave dispatcher
/// shut down).
pub struct Supervisor {
    config: SupervisorConfig,
    hitl_rx: IronclawHitlRx,
    pipeline: DecisionPipeline,
}

impl Supervisor {
    /// Create a new Supervisor.
    ///
    /// The `HashChain` is opened (or created) lazily on [`Supervisor::run`] so
    /// filesystem errors surface at task startup, not at construction time.
    #[must_use]
    pub fn new(config: SupervisorConfig, hitl_rx: IronclawHitlRx) -> Self {
        Self {
            config,
            hitl_rx,
            pipeline: DecisionPipeline::new(),
        }
    }

    /// Spawn the supervisor as a background Tokio task.
    ///
    /// The task runs until the sender side of `hitl_rx` is dropped. Each
    /// [`HitlEscalation`] is:
    ///
    /// 1. Evaluated by [`DecisionPipeline::evaluate`].
    /// 2. Appended to the HMAC-chained decision log.
    /// 3. Returned to the worker via `escalation.respond`.
    ///
    /// A `poll_tick` tracing span is emitted for every evaluation — AYIN can
    /// aggregate these for the D8 compression benchmark (Canon XXXVI).
    pub fn run(mut self) -> tokio::task::JoinHandle<Result<(), ChainError>> {
        tokio::spawn(async move {
            let log_path = self
                .config
                .decisions_dir
                .join(format!("decisions-{}.ndjson", self.config.codename));

            let mut chain = HashChain::open(&log_path, self.config.chain_key)?;

            info!(
                codename = %self.config.codename,
                log = %log_path.display(),
                "supervisor started"
            );

            while let Some(escalation) = self.hitl_rx.recv().await {
                let result = self.handle_escalation(&escalation, &mut chain);

                if escalation.respond.send(result).is_err() {
                    warn!(
                        task_id = %escalation.task_id,
                        "worker dropped the oneshot receiver before verdict arrived"
                    );
                }
            }

            info!(codename = %self.config.codename, "supervisor exiting — channel closed");
            Ok(())
        })
    }

    // ── Private ──────────────────────────────────────────────────────────────

    #[instrument(
        name = "supervisor.poll_tick",
        skip(self, chain),
        fields(
            task_id = %escalation.task_id,
            action = ?escalation.context.action_kind,
        )
    )]
    fn handle_escalation(
        &self,
        escalation: &HitlEscalation,
        chain: &mut HashChain,
    ) -> PipelineResult {
        let result = self.pipeline.evaluate(&escalation.context);

        let (layer, decision_text) = match &result {
            PipelineResult::Approved { layer, citation } => {
                let text = citation
                    .as_deref()
                    .map_or_else(|| "APPROVED".to_owned(), |c| format!("APPROVED — {c}"));
                (layer.clone(), text)
            }
            PipelineResult::Blocked {
                reason,
                layer,
                citation,
            } => {
                let text = citation.as_deref().map_or_else(
                    || format!("BLOCKED — {reason}"),
                    |c| format!("BLOCKED — {reason} ({c})"),
                );
                (layer.clone(), text)
            }
            PipelineResult::UserEscalation { reason, .. } => {
                (DecisionLayer::User, format!("ESCALATE — {reason}"))
            }
        };

        let entry = DecisionEntry {
            seq: 0, // overwritten by HashChain::append
            timestamp: Utc::now(),
            layer,
            question: escalation.context.description.clone(),
            decision: decision_text,
            citation: None,
            prev_hash: None,       // overwritten by append
            entry_hash: [0u8; 32], // overwritten by append
        };

        if let Err(e) = chain.append(entry) {
            warn!(
                task_id = %escalation.task_id,
                error = %e,
                "failed to append decision to chain — verdict still returned"
            );
        }

        result
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;
    use crate::lightsquad::decision_pipeline::ActionKind;

    fn test_config(dir: &TempDir) -> SupervisorConfig {
        SupervisorConfig {
            codename: "test-build".to_owned(),
            decisions_dir: dir.path().to_path_buf(),
            chain_key: [0xAB; 32],
        }
    }

    fn safe_escalation(task_id: &str) -> (HitlEscalation, oneshot::Receiver<PipelineResult>) {
        let (respond, rx) = oneshot::channel();
        let escalation = HitlEscalation {
            task_id: task_id.to_owned(),
            context: DecisionContext {
                task_id: task_id.to_owned(),
                description: "write src/lib.rs".to_owned(),
                action_kind: ActionKind::FileWrite,
                file_paths: vec![PathBuf::from("src/lib.rs")],
            },
            traceparent: None,
            respond,
        };
        (escalation, rx)
    }

    fn dep_escalation(task_id: &str) -> (HitlEscalation, oneshot::Receiver<PipelineResult>) {
        let (respond, rx) = oneshot::channel();
        let escalation = HitlEscalation {
            task_id: task_id.to_owned(),
            context: DecisionContext {
                task_id: task_id.to_owned(),
                description: "add serde-evil to Cargo.toml".to_owned(),
                action_kind: ActionKind::DependencyAdd {
                    dep_name: "serde-evil".to_owned(),
                },
                file_paths: vec![],
            },
            traceparent: Some("00-abc-def-01".to_owned()),
            respond,
        };
        (escalation, rx)
    }

    #[tokio::test]
    async fn safe_action_approved() {
        let dir = TempDir::new().unwrap();
        let (tx, rx) = hitl_channel();
        let supervisor = Supervisor::new(test_config(&dir), rx);
        let handle = supervisor.run();

        let (esc, reply) = safe_escalation("task-001");
        tx.send(esc).await.unwrap();
        let result = reply.await.unwrap();
        assert!(result.is_approved(), "safe file write must be approved");

        drop(tx); // signal shutdown
        let outcome = handle.await.unwrap();
        assert!(outcome.is_ok());
    }

    #[tokio::test]
    async fn dep_add_escalates_to_user() {
        let dir = TempDir::new().unwrap();
        let (tx, rx) = hitl_channel();
        let supervisor = Supervisor::new(test_config(&dir), rx);
        let handle = supervisor.run();

        let (esc, reply) = dep_escalation("task-002");
        tx.send(esc).await.unwrap();
        let result = reply.await.unwrap();
        assert!(
            result.requires_user(),
            "dep addition must route to user escalation"
        );

        drop(tx);
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn decisions_written_to_chain() {
        let dir = TempDir::new().unwrap();
        let (tx, rx) = hitl_channel();
        let config = test_config(&dir);
        let log_path = config
            .decisions_dir
            .join(format!("decisions-{}.ndjson", config.codename));
        let chain_key = config.chain_key;

        let supervisor = Supervisor::new(config, rx);
        let handle = supervisor.run();

        // Send two escalations.
        let (esc1, reply1) = safe_escalation("task-001");
        let (esc2, reply2) = dep_escalation("task-002");
        tx.send(esc1).await.unwrap();
        tx.send(esc2).await.unwrap();

        reply1.await.unwrap();
        reply2.await.unwrap();

        drop(tx);
        handle.await.unwrap().unwrap();

        // Verify chain integrity.
        let chain = HashChain::open(&log_path, chain_key).unwrap();
        chain.verify_all().unwrap();

        // Verify two entries were written.
        let contents = std::fs::read_to_string(&log_path).unwrap();
        let line_count = contents.lines().count();
        assert_eq!(line_count, 2, "expected 2 decision log entries");
    }

    #[tokio::test]
    async fn supervisor_exits_cleanly_on_channel_close() {
        let dir = TempDir::new().unwrap();
        let (tx, rx) = hitl_channel();
        let supervisor = Supervisor::new(test_config(&dir), rx);
        let handle = supervisor.run();

        drop(tx); // close immediately — no escalations
        let result = handle.await.unwrap();
        assert!(
            result.is_ok(),
            "supervisor must exit cleanly with no errors"
        );
    }

    #[test]
    fn hitl_channel_capacity_is_positive() {
        // Compile-time guard: value is non-zero (assertion checked by compiler).
        const _: () = assert!(HITL_CHANNEL_CAPACITY > 0);
    }

    #[test]
    fn supervisor_config_from_program() {
        let config = ProgramConfig {
            codename: "my-build".to_owned(),
            repo_root: PathBuf::from("/repo"),
            worktree_root: PathBuf::from("/worktrees"),
            feat_branch: "feat/my-build".to_owned(),
            waves: vec![],
        };
        let sc = SupervisorConfig::from_program(&config, [0u8; 32]);
        assert_eq!(sc.codename, "my-build");
        assert!(sc.decisions_dir.ends_with("decisions"));
    }
}
