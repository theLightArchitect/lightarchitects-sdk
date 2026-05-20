//! `FleetTracker` — DashMap-backed state machine for live agent fleet tracking.
//!
//! # Design
//!
//! `FleetTracker` is cheaply cloneable via its inner `Arc`.  All public methods
//! are either `async` (for the `active_stack` mutex) or lock-free (`DashMap`
//! shard-level locking only).
//!
//! Parent inference (OQ1): the `active_stack` is a LIFO stack of running agent
//! IDs maintained by the tailer.  The top of the stack when an agent spawns is
//! recorded as `parent_agent_id`.

use std::sync::Arc;

use chrono::Utc;
use dashmap::DashMap;
use serde::Serialize;
use tokio::sync::Mutex;

use super::span::{ExitPath, FleetSpan, FleetStatus};

/// Serialisable view of a `FleetSpan` — the payload emitted over SSE.
///
/// Derived from `FleetSpan` via [`From<&FleetSpan>`].  Intentionally omits
/// `spawned_at` / `completed_at` to keep the SSE payload compact; the full
/// timestamps are available in the internal span if required later.
#[derive(Debug, Clone, Serialize)]
pub struct FleetNode {
    /// Stable unique identifier for this agent invocation.
    pub agent_id: String,
    /// `subagent_type` from the Agent tool call.
    pub agent_type: String,
    /// Sanitized task description (≤ 200 chars, no control chars).
    pub description: String,
    /// Parent agent inferred from active-stack at spawn time.
    pub parent_agent_id: Option<String>,
    /// Worktree path — always `None` in V1 (OQ2).
    pub worktree_path: Option<String>,
    /// Whether the agent was launched in background.
    pub run_in_background: bool,
    /// Current lifecycle state.
    pub status: FleetStatus,
    /// Turn count — always `0` in V1 (OQ4 / SCR1-F2).
    pub turns: u64,
    /// Elapsed wall-clock milliseconds.
    pub elapsed_ms: u64,
    /// How the agent exited — `None` while running.
    pub exit_path: Option<ExitPath>,
}

impl From<&FleetSpan> for FleetNode {
    fn from(span: &FleetSpan) -> Self {
        Self {
            agent_id: span.agent_id.clone(),
            agent_type: span.agent_type.clone(),
            description: span.description.clone(),
            parent_agent_id: span.parent_agent_id.clone(),
            worktree_path: span.worktree_path.clone(),
            run_in_background: span.run_in_background,
            status: span.status.clone(),
            turns: span.turns,
            elapsed_ms: span.elapsed_ms,
            exit_path: span.exit_path.clone(),
        }
    }
}

/// Point-in-time snapshot of the entire fleet, ready for SSE emission.
#[derive(Clone, Debug, Serialize)]
pub struct FleetSnapshot {
    /// All known agent nodes at the moment of capture.
    pub nodes: Vec<FleetNode>,
    /// RFC 3339 UTC timestamp when the snapshot was taken.
    pub captured_at: String,
}

/// Shared handle — cheap to clone (`Arc` inside).
#[derive(Clone, Debug)]
pub struct FleetTracker {
    spans: Arc<DashMap<String, FleetSpan>>,
    /// LIFO stack of currently-running agent IDs for parent inference (OQ1).
    active_stack: Arc<Mutex<Vec<String>>>,
}

impl Default for FleetTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl FleetTracker {
    /// Construct a new, empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            spans: Arc::new(DashMap::new()),
            active_stack: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Record an agent spawn event.
    ///
    /// `parent_agent_id` is inferred from the top of `active_stack` (OQ1).
    /// After recording, the new `agent_id` is pushed onto the stack.
    pub async fn agent_spawned(
        &self,
        agent_id: String,
        agent_type: String,
        description: String,
        run_in_background: bool,
    ) {
        let parent = {
            let stack = self.active_stack.lock().await;
            stack.last().cloned()
        };

        let span = FleetSpan::new(
            agent_id.clone(),
            agent_type,
            description,
            parent,
            run_in_background,
        );
        self.spans.insert(agent_id.clone(), span);

        let mut stack = self.active_stack.lock().await;
        stack.push(agent_id);
    }

    /// Record agent completion.
    ///
    /// Transitions the span to [`FleetStatus::Completed`] or
    /// [`FleetStatus::Failed`] depending on `exit_path`, sets `completed_at`,
    /// and removes the agent from `active_stack`.
    ///
    /// No-ops silently if `agent_id` was never spawned (idempotent on duplicate
    /// completion events from the tailer).
    pub async fn agent_completed(&self, agent_id: &str, exit_path: ExitPath) {
        if let Some(mut span) = self.spans.get_mut(agent_id) {
            // Guard illegal Completed → Running transition.
            if span.status == FleetStatus::Completed || span.status == FleetStatus::Failed {
                return;
            }
            span.status = match exit_path {
                ExitPath::Completed => FleetStatus::Completed,
                ExitPath::Error => FleetStatus::Failed,
                ExitPath::WatchdogStall => FleetStatus::Stalled,
            };
            span.exit_path = Some(exit_path);
            span.completed_at = Some(Utc::now());
        }

        let mut stack = self.active_stack.lock().await;
        stack.retain(|id| id != agent_id);
    }

