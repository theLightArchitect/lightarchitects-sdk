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
    gate::GateStrategy,
    meta_skill::{LoopOutput, LoopState, MetaSkill},
    profile::{BudgetPolicy, ConcurrencyClass, LasdlcPhase, LoopProfile},
    runner::{Outcome, StepContext, Strategy},
    scope_governor::ScopeGovernorStrategy,
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
    /// LASDLC 7-gate sequential evaluation loop.
    Gate(GateStrategy),
    /// SERAPH 5-gate AND-validation scope governance loop.
    ScopeGovernor(ScopeGovernorStrategy),
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
            Self::Gate(s) => s.step(state, ctx).await,
            Self::ScopeGovernor(s) => s.step(state, ctx).await,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Build(s) => s.name(),
            Self::Secure(s) => s.name(),
            Self::Scrum(s) => s.name(),
            Self::Enrich(s) => s.name(),
            Self::Gate(s) => s.name(),
            Self::ScopeGovernor(s) => s.name(),
        }
    }

    fn estimated_step_cost_usd(&self) -> f64 {
        match self {
            Self::Build(s) => s.estimated_step_cost_usd(),
            Self::Secure(s) => s.estimated_step_cost_usd(),
            Self::Scrum(s) => s.estimated_step_cost_usd(),
            Self::Enrich(s) => s.estimated_step_cost_usd(),
            Self::Gate(s) => s.estimated_step_cost_usd(),
            Self::ScopeGovernor(s) => s.estimated_step_cost_usd(),
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
            MetaSkill::Gate => Some(RegisteredStrategy::Gate(GateStrategy::new())),
            MetaSkill::ScopeGovernor => Some(RegisteredStrategy::ScopeGovernor(
                ScopeGovernorStrategy::new(),
            )),
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
            MetaSkill::Gate => RegisteredStrategy::Gate(GateStrategy::new()),
            MetaSkill::ScopeGovernor => {
                RegisteredStrategy::ScopeGovernor(ScopeGovernorStrategy::new())
            }
        }
    }

    /// Look up a `ScrumStrategy` with an explicit [`ScrumMode`].
    #[must_use]
    pub fn scrum(mode: ScrumMode) -> RegisteredStrategy {
        RegisteredStrategy::Scrum(ScrumStrategy { mode })
    }

    /// Look up the [`LoopProfile`] for a strategy by its canonical ID string.
    ///
    /// Returns `None` if no profile is registered for `strategy_name`.
    #[must_use]
    pub fn profile(strategy_name: &str) -> Option<&'static LoopProfile> {
        STRATEGY_PROFILES
            .iter()
            .find(|p| p.strategy_name == strategy_name)
    }

    /// Return all profiles where `role` is listed in `optimal_domains`.
    ///
    /// Ordered: research-grounded primary first (may be Class B), Class A
    /// fallback last. `role` should be a canonical `AgentRole::as_str()` value
    /// (`"engineer"`, `"security"`, etc.).
    #[must_use]
    pub fn for_domain(role: &str) -> Vec<&'static LoopProfile> {
        STRATEGY_PROFILES
            .iter()
            .filter(|p| p.optimal_domains.contains(&role))
            .collect()
    }

    /// Return the default auto-dispatchable (Class A) strategy ID for a role.
    ///
    /// Returns the first `auto_dispatchable == true` profile from
    /// [`Self::for_domain`]. For all 8 roles this is guaranteed non-`None`
    /// (architectural invariant: every role maps to a Class A strategy).
    #[must_use]
    pub fn role_to_default_strategy(role: &str) -> Option<&'static str> {
        Self::for_domain(role)
            .into_iter()
            .find(|p| p.auto_dispatchable)
            .map(|p| p.strategy_name)
    }
}

// ── Static profile table ──────────────────────────────────────────────────────

