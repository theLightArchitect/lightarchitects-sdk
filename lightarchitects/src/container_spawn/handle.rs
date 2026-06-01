//! Container handle and hardening state types.

/// Result of a successfully spawned agent container.
///
/// Returned by callers of the container spawn machinery so they can route
/// the WebSocket relay and attach monitoring without knowing internal naming
/// conventions.
#[derive(Debug, Clone)]
pub struct ContainerHandle {
    /// Docker container ID (e.g. `la-<build_id>-<wave>-<task>`).
    pub container_id: String,
    /// Absolute WebSocket path for the relay endpoint served by the webshell.
    pub relay_url: String,
}

/// Linux user-namespace remapping state probed at spawn time.
///
/// Remapping maps root inside the container to an unprivileged UID on the
/// host, reducing the blast radius of a container-escape to a non-root host
/// process.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsernsState {
    /// `/proc/sys/kernel/unprivileged_userns_clone` is 1 and the Docker
    /// daemon is configured with `userns-remap`.  Root inside = UID ≥65536
    /// on the host.
    Remapped,
    /// Daemon is reachable but `userns-remap` is not configured.  Root
    /// inside = root on the host — avoid this in Hardened/Airgapped mode.
    Host,
    /// Could not determine state (non-Linux host or probe failed).
    Unsupported,
}

/// Hardening options applied to a spawned container.
///
/// Populated by the spawn machinery and attached to [`ContainerHandle`] so
/// callers can log or audit the actual isolation achieved.
#[derive(Debug, Clone)]
pub struct HardeningLevel {
    /// `--security-opt seccomp=<profile>` was passed (namespace-blocking
    /// profile embedded in [`crate::container_spawn::seccomp::SECCOMP_PROFILE_JSON`]).
    pub seccomp: bool,
    /// `--cap-drop ALL` was passed (`--cap-add NET_BIND_SERVICE` re-added).
    pub cap_drop: bool,
    /// User-namespace remapping state at spawn time.
    pub userns: UsernsState,
}

impl HardeningLevel {
    /// Returns `true` if all available hardening flags were applied.
    ///
    /// "Full hardening" requires `seccomp + cap_drop + Remapped` userns.
    /// Callers in Hardened/Airgapped mode should warn when this returns
    /// `false`.
    #[must_use]
    pub fn is_fully_hardened(&self) -> bool {
        self.seccomp && self.cap_drop && self.userns == UsernsState::Remapped
    }
}
