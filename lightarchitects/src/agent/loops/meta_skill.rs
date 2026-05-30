//! Unified state and output types shared by all registered chatroom strategies.
//!
//! [`MetaSkill`] names the four strategy loops dispatched by [`super::registry`].
//! [`LoopState`] and [`LoopOutput`] are the concrete `Strategy::State` and
//! `Strategy::Output` types shared across all four strategies, enabling the
//! registry to return a single enum that implements [`Strategy`] without
//! sacrificing type safety.

use std::collections::HashMap;

// ── LoopState ─────────────────────────────────────────────────────────────────

/// Mutable state threaded through each step of a registered strategy.
#[derive(Debug, Clone)]
pub struct LoopState {
    /// Current phase index (0-based; strategy-specific meaning).
    pub phase: u32,
    /// Freeform context string passed between phases (e.g., accumulated plan text).
    pub context: String,
    /// Artifacts produced so far (file paths, URLs, identifiers).
    pub artifacts: Vec<String>,
    /// Arbitrary per-strategy metadata (key-value extension bag).
    pub meta: HashMap<String, String>,
}

impl LoopState {
    /// Create an empty initial state.
    #[must_use]
    pub fn new(context: impl Into<String>) -> Self {
        Self {
            phase: 0,
            context: context.into(),
            artifacts: Vec::new(),
            meta: HashMap::new(),
        }
    }
}

// ── LoopOutput ────────────────────────────────────────────────────────────────

/// Terminal output produced when a registered strategy halts.
#[derive(Debug)]
pub struct LoopOutput {
    /// Name of the strategy that produced this output.
    pub strategy_name: String,
    /// Human-readable summary of what was accomplished.
    pub summary: String,
    /// Total number of phases completed.
    pub phases_run: u32,
    /// All artifacts produced across all phases.
    pub artifacts: Vec<String>,
}

// ── MetaSkill ─────────────────────────────────────────────────────────────────

/// Named strategy loop, one-to-one with [`crate::chat::Mode`] strategy variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaSkill {
    /// CORSO-primary 3-phase build pipeline.
    Build,
    /// SERAPH-primary 3-phase security assessment.
    Secure,
    /// Dual-mode squad review or meeting.
    Scrum,
    /// EVA-primary 3-phase memory enrichment.
    Enrich,
    /// LASDLC 7-gate sequential evaluation loop.
    Gate,
    /// SERAPH 5-gate AND-validation scope governance loop.
    ScopeGovernor,
}

impl MetaSkill {
    /// Resolve from a strategy ID string (matches `Mode::strategy_id()` output).
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "build" => Some(Self::Build),
            "secure" => Some(Self::Secure),
            "scrum" => Some(Self::Scrum),
            "enrich" => Some(Self::Enrich),
            "gate" => Some(Self::Gate),
            "scope_governor" => Some(Self::ScopeGovernor),
            _ => None,
        }
    }

    /// Canonical strategy ID (inverse of [`from_id`]).
    #[must_use]
    pub fn strategy_id(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Secure => "secure",
            Self::Scrum => "scrum",
            Self::Enrich => "enrich",
            Self::Gate => "gate",
            Self::ScopeGovernor => "scope_governor",
        }
    }

    /// Primary sibling owner for this strategy.
    #[must_use]
    pub fn primary_sibling(self) -> &'static str {
        match self {
            Self::Build => "corso",
            Self::Secure | Self::ScopeGovernor => "seraph",
            Self::Scrum => "claude",
            Self::Enrich => "eva",
            Self::Gate => "laex",
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn from_id_round_trips() {
        for id in [
            "build",
            "secure",
            "scrum",
            "enrich",
            "gate",
            "scope_governor",
        ] {
            let skill = MetaSkill::from_id(id).unwrap();
            assert_eq!(skill.strategy_id(), id);
        }
    }

    #[test]
    fn from_id_unknown_returns_none() {
        assert!(MetaSkill::from_id("unknown").is_none());
        assert!(MetaSkill::from_id("").is_none());
    }

    #[test]
    fn primary_siblings_are_correct() {
        assert_eq!(MetaSkill::Build.primary_sibling(), "corso");
        assert_eq!(MetaSkill::Secure.primary_sibling(), "seraph");
        assert_eq!(MetaSkill::Scrum.primary_sibling(), "claude");
        assert_eq!(MetaSkill::Enrich.primary_sibling(), "eva");
        assert_eq!(MetaSkill::Gate.primary_sibling(), "laex");
        assert_eq!(MetaSkill::ScopeGovernor.primary_sibling(), "seraph");
    }

    #[test]
    fn loop_state_new_starts_at_phase_zero() {
        let s = LoopState::new("ctx");
        assert_eq!(s.phase, 0);
        assert!(s.artifacts.is_empty());
        assert!(s.meta.is_empty());
    }
}
