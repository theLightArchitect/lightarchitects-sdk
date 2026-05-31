//! Docker bridge-network CIDR guard.
//!
//! Containers must not be able to call `PATCH /api/container/policy` to
//! manipulate their own spawn constraints.  This guard rejects requests whose
//! source IP falls within any Docker bridge CIDR (G4 + M1 fix per SERAPH
//! Round 2 — custom user-created networks must not bypass).
//!
//! # Startup flow
//!
//! 1. `BridgeCidrGuard::from_docker()` enumerates all bridge networks via
//!    `docker network ls` + `docker network inspect`.
//! 2. The guard is stored in `AppState`; handlers call `is_blocked(addr)`.
//!
//! Loopback (`127.0.0.1`, `::1`) is always permitted regardless of bridge CIDRs.

use std::net::{IpAddr, Ipv4Addr};

/// Guard that blocks requests whose source IP lies in a Docker bridge CIDR.
#[derive(Debug, Clone, Default)]
pub struct BridgeCidrGuard {
    /// Bridge network CIDRs in `"A.B.C.D/prefix"` notation.
    cidrs: Vec<(Ipv4Addr, u8)>,
}

impl BridgeCidrGuard {
    /// Probes Docker for all bridge network CIDRs at startup.
    ///
    /// Returns an empty guard (no blocks) when Docker is unavailable or the
    /// probe fails — fail-open for the CIDR check, not fail-closed, because
    /// the primary defense is the `127.0.0.1` server bind.
    #[must_use]
    pub fn from_docker() -> Self {
        let names = match list_bridge_networks() {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(error = %e, "CIDR guard: docker network ls failed — bridge filter inactive");
                return Self::default();
            }
        };

        let mut cidrs = Vec::new();
        for name in &names {
            match inspect_network_cidrs(name) {
                Ok(mut cs) => cidrs.append(&mut cs),
                Err(e) => {
                    tracing::warn!(network = %name, error = %e, "CIDR guard: inspect failed — skipping network");
                }
            }
        }

        tracing::info!(
            count = cidrs.len(),
            "CIDR guard: {} bridge CIDR(s) registered",
            cidrs.len()
        );
        Self { cidrs }
    }

    /// Returns `true` if `addr` falls within any registered bridge CIDR.
    ///
    /// Loopback addresses are always permitted (returns `false`).
    #[must_use]
    pub fn is_blocked(&self, addr: IpAddr) -> bool {
        if addr.is_loopback() {
            return false;
        }
        let IpAddr::V4(v4) = addr else {
            return false;
        };
        self.cidrs
            .iter()
            .any(|(net, prefix)| in_cidr_v4(v4, *net, *prefix))
    }
}

/// Returns `true` if `ip` is within the CIDR defined by `network/prefix`.
fn in_cidr_v4(ip: Ipv4Addr, network: Ipv4Addr, prefix: u8) -> bool {
    if prefix == 0 {
        return true;
    }
    let mask: u32 = if prefix >= 32 {
        u32::MAX
    } else {
        !0u32 << (32 - prefix)
    };
    let ip_u32 = u32::from(ip);
    let net_u32 = u32::from(network);
    (ip_u32 & mask) == (net_u32 & mask)
}

/// Lists all Docker bridge network names via `docker network ls`.
fn list_bridge_networks() -> std::io::Result<Vec<String>> {
    let out = std::process::Command::new("docker")
        .args([
            "network",
            "ls",
            "--filter",
            "driver=bridge",
            "--format",
            "{{.Name}}",
        ])
        .output()?;
    if !out.status.success() {
        return Err(std::io::Error::other("docker network ls non-zero exit"));
    }
    let names = std::str::from_utf8(&out.stdout)
        .map_err(std::io::Error::other)?
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(names)
}

/// Inspects a network and returns its bridge CIDRs.
fn inspect_network_cidrs(network: &str) -> std::io::Result<Vec<(Ipv4Addr, u8)>> {
    let out = std::process::Command::new("docker")
        .args([
            "network",
            "inspect",
            network,
            "--format",
            "{{range .IPAM.Config}}{{.Subnet}}{{\"\\n\"}}{{end}}",
        ])
        .output()?;
    if !out.status.success() {
        return Err(std::io::Error::other(format!(
            "docker network inspect {network} non-zero exit"
        )));
    }
    let mut cidrs = Vec::new();
    for line in std::str::from_utf8(&out.stdout)
        .map_err(std::io::Error::other)?
        .lines()
    {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((net, prefix)) = parse_cidr_v4(line) {
            cidrs.push((net, prefix));
        }
    }
    Ok(cidrs)
}

/// Parses `"A.B.C.D/prefix"` → `(Ipv4Addr, u8)`.
fn parse_cidr_v4(cidr: &str) -> Option<(Ipv4Addr, u8)> {
    let (addr_str, prefix_str) = cidr.split_once('/')?;
    let addr: Ipv4Addr = addr_str.trim().parse().ok()?;
    let prefix: u8 = prefix_str.trim().parse().ok()?;
    if prefix > 32 {
        return None;
    }
    Some((addr, prefix))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn loopback_is_always_permitted() {
        let guard = BridgeCidrGuard {
            cidrs: vec![("172.17.0.0".parse().unwrap(), 16)],
        };
        assert!(!guard.is_blocked(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(!guard.is_blocked(IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn bridge_ip_is_blocked() {
        let guard = BridgeCidrGuard {
            cidrs: vec![("172.17.0.0".parse().unwrap(), 16)],
        };
        // 172.17.0.2 is inside 172.17.0.0/16
        let ip: IpAddr = "172.17.0.2".parse().unwrap();
        assert!(guard.is_blocked(ip));
    }

    #[test]
    fn non_bridge_ip_is_permitted() {
        let guard = BridgeCidrGuard {
            cidrs: vec![("172.17.0.0".parse().unwrap(), 16)],
        };
        // 10.0.0.1 is outside 172.17.0.0/16
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        assert!(!guard.is_blocked(ip));
    }

    #[test]
    fn parse_cidr_v4_valid() {
        let (net, prefix) = parse_cidr_v4("172.17.0.0/16").unwrap();
        assert_eq!(net, "172.17.0.0".parse::<Ipv4Addr>().unwrap());
        assert_eq!(prefix, 16);
    }

    #[test]
    fn in_cidr_boundary() {
        let net: Ipv4Addr = "172.17.0.0".parse().unwrap();
        // First and last address in /16
        assert!(in_cidr_v4("172.17.0.0".parse().unwrap(), net, 16));
        assert!(in_cidr_v4("172.17.255.255".parse().unwrap(), net, 16));
        // Just outside the /16
        assert!(!in_cidr_v4("172.18.0.0".parse().unwrap(), net, 16));
    }

    #[test]
    fn empty_guard_blocks_nothing() {
        let guard = BridgeCidrGuard::default();
        let ip: IpAddr = "172.17.0.2".parse().unwrap();
        assert!(!guard.is_blocked(ip));
    }
}
