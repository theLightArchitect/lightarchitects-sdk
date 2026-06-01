//! Docker capability types for transparent containerization.

use std::time::Instant;

/// Result of probing the Docker daemon at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockerCapability {
    /// Docker socket reachable, CLI responsive, permission check passed.
    Ready,
    /// Docker socket exists but we cannot run containers (permissions).
    NoPermission,
    /// Docker socket absent or daemon unreachable.
    Unavailable,
}

/// User override for container mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerMode {
    /// Probe Docker and decide automatically.
    Auto,
    /// Force container path; error if Docker is unavailable.
    ForceEnable,
    /// Force native PTY path.
    ForceDisable,
}

impl ContainerMode {
    /// Resolve from `LA_CONTAINER_MODE` env var.
    ///
    /// - `0` → [`ForceDisable`]
    /// - `1` → [`ForceEnable`]
    /// - unset / any other value → [`Auto`]
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var("LA_CONTAINER_MODE").ok().as_deref() {
            Some("0") => Self::ForceDisable,
            Some("1") => Self::ForceEnable,
            _ => Self::Auto,
        }
    }
}

/// Discriminates active container entries by how they were spawned.
///
/// Used by the reaper to apply kind-appropriate grace periods and by
/// the WebSocket relay to reject connections to non-PTY containers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerKind {
    /// Interactive PTY session spawned by a browser WebSocket connect.
    Pty,
    /// Autonomous wave-task container spawned by `wave_dispatcher`.
    WorkerTask {
        /// `IronClaw` task identifier.
        task_id: String,
        /// Wave index within the build (zero-based).
        wave_index: usize,
    },
}

/// An entry in the `active_containers` registry.
///
/// Replaces the previous bare `Instant` value so the reaper and relay
/// can make kind-aware decisions.
#[derive(Debug, Clone)]
pub struct ActiveContainerEntry {
    /// Whether this is a PTY session or an autonomous worker task.
    pub kind: ContainerKind,
    /// Wall-clock time at which `docker run` succeeded.
    pub started_at: Instant,
    /// `IsoMode` snapshot at spawn time — used for audit logging.
    pub policy_snapshot_iso_mode: lightarchitects::container_spawn::IsoMode,
}

/// Result of successfully spawning a container session.
///
/// Returned by `spawn_session` so the caller can route the WebSocket
/// relay without knowing the internal container naming scheme.
#[derive(Debug, Clone)]
pub struct ContainerHandle {
    /// Docker container ID returned by `docker run -d` (e.g., `la-<build_id>`).
    pub container_id: String,
    /// Absolute WebSocket path for the relay endpoint served by the webshell.
    pub relay_url: String,
}

/// Errors specific to container operations.
#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    /// Docker daemon is not reachable or responsive.
    #[error("docker unavailable")]
    DockerUnavailable,
    /// Docker is present but we lack permissions to run containers.
    #[error("docker permission denied")]
    DockerNoPermission,
    /// Image build from embedded Dockerfile failed.
    #[error("image build failed: {0}")]
    ImageBuildFailed(String),
    /// Image pull from registry failed.
    #[error("image pull failed: {0}")]
    ImagePullFailed(String),
    /// I/O error during temp file or binary copy.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// The container WebSocket relay is not yet implemented.
    #[error("container WebSocket relay not implemented")]
    RelayNotImplemented,
    /// Active container count has reached the configured concurrent cap.
    ///
    /// The operator must wait for an existing session to end, or increase
    /// [`ContainerPolicy::resources.max_concurrent`] via the policy API.
    #[error("concurrent container cap reached — no semaphore permits available")]
    ConcurrencyCapExceeded,
    /// Policy produced an invalid or unsupported docker-args configuration.
    #[error("policy error: {0}")]
    PolicyError(String),
}
