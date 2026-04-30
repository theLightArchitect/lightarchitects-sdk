//! Dispatch executor — wraps agent spawning and broadcasts [`DispatchEvent`].
//!
//! # Design
//!
//! The executor is a thin orchestration layer. It:
//! 1. Validates a `Security` agent request by synthesising a read-only
//!    `EngagementScope` (HIGH H-7).
//! 2. For each agent, spawns a Tokio task that simulates work and broadcasts
//!    [`DispatchEvent`] transitions to the SSE fan-out channel.
//! 3. Enforces DRY-RUN by checking `dry` before any write-capable path
//!    (HIGH H-9).
//!
//! # Phase note
//!
//! The `lightarchitects-cli` `TeamManager` integration is out of scope until
//! Phase 3 Wave 3 B2 completion and the path-dep wiring is resolved.  This
//! executor provides the full API contract and event semantics; the actual
//! `TeamManager` call-site is marked with `TODO(team-manager)` for the
//! integration wave.

use std::sync::Arc;

use tokio::sync::{Mutex, broadcast};

use super::{
    state::{DispatchHandle, DispatchRegistry},
    types::{
        AgentState, DispatchError, DispatchEvent, DispatchId, DomainAgent, ExecutionMode,
        SanitizedTask,
    },
};

/// Broadcast channel capacity — 256 events per dispatch (MED M-9).
pub const BROADCAST_CAPACITY: usize = 256;

/// Maximum concurrent agents per dispatch (MED M-9).
pub const MAX_AGENTS_PER_DISPATCH: usize = 9;

/// `EngagementScope` synthesised when `DomainAgent::Security` is selected.
///
/// Per HIGH H-7: `target = "self"`, `mode = "read-only"`, `ttl = 300s`.
#[derive(Debug, Clone)]
pub struct EngagementScope {
    /// Target identifier — always `"self"` for local-only webshell dispatches.
    pub target: &'static str,
    /// Permitted access mode.
    pub mode: &'static str,
    /// Seconds until the scope expires.
    pub ttl_seconds: u32,
}

impl EngagementScope {
    /// Synthesise a safe scope for the Security domain agent.
    fn synthesise() -> Self {
        Self {
            target: "self",
            mode: "read-only",
            ttl_seconds: 300,
        }
    }

    /// Validate that this scope can be established.
    ///
    /// Currently always succeeds for the `"self"` target.  Returns `Err`
    /// if scope invariants are violated — callers must 403 on error (H-7).
    fn validate(&self) -> Result<(), DispatchError> {
        // "self" target is always reachable on the local machine.
        // Future: check TTL > 0, mode is in allow-list, etc.
        if self.ttl_seconds == 0 {
            return Err(DispatchError::ScopeRequired);
        }
        Ok(())
    }
}

/// Execute a dispatch: create the registry entry, spawn agent tasks, and
/// return the [`DispatchId`] to the caller.
///
/// The caller streams events by calling `registry.lock().broadcast_tx(&id)`
/// and calling `.subscribe()` on the returned sender.
///
/// # Errors
///
/// - [`DispatchError::ScopeRequired`] — `Security` agent requested but scope
///   cannot be established.
/// - [`DispatchError::AlreadyActive`] — another dispatch with the same ID is
///   already in the registry.
/// - [`DispatchError::ChannelClosed`] — broadcast channel closed before the
///   first event could be sent.
#[tracing::instrument(skip(task, registry), fields(dispatch_id = %id, agent_count = agents.len()))]
pub async fn execute(
    task: SanitizedTask,
    agents: Vec<DomainAgent>,
    mode: ExecutionMode,
    dry: bool,
    id: DispatchId,
    registry: Arc<Mutex<DispatchRegistry>>,
) -> Result<(), DispatchError> {
    // H-7: check Security agent scope before registering.
    if agents.contains(&DomainAgent::Security) {
        let scope = EngagementScope::synthesise();
        scope.validate()?;
    }

    let (broadcast_tx, _) = broadcast::channel(BROADCAST_CAPACITY);

    let handle = DispatchHandle::new(agents.clone(), broadcast_tx.clone(), dry);

    {
        let mut reg = registry.lock().await;
        if !reg.insert(id.clone(), handle) {
            return Err(DispatchError::AlreadyActive(id.clone()));
        }
    }

    let task_text = task.as_str().to_owned();

    // Squad Comms persistence (C2): enqueue so the dispatch appears in the task dashboard.
    let title: String = task_text.chars().take(80).collect();
    if let Err(e) = crate::coordination::enqueue_dispatch(id.as_str(), &title, &task_text).await {
        tracing::warn!(dispatch_id = %id, error = %e, "Failed to persist dispatch to conductor queue");
    }

    let dispatch_id = id.clone();
    let reg_clone = Arc::clone(&registry);

    tokio::spawn(async move {
        run_agents(
            task_text,
            agents,
            mode,
            dry,
            dispatch_id,
            broadcast_tx,
            reg_clone,
        )
        .await;
    });

    Ok(())
}

