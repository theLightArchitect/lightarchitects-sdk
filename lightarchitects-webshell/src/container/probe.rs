//! Docker capability probe — three-check cascade.

use std::path::Path;

use crate::container::{docker_cmd, types::DockerCapability};

/// Probe Docker availability with a three-check cascade.
///
/// 1. Check `/var/run/docker.sock` exists and is writable.
/// 2. Run `docker version` to confirm CLI → daemon communication.
/// 3. Run `docker run --rm hello-world` to verify spawn permission.
///
/// The full cascade takes ~1–3 s on a warm daemon. Called once at startup and
/// cached in [`AppState`](crate::server::AppState).
pub async fn probe_docker() -> DockerCapability {
    // Check 1: socket exists and writable
    let socket = Path::new("/var/run/docker.sock");
    let socket_ok = socket.exists()
        && std::fs::metadata(socket).is_ok_and(|m| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let mode = m.mode() & 0o777;
                mode & 0o200 != 0
            }
            #[cfg(not(unix))]
            {
                true
            }
        });

    if !socket_ok {
        tracing::debug!(target: "container", "docker socket absent or not writable");
        return DockerCapability::Unavailable;
    }

    // Check 2: docker CLI responds
    if let Some(ver) = docker_cmd::version().await {
        tracing::debug!(target: "container", version = %ver, "docker version OK");
    } else {
        tracing::debug!(target: "container", "docker version failed or timed out");
        return DockerCapability::Unavailable;
    }

    // Check 3: permission check — can we run a trivial container?
    if docker_cmd::check_run_permission().await {
        tracing::info!(target: "container", "docker_capable=Ready");
        DockerCapability::Ready
    } else {
        tracing::warn!(target: "container", "docker_capable=NoPermission");
        DockerCapability::NoPermission
    }
}