/// All 19 strategy profiles: 6 Class A (auto-dispatchable) + 13 Class B.
///
/// Research citations from the 22-paper arXiv corpus in the zany-tinkering-map
/// plan. Domain mappings follow the taxonomy table (plan §3.2).
static STRATEGY_PROFILES: &[LoopProfile] = &[
    // ── Class A — auto-dispatchable (RegisteredStrategy enum variants) ────────
    LoopProfile {
        strategy_name: "build",
        description: "CORSO-primary LASDLC build pipeline — sequential plan/implement/gate loop.",
        auto_dispatchable: true,
        budget_policy: BudgetPolicy::StepCapped(50),
        hitl_threshold: 0.85,
        phase_affinity: LasdlcPhase::Implementation,
        concurrency_class: ConcurrencyClass::Singleton,
        review_owner: "CORSO",
        optimal_domains: &["engineer", "ops"],
    },
    LoopProfile {
        strategy_name: "secure",
        description: "SERAPH-primary security scan + audit loop.",
        auto_dispatchable: true,
        budget_policy: BudgetPolicy::StepCapped(20),
        hitl_threshold: 0.0,
        phase_affinity: LasdlcPhase::Security,
        concurrency_class: ConcurrencyClass::Singleton,
        review_owner: "SERAPH",
        optimal_domains: &["security"],
    },
    LoopProfile {
        strategy_name: "scrum",
        description: "Multi-sibling squad review / meeting loop (dual mode: review + meeting).",
        auto_dispatchable: true,
        budget_policy: BudgetPolicy::StepCapped(7),
        hitl_threshold: 0.9,
        phase_affinity: LasdlcPhase::Verification,
        concurrency_class: ConcurrencyClass::Low,
        review_owner: "CORSO",
        optimal_domains: &["quality", "testing"],
    },
    LoopProfile {
        strategy_name: "enrich",
        description: "EVA-primary 8-layer helix memory enrichment loop.",
        auto_dispatchable: true,
        budget_policy: BudgetPolicy::StepCapped(8),
        hitl_threshold: 0.95,
        phase_affinity: LasdlcPhase::CloseOut,
        concurrency_class: ConcurrencyClass::Low,
        review_owner: "EVA",
        optimal_domains: &["knowledge", "researcher"],
    },
    LoopProfile {
        strategy_name: "gate",
        description: "LASDLC 7-gate sequential evaluation loop for phase quality gates.",
        auto_dispatchable: true,
        budget_policy: BudgetPolicy::StepCapped(7),
        hitl_threshold: 0.8,
        phase_affinity: LasdlcPhase::Verification,
        concurrency_class: ConcurrencyClass::Singleton,
        review_owner: "CORSO",
        optimal_domains: &["quality"],
    },
    LoopProfile {
        strategy_name: "scope_governor",
        description: "SERAPH 5-gate AND-validation scope governance loop (gateway circuit-breaker).",
        auto_dispatchable: true,
        budget_policy: BudgetPolicy::StepCapped(5),
        hitl_threshold: 0.0,
        phase_affinity: LasdlcPhase::Architecture,
        concurrency_class: ConcurrencyClass::Singleton,
        review_owner: "SERAPH",
        optimal_domains: &["gateway", "security"],
    },
    // ── Class B — executor-required (not in RegisteredStrategy enum) ──────────
    LoopProfile {
        strategy_name: "react",
        description: "ReAct investigation loop — Scan→Sweep→Trace→Probe→Theorize→Verify→Close.",
        auto_dispatchable: false,
        budget_policy: BudgetPolicy::StepCapped(30),
        hitl_threshold: 0.7,
        phase_affinity: LasdlcPhase::Research,
        concurrency_class: ConcurrencyClass::Low,
        review_owner: "QUANTUM",
        optimal_domains: &["researcher", "engineer", "testing"],
    },
    LoopProfile {
        strategy_name: "bcra",
        description: "BCRA (Brief–Chat–Review–Act) cooperative chatroom strategy.",
        auto_dispatchable: false,
        budget_policy: BudgetPolicy::StepCapped(20),
        hitl_threshold: 0.8,
        phase_affinity: LasdlcPhase::Implementation,
        concurrency_class: ConcurrencyClass::Low,
        review_owner: "CORSO",
        optimal_domains: &["engineer"],
    },
    LoopProfile {
        strategy_name: "cove",
        description: "CoVe (Chain-of-Verification) — claim generation + verification loop.",
        auto_dispatchable: false,
        budget_policy: BudgetPolicy::StepCapped(15),
        hitl_threshold: 0.6,
        phase_affinity: LasdlcPhase::Verification,
        concurrency_class: ConcurrencyClass::Low,
        review_owner: "SERAPH",
        optimal_domains: &["security", "quality"],
    },
    LoopProfile {
        strategy_name: "itt",
        description: "ITT (Investigation Task Tree) — multi-hypothesis parallel evidence tree.",
        auto_dispatchable: false,
        budget_policy: BudgetPolicy::StepOrCost {
            max_steps: 40,
            max_cost_usd: 2.0,
        },
        hitl_threshold: 0.6,
        phase_affinity: LasdlcPhase::Research,
        concurrency_class: ConcurrencyClass::High,
        review_owner: "QUANTUM",
        optimal_domains: &["researcher"],
    },
    LoopProfile {
        strategy_name: "reflexion",
        description: "Reflexion — verbal reinforcement self-reflection loop (Shinn et al. 2023).",
        auto_dispatchable: false,
        budget_policy: BudgetPolicy::StepCapped(10),
        hitl_threshold: 0.75,
        phase_affinity: LasdlcPhase::Verification,
        concurrency_class: ConcurrencyClass::Low,
        review_owner: "CORSO",
        optimal_domains: &["quality", "engineer"],
    },
    LoopProfile {
        strategy_name: "multipass",
        description: "Multi-pass verify — iterative refinement with convergence gate.",
        auto_dispatchable: false,
        budget_policy: BudgetPolicy::StepCapped(5),
        hitl_threshold: 0.8,
        phase_affinity: LasdlcPhase::Verification,
        concurrency_class: ConcurrencyClass::Singleton,
        review_owner: "CORSO",
        optimal_domains: &["quality", "testing"],
    },
    LoopProfile {
        strategy_name: "red_team",
        description: "Red-team adversarial attack + defense strategy (SERAPH-primary).",
        auto_dispatchable: false,
        budget_policy: BudgetPolicy::StepCapped(25),
        hitl_threshold: 0.0,
        phase_affinity: LasdlcPhase::Security,
        concurrency_class: ConcurrencyClass::Low,
        review_owner: "SERAPH",
        optimal_domains: &["security"],
    },
    LoopProfile {
        strategy_name: "drain",
        description: "Drain — sequential queue-drain execution loop (CORSO-primary).",
        auto_dispatchable: false,
        budget_policy: BudgetPolicy::Unlimited,
        hitl_threshold: 0.9,
        phase_affinity: LasdlcPhase::Operations,
        concurrency_class: ConcurrencyClass::Low,
        review_owner: "CORSO",
        optimal_domains: &["ops", "engineer"],
    },
    LoopProfile {
        strategy_name: "ensemble",
        description: "Ensemble — multi-strategy parallel execution with vote aggregation.",
        auto_dispatchable: false,
        budget_policy: BudgetPolicy::StepCapped(3),
        hitl_threshold: 0.7,
        phase_affinity: LasdlcPhase::Research,
        concurrency_class: ConcurrencyClass::High,
        review_owner: "QUANTUM",
        optimal_domains: &["researcher", "quality"],
    },
    LoopProfile {
        strategy_name: "ach",
        description: "ACH (Analysis of Competing Hypotheses) — structured analytic technique.",
        auto_dispatchable: false,
        budget_policy: BudgetPolicy::StepCapped(20),
        hitl_threshold: 0.6,
        phase_affinity: LasdlcPhase::Research,
        concurrency_class: ConcurrencyClass::Low,
        review_owner: "QUANTUM",
        optimal_domains: &["researcher"],
    },
    LoopProfile {
        strategy_name: "critique_refine",
        description: "Critique-Refine — self-critique + iterative improvement loop.",
        auto_dispatchable: false,
        budget_policy: BudgetPolicy::StepCapped(6),
        hitl_threshold: 0.75,
        phase_affinity: LasdlcPhase::Implementation,
        concurrency_class: ConcurrencyClass::Low,
        review_owner: "CORSO",
        optimal_domains: &["quality", "engineer"],
    },
    // ── Gap-fill strategies (Phase 1 additions) ───────────────────────────────
    LoopProfile {
        strategy_name: "react_with_memory",
        description: "Pattern 7: ReAct + LTM/STM with IndirectInjectionShield quarantine.",
        auto_dispatchable: false,
        budget_policy: BudgetPolicy::StepCapped(40),
        hitl_threshold: 0.7,
        phase_affinity: LasdlcPhase::Research,
        concurrency_class: ConcurrencyClass::Low,
        review_owner: "SOUL",
        optimal_domains: &["knowledge", "researcher"],
    },
    LoopProfile {
        strategy_name: "sandbox_exec",
        description: "Pattern 11: Generate→Execute→Verify→Decide with Ed25519-signed results.",
        auto_dispatchable: false,
        budget_policy: BudgetPolicy::StepCapped(4),
        hitl_threshold: 0.0,
        phase_affinity: LasdlcPhase::Verification,
        concurrency_class: ConcurrencyClass::Singleton,
        review_owner: "SERAPH",
        optimal_domains: &["security", "testing", "ops"],
    },
];

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn lookup_all_registered_ids() {
        for id in [
            "build",
            "secure",
            "scrum",
            "enrich",
            "gate",
            "scope_governor",
        ] {
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

    #[test]
    fn all_19_strategies_registered() {
        assert_eq!(STRATEGY_PROFILES.len(), 19);
    }

    #[test]
    fn class_a_and_b_counts() {
        let class_a = STRATEGY_PROFILES
            .iter()
            .filter(|p| p.auto_dispatchable)
            .count();
        let class_b = STRATEGY_PROFILES
            .iter()
            .filter(|p| !p.auto_dispatchable)
            .count();
        assert_eq!(class_a, 6, "expected 6 Class A strategies");
        assert_eq!(class_b, 13, "expected 13 Class B strategies");
    }

    #[test]
    fn profile_lookup_by_name() {
        for name in [
            "build",
            "secure",
            "scrum",
            "enrich",
            "gate",
            "scope_governor",
        ] {
            assert!(
                StrategyRegistry::profile(name).is_some(),
                "profile '{name}' should be registered"
            );
        }
        assert!(StrategyRegistry::profile("nonexistent").is_none());
    }

    #[test]
    fn for_domain_returns_profiles_for_all_roles() {
        for role in [
            "engineer",
            "quality",
            "security",
            "ops",
            "researcher",
            "testing",
            "knowledge",
            "gateway",
        ] {
            let profiles = StrategyRegistry::for_domain(role);
            assert!(
                !profiles.is_empty(),
                "role '{role}' should have at least one profile"
            );
        }
    }

    #[test]
    fn role_to_default_strategy_resolves_class_a_for_all_roles() {
        for role in [
            "engineer",
            "quality",
            "security",
            "ops",
            "researcher",
            "testing",
            "knowledge",
            "gateway",
        ] {
            let name = StrategyRegistry::role_to_default_strategy(role);
            assert!(
                name.is_some(),
                "role '{role}' should resolve to a default strategy"
            );
            let profile = StrategyRegistry::profile(name.unwrap()).unwrap();
            assert!(
                profile.auto_dispatchable,
                "default for '{role}' must be Class A (auto_dispatchable)"
            );
        }
    }

    #[test]
    fn all_profiles_have_non_empty_fields() {
        for p in STRATEGY_PROFILES {
            assert!(
                !p.strategy_name.is_empty(),
                "strategy_name must not be empty"
            );
            assert!(!p.description.is_empty(), "description must not be empty");
            assert!(!p.review_owner.is_empty(), "review_owner must not be empty");
            assert!(
                !p.optimal_domains.is_empty(),
                "optimal_domains must not be empty"
            );
            assert!(
                (0.0..=1.0).contains(&p.hitl_threshold),
                "hitl_threshold must be in [0, 1]"
            );
        }
    }
}