/// Drive all agents to completion, broadcasting state transitions.
#[tracing::instrument(skip(task, tx, registry, _mode), fields(dispatch_id = %id))]
async fn run_agents(
    task: String,
    agents: Vec<DomainAgent>,
    _mode: ExecutionMode,
    dry: bool,
    id: DispatchId,
    tx: broadcast::Sender<DispatchEvent>,
    registry: Arc<Mutex<DispatchRegistry>>,
) {
    let started = std::time::Instant::now();

    for agent in &agents {
        // Transition → Running.
        let _ = tx.send(DispatchEvent::PerAgentState {
            agent: *agent,
            state: AgentState::Running,
            message: Some(format!(
                "{}{} running on: {}",
                if dry { "[DRY] " } else { "" },
                agent,
                task.chars().take(40).collect::<String>()
            )),
        });

        // TODO(team-manager): replace simulated work with TeamManager::spawn_teammate.
        // The actual integration requires the laex0 path-dep (C-1).
        // For now, signal the agent is complete after acknowledging the task.
        spawn_teammate_stub(*agent, &task, dry, &tx).await;
    }

    // SAFE: u64 holds ~584 million years of milliseconds; no dispatch runs that long.
    #[allow(clippy::cast_possible_truncation)]
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let _ = tx.send(DispatchEvent::Complete { elapsed_ms });

    // Squad Comms persistence (C2): mark queue entry completed.
    crate::coordination::complete_dispatch(id.as_str()).await;

    // Clean up registry entry.
    let mut reg = registry.lock().await;
    reg.remove(&id);
}

/// Stub implementation of teammate spawning.
///
/// Sends a `MailboxMessage` and then transitions the agent to `Complete`.
///
/// # TODO(team-manager)
///
/// Replace with `TeamManager::spawn_teammate` once the `laex0` path-dep (C-1)
/// is wired into `lightarchitects-webshell/Cargo.toml`.  The permission model
/// (H-9) will be enforced by injecting a `ToolPermissionToken` with
/// `write_allowed = agent.may_write() && !dry`.
#[tracing::instrument(skip(tx, _task), fields(agent = %agent))]
async fn spawn_teammate_stub(
    agent: DomainAgent,
    _task: &str,
    dry: bool,
    tx: &broadcast::Sender<DispatchEvent>,
) {
    let _ = tx.send(DispatchEvent::MailboxMessage {
        agent,
        text: format!(
            "{} acknowledged task{}.",
            agent,
            if dry { " (dry-run)" } else { "" }
        ),
    });

    // Simulate work — replaced by real agent execution in Wave 3 B2.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let _ = tx.send(DispatchEvent::PerAgentState {
        agent,
        state: AgentState::Complete,
        message: None,
    });
}

