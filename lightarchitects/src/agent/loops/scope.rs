//! `DomainScopeResolver` — 3-way merge for loop profile configuration.
//!
//! Resolves the effective `LoopProfile` fields for a runtime dispatch request
//! by merging three configuration layers in priority order:
//!
//! 1. **Global default** — baseline from the strategy's `LoopProfile` entry.
//! 2. **Domain override** — `DomainScopeConfig` for the caller's domain.
//! 3. **Phase override** — `PhaseOverride` for the active LASDLC phase.
//!
//! The last non-`None` value wins. This lets domain admins tighten budgets
//! without affecting other domains, and phase configs tighten further for
//! security-sensitive phases (e.g. `LasdlcPhase::Security` → HITL threshold 0.0).

use super::profile::{BudgetPolicy, ConcurrencyClass, DomainScopeConfig, LasdlcPhase, LoopProfile};

/// Resolved configuration for a single loop dispatch.
///
/// All fields are guaranteed to have a concrete value — `None` has been
/// eliminated by the 3-way merge. Callers use this to construct a `Budget`
/// and configure the `LoopRunner`.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// Effective budget policy (after all merge layers).
    pub budget_policy: BudgetPolicy,
    /// Effective HITL escalation threshold [0.0, 1.0].
    pub hitl_threshold: f64,
    /// Effective concurrency class.
    pub concurrency_class: ConcurrencyClass,
}

// ── DomainScopeResolver ───────────────────────────────────────────────────────

/// Performs the 3-way merge: global default → domain config → phase override.
pub struct DomainScopeResolver;

