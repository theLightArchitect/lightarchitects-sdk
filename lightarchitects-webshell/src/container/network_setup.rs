//! Worker-bridge Docker network provisioner (SERAPH C2 — T8 mitigation).
//!
//! Provisions `la-worker-bridge`, a dedicated Docker bridge network for
//! [`ContainerKind::WorkerTask`] containers. By running worker containers on a
//! separate network the platform provides:
//!
//! - Traffic namespace isolation from PTY session containers.
//! - A named network for [`BridgeCidrGuard`] to enumerate when blocking
//!   container-originated policy-manipulation requests.
//!
//! # iptables egress restriction (Linux production only)
//!
//! The full T8 mitigation requires an iptables FORWARD rule allowing only
//! egress to the `LiteLLM` proxy port (default 4000) and dropping all other
//! outbound traffic from `la-worker-bridge`. This rule is **not applied
//! automatically** because:
//!
//! 1. macOS uses `pf`, not `iptables` — rule injection is impossible.
//! 2. Injection requires `CAP_NET_ADMIN` / root privileges.
//! 3. The bridge interface name (`br-<id>`) is assigned dynamically by Docker.
//!
//! For Linux production deployments, apply the rule manually after first
//! startup (substitute the actual interface name from `docker network inspect
//! la-worker-bridge`):
//!
//! ```text
//! # Allow LiteLLM proxy only
//! iptables -I DOCKER-USER -i br-<id> -p tcp --dport 4000 -j ACCEPT
//! # Drop all other forwarded traffic from this bridge
//! iptables -I DOCKER-USER -i br-<id> -j DROP
//! ```
//!
//! The iptables gap is tracked as HIGH (not CRITICAL) in the SERAPH T8 finding
//! — the dedicated bridge still provides valuable isolation from the default
//! Docker network and is a prerequisite for the full iptables configuration.
//! Full enforcement is scheduled for the `litellm-virtual-keys-per-agent` build.
//!
//! [`BridgeCidrGuard`]: crate::container::cidr_guard::BridgeCidrGuard
//! [`ContainerKind::WorkerTask`]: crate::container::types::ContainerKind::WorkerTask

use std::process::Stdio;

/// Docker network name for `WorkerTask` container isolation.
pub const WORKER_BRIDGE_NETWORK: &str = "la-worker-bridge";

/// Error from bridge network provisioning.
#[derive(Debug)]
pub enum BridgeSetupError {
    /// `docker network create` returned a non-zero exit code.
    CreateFailed(String),
}

impl std::fmt::Display for BridgeSetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateFailed(msg) => write!(f, "la-worker-bridge creation failed: {msg}"),
        }
    }
}

/// Provision the `la-worker-bridge` Docker network.
///
/// Idempotent: returns immediately without error when the network already
/// exists. After ensuring the network exists, sweeps any running containers
/// from a previous gateway session that remain attached to it.
///
/// # Errors
///
/// Returns [`BridgeSetupError::CreateFailed`] when `docker network create`
/// returns a non-zero exit code and the error is not "already exists".
pub fn ensure_worker_bridge() -> Result<(), BridgeSetupError> {
    if worker_bridge_exists() {
        tracing::debug!(target: "container", "la-worker-bridge already exists — skipping create");
    } else {
        create_worker_bridge()?;
        tracing::info!(target: "container", "la-worker-bridge created");
    }
    sweep_orphan_containers();
    Ok(())
}

/// Returns `true` when `la-worker-bridge` is listed by `docker network ls`.
fn worker_bridge_exists() -> bool {
    std::process::Command::new(crate::container::docker_cmd::docker_bin())
        .args([
            "network",
            "ls",
            "--filter",
            &format!("name={WORKER_BRIDGE_NETWORK}"),
            "--format",
            "{{.Name}}",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|out| {
            out.status.success()
                && String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .any(|l| l.trim() == WORKER_BRIDGE_NETWORK)
        })
}

/// Runs `docker network create --driver bridge la-worker-bridge`.
fn create_worker_bridge() -> Result<(), BridgeSetupError> {
    let out = std::process::Command::new(crate::container::docker_cmd::docker_bin())
        .args([
            "network",
            "create",
            "--driver",
            "bridge",
            WORKER_BRIDGE_NETWORK,
        ])
        .output()
        .map_err(|e| BridgeSetupError::CreateFailed(e.to_string()))?;

    if out.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&out.stderr);
    // Docker exits non-zero AND prints "already exists" on a create race —
    // treat this as idempotent success.
    if stderr.contains("already exists") {
        return Ok(());
    }
    Err(BridgeSetupError::CreateFailed(stderr.into_owned()))
}

/// Removes any containers still attached to `la-worker-bridge` from a previous
/// gateway session. Fire-and-forget: failures are logged but never block startup.
fn sweep_orphan_containers() {
    let Ok(out) = std::process::Command::new(crate::container::docker_cmd::docker_bin())
        .args([
            "ps",
            "-q",
            "--filter",
            &format!("network={WORKER_BRIDGE_NETWORK}"),
        ])
        .output()
    else {
        return;
    };

    if !out.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let ids: Vec<&str> = stdout.split_whitespace().collect();
    if ids.is_empty() {
        return;
    }

    tracing::warn!(
        target: "container",
        count = ids.len(),
        "sweeping orphan worker containers from previous gateway session on la-worker-bridge"
    );

    let mut cmd = std::process::Command::new(crate::container::docker_cmd::docker_bin());
    cmd.arg("rm").arg("-f");
    for id in &ids {
        cmd.arg(id);
    }
    let _ = cmd.output();
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_bridge_network_name_is_correct() {
        assert_eq!(WORKER_BRIDGE_NETWORK, "la-worker-bridge");
    }

    #[test]
    fn bridge_setup_error_display() {
        let e = BridgeSetupError::CreateFailed("network already exists".to_owned());
        assert!(e.to_string().contains("la-worker-bridge"));
        assert!(e.to_string().contains("network already exists"));
    }

    /// Full integration test — only runs when Docker is explicitly available.
    ///
    /// `WORKER_BRIDGE_E2E=1 cargo test -p lightarchitects-webshell worker_bridge_e2e`
    #[allow(clippy::expect_used)]
    #[test]
    fn worker_bridge_e2e() {
        if std::env::var("WORKER_BRIDGE_E2E").is_err() {
            return;
        }
        ensure_worker_bridge().expect("bridge provisioning should succeed with live Docker");
        ensure_worker_bridge().expect("second call should be idempotent");
    }
}
