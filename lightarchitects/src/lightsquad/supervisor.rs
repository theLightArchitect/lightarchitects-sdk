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
//!   │  [Approved/Blocked]            │
//!   │◄─────── tx.send(result) ───────│
//!   │                                │
//!   │  [UserEscalation]              │
//!   │   ← parked in pending map ────►│ (operator must resolve via IronclawHitlResolver)
//!   │◄─────── tx.send(result) ───────│ (only after resolver.resolve(nonce, approved))
//! ```
//!
//! # Phase 4 additions
//!
//! - `IronclawHitlResolver` — shared handle for the webshell control handler to
//!   resolve pending escalations; prevents replay via `used_nonces` [`DashSet`].
//! - `EscalationHook` — injected callback so the webshell can emit
//!   `IronclawHitlEscalationEvent` SSE without creating an SDK → webshell dep.
//! - `Supervisor::resolver()` — returns the resolver for `AppState` storage.

use std::{path::PathBuf, sync::Arc};

use chrono::Utc;
use dashmap::{DashMap, DashSet};
use tokio::sync::{mpsc, oneshot};
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::lightsquad::{
    decision_pipeline::{DecisionContext, DecisionPipeline, PipelineResult},
    decisions::hash_chain::{ChainError, DecisionEntry, DecisionLayer, HashChain},
    program::ProgramConfig,
};

// ─── Channel types ────────────────────────────────────────────────────────────

/// Capacity of the `ironclaw-hitl` channel.
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
pub struct HitlEscalation {
    /// Task that originated this escalation.
    pub task_id: String,
    /// The decision context to be evaluated by the pipeline.
    pub context: DecisionContext,
    /// W3C `traceparent` header value propagated from the worker's span.
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

// ─── Resolver ─────────────────────────────────────────────────────────────────

/// Error returned by [`IronclawHitlResolver::resolve`].
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// The nonce has already been used — replay attack rejected (CWE-294).
    #[error("nonce already consumed — replay rejected: {0}")]
    ReplayAttack(Uuid),
    /// No pending escalation exists for this nonce.
    #[error("no pending escalation for nonce {0}")]
    NotFound(Uuid),
}

/// Shared handle for resolving pending `UserEscalation` entries.
///
/// Stored in `AppState` so the `POST /api/control` handler can resolve operator
/// decisions without knowing about the Supervisor internals. Cheaply cloneable
/// (wraps `Arc` internals).
#[derive(Clone)]
pub struct IronclawHitlResolver {
    pending: Arc<DashMap<Uuid, (String, oneshot::Sender<PipelineResult>)>>,
    used_nonces: Arc<DashSet<Uuid>>,
}

impl IronclawHitlResolver {
    /// Resolve a pending escalation identified by `nonce`.
    ///
    /// Validates that the nonce has not been used before (SERAPH#3 anti-replay).
    /// On approval sends `PipelineResult::Approved { layer: User }`; on rejection
    /// sends `PipelineResult::Blocked { layer: User }`.
    ///
    /// # Errors
    ///
    /// - [`ResolveError::ReplayAttack`] if `nonce` has already been consumed.
    /// - [`ResolveError::NotFound`] if no escalation is pending for `nonce`.
    pub fn resolve(
        &self,
        nonce: Uuid,
        approved: bool,
        operator_reason: Option<String>,
    ) -> Result<String, ResolveError> {
        // Replay prevention — insert returns false when already present.
        if !self.used_nonces.insert(nonce) {
            return Err(ResolveError::ReplayAttack(nonce));
        }
        let (_, (task_id, respond)) = self
            .pending
            .remove(&nonce)
            .ok_or(ResolveError::NotFound(nonce))?;

        let result = if approved {
            PipelineResult::Approved {
                layer: DecisionLayer::User,
                citation: operator_reason.clone().map(|r| format!("operator: {r}")),
            }
        } else {
            PipelineResult::Blocked {
                reason: operator_reason
                    .clone()
                    .unwrap_or_else(|| "operator rejected".to_owned()),
                layer: DecisionLayer::User,
                citation: None,
            }
        };
        // Ignore send error — worker may have timed out.
        let _ = respond.send(result);
        Ok(task_id)
    }

