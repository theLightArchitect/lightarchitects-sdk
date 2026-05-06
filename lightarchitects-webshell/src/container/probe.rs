//! Docker capability probe — three-check cascade.

use std::path::Path;

use crate::container::types::DockerCapability;

/// Probe Docker availability with a three-check cascade.
///
/// 1. Check `/var/run/docker.sock` exists and is writable.
/// 2. Run `docker version --format {{.Server.Version}}` to confirm CLI → daemon
///    communication.
/// 3. Run `docker run --rm hello-world` to verify we can actually spawn containers.
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

    // Check 2: docker CLI responds
    let version = tokio::process::Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .await;

    match version {
        Ok(out) if out.status.success() => {
            let ver = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            tracing::debug!(target: "container", version = %ver, "docker version OK");
        }
        _ => {
            tracing::debug!(target: "container", "docker version failed");
            return DockerCapability::Unavailable;
        }
    }

    // Check 3: permission check — can we run a trivial container?
    let test = tokio::process::Command::new("docker")
        .args(["run", "--rm", "hello-world"])
        .status()
        .await;

    match test {
        Ok(st) if st.success() => {
            tracing::info!(target: "container", "docker_capable=Ready");
            DockerCapability::Ready
        }
        Ok(_) => {
            tracing::warn!(target: "container", "docker_capable=NoPermission");
            DockerCapability::NoPermission
        }
        Err(e) => {
            tracing::warn!(target: "container", error = %e, "docker hello-world failed");
            DockerCapability::Unavailable
        }
    }
}
