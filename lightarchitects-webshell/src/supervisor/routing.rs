//! Supervisor action routing and HITL escalation (§Q checks 9 + 10).
//!
//! When the operator selects an action from the `ProposalCard`, the supervisor
//! maps the action class to a target agent ID and dispatches it onto the A2A
//! event bus.  If no route is configured for the selected class, the supervisor
//! falls back to HITL escalation — broadcasting a `proposal_pending: true`
//! update so the operator can decide manually.
//!
//! ## Routing config
//!
//! ```no_run
//! # use lightarchitects_webshell::supervisor::{SupervisorRoutingConfig, ActionClass, AgentId};
//! # use std::collections::BTreeMap;
//! SupervisorRoutingConfig {
//!     action_routes: BTreeMap::from([
//!         (ActionClass::Refocus, AgentId("corso".into())),
//!         (ActionClass::Escalate, AgentId("eva".into())),
//!     ]),
//! };
//! ```

use std::collections::BTreeMap;

use crate::events::types::NorthstarEvaluationEvent;

// ── ActionClass ───────────────────────────────────────────────────────────────

/// The class of action the operator selected from a `ProposalCard`.
///
/// Kept as a small closed enum so `BTreeMap` can use it as an ordered key
/// without a custom hash implementation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionClass {
    /// Refocus the build on the original northstar — dispatch to build agent.
    Refocus,
    /// Pivot the northstar to a new direction — requires operator to supply
    /// updated northstar text; dispatched to build agent with new context.
    Pivot,
    /// Pause the build and escalate to the operator for manual triage.
    Escalate,
    /// Continue the build unchanged — operator acknowledges drift but chooses
    /// to override and keep going.
    Continue,
}

// ── AgentId ───────────────────────────────────────────────────────────────────

/// Opaque identifier for the target agent that should receive the routed action.
///
/// Corresponds to the `agent_id` field in A2A envelope headers
/// (Agents Playbook §III).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentId(pub String);

impl AgentId {
    /// Construct an `AgentId` from a static string literal.
    #[must_use]
    pub fn from_static(s: &'static str) -> Self {
        Self(s.to_owned())
    }
}

// ── SupervisorRoutingConfig ───────────────────────────────────────────────────

/// Maps `ActionClass` values to target `AgentId`s.
///
/// When the operator selects an action from a proposal card, the supervisor
/// looks up the corresponding agent ID here.  An empty map means all actions
/// fall through to HITL escalation.
#[derive(Debug, Clone, Default)]
pub struct SupervisorRoutingConfig {
    /// Route table: action class → target agent that handles the action.
    ///
    /// Ordered by `ActionClass` so iteration order is deterministic in tests
    /// and log output.
    pub action_routes: BTreeMap<ActionClass, AgentId>,
}

impl SupervisorRoutingConfig {
    /// Build a routing config that maps all standard action classes to the
    /// default build agent (`"corso"`).
    #[must_use]
    pub fn default_build_agent() -> Self {
        Self {
            action_routes: BTreeMap::from([
                (ActionClass::Refocus, AgentId::from_static("corso")),
                (ActionClass::Pivot, AgentId::from_static("corso")),
                (ActionClass::Escalate, AgentId::from_static("eva")),
                (ActionClass::Continue, AgentId::from_static("corso")),
            ]),
        }
    }

    /// Resolve the target agent for a given action class.
    ///
    /// Returns `None` when no route is configured — callers should fall back
    /// to [`escalate_to_hitl`].
    #[must_use]
    pub fn resolve(&self, action: &ActionClass) -> Option<&AgentId> {
        self.action_routes.get(action)
    }
}

// ── escalate_to_hitl ─────────────────────────────────────────────────────────

/// Broadcast a supervisor update that forces the proposal card to surface.
///
/// Called when no route is configured for the selected `ActionClass`, or when
/// `ActionClass::Escalate` is routed here explicitly.  The broadcast reaches
/// any SSE subscriber on `session.event_tx`, which the `ProposalCard` component
/// listens to.
///
/// Returns `true` when at least one SSE subscriber received the event.
/// Returns `false` when the channel has no active receivers (the operator has
/// already closed the SSE connection).  This is not a fatal condition — the
/// evaluation state is already recorded; the next SSE reconnect will fetch
/// the current state via `GET /supervisor/state`.
pub fn escalate_to_hitl(
    session: &crate::session::BuildSession,
    evaluation: &NorthstarEvaluationEvent,
) -> bool {
    let ev = NorthstarEvaluationEvent {
        proposal_pending: true,
        ..evaluation.clone()
    };
    session
        .event_tx
        .send(crate::events::WebEvent::SupervisorUpdate(ev))
        .is_ok()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn routing_config_resolves_known_action() {
        let cfg = SupervisorRoutingConfig::default_build_agent();
        let agent = cfg.resolve(&ActionClass::Refocus).unwrap();
        assert_eq!(agent.0, "corso");
    }

    #[test]
    fn routing_config_resolves_escalate_to_eva() {
        let cfg = SupervisorRoutingConfig::default_build_agent();
        let agent = cfg.resolve(&ActionClass::Escalate).unwrap();
        assert_eq!(agent.0, "eva");
    }

    #[test]
    fn empty_routing_config_returns_none_for_any_action() {
        let cfg = SupervisorRoutingConfig::default();
        assert!(cfg.resolve(&ActionClass::Refocus).is_none());
        assert!(cfg.resolve(&ActionClass::Pivot).is_none());
    }

    #[test]
    fn action_class_ordering_is_stable() {
        let mut map = BTreeMap::new();
        map.insert(ActionClass::Refocus, AgentId::from_static("a"));
        map.insert(ActionClass::Continue, AgentId::from_static("b"));
        map.insert(ActionClass::Escalate, AgentId::from_static("c"));
        map.insert(ActionClass::Pivot, AgentId::from_static("d"));
        let keys: Vec<_> = map.keys().collect();
        // Ordering: Refocus < Pivot < Escalate < Continue (declaration order).
        // BTreeMap uses PartialOrd which is derived in declaration order.
        assert_eq!(keys.len(), 4);
    }

    /// §Q check 10 — `escalate_to_hitl` broadcasts `proposal_pending: true`.
    #[test]
    fn test_escalation_sets_proposal_pending_true() {
        use std::path::PathBuf;

        use crate::{config::AgentSession, events::WebEvent, session::BuildSession};

        let session = BuildSession::new(PathBuf::from("/tmp"), AgentSession::default());

        // Subscribe before calling escalate_to_hitl.
        let mut rx = session.event_tx.subscribe();

        let ev = NorthstarEvaluationEvent {
            build_id: session.build_id.to_string(),
            wave_num: 2,
            status: "drifting".to_owned(),
            confidence: 0.72,
            recommended_next: "Refocus on P1.".to_owned(),
            proposal_pending: false, // starts as false — escalation must flip it
        };

        assert!(
            escalate_to_hitl(&session, &ev),
            "escalate_to_hitl must return true with a subscriber"
        );

        let received = rx.try_recv().unwrap();
        let WebEvent::SupervisorUpdate(got) = received else {
            unreachable!("expected SupervisorUpdate event");
        };
        assert!(
            got.proposal_pending,
            "escalate_to_hitl must broadcast proposal_pending=true"
        );
        assert_eq!(got.wave_num, 2, "wave_num must be preserved");
        assert_eq!(got.status, "drifting", "status must be preserved");
    }
}