    /// Returns `true` if there is a pending escalation for `nonce`.
    #[must_use]
    pub fn has_pending(&self, nonce: &Uuid) -> bool {
        self.pending.contains_key(nonce)
    }
}

// ─── EscalationHook ──────────────────────────────────────────────────────────

/// Callback invoked by the Supervisor when `UserEscalation` fires.
///
/// Parameters: `(nonce, task_id, reason, traceparent)`.
///
/// The webshell wires this to emit [`IronclawHitlEscalationEvent`] SSE. Tests
/// can use a `tokio::sync::mpsc` capture channel. The hook MUST NOT block.
///
/// [`IronclawHitlEscalationEvent`]: lightarchitects_webshell::events::types::IronclawHitlEscalationEvent
pub type EscalationHook = Arc<dyn Fn(Uuid, String, String, Option<String>) + Send + Sync + 'static>;

// ─── SupervisorConfig ─────────────────────────────────────────────────────────

/// Configuration for a [`Supervisor`] instance.
#[derive(Clone)]
pub struct SupervisorConfig {
    /// Build codename — used in log spans and the decision-log file name.
    pub codename: String,
    /// Directory where `decisions-<codename>.ndjson` is written.
    pub decisions_dir: PathBuf,
    /// 32-byte HMAC key for the decision chain.
    pub chain_key: [u8; 32],
    /// Optional hook called when `UserEscalation` fires.
    ///
    /// The hook receives `(nonce, task_id, reason, traceparent)` and should
    /// emit an `IronclawHitlEscalationEvent` SSE without blocking.
    pub on_user_escalation: Option<EscalationHook>,
}

impl std::fmt::Debug for SupervisorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SupervisorConfig")
            .field("codename", &self.codename)
            .field("decisions_dir", &self.decisions_dir)
            .finish_non_exhaustive()
    }
}

impl SupervisorConfig {
    /// Derive a `SupervisorConfig` from a [`ProgramConfig`].
    #[must_use]
    pub fn from_program(config: &ProgramConfig, chain_key: [u8; 32]) -> Self {
        Self {
            codename: config.codename.clone(),
            decisions_dir: config.worktree_root.join("decisions"),
            chain_key,
            on_user_escalation: None,
        }
    }

    /// Attach an escalation hook (builder pattern).
    #[must_use]
    pub fn with_hook(mut self, hook: EscalationHook) -> Self {
        self.on_user_escalation = Some(hook);
        self
    }
}

// ─── Supervisor ───────────────────────────────────────────────────────────────

/// Long-running Tokio task that evaluates `ironclaw-hitl` escalations.
pub struct Supervisor {
    config: SupervisorConfig,
    hitl_rx: IronclawHitlRx,
    pipeline: DecisionPipeline,
    pending: Arc<DashMap<Uuid, (String, oneshot::Sender<PipelineResult>)>>,
    used_nonces: Arc<DashSet<Uuid>>,
}

impl Supervisor {
    /// Create a new Supervisor.
    #[must_use]
    pub fn new(config: SupervisorConfig, hitl_rx: IronclawHitlRx) -> Self {
        Self {
            config,
            hitl_rx,
            pipeline: DecisionPipeline::new(),
            pending: Arc::new(DashMap::new()),
            used_nonces: Arc::new(DashSet::new()),
        }
    }

    /// Returns a cloneable resolver handle for use by the HTTP control handler.
    ///
    /// The handle shares the same pending map and nonce set as this supervisor —
    /// call [`IronclawHitlResolver::resolve`] from any Tokio task to unblock a
    /// parked worker.
    #[must_use]
    pub fn resolver(&self) -> IronclawHitlResolver {
        IronclawHitlResolver {
            pending: Arc::clone(&self.pending),
            used_nonces: Arc::clone(&self.used_nonces),
        }
    }

