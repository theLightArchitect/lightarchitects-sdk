//! `StrategyRegistry` — lookup and enum-dispatch for chatroom strategy loops.
//!
//! [`Strategy`] has associated types (`State`, `Output`), making it not
//! object-safe (`dyn Strategy` is forbidden).  [`RegisteredStrategy`] solves
//! this with enum dispatch: all four strategies share [`LoopState`] /
//! [`LoopOutput`] as their concrete associated types, and
//! `impl Strategy for RegisteredStrategy` fans the call out to the right arm.
//!
//! # Usage
//!
//! ```rust,ignore
//! use lightarchitects::agent::loops::{StrategyRegistry, LoopRunner, budget::Budget};
//! use lightarchitects::agent::loops::meta_skill::LoopState;
//!
//! let strategy = StrategyRegistry::lookup("build").unwrap();
//! let mut stream = LoopRunner::new(strategy, Budget::unlimited())
//!     .run(LoopState::new("my build context"), Default::default());
//! ```

use async_trait::async_trait;

use super::{
    build::BuildStrategy,
    enrich::EnrichStrategy,
    error::LoopError,
    meta_skill::{LoopOutput, LoopState, MetaSkill},
    runner::{Outcome, StepContext, Strategy},
    scrum::{ScrumMode, ScrumStrategy},
    secure::SecureStrategy,
};

// ── RegisteredStrategy ────────────────────────────────────────────────────────

/// Enum-dispatched union of all registered chatroom strategies.
///
/// Implements [`Strategy`] with `State = LoopState` and
/// `Output = LoopOutput`, enabling `LoopRunner<RegisteredStrategy>`.
pub enum RegisteredStrategy {
    /// CORSO-primary build pipeline.
    Build(BuildStrategy),
    /// SERAPH-primary security assessment.
    Secure(SecureStrategy),
    /// Dual-mode squad review / meeting.
    Scrum(ScrumStrategy),
    /// EVA-primary memory enrichment.
    Enrich(EnrichStrategy),
}

#[async_trait]
impl Strategy for RegisteredStrategy {
    type State = LoopState;
    type Output = LoopOutput;

    async fn step(
        &self,
        state: LoopState,
        ctx: &StepContext,
    ) -> Result<Outcome<LoopState, LoopOutput>, LoopError> {
        match self {
            Self::Build(s) => s.step(state, ctx).await,
            Self::Secure(s) => s.step(state, ctx).await,
            Self::Scrum(s) => s.step(state, ctx).await,
            Self::Enrich(s) => s.step(state, ctx).await,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Build(s) => s.name(),
            Self::Secure(s) => s.name(),
            Self::Scrum(s) => s.name(),
            Self::Enrich(s) => s.name(),
        }
    }

    fn estimated_step_cost_usd(&self) -> f64 {
        match self {
            Self::Build(s) => s.estimated_step_cost_usd(),
            Self::Secure(s) => s.estimated_step_cost_usd(),
            Self::Scrum(s) => s.estimated_step_cost_usd(),
            Self::Enrich(s) => s.estimated_step_cost_usd(),
        }
    }
}

// ── StrategyRegistry ──────────────────────────────────────────────────────────

/// Stateless registry for chatroom strategy lookup.
///
/// All four strategies are constructed with their `Default` implementations.
/// For [`ScrumStrategy`], the default is [`ScrumMode::Review`].
pub struct StrategyRegistry;

impl StrategyRegistry {
    /// Look up a strategy by its canonical ID string.
    ///
    /// Returns `None` if the ID is not registered.
    /// ID strings match `Mode::strategy_id()` output: `"build"`, `"secure"`,
    /// `"scrum"`, `"enrich"`.
    #[must_use]
    pub fn lookup(id: &str) -> Option<RegisteredStrategy> {
        match MetaSkill::from_id(id)? {
            MetaSkill::Build => Some(RegisteredStrategy::Build(BuildStrategy::new())),
            MetaSkill::Secure => Some(RegisteredStrategy::Secure(SecureStrategy::new())),
            MetaSkill::Scrum => Some(RegisteredStrategy::Scrum(ScrumStrategy::review())),
            MetaSkill::Enrich => Some(RegisteredStrategy::Enrich(EnrichStrategy::new())),
        }
    }

    /// Look up a strategy by [`MetaSkill`] variant.
    #[must_use]
    pub fn lookup_skill(skill: MetaSkill) -> RegisteredStrategy {
        match skill {
            MetaSkill::Build => RegisteredStrategy::Build(BuildStrategy::new()),
            MetaSkill::Secure => RegisteredStrategy::Secure(SecureStrategy::new()),
            MetaSkill::Scrum => RegisteredStrategy::Scrum(ScrumStrategy::review()),
            MetaSkill::Enrich => RegisteredStrategy::Enrich(EnrichStrategy::new()),
        }
    }

    /// Look up a `ScrumStrategy` with an explicit [`ScrumMode`].
    #[must_use]
    pub fn scrum(mode: ScrumMode) -> RegisteredStrategy {
        RegisteredStrategy::Scrum(ScrumStrategy { mode })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn lookup_all_registered_ids() {
        for id in ["build", "secure", "scrum", "enrich"] {
            assert!(
                StrategyRegistry::lookup(id).is_some(),
                "id '{id}' should be registered"
            );
        }
    }

    #[test]
    fn lookup_unknown_returns_none() {
        assert!(StrategyRegistry::lookup("unknown").is_none());
        assert!(StrategyRegistry::lookup("").is_none());
    }

    #[test]
    fn lookup_skill_matches_lookup_by_id() {
        let by_id = StrategyRegistry::lookup("build").unwrap();
        let by_skill = StrategyRegistry::lookup_skill(MetaSkill::Build);
        assert_eq!(by_id.name(), by_skill.name());
    }

    #[test]
    fn scrum_meeting_mode_name_differs_from_review() {
        let review = StrategyRegistry::scrum(ScrumMode::Review);
        let meeting = StrategyRegistry::scrum(ScrumMode::Meeting);
        assert_ne!(review.name(), meeting.name());
    }
}
