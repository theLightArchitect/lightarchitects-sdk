//! Property-based tests for the monotonic-tightening invariant.

use proptest::prelude::*;

use crate::container_spawn::policy::{
    AgentTier, ContainerPolicy, ContainerResources, IsoMode, MAX_CONCURRENT, MAX_CPUS,
    MAX_MEMORY_MB_ABSOLUTE, MAX_PIDS, MIN_CONCURRENT, MIN_CPUS, MIN_MEMORY_MB, MIN_PIDS,
    NetworkPolicy, PolicyStore, SpawnError, SpawnPolicy,
};

fn arb_resources() -> impl Strategy<Value = ContainerResources> {
    (
        MIN_MEMORY_MB..=MAX_MEMORY_MB_ABSOLUTE,
        MIN_CPUS..=MAX_CPUS,
        MIN_PIDS..=MAX_PIDS,
        MIN_CONCURRENT..=MAX_CONCURRENT,
    )
        .prop_map(
            |(memory_mb, cpus, pids_limit, max_concurrent)| ContainerResources {
                memory_mb,
                cpus,
                pids_limit,
                max_concurrent,
            },
        )
}

fn arb_iso_mode() -> impl Strategy<Value = IsoMode> {
    prop_oneof![
        Just(IsoMode::Standard),
        Just(IsoMode::Hardened),
        Just(IsoMode::Airgapped),
    ]
}

proptest! {
    #[test]
    fn tighten_accepts_iff_all_dimensions_tighter(
        base_mem in MIN_MEMORY_MB..=MAX_MEMORY_MB_ABSOLUTE,
        override_mem in MIN_MEMORY_MB..=MAX_MEMORY_MB_ABSOLUTE,
        base_cpus in MIN_CPUS..=MAX_CPUS,
        override_cpus in MIN_CPUS..=MAX_CPUS,
        base_pids in MIN_PIDS..=MAX_PIDS,
        override_pids in MIN_PIDS..=MAX_PIDS,
        base_con in MIN_CONCURRENT..=MAX_CONCURRENT,
        override_con in MIN_CONCURRENT..=MAX_CONCURRENT,
    ) {
        let tier = AgentTier::Custom;
        let base_policy = ContainerPolicy {
            resources: ContainerResources {
                memory_mb: base_mem,
                cpus: base_cpus,
                pids_limit: base_pids,
                max_concurrent: base_con,
            },
            tier,
            ..ContainerPolicy::default()
        };
        let store = PolicyStore::new(base_policy)?;

        let override_policy = ContainerPolicy {
            resources: ContainerResources {
                memory_mb: override_mem,
                cpus: override_cpus,
                pids_limit: override_pids,
                max_concurrent: override_con,
            },
            tier,
            ..ContainerPolicy::default()
        };

        let result = store.tighten_for_build(&override_policy);
        let all_tighter = override_mem <= base_mem
            && override_cpus <= base_cpus
            && override_pids <= base_pids
            && override_con <= base_con;

        match result {
            Ok(_) => prop_assert!(all_tighter, "accepted but not all dimensions tighter"),
            Err(SpawnError::PolicyTighteningViolation(_)) => {
                prop_assert!(!all_tighter, "rejected but all dimensions were tighter");
            }
            Err(e) => return Err(TestCaseError::fail(format!("unexpected error: {e}"))),
        }
    }

    #[test]
    fn iso_rank_is_monotone(
        base_iso in arb_iso_mode(),
        override_iso in arb_iso_mode(),
        resources in arb_resources(),
    ) {
        let tier = AgentTier::Custom;
        let base_network = match base_iso {
            IsoMode::Airgapped => NetworkPolicy::None,
            _ => NetworkPolicy::Bridge,
        };
        let override_network = match override_iso {
            IsoMode::Airgapped => NetworkPolicy::None,
            _ => NetworkPolicy::Bridge,
        };

        let base_policy = ContainerPolicy {
            iso_mode: base_iso,
            network: base_network,
            resources: resources.clone(),
            tier,
            ..ContainerPolicy::default()
        };
        let store = PolicyStore::new(base_policy)?;

        let override_policy = ContainerPolicy {
            iso_mode: override_iso,
            network: override_network,
            resources,
            tier,
            ..ContainerPolicy::default()
        };

        let base_rank = iso_rank(base_iso);
        let override_rank = iso_rank(override_iso);

        let result = store.tighten_for_build(&override_policy);
        match result {
            Ok(_) => prop_assert!(override_rank >= base_rank),
            Err(SpawnError::PolicyTighteningViolation(_)) => {
                prop_assert!(override_rank < base_rank);
            }
            Err(_) => {} // other errors (e.g. resource bound) are acceptable
        }
    }
}

fn iso_rank(mode: IsoMode) -> u8 {
    match mode {
        IsoMode::Standard => 0,
        IsoMode::Hardened => 1,
        IsoMode::Airgapped => 2,
    }
}
