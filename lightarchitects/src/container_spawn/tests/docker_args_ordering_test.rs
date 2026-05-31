//! Verifies the PINNED docker-args order for each [`IsoMode`].
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use crate::container_spawn::policy::{
    AgentTier, ContainerPolicy, IsoMode, NetworkPolicy, SpawnError,
};

fn args_for(iso: IsoMode, network: NetworkPolicy) -> Vec<String> {
    let tier = AgentTier::Standard;
    let policy = ContainerPolicy {
        iso_mode: iso,
        network,
        resources: tier.default_resources(),
        tier,
        seccomp_profile_path: Some(PathBuf::from("/tmp/seccomp.json")),
        ..ContainerPolicy::default()
    };
    policy.build_docker_args().unwrap()
}

fn arg_pos(args: &[String], flag: &str) -> Option<usize> {
    args.iter().position(|a| a == flag)
}

#[test]
fn standard_bridge_pinned_order() {
    let args = args_for(IsoMode::Standard, NetworkPolicy::Bridge);

    let pos_no_new_privs = arg_pos(&args, "--security-opt").unwrap();
    let pos_seccomp = args.iter().position(|a| a.starts_with("seccomp=")).unwrap();
    let pos_memory = arg_pos(&args, "--memory").unwrap();
    let pos_cpus = arg_pos(&args, "--cpus").unwrap();
    let pos_pids = arg_pos(&args, "--pids-limit").unwrap();
    let pos_network = arg_pos(&args, "--network").unwrap();
    let pos_label = arg_pos(&args, "--label").unwrap();
    let pos_restart = arg_pos(&args, "--restart").unwrap();

    assert!(
        pos_no_new_privs < pos_seccomp,
        "no-new-privileges must precede seccomp"
    );
    assert!(pos_seccomp < pos_memory, "seccomp must precede --memory");
    assert!(pos_memory < pos_cpus, "--memory must precede --cpus");
    assert!(pos_cpus < pos_pids, "--cpus must precede --pids-limit");
    assert!(
        pos_pids < pos_network,
        "--pids-limit must precede --network"
    );
    assert!(pos_network < pos_label, "--network must precede --label");
    assert!(pos_label < pos_restart, "--label must precede --restart");
}

#[test]
fn hardened_pinned_order_includes_cap_and_user() {
    let args = args_for(IsoMode::Hardened, NetworkPolicy::Bridge);

    let pos_seccomp = args.iter().position(|a| a.starts_with("seccomp=")).unwrap();
    let pos_cap_drop = arg_pos(&args, "--cap-drop").unwrap();
    let pos_cap_add = arg_pos(&args, "--cap-add").unwrap();
    let pos_user = arg_pos(&args, "--user").unwrap();
    let pos_memory = arg_pos(&args, "--memory").unwrap();
    let pos_read_only = arg_pos(&args, "--read-only").unwrap();
    let pos_tmpfs = arg_pos(&args, "--tmpfs").unwrap();

    assert!(pos_seccomp < pos_cap_drop, "seccomp before cap-drop");
    assert!(pos_cap_drop < pos_cap_add, "cap-drop before cap-add");
    assert!(pos_cap_add < pos_user, "cap-add before user");
    assert!(pos_user < pos_memory, "user before memory");
    assert!(pos_memory < pos_read_only, "memory before read-only");
    assert!(pos_read_only < pos_tmpfs, "read-only before tmpfs");
}

#[test]
fn airgapped_network_is_none() {
    let tier = AgentTier::Standard;
    let policy = ContainerPolicy {
        iso_mode: IsoMode::Airgapped,
        network: NetworkPolicy::None,
        resources: tier.default_resources(),
        tier,
        seccomp_profile_path: None,
        ..ContainerPolicy::default()
    };
    #[allow(clippy::unwrap_used)]
    let args = policy.build_docker_args().unwrap();
    let network_idx = arg_pos(&args, "--network").unwrap();
    assert_eq!(args[network_idx + 1], "none");
}

#[test]
fn balanced_returns_not_yet_implemented() {
    let tier = AgentTier::Standard;
    let policy = ContainerPolicy {
        iso_mode: IsoMode::Standard,
        network: NetworkPolicy::Balanced,
        resources: tier.default_resources(),
        tier,
        seccomp_profile_path: None,
        ..ContainerPolicy::default()
    };
    assert!(matches!(
        policy.build_docker_args(),
        Err(SpawnError::NotYetImplemented(_))
    ));
}

#[test]
fn tmpfs_has_no_noexec_flag() {
    let args = args_for(IsoMode::Hardened, NetworkPolicy::Bridge);
    let tmpfs_idx = arg_pos(&args, "--tmpfs").unwrap();
    let tmpfs_value = &args[tmpfs_idx + 1];
    assert!(
        !tmpfs_value.contains("noexec"),
        "noexec must not appear in tmpfs options (M3 fix)"
    );
}

#[test]
fn standard_has_no_cap_drop_or_read_only() {
    let args = args_for(IsoMode::Standard, NetworkPolicy::Bridge);
    assert!(
        !args.iter().any(|a| a == "--cap-drop"),
        "--cap-drop must not appear in Standard mode"
    );
    assert!(
        !args.iter().any(|a| a == "--read-only"),
        "--read-only must not appear in Standard mode"
    );
}