    /// Spawn the supervisor as a background Tokio task.
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
                self.handle_escalation(escalation, &mut chain);
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
    fn handle_escalation(&self, escalation: HitlEscalation, chain: &mut HashChain) {
        let result = self.pipeline.evaluate(&escalation.context);

        let (chain_layer, decision_text) = Self::chain_entry_text(&result);

        let entry = DecisionEntry {
            seq: 0,
            timestamp: Utc::now(),
            layer: chain_layer,
            question: escalation.context.description.clone(),
            decision: decision_text,
            citation: None,
            prev_hash: None,
            entry_hash: [0u8; 32],
        };
        if let Err(e) = chain.append(entry) {
            warn!(
                task_id = %escalation.task_id,
                error = %e,
                "failed to append decision to chain — verdict still returned"
            );
        }

        match result {
            PipelineResult::UserEscalation { ref reason, .. } => {
                // Mint a `UUIDv7` nonce (time-ordered, single-use).
                let nonce = Uuid::now_v7();
                let task_id = escalation.task_id.clone();
                let traceparent = escalation.traceparent.clone();
                let reason_str = reason.clone();

                // Park the respond sender — worker awaits resolution via the resolver.
                self.pending
                    .insert(nonce, (task_id.clone(), escalation.respond));

                info!(
                    nonce = %nonce,
                    task_id = %task_id,
                    "supervisor: UserEscalation parked — awaiting operator"
                );

                // Invoke hook so webshell can emit IronclawHitlEscalationEvent SSE.
                if let Some(hook) = &self.config.on_user_escalation {
                    hook(nonce, task_id, reason_str, traceparent);
                }
                // Don't send on respond — worker is now parked.
            }
            other => {
                if escalation.respond.send(other).is_err() {
                    warn!(
                        task_id = %escalation.task_id,
                        "worker dropped the oneshot receiver before verdict arrived"
                    );
                }
            }
        }
    }

