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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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
            optimal_domains: &["engineer"],
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

    // ── Registry integration tests ────────────────────────────────────────────

    #[test]
    fn resolver_accepts_all_registered_profiles() {
        use crate::agent::loops::registry::StrategyRegistry;

        // Every registered profile should resolve cleanly with no domain config
        // (global-only path) for every LASDLC phase.
        for phase in [
            LasdlcPhase::Research,
            LasdlcPhase::Architecture,
            LasdlcPhase::Implementation,
            LasdlcPhase::Verification,
            LasdlcPhase::Security,
            LasdlcPhase::Operations,
            LasdlcPhase::CloseOut,
        ] {
            for name in [
                "build",
                "secure",
                "scrum",
                "enrich",
                "gate",
                "scope_governor",
                "react",
                "bcra",
                "cove",
                "itt",
                "reflexion",
                "multipass",
                "red_team",
                "drain",
                "ensemble",
                "ach",
                "critique_refine",
                "react_with_memory",
                "sandbox_exec",
            ] {
                let profile = StrategyRegistry::profile(name)
                    .unwrap_or_else(|| panic!("profile '{name}' not registered"));
                let cfg = DomainScopeResolver::resolve(profile, None, phase);
                assert!(
                    (0.0..=1.0).contains(&cfg.hitl_threshold),
                    "profile '{name}' phase '{phase:?}' hitl_threshold out of range"
                );
            }
        }
    }

    #[test]
    fn security_role_default_produces_class_a_profile_resolvable() {
        use crate::agent::loops::registry::StrategyRegistry;

        let strategy_name = StrategyRegistry::role_to_default_strategy("security")
            .expect("security role must have a default strategy");
        let profile =
            StrategyRegistry::profile(strategy_name).expect("default strategy must have a profile");
        assert!(profile.auto_dispatchable);

        // Security phase override: max HITL sensitivity
        let mut overrides = HashMap::new();
        overrides.insert(
            LasdlcPhase::Security,
            PhaseOverride {
                hitl_threshold: Some(0.0),
                concurrency_class: Some(ConcurrencyClass::Singleton),
                ..Default::default()
            },
        );
        let domain = DomainScopeConfig {
            phase_overrides: overrides,
            ..Default::default()
        };
        let cfg = DomainScopeResolver::resolve(profile, Some(&domain), LasdlcPhase::Security);
        assert!((cfg.hitl_threshold - 0.0).abs() < f64::EPSILON);
        assert_eq!(cfg.concurrency_class, ConcurrencyClass::Singleton);
    }

    #[test]
    fn all_roles_resolve_to_valid_config_with_strict_security_domain() {
        use crate::agent::loops::registry::StrategyRegistry;

        // A maximally-strict domain config (Singleton, HITL=0) should resolve
        // for any role without panicking.
        let mut overrides = HashMap::new();
        for phase in [
            LasdlcPhase::Research,
            LasdlcPhase::Security,
            LasdlcPhase::Verification,
        ] {
            overrides.insert(
                phase,
                PhaseOverride {
                    concurrency_class: Some(ConcurrencyClass::Singleton),
                    hitl_threshold: Some(0.0),
                    budget_policy: Some(BudgetPolicy::StepCapped(5)),
                },
            );
        }
        let strict_domain = DomainScopeConfig {
            budget_policy: Some(BudgetPolicy::StepCapped(50)),
            hitl_threshold: Some(0.1),
            concurrency_class: Some(ConcurrencyClass::Singleton),
            phase_overrides: overrides,
        };

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
            let name = StrategyRegistry::role_to_default_strategy(role)
                .unwrap_or_else(|| panic!("role '{role}' missing default"));
            let profile = StrategyRegistry::profile(name)
                .unwrap_or_else(|| panic!("profile '{name}' not registered"));
            let cfg =
                DomainScopeResolver::resolve(profile, Some(&strict_domain), LasdlcPhase::Security);
            assert_eq!(
                cfg.concurrency_class,
                ConcurrencyClass::Singleton,
                "role '{role}': phase override should enforce Singleton"
            );
            assert!((cfg.hitl_threshold - 0.0).abs() < f64::EPSILON);
        }
    }
}
