//! Docker capability probe — three-check cascade.

use std::path::Path;
use std::time::Duration;

use crate::container::types::DockerCapability;

/// Per-command timeout for Docker subprocess checks.
const DOCKER_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Probe Docker availability with a three-check cascade.
///
/// 1. Check `/var/run/docker.sock` exists and is writable.
/// 2. Run `docker version --format {{.Server.Version}}` to confirm CLI → daemon
///    communication.
/// 3. Run `docker run --rm hello-world` to verify we can actually spawn containers.
///
/// Each subprocess step is bounded by [`DOCKER_PROBE_TIMEOUT`] (5 s) so a
/// hung or slow daemon never blocks webshell startup. Called once at startup
/// and cached in [`AppState`](crate::server::AppState).
pub async fn probe_docker() -> DockerCapability {
    // Check 1: socket exists and writable
    let socket = Path::new("/var/run/docker.sock");
    let socket_ok = socket.exists()
        && std::fs::metadata(socket).is_ok_and(|m| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let mode = m.mode() & 0o777;
                mode & 0o200 != 0 // owner or group or other writable
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

    // Check 2: docker CLI responds (bounded)
    let version_result = tokio::time::timeout(
        DOCKER_PROBE_TIMEOUT,
        tokio::process::Command::new("docker")
            .args(["version", "--format", "{{.Server.Version}}"])
            .output(),
    )
    .await;

    match version_result {
        Ok(Ok(out)) if out.status.success() => {
            let ver = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            tracing::debug!(target: "container", version = %ver, "docker version OK");
        }
        Ok(_) => {
            tracing::debug!(target: "container", "docker version failed");
            return DockerCapability::Unavailable;
        }
        Err(_) => {
            tracing::warn!(target: "container", "docker version timed out");
            return DockerCapability::Unavailable;
        }
    }

    // Check 3: permission check — can we run a trivial container? (bounded)
    let test_result = tokio::time::timeout(
        DOCKER_PROBE_TIMEOUT,
        tokio::process::Command::new("docker")
            .args(["run", "--rm", "hello-world"])
            .status(),
    )
    .await;

    match test_result {
        Ok(Ok(st)) if st.success() => {
            tracing::info!(target: "container", "docker_capable=Ready");
            DockerCapability::Ready
        }
        Ok(Ok(_)) => {
            tracing::warn!(target: "container", "docker_capable=NoPermission");
            DockerCapability::NoPermission
        }
        Ok(Err(e)) => {
            tracing::warn!(target: "container", error = %e, "docker hello-world failed");
            DockerCapability::Unavailable
        }
        Err(_) => {
            tracing::warn!(target: "container", "docker hello-world timed out");
            DockerCapability::Unavailable
        }
    }
}