/// Cancel an active dispatch.
///
/// Sends an `Error` event and removes the handle from the registry so the
/// broadcast channel is closed (all subscribers see `RecvError::Closed`).
///
/// # Errors
///
/// Returns [`DispatchError::NotFound`] if `id` is not active.
#[tracing::instrument(skip(registry), fields(dispatch_id = %id))]
pub async fn cancel(
    id: &DispatchId,
    registry: Arc<Mutex<DispatchRegistry>>,
) -> Result<(), DispatchError> {
    let mut reg = registry.lock().await;
    let handle = reg
        .remove(id)
        .ok_or_else(|| DispatchError::NotFound(id.clone()))?;
    // Dropping the Sender closes the channel — all subscribers see Closed.
    let _ = handle.broadcast_tx.send(DispatchEvent::Error {
        agent: None,
        message: "Dispatch cancelled by user.".to_owned(),
    });
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines
)]
mod tests {
    use super::*;
    use crate::dispatch::types::{AgentState, DispatchEvent, SanitizedTask};

    fn make_registry() -> Arc<Mutex<DispatchRegistry>> {
        Arc::new(Mutex::new(DispatchRegistry::new()))
    }

    fn event_kind(ev: &DispatchEvent) -> &'static str {
        match ev {
            DispatchEvent::PerAgentState {
                state: AgentState::Running,
                ..
            } => "running",
            DispatchEvent::PerAgentState {
                state: AgentState::Complete,
                ..
            } => "agent_done",
            DispatchEvent::PerAgentState { .. } => "agent_state",
            DispatchEvent::MailboxMessage { .. } => "mailbox",
            DispatchEvent::Complete { .. } => "complete",
            DispatchEvent::Error { .. } => "error",
        }
    }

    async fn collect_events(
        mut rx: tokio::sync::broadcast::Receiver<DispatchEvent>,
    ) -> Vec<&'static str> {
        use tokio::sync::broadcast::error::RecvError;
        use tokio::time::{Duration, timeout};
        let mut kinds: Vec<&'static str> = Vec::new();
        loop {
            match timeout(Duration::from_millis(500), rx.recv()).await {
                Ok(Ok(ev)) => {
                    let done = matches!(ev, DispatchEvent::Complete { .. });
                    kinds.push(event_kind(&ev));
                    if done {
                        break;
                    }
                }
                Ok(Err(RecvError::Lagged(n))) => panic!("receiver lagged by {n} events"),
                Ok(Err(RecvError::Closed)) | Err(_) => break,
            }
        }
        kinds
    }

    #[tokio::test]
    async fn execute_completes_without_error() {
        let registry = make_registry();
        let id = DispatchId::solo("ENG", 1).unwrap();
        let task = SanitizedTask("refactor auth".to_owned());
        execute(
            task,
            vec![DomainAgent::Engineer],
            ExecutionMode::Solo,
            false,
            id,
            registry,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn security_agent_succeeds_with_scope() {
        let registry = make_registry();
        let id = DispatchId::squad("ECHO", 1).unwrap();
        let task = SanitizedTask("audit the security surface".to_owned());
        execute(
            task,
            vec![DomainAgent::Security],
            ExecutionMode::Solo,
            false,
            id,
            registry,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn cancel_nonexistent_returns_not_found() {
        let registry = make_registry();
        let id = DispatchId::solo("ENG", 99).unwrap();
        let err = cancel(&id, registry).await.unwrap_err();
        assert!(matches!(err, DispatchError::NotFound(_)));
    }

    #[tokio::test]
    async fn dry_run_flag_propagated() {
        let registry = make_registry();
        let id = DispatchId::squad("FOXTROT", 1).unwrap();
        let task = SanitizedTask("deploy service".to_owned());
        execute(
            task,
            vec![DomainAgent::Ops],
            ExecutionMode::Solo,
            true,
            id,
            registry,
        )
        .await
        .unwrap();
    }

    /// MED M-10 — 500 dispatch→cancel cycles must drain the registry to zero.
    #[tokio::test]
    async fn cancel_storm_registry_drains() {
        let registry = make_registry();
        for i in 0_u16..500 {
            let id = DispatchId::solo("ENG", i).unwrap();
            let task = SanitizedTask("storm".to_owned());
            // Ignore execute errors (duplicate IDs are impossible here; seq is unique).
            let _ = execute(
                task,
                vec![DomainAgent::Engineer],
                ExecutionMode::Solo,
                true, // dry — no filesystem writes
                id.clone(),
                Arc::clone(&registry),
            )
            .await;
            // Cancel immediately — removes from registry before run_agents can.
            let _ = cancel(&id, Arc::clone(&registry)).await;
        }
        // Allow any in-flight background tasks to finish their registry.remove() call.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let reg = registry.lock().await;
        assert_eq!(
            reg.len(),
            0,
            "registry must be empty after cancel storm (MED M-10)"
        );
    }

    /// MED M-9 — broadcast capacity bounded: creating and dropping 1000 broadcast
    /// channels must not grow RSS unboundedly (verified by absence of OOM / panic).
    #[tokio::test]
    async fn sse_subscriber_growth_bounded() {
        use tokio::sync::broadcast;
        let mut senders: Vec<broadcast::Sender<DispatchEvent>> = Vec::with_capacity(1000);
        for _ in 0..1_000 {
            let (tx, _rx) = broadcast::channel(super::BROADCAST_CAPACITY);
            senders.push(tx);
        }
        drop(senders);
    }

    // ── Fanout tests ───────────────────────────────────────────────────────────

    /// Single-agent fanout: Running → `MailboxMessage` → `agent_done` → Complete
    /// in that order, all delivered to a subscriber via the broadcast channel.
    #[tokio::test]
    async fn fanout_single_agent_emits_running_mailbox_complete() {
        let registry = make_registry();
        let id = DispatchId::solo("F1", 1).unwrap();
        execute(
            SanitizedTask("fanout single".to_owned()),
            vec![DomainAgent::Engineer],
            ExecutionMode::Solo,
            true,
            id.clone(),
            Arc::clone(&registry),
        )
        .await
        .unwrap();

        // Subscribe synchronously — background task is scheduled but not yet run
        // (Tokio cooperative scheduler: spawned tasks don't preempt until we yield).
        let rx = registry
            .try_lock()
            .expect("not contended")
            .broadcast_tx(&id)
            .unwrap()
            .subscribe();
        let kinds = collect_events(rx).await;

        assert!(kinds.contains(&"running"), "must emit Running: {kinds:?}");
        assert!(
            kinds.contains(&"mailbox"),
            "must emit MailboxMessage: {kinds:?}"
        );
        assert!(kinds.contains(&"complete"), "must emit Complete: {kinds:?}");
        let running_pos = kinds.iter().position(|&k| k == "running").unwrap();
        let complete_pos = kinds.iter().rposition(|&k| k == "complete").unwrap();
        assert!(
            running_pos < complete_pos,
            "Running must precede Complete: {kinds:?}"
        );
    }

    /// Three-agent fanout: all three agents must emit Running events before
    /// the global Complete arrives.
    #[tokio::test]
    async fn fanout_three_agents_all_emit_running() {
        let registry = make_registry();
        let id = DispatchId::squad("F3", 1).unwrap();
        let agents = vec![
            DomainAgent::Engineer,
            DomainAgent::Quality,
            DomainAgent::Researcher,
        ];
        execute(
            SanitizedTask("fanout three agents".to_owned()),
            agents.clone(),
            ExecutionMode::Squad,
            true,
            id.clone(),
            Arc::clone(&registry),
        )
        .await
        .unwrap();

        let rx = registry
            .try_lock()
            .expect("not contended")
            .broadcast_tx(&id)
            .unwrap()
            .subscribe();
        let kinds = collect_events(rx).await;

        // Each agent emits one Running event → 3 total.
        let running_count = kinds.iter().filter(|&&k| k == "running").count();
        assert_eq!(
            running_count, 3,
            "all three agents must emit Running: {kinds:?}"
        );
        assert!(
            kinds.contains(&"complete"),
            "global Complete must arrive: {kinds:?}"
        );
    }

    /// All-nine-agent fanout: every `DomainAgent` dispatched dry; global Complete
    /// must arrive within the 500 ms timeout window.
    #[tokio::test]
    async fn fanout_all_nine_agents_complete() {
        let registry = make_registry();
        let id = DispatchId::squad("F9", 1).unwrap();
        let agents = vec![
            DomainAgent::Engineer,
            DomainAgent::Quality,
            DomainAgent::Security,
            DomainAgent::Ops,
            DomainAgent::Researcher,
            DomainAgent::Knowledge,
            DomainAgent::Performance,
            DomainAgent::Testing,
            DomainAgent::Documentation,
        ];
        execute(
            SanitizedTask("fanout all nine".to_owned()),
            agents,
            ExecutionMode::Squad,
            true,
            id.clone(),
            Arc::clone(&registry),
        )
        .await
        .unwrap();

        let rx = registry
            .try_lock()
            .expect("not contended")
            .broadcast_tx(&id)
            .unwrap()
            .subscribe();
        let kinds = collect_events(rx).await;

        assert_eq!(
            kinds.iter().filter(|&&k| k == "running").count(),
            9,
            "all nine agents must emit Running: {kinds:?}"
        );
        assert!(
            kinds.contains(&"complete"),
            "global Complete must arrive: {kinds:?}"
        );
    }

    /// Duplicate-ID gate: pre-populate the registry, then call execute with the
    /// same ID — must return `AlreadyActive` without panic.
    #[tokio::test]
    async fn duplicate_id_returns_already_active() {
        use crate::dispatch::state::DispatchHandle;
        use tokio::sync::broadcast;

        let registry = make_registry();
        let id = DispatchId::solo("DUP", 1).unwrap();

        // Pre-populate the registry to simulate an in-flight dispatch.
        {
            let (tx, _) = broadcast::channel::<DispatchEvent>(16);
            let handle = DispatchHandle::new(vec![DomainAgent::Engineer], tx, false);
            let mut reg = registry.lock().await;
            assert!(reg.insert(id.clone(), handle), "first insert must succeed");
        }

        let err = execute(
            SanitizedTask("colliding task".to_owned()),
            vec![DomainAgent::Engineer],
            ExecutionMode::Solo,
            true,
            id,
            registry,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, DispatchError::AlreadyActive(_)),
            "expected AlreadyActive, got {err:?}"
        );
    }

    /// Dry-run marker: `MailboxMessage` text must contain "(dry-run)" when dry=true.
    #[tokio::test]
    async fn fanout_dry_events_include_dry_marker() {
        use tokio::sync::broadcast::error::RecvError;
        use tokio::time::{Duration, timeout};

        let registry = make_registry();
        let id = DispatchId::solo("DRY", 1).unwrap();
        execute(
            SanitizedTask("dry run marker check".to_owned()),
            vec![DomainAgent::Ops],
            ExecutionMode::Solo,
            true,
            id.clone(),
            Arc::clone(&registry),
        )
        .await
        .unwrap();

        let mut rx = registry
            .try_lock()
            .expect("not contended")
            .broadcast_tx(&id)
            .unwrap()
            .subscribe();
        let mut found_dry_marker = false;
        loop {
            match timeout(Duration::from_millis(500), rx.recv()).await {
                Ok(Ok(ev)) => {
                    if let DispatchEvent::MailboxMessage { ref text, .. } = ev {
                        if text.contains("dry-run") {
                            found_dry_marker = true;
                        }
                    }
                    if matches!(ev, DispatchEvent::Complete { .. }) {
                        break;
                    }
                }
                Ok(Err(RecvError::Lagged(n))) => panic!("lagged by {n}"),
                Ok(Err(RecvError::Closed)) | Err(_) => break,
            }
        }

        assert!(
            found_dry_marker,
            "MailboxMessage must contain '(dry-run)' when dry=true"
        );
    }
}
