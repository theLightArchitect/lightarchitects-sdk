//! `LoopProfile` — optimal loop configuration per strategy × domain.
//!
//! Maps every combination of strategy name, domain, LASDLC phase, budget policy,
//! HITL threshold, and concurrency class into a single `LoopProfile` record.
//! `StrategyRegistry` carries a `HashMap<&'static str, LoopProfile>` keyed by
//! strategy name (not the `RegisteredStrategy` enum) so Class B strategies can
//! be profiled without requiring enum extension.
//!
//! The 3-way merge that resolves the effective profile for a runtime request
//! (global default → domain config → phase override) lives in [`super::scope`].

use std::collections::HashMap;

// ── BudgetPolicy ──────────────────────────────────────────────────────────────

/// Token/cost budget enforcement policy for a loop run.
#[derive(Debug, Clone, PartialEq)]
pub enum BudgetPolicy {
    /// No budget cap — loop runs until the strategy halts.
    Unlimited,
    /// Hard step cap (max number of `Strategy::step` calls).
    StepCapped(u32),
    /// Hard USD cost cap across all steps.
    CostCapped(f64),
    /// Whichever of step or cost is hit first terminates the loop.
    StepOrCost {
        /// Maximum number of steps before budget is exceeded.
        max_steps: u32,
        /// Maximum USD cost before budget is exceeded.
        max_cost_usd: f64,
    },
}

impl Default for BudgetPolicy {
    fn default() -> Self {
        Self::Unlimited
    }
}

// ── LasdlcPhase ───────────────────────────────────────────────────────────────

/// LASDLC phase affinity for profile selection.
///
/// Profiles may specify a preferred phase — the `DomainScopeResolver` uses this
/// to apply phase-specific overrides on top of the domain-level config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LasdlcPhase {
    /// Research / investigation phase (LASDLC Phase 0/1 depending on tier).
    Research,
    /// Architecture + design phase.
    Architecture,
    /// Implementation phase (primary code production).
    Implementation,
    /// Verification + testing phase.
    Verification,
    /// Security audit + hardening phase.
    Security,
    /// Deploy + operations phase.
    Operations,
    /// Close-out, documentation, enrichment.
    CloseOut,
}

impl LasdlcPhase {
    /// Canonical kebab-case ID used in span metadata.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Research => "research",
            Self::Architecture => "architecture",
            Self::Implementation => "implementation",
            Self::Verification => "verification",
            Self::Security => "security",
            Self::Operations => "operations",
            Self::CloseOut => "close-out",
        }
    }
}

// ── ConcurrencyClass ──────────────────────────────────────────────────────────

/// How many concurrent loop instances are safe for this strategy × domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConcurrencyClass {
    /// At most one concurrent instance (e.g. strategies writing shared state).
    Singleton,
    /// Up to 3 concurrent instances (most strategies).
    Low,
    /// Up to 8 concurrent instances (read-heavy, stateless strategies).
    High,
}

impl Default for ConcurrencyClass {
    fn default() -> Self {
        Self::Low
    }
}

// ── LoopProfile ───────────────────────────────────────────────────────────────

/// Optimal loop configuration for a specific strategy × domain combination.
///
/// Profiles are resolved at `LoopRunner::new` time by `DomainScopeResolver`.
/// The span emitted by `trace::emit_dispatch` includes the resolved profile's
/// `strategy_name` and `phase_affinity` for AYIN correlation (P3 check #4).
#[derive(Debug, Clone)]
pub struct LoopProfile {
    /// Canonical strategy ID (matches `Strategy::name()` output).
    pub strategy_name: &'static str,
    /// Human-readable description of this strategy × domain combination.
    pub description: &'static str,
    /// Whether the platform can dispatch this strategy automatically.
    ///
    /// Class A strategies (`auto_dispatchable: true`) are registered in
    /// `StrategyRegistry`. Class B strategies require a caller-constructed
    /// executor and set this to `false`.
    pub auto_dispatchable: bool,
    /// Budget policy to apply for this profile.
    pub budget_policy: BudgetPolicy,
    /// HITL escalation threshold — fraction of steps [0.0, 1.0] that may fail
    /// before the loop pauses for operator input.
    ///
    /// `1.0` means never escalate automatically; `0.0` means escalate on first
    /// step failure.
    pub hitl_threshold: f64,
    /// LASDLC phase this profile is optimised for.
    pub phase_affinity: LasdlcPhase,
    /// Safe concurrency level for this strategy.
    pub concurrency_class: ConcurrencyClass,
    /// Primary sibling domain responsible for reviewing this strategy's outputs.
    pub review_owner: &'static str,
    /// Canonical role strings where this strategy is applicable.
    ///
    /// Used by `StrategyRegistry::for_domain(role)` to filter profiles. Role
    /// strings match `AgentRole::as_str()` output: `"engineer"`, `"security"`,
    /// `"quality"`, `"ops"`, `"researcher"`, `"testing"`, `"knowledge"`,
    /// `"gateway"`.
    pub optimal_domains: &'static [&'static str],
}