    /// Add `delta_ms` to `elapsed_ms` for every span in [`FleetStatus::Running`].
    ///
    /// Called by the broadcast ticker every 500 ms.  Lock-free — `DashMap` shard
    /// locks only.
    pub fn tick_elapsed(&self, delta_ms: u64) {
        for mut entry in self.spans.iter_mut() {
            if entry.status == FleetStatus::Running {
                entry.elapsed_ms = entry.elapsed_ms.saturating_add(delta_ms);
            }
        }
    }

    /// Take a point-in-time snapshot of all spans.
    #[must_use]
    pub fn snapshot(&self) -> FleetSnapshot {
        let nodes: Vec<FleetNode> = self
            .spans
            .iter()
            .map(|entry| FleetNode::from(entry.value()))
            .collect();

        FleetSnapshot {
            nodes,
            captured_at: Utc::now().to_rfc3339(),
        }
    }

    /// Shared reference to the underlying span map (for broadcast emission).
    #[must_use]
    pub fn spans(&self) -> Arc<DashMap<String, FleetSpan>> {
        Arc::clone(&self.spans)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Basic lifecycle ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn spawn_and_complete_happy_path() {
        let tracker = FleetTracker::new();
        tracker
            .agent_spawned("a1".into(), "engineer".into(), "task".into(), false)
            .await;

        let snap = tracker.snapshot();
        assert_eq!(snap.nodes.len(), 1);
        assert_eq!(snap.nodes[0].status, FleetStatus::Running);

        tracker.agent_completed("a1", ExitPath::Completed).await;

        let snap = tracker.snapshot();
        assert_eq!(snap.nodes[0].status, FleetStatus::Completed);
        assert_eq!(snap.nodes[0].exit_path, Some(ExitPath::Completed));
    }

    #[tokio::test]
    async fn completed_to_running_transition_blocked() {
        let tracker = FleetTracker::new();
        tracker
            .agent_spawned("a1".into(), "eng".into(), "t".into(), false)
            .await;
        tracker.agent_completed("a1", ExitPath::Completed).await;

        // Second completion event must not change state back.
        tracker.agent_completed("a1", ExitPath::Error).await;

        let snap = tracker.snapshot();
        assert_eq!(snap.nodes[0].status, FleetStatus::Completed);
        assert_eq!(snap.nodes[0].exit_path, Some(ExitPath::Completed));
    }

    #[tokio::test]
    async fn unknown_agent_completion_is_noop() {
        let tracker = FleetTracker::new();
        // Must not panic or error.
        tracker.agent_completed("ghost", ExitPath::Error).await;
        assert_eq!(tracker.snapshot().nodes.len(), 0);
    }

    // ── Parent inference ──────────────────────────────────────────────────────

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn parent_inferred_from_active_stack() {
        let tracker = FleetTracker::new();
        tracker
            .agent_spawned("parent".into(), "eng".into(), "root".into(), false)
            .await;
        tracker
            .agent_spawned("child".into(), "quality".into(), "sub".into(), true)
            .await;

        let snap = tracker.snapshot();
        let child = snap.nodes.iter().find(|n| n.agent_id == "child").unwrap();
        assert_eq!(child.parent_agent_id, Some("parent".into()));
    }

    #[tokio::test]
    async fn root_agent_has_no_parent() {
        let tracker = FleetTracker::new();
        tracker
            .agent_spawned("root".into(), "eng".into(), "task".into(), false)
            .await;
        let snap = tracker.snapshot();
        assert!(snap.nodes[0].parent_agent_id.is_none());
    }

    // ── Elapsed ticking ───────────────────────────────────────────────────────

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn tick_elapsed_only_affects_running() {
        let tracker = FleetTracker::new();
        tracker
            .agent_spawned("a1".into(), "eng".into(), "t".into(), false)
            .await;
        tracker
            .agent_spawned("a2".into(), "eng".into(), "t".into(), false)
            .await;
        tracker.agent_completed("a2", ExitPath::Completed).await;

        tracker.tick_elapsed(500);

        let snap = tracker.snapshot();
        let a1 = snap.nodes.iter().find(|n| n.agent_id == "a1").unwrap();
        let a2 = snap.nodes.iter().find(|n| n.agent_id == "a2").unwrap();
        assert_eq!(a1.elapsed_ms, 500);
        assert_eq!(a2.elapsed_ms, 0);
    }

    // ── FleetNode conversion ──────────────────────────────────────────────────

    #[tokio::test]
    async fn completed_node_has_some_exit_path() {
        let tracker = FleetTracker::new();
        tracker
            .agent_spawned("a1".into(), "eng".into(), "t".into(), false)
            .await;
        tracker.agent_completed("a1", ExitPath::Error).await;

        let snap = tracker.snapshot();
        assert!(snap.nodes[0].exit_path.is_some());
    }

    // ── FleetNode From impl ───────────────────────────────────────────────────

    #[test]
    fn fleet_node_from_span() {
        let span = FleetSpan::new(
            "x".into(),
            "researcher".into(),
            "investigate".into(),
            Some("parent_x".into()),
            true,
        );
        let node = FleetNode::from(&span);
        assert_eq!(node.agent_id, "x");
        assert_eq!(node.parent_agent_id, Some("parent_x".into()));
        assert_eq!(node.status, FleetStatus::Running);
        assert!(node.worktree_path.is_none());
    }
}
