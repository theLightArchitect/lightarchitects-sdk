//! Policy default construction, tier resource mapping, and validation tests.

use crate::container_spawn::policy::{
    AgentTier, ContainerPolicy, ContainerResources, CredentialStrategy, IsoMode, MIN_CONCURRENT,
    MIN_CPUS, MIN_MEMORY_MB, MIN_PIDS, NetworkPolicy, PolicyStore, SpawnError, SpawnPolicy,
};

#[test]
fn default_policy_is_standard_bridge() {
    let p = ContainerPolicy::default();
    assert_eq!(p.iso_mode, IsoMode::Standard);
    assert_eq!(p.network, NetworkPolicy::Bridge);
    assert_eq!(p.credentials, CredentialStrategy::Inherit);
    assert_eq!(p.tier, AgentTier::Standard);
}

#[test]
fn default_policy_validates_cleanly() {
    #[allow(clippy::unwrap_used)]
    ContainerPolicy::default().validate().unwrap();
}

#[test]
fn agent_tier_micro_resources() {
    let r = AgentTier::Micro.default_resources();
    assert_eq!(r.memory_mb, 512);
    assert!((r.cpus - 0.5).abs() < f64::EPSILON);
    assert_eq!(r.pids_limit, 64);
    assert_eq!(r.max_concurrent, 2);
}

#[test]
fn agent_tier_large_resources() {
    let r = AgentTier::Large.default_resources();
    assert_eq!(r.memory_mb, 8_192);
    assert!((r.cpus - 4.0).abs() < f64::EPSILON);
    assert_eq!(r.pids_limit, 512);
    assert_eq!(r.max_concurrent, 8);
}

#[test]
fn validate_rejects_airgapped_with_bridge_network() {
    let tier = AgentTier::Standard;
    let p = ContainerPolicy {
        iso_mode: IsoMode::Airgapped,
        network: NetworkPolicy::Bridge,
        resources: tier.default_resources(),
        tier,
        ..ContainerPolicy::default()
    };
    assert!(matches!(p.validate(), Err(SpawnError::PolicyConflict(_))));
}

#[test]
fn validate_accepts_airgapped_with_none_network() {
    let tier = AgentTier::Standard;
    let p = ContainerPolicy {
        iso_mode: IsoMode::Airgapped,
        network: NetworkPolicy::None,
        resources: tier.default_resources(),
        tier,
        ..ContainerPolicy::default()
    };
    #[allow(clippy::unwrap_used)]
    p.validate().unwrap();
}

#[test]
fn validate_rejects_proxy_credentials() {
    let tier = AgentTier::Standard;
    let p = ContainerPolicy {
        credentials: CredentialStrategy::Proxy,
        resources: tier.default_resources(),
        tier,
        ..ContainerPolicy::default()
    };
    assert!(matches!(
        p.validate(),
        Err(SpawnError::NotYetImplemented(_))
    ));
}

#[test]
fn validate_rejects_memory_below_min() {
    let tier = AgentTier::Custom;
    let p = ContainerPolicy {
        resources: ContainerResources {
            memory_mb: MIN_MEMORY_MB - 1,
            cpus: MIN_CPUS,
            pids_limit: MIN_PIDS,
            max_concurrent: MIN_CONCURRENT,
        },
        tier,
        ..ContainerPolicy::default()
    };
    assert!(matches!(
        p.validate(),
        Err(SpawnError::ResourceOutOfBounds(_))
    ));
}

#[test]
fn policy_store_update_replaces_system_policy() {
    #[allow(clippy::unwrap_used)]
    let store = PolicyStore::new(ContainerPolicy::default()).unwrap();
    let tier = AgentTier::Large;
    let new_policy = ContainerPolicy {
        resources: tier.default_resources(),
        tier,
        ..ContainerPolicy::default()
    };
    #[allow(clippy::unwrap_used)]
    store.update_system_policy(new_policy).unwrap();

    let effective = store.effective_policy();
    assert_eq!(effective.tier, AgentTier::Large);
    assert_eq!(effective.resources.memory_mb, 8_192);
}

#[test]
fn tighten_for_build_accepts_lower_memory() {
    #[allow(clippy::unwrap_used)]
    let store = PolicyStore::new(ContainerPolicy::default()).unwrap();
    let tier = AgentTier::Micro;
    let tighter = ContainerPolicy {
        resources: tier.default_resources(),
        tier,
        ..ContainerPolicy::default()
    };
    #[allow(clippy::unwrap_used)]
    let derived = store.tighten_for_build(&tighter).unwrap();
    assert_eq!(derived.resources.memory_mb, 512);
}

#[test]
fn tighten_for_build_rejects_higher_memory() {
    let tier = AgentTier::Micro;
    let initial = ContainerPolicy {
        resources: tier.default_resources(),
        tier,
        ..ContainerPolicy::default()
    };
    #[allow(clippy::unwrap_used)]
    let store = PolicyStore::new(initial).unwrap();

    let larger_tier = AgentTier::Large;
    let looser = ContainerPolicy {
        resources: larger_tier.default_resources(),
        tier: larger_tier,
        ..ContainerPolicy::default()
    };
    assert!(matches!(
        store.tighten_for_build(&looser),
        Err(SpawnError::PolicyTighteningViolation(_))
    ));
}

#[test]
fn tighten_for_build_rejects_lower_iso_mode() {
    let tier = AgentTier::Standard;
    let initial = ContainerPolicy {
        iso_mode: IsoMode::Hardened,
        network: NetworkPolicy::Bridge,
        resources: tier.default_resources(),
        tier,
        ..ContainerPolicy::default()
    };
    #[allow(clippy::unwrap_used)]
    let store = PolicyStore::new(initial).unwrap();

    let weaker = ContainerPolicy {
        iso_mode: IsoMode::Standard,
        resources: tier.default_resources(),
        tier,
        ..ContainerPolicy::default()
    };
    assert!(matches!(
        store.tighten_for_build(&weaker),
        Err(SpawnError::PolicyTighteningViolation(_))
    ));
}