// ── DomainScopeConfig ─────────────────────────────────────────────────────────

/// Domain-level configuration that overrides global defaults.
///
/// Applied as the second layer in the 3-way merge:
/// `global default → DomainScopeConfig → phase override`.
#[derive(Debug, Clone, Default)]
pub struct DomainScopeConfig {
    /// Optional budget policy override for all strategies in this domain.
    pub budget_policy: Option<BudgetPolicy>,
    /// Optional HITL threshold override for all strategies in this domain.
    pub hitl_threshold: Option<f64>,
    /// Optional concurrency class cap for this domain.
    pub concurrency_class: Option<ConcurrencyClass>,
    /// Per-phase overrides applied as the third merge layer.
    pub phase_overrides: HashMap<LasdlcPhase, PhaseOverride>,
}

/// Phase-level overrides — third and final merge layer.
#[derive(Debug, Clone, Default)]
pub struct PhaseOverride {
    /// Override budget policy for this specific phase.
    pub budget_policy: Option<BudgetPolicy>,
    /// Override HITL threshold for this specific phase.
    pub hitl_threshold: Option<f64>,
    /// Override concurrency class for this specific phase.
    pub concurrency_class: Option<ConcurrencyClass>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn budget_policy_default_is_unlimited() {
        assert_eq!(BudgetPolicy::default(), BudgetPolicy::Unlimited);
    }

    #[test]
    fn concurrency_class_ordering() {
        assert!(ConcurrencyClass::Singleton < ConcurrencyClass::Low);
        assert!(ConcurrencyClass::Low < ConcurrencyClass::High);
    }

    #[test]
    fn lasdlc_phase_as_str_is_non_empty() {
        for phase in [
            LasdlcPhase::Research,
            LasdlcPhase::Architecture,
            LasdlcPhase::Implementation,
            LasdlcPhase::Verification,
            LasdlcPhase::Security,
            LasdlcPhase::Operations,
            LasdlcPhase::CloseOut,
        ] {
            assert!(
                !phase.as_str().is_empty(),
                "phase {phase:?} has empty as_str"
            );
        }
    }

    #[test]
    fn loop_profile_fields_are_accessible() {
        let profile = LoopProfile {
            strategy_name: "build",
            description: "CORSO build pipeline",
            auto_dispatchable: true,
            budget_policy: BudgetPolicy::StepCapped(50),
            hitl_threshold: 0.8,
            phase_affinity: LasdlcPhase::Implementation,
            concurrency_class: ConcurrencyClass::Singleton,
            review_owner: "CORSO",
            optimal_domains: &["engineer", "ops"],
        };
        assert_eq!(profile.strategy_name, "build");
        assert_eq!(profile.concurrency_class, ConcurrencyClass::Singleton);
        assert!((profile.hitl_threshold - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn domain_scope_config_phase_override_roundtrip() {
        let mut config = DomainScopeConfig::default();
        config.phase_overrides.insert(
            LasdlcPhase::Security,
            PhaseOverride {
                hitl_threshold: Some(0.0),
                ..Default::default()
            },
        );
        let ov = config.phase_overrides.get(&LasdlcPhase::Security).unwrap();
        assert_eq!(ov.hitl_threshold, Some(0.0));
    }
}