impl DomainScopeResolver {
    /// Resolve the effective configuration for a single dispatch.
    ///
    /// # Arguments
    ///
    /// * `profile` — the strategy's base `LoopProfile` (Layer 1: global default).
    /// * `domain_config` — optional domain-level overrides (Layer 2).
    /// * `active_phase` — the LASDLC phase active at dispatch time, used to
    ///   look up `PhaseOverride` entries in `domain_config` (Layer 3).
    #[must_use]
    pub fn resolve(
        profile: &LoopProfile,
        domain_config: Option<&DomainScopeConfig>,
        active_phase: LasdlcPhase,
    ) -> ResolvedConfig {
        // Layer 1 — global defaults from the profile.
        let mut budget = profile.budget_policy.clone();
        let mut hitl = profile.hitl_threshold;
        let mut concurrency = profile.concurrency_class;

        // Layer 2 — domain-level overrides.
        if let Some(domain) = domain_config {
            if let Some(ref b) = domain.budget_policy {
                budget = b.clone();
            }
            if let Some(h) = domain.hitl_threshold {
                hitl = h;
            }
            if let Some(c) = domain.concurrency_class {
                concurrency = c;
            }

            // Layer 3 — phase-level overrides (only if the domain knows this phase).
            if let Some(phase_ov) = domain.phase_overrides.get(&active_phase) {
                if let Some(ref b) = phase_ov.budget_policy {
                    budget = b.clone();
                }
                if let Some(h) = phase_ov.hitl_threshold {
                    hitl = h;
                }
                if let Some(c) = phase_ov.concurrency_class {
                    concurrency = c;
                }
            }
        }

        ResolvedConfig {
            budget_policy: budget,
            hitl_threshold: hitl,
            concurrency_class: concurrency,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::agent::loops::profile::PhaseOverride;

    fn base_profile() -> LoopProfile {
        LoopProfile {
            strategy_name: "build",
            description: "test profile",
            auto_dispatchable: true,
            budget_policy: BudgetPolicy::StepCapped(100),
            hitl_threshold: 0.9,
            phase_affinity: LasdlcPhase::Implementation,
            concurrency_class: ConcurrencyClass::Low,
            review_owner: "CORSO",
        }
    }

    #[test]
    fn global_only_returns_profile_defaults() {
        let cfg = DomainScopeResolver::resolve(&base_profile(), None, LasdlcPhase::Implementation);
        assert_eq!(cfg.budget_policy, BudgetPolicy::StepCapped(100));
        assert!((cfg.hitl_threshold - 0.9).abs() < f64::EPSILON);
        assert_eq!(cfg.concurrency_class, ConcurrencyClass::Low);
    }

    #[test]
    fn domain_override_wins_over_global() {
        let domain = DomainScopeConfig {
            budget_policy: Some(BudgetPolicy::StepCapped(20)),
            hitl_threshold: Some(0.5),
            concurrency_class: None,
            phase_overrides: HashMap::new(),
        };
        let cfg = DomainScopeResolver::resolve(
            &base_profile(),
            Some(&domain),
            LasdlcPhase::Implementation,
        );
        assert_eq!(cfg.budget_policy, BudgetPolicy::StepCapped(20));
        assert!((cfg.hitl_threshold - 0.5).abs() < f64::EPSILON);
        // concurrency not overridden — falls back to global
        assert_eq!(cfg.concurrency_class, ConcurrencyClass::Low);
    }

    #[test]
    fn phase_override_wins_over_domain() {
        let mut overrides = HashMap::new();
        overrides.insert(
            LasdlcPhase::Security,
            PhaseOverride {
                hitl_threshold: Some(0.0), // always escalate in security phase
                budget_policy: Some(BudgetPolicy::StepCapped(10)),
                concurrency_class: Some(ConcurrencyClass::Singleton),
            },
        );
        let domain = DomainScopeConfig {
            budget_policy: Some(BudgetPolicy::StepCapped(50)),
            hitl_threshold: Some(0.7),
            concurrency_class: None,
            phase_overrides: overrides,
        };
        let cfg =
            DomainScopeResolver::resolve(&base_profile(), Some(&domain), LasdlcPhase::Security);
        assert_eq!(cfg.budget_policy, BudgetPolicy::StepCapped(10));
        assert!((cfg.hitl_threshold - 0.0).abs() < f64::EPSILON);
        assert_eq!(cfg.concurrency_class, ConcurrencyClass::Singleton);
    }

    #[test]
    fn all_three_layers_applied_in_order() {
        // Global: step=100, hitl=0.9, concurrency=Low
        // Domain: step=50,  hitl=0.6, concurrency=High
        // Phase:  step=5,   hitl=0.1, concurrency=Singleton
        let mut overrides = HashMap::new();
        overrides.insert(
            LasdlcPhase::CloseOut,
            PhaseOverride {
                budget_policy: Some(BudgetPolicy::StepCapped(5)),
                hitl_threshold: Some(0.1),
                concurrency_class: Some(ConcurrencyClass::Singleton),
            },
        );
        let domain = DomainScopeConfig {
            budget_policy: Some(BudgetPolicy::StepCapped(50)),
            hitl_threshold: Some(0.6),
            concurrency_class: Some(ConcurrencyClass::High),
            phase_overrides: overrides,
        };
        let cfg =
            DomainScopeResolver::resolve(&base_profile(), Some(&domain), LasdlcPhase::CloseOut);
        assert_eq!(cfg.budget_policy, BudgetPolicy::StepCapped(5));
        assert!((cfg.hitl_threshold - 0.1).abs() < f64::EPSILON);
        assert_eq!(cfg.concurrency_class, ConcurrencyClass::Singleton);
    }

    #[test]
    fn empty_domain_config_uses_global() {
        let domain = DomainScopeConfig::default();
        let cfg = DomainScopeResolver::resolve(
            &base_profile(),
            Some(&domain),
            LasdlcPhase::Implementation,
        );
        assert_eq!(cfg.budget_policy, BudgetPolicy::StepCapped(100));
        assert!((cfg.hitl_threshold - 0.9).abs() < f64::EPSILON);
        assert_eq!(cfg.concurrency_class, ConcurrencyClass::Low);
    }

    #[test]
    fn missing_phase_in_domain_falls_back_to_domain() {
        // Domain has phase_overrides, but NOT for the requested phase.
        let mut overrides = HashMap::new();
        overrides.insert(
            LasdlcPhase::Security,
            PhaseOverride {
                hitl_threshold: Some(0.0),
                ..Default::default()
            },
        );
        let domain = DomainScopeConfig {
            hitl_threshold: Some(0.55),
            phase_overrides: overrides,
            ..Default::default()
        };
        let cfg =
            DomainScopeResolver::resolve(&base_profile(), Some(&domain), LasdlcPhase::Research);
        // Research phase not in overrides → domain hitl_threshold wins
        assert!((cfg.hitl_threshold - 0.55).abs() < f64::EPSILON);
    }

    #[test]
    fn conflict_resolution_last_layer_wins() {
        // Both domain and phase set budget — phase must win.
        let mut overrides = HashMap::new();
        overrides.insert(
            LasdlcPhase::Verification,
            PhaseOverride {
                budget_policy: Some(BudgetPolicy::CostCapped(0.50)),
                ..Default::default()
            },
        );
        let domain = DomainScopeConfig {
            budget_policy: Some(BudgetPolicy::Unlimited),
            phase_overrides: overrides,
            ..Default::default()
        };
        let cfg =
            DomainScopeResolver::resolve(&base_profile(), Some(&domain), LasdlcPhase::Verification);
        assert_eq!(cfg.budget_policy, BudgetPolicy::CostCapped(0.50));
    }
}
