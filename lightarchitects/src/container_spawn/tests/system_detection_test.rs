//! Tests for `ContainerResources::from_system()` output validity.

use crate::container_spawn::policy::{
    ContainerResources, MAX_CONCURRENT, MAX_CPUS, MAX_MEMORY_MB_ABSOLUTE, MAX_PIDS, MIN_CONCURRENT,
    MIN_CPUS, MIN_MEMORY_MB, MIN_PIDS,
};

#[test]
fn from_system_returns_valid_memory() {
    let r = ContainerResources::from_system();
    assert!(
        r.memory_mb >= MIN_MEMORY_MB * 4,
        "memory_mb {} is below minimum from_system floor ({})",
        r.memory_mb,
        MIN_MEMORY_MB * 4
    );
    assert!(
        r.memory_mb <= MAX_MEMORY_MB_ABSOLUTE / 2,
        "memory_mb {} exceeds from_system ceiling ({})",
        r.memory_mb,
        MAX_MEMORY_MB_ABSOLUTE / 2
    );
}

#[test]
fn from_system_returns_valid_cpus() {
    let r = ContainerResources::from_system();
    assert!(
        r.cpus >= MIN_CPUS * 2.0,
        "cpus {:.2} is below from_system floor ({:.2})",
        r.cpus,
        MIN_CPUS * 2.0
    );
    assert!(
        r.cpus <= MAX_CPUS / 2.0,
        "cpus {:.2} exceeds from_system ceiling ({:.2})",
        r.cpus,
        MAX_CPUS / 2.0
    );
}

#[test]
fn from_system_pids_in_valid_range() {
    let r = ContainerResources::from_system();
    assert!(
        r.pids_limit >= MIN_PIDS,
        "pids_limit {} is below MIN_PIDS {}",
        r.pids_limit,
        MIN_PIDS
    );
    assert!(
        r.pids_limit <= MAX_PIDS,
        "pids_limit {} exceeds MAX_PIDS {}",
        r.pids_limit,
        MAX_PIDS
    );
}

#[test]
fn from_system_concurrent_in_valid_range() {
    let r = ContainerResources::from_system();
    assert!(
        r.max_concurrent >= MIN_CONCURRENT,
        "max_concurrent {} is below MIN_CONCURRENT {}",
        r.max_concurrent,
        MIN_CONCURRENT
    );
    assert!(
        r.max_concurrent <= MAX_CONCURRENT,
        "max_concurrent {} exceeds MAX_CONCURRENT {}",
        r.max_concurrent,
        MAX_CONCURRENT
    );
}

#[test]
fn from_system_validates_in_container_policy() {
    use crate::container_spawn::policy::{AgentTier, ContainerPolicy};
    let resources = ContainerResources::from_system();
    let policy = ContainerPolicy {
        resources,
        tier: AgentTier::Custom,
        ..ContainerPolicy::default()
    };
    #[allow(clippy::unwrap_used)]
    policy.validate().unwrap();
}
