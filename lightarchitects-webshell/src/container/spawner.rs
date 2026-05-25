//! Session spawner — transparent router between container and native PTY paths.

use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

use crate::container::types::{ContainerError, ContainerHandle, ContainerMode, DockerCapability};
use crate::server::AppState;

/// Default image name — mirrors `ImageManager::DEFAULT_IMAGE_NAME`.
const DEFAULT_IMAGE_NAME: &str = "lightarchitects/agent:latest";

/// Hardcoded container image allowlist.
///
/// Only images on this list may be spawned via `container_spawn`. Any other
/// image name is rejected with an I/O error at the entry guard. Override the
/// active image via `LA_AGENT_IMAGE`; the override must still appear here.
const ALLOWED_IMAGES: &[&str] = &["lightarchitects/agent:latest", "la-sandbox:latest"];

/// Spawn a session, routing to either the container path or native PTY path.
///
/// When the container path succeeds, returns a [`ContainerHandle`] the caller
/// can use to connect the browser WebSocket to the relay endpoint.
/// When the native PTY path is selected, returns `Ok(None)`.
///
/// # Errors
///
/// Returns an error if the container path is selected and `docker run` fails or
/// the image is not in the allowlist.
pub async fn spawn_session(
    build_id: &str,
    agent_cli: &str,
    cwd: &Path,
    state: &AppState,
) -> Result<Option<ContainerHandle>, ContainerError> {
    let should_containerize = state.docker_capable == DockerCapability::Ready
        && state.config.container_mode != ContainerMode::ForceDisable;

    if should_containerize {
        state.image_manager.ensure_image().await?;
        let handle = container_spawn(build_id, agent_cli, cwd, state).await?;
        Ok(Some(handle))
    } else {
        Ok(None)
    }
}

/// Container-specific spawn — runs `docker run -d` with resource limits.
///
/// # Security
///
/// Image name is validated against [`ALLOWED_IMAGES`] before any Docker call.
/// Resource limits: 512 MiB memory, 1.0 CPU, 256 pids, no new privileges.
///
/// # Errors
///
/// Returns [`ContainerError::Io`] if the image is not in the allowlist or
/// if `docker run` fails.
async fn container_spawn(
    build_id: &str,
    _agent_cli: &str,
    _cwd: &Path,
    _state: &AppState,
) -> Result<ContainerHandle, ContainerError> {
    let image = std::env::var("LA_AGENT_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE_NAME.to_owned());

    // Image allowlist guard — reject unknown images before any subprocess call.
    if !ALLOWED_IMAGES.contains(&image.as_str()) {
        return Err(ContainerError::Io(std::io::Error::other(format!(
            "image '{image}' is not in the container allowlist"
        ))));
    }

    let container_name = format!("la-{}", sanitize_build_id(build_id));

    let output = Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            &container_name,
            "--memory",
            "512m",
            "--cpus",
            "1.0",
            "--pids-limit",
            "256",
            "--security-opt",
            "no-new-privileges",
            "--label",
            "managed-by=la-hitl",
            "--restart",
            "no",
            &image,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(ContainerError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ContainerError::Io(std::io::Error::other(format!(
            "docker run failed: {stderr}"
        ))));
    }

    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let relay_url = format!("/api/terminal/container/{container_id}");

    tracing::info!(
        target: "container",
        container_id = %container_id,
        container_name = %container_name,
        "container spawned"
    );

    Ok(ContainerHandle {
        container_id,
        relay_url,
    })
}

/// Sanitizes a build ID so it is safe to use as part of a Docker container name.
///
/// Docker container names allow `[a-zA-Z0-9_.-]`. We strip everything else.
fn sanitize_build_id(build_id: &str) -> String {
    build_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        .take(48)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::sanitize_build_id;

    #[test]
    fn sanitize_strips_dangerous_chars() {
        assert_eq!(sanitize_build_id("abc/def;$(rm -rf)"), "abcdefrm-rf");
    }

    #[test]
    fn sanitize_allows_valid_chars() {
        assert_eq!(sanitize_build_id("my-build_v1.0"), "my-build_v1.0");
    }

    #[test]
    fn sanitize_truncates_at_48() {
        let long = "a".repeat(60);
        assert_eq!(sanitize_build_id(&long).len(), 48);
    }
}
