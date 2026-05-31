//! Docker capability types for transparent containerization.

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

/// Result of successfully spawning a container session.
///
/// Returned by `container_spawn` so the caller can route the WebSocket
/// relay without knowing the internal container naming scheme.
#[derive(Debug, Clone)]
pub struct ContainerHandle {
    /// Docker container ID returned by `docker run -d` (e.g., `la-<build_id>`).
    pub container_id: String,
    /// Absolute WebSocket path for the relay endpoint served by the webshell.
    pub relay_url: String,
}

/// Isolation level applied to spawned agent containers.
///
/// Controlled by the `LA_ISO_MODE` environment variable. Graduated levels
/// layer additional Docker security flags on top of the standard resource
/// limits already applied in [`Standard`] mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IsoMode {
    /// Standard resource limits: memory, CPU, pids, no-new-privileges.
    #[default]
    Standard,
    /// Hardened: standard + read-only root fs + `/tmp` tmpfs (256 MiB).
    ///
    /// Prevents agents from writing to the container filesystem outside of
    /// explicitly mounted tmpfs paths. Workspace writes go to `/tmp`.
    Hardened,
    /// Airgapped: hardened + `--network none`.
    ///
    /// Agents have no outbound network access. Use when the agent task is
    /// purely local (code analysis, refactoring) and must not exfiltrate data.
    Airgapped,
}

impl IsoMode {
    /// Resolve from `LA_ISO_MODE` env var.
    ///
    /// - `"hardened"` → [`Hardened`](Self::Hardened)
    /// - `"airgapped"` → [`Airgapped`](Self::Airgapped)
    /// - unset / any other value → [`Standard`](Self::Standard)
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var("LA_ISO_MODE").ok().as_deref() {
            Some("hardened") => Self::Hardened,
            Some("airgapped") => Self::Airgapped,
            _ => Self::Standard,
        }
    }

    /// Extra `docker run` args for this isolation level.
    ///
    /// These are appended after the fixed resource-limit args in
    /// [`container_spawn`](crate::container::spawner::container_spawn).
    #[must_use]
    pub fn docker_args(self) -> &'static [&'static str] {
        match self {
            Self::Standard => &[],
            Self::Hardened => &["--read-only", "--tmpfs", "/tmp:rw,noexec,nosuid,size=256m"],
            Self::Airgapped => &[
                "--read-only",
                "--tmpfs",
                "/tmp:rw,noexec,nosuid,size=256m",
                "--network",
                "none",
            ],
        }
    }
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
}