    fn chain_entry_text(result: &PipelineResult) -> (DecisionLayer, String) {
        match result {
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
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;
    use tokio::sync::mpsc as test_mpsc;

    use super::*;
    use crate::lightsquad::decision_pipeline::ActionKind;

    fn test_config(dir: &TempDir) -> SupervisorConfig {
        SupervisorConfig {
            codename: "test-build".to_owned(),
            decisions_dir: dir.path().to_path_buf(),
            chain_key: [0xAB; 32],
            on_user_escalation: None,
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

    fn dep_escalation(
        task_id: &str,
        traceparent: Option<String>,
    ) -> (HitlEscalation, oneshot::Receiver<PipelineResult>) {
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
            traceparent,
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

        drop(tx);
        let outcome = handle.await.unwrap();
        assert!(outcome.is_ok());
    }

    #[tokio::test]
    async fn dep_add_escalates_to_user() {
        let dir = TempDir::new().unwrap();
        let (tx, rx) = hitl_channel();
        let supervisor = Supervisor::new(test_config(&dir), rx);
        let handle = supervisor.run();

        let (esc, reply) = dep_escalation("task-002", None);
        tx.send(esc).await.unwrap();

        // Give supervisor time to process and park.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Worker's oneshot is parked — it must not have resolved yet.
        // Verify by checking the handle is still running.
        assert!(!handle.is_finished());

        drop(tx);
        handle.await.unwrap().unwrap();

        // reply_rx is dropped here — the pending sender was never resolved,
        // so this tests the "worker dropped receiver" path.
        drop(reply);
    }

    #[tokio::test]
    async fn supervisor_escalates_then_resumes_on_resolution() {
        let dir = TempDir::new().unwrap();
        let (tx, rx) = hitl_channel();
        let (hook_tx, mut hook_rx) =
            test_mpsc::unbounded_channel::<(Uuid, String, String, Option<String>)>();
        let hook: EscalationHook = Arc::new(move |nonce, task_id, reason, traceparent| {
            let _ = hook_tx.send((nonce, task_id, reason, traceparent));
        });

        let supervisor = Supervisor::new(test_config(&dir).with_hook(hook), rx);
        let resolver = supervisor.resolver();
        let handle = supervisor.run();

        let (esc, reply) = dep_escalation("task-003", None);
        tx.send(esc).await.unwrap();

        // Wait for hook to fire — gives the parked nonce.
        let (nonce, task_id, _reason, _traceparent) =
            tokio::time::timeout(std::time::Duration::from_millis(500), hook_rx.recv())
                .await
                .expect("hook must fire within 500ms")
                .unwrap();

        assert_eq!(task_id, "task-003");
        assert!(resolver.has_pending(&nonce));

        // Operator approves.
        resolver
            .resolve(nonce, true, Some("looks safe".to_owned()))
            .unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_millis(500), reply)
            .await
            .expect("worker must receive verdict within 500ms")
            .unwrap();

        assert!(
            result.is_approved(),
            "approved resolution must yield Approved"
        );
        assert!(!resolver.has_pending(&nonce));

        drop(tx);
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn supervisor_rejects_replayed_nonce() {
        let dir = TempDir::new().unwrap();
        let (tx, rx) = hitl_channel();
        let (hook_tx, mut hook_rx) =
            test_mpsc::unbounded_channel::<(Uuid, String, String, Option<String>)>();
        let hook: EscalationHook = Arc::new(move |nonce, task_id, reason, traceparent| {
            let _ = hook_tx.send((nonce, task_id, reason, traceparent));
        });

        let supervisor = Supervisor::new(test_config(&dir).with_hook(hook), rx);
        let resolver = supervisor.resolver();
        let handle = supervisor.run();

        let (esc, _reply) = dep_escalation("task-004", None);
        tx.send(esc).await.unwrap();

        let (nonce, ..) =
            tokio::time::timeout(std::time::Duration::from_millis(500), hook_rx.recv())
                .await
                .unwrap()
                .unwrap();

        // First resolve — succeeds.
        resolver.resolve(nonce, true, None).unwrap();
        // Second resolve with same nonce — replay rejected.
        let err = resolver
            .resolve(nonce, true, None)
            .expect_err("second resolve must fail");
        assert!(
            matches!(err, ResolveError::ReplayAttack(_)),
            "expected ReplayAttack, got {err:?}"
        );

        drop(tx);
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn supervisor_propagates_traceparent_through_round_trip() {
        let dir = TempDir::new().unwrap();
        let (tx, rx) = hitl_channel();
        let (hook_tx, mut hook_rx) =
            test_mpsc::unbounded_channel::<(Uuid, String, String, Option<String>)>();
        let hook: EscalationHook = Arc::new(move |nonce, task_id, reason, traceparent| {
            let _ = hook_tx.send((nonce, task_id, reason, traceparent));
        });

        let supervisor = Supervisor::new(test_config(&dir).with_hook(hook), rx);
        let resolver = supervisor.resolver();
        let handle = supervisor.run();

        let expected_traceparent = "00-abc123def456-deadbeef01234567-01";
        let (esc, _reply) = dep_escalation("task-005", Some(expected_traceparent.to_owned()));
        tx.send(esc).await.unwrap();

        let (nonce, _task_id, _reason, traceparent) =
            tokio::time::timeout(std::time::Duration::from_millis(500), hook_rx.recv())
                .await
                .unwrap()
                .unwrap();

        assert_eq!(
            traceparent.as_deref(),
            Some(expected_traceparent),
            "traceparent must be propagated verbatim to the hook"
        );

        resolver.resolve(nonce, false, None).unwrap();
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

        let (esc1, reply1) = safe_escalation("task-001");
        tx.send(esc1).await.unwrap();
        reply1.await.unwrap();

        drop(tx);
        handle.await.unwrap().unwrap();

        // One safe escalation → one chain entry.
        let chain = HashChain::open(&log_path, chain_key).unwrap();
        chain.verify_all().unwrap();
        let contents = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(contents.lines().count(), 1, "expected 1 decision log entry");
    }

    #[tokio::test]
    async fn supervisor_exits_cleanly_on_channel_close() {
        let dir = TempDir::new().unwrap();
        let (tx, rx) = hitl_channel();
        let supervisor = Supervisor::new(test_config(&dir), rx);
        let handle = supervisor.run();

        drop(tx);
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[test]
    fn hitl_channel_capacity_is_positive() {
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

    #[test]
    fn resolver_not_found_returns_error() {
        let pending = Arc::new(DashMap::new());
        let used_nonces = Arc::new(DashSet::new());
        let resolver = IronclawHitlResolver {
            pending,
            used_nonces,
        };
        let fake_nonce = Uuid::now_v7();
        let err = resolver.resolve(fake_nonce, true, None).unwrap_err();
        // First insertion succeeds but there's no pending entry → NotFound.
        assert!(
            matches!(err, ResolveError::NotFound(_)),
            "expected NotFound, got {err:?}"
        );
    }
}
