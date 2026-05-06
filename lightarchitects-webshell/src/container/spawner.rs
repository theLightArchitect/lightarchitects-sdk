//! Session spawner — transparent router between container and native PTY paths.

use std::path::Path;

use crate::container::types::{ContainerError, ContainerMode, DockerCapability};
use crate::server::AppState;

/// Spawn a session, routing to either the container path or native PTY path.
///
/// - If `state.docker_capable` is [`Ready`](DockerCapability::Ready) **and**
///   `state.config.container_mode` is not [`ForceDisable`](ContainerMode::ForceDisable),
///   attempts the container path.
/// - Otherwise, falls back to the native PTY path (exact same code as today).
///
/// # Scope reduction
///
/// The container path currently returns [`ContainerError::RelayNotImplemented`]
/// because the WebSocket relay between browser and container is a separate
/// unsolved design piece. All container infrastructure (probe, image manager,
/// types, spawner skeleton) is built, but the actual container runtime is gated
/// behind this placeholder. Native PTY is the only working spawn path.
///
/// # Errors
///
/// Returns [`ContainerError::DockerUnavailable`] if the image manager cannot
/// ensure the image is present. Returns [`ContainerError::RelayNotImplemented`]
/// when the container path is selected (placeholder).
pub async fn spawn_session(
    build_id: &str,
    agent_cli: &str,
    _cwd: &Path,
    state: &AppState,
) -> Result<(), ContainerError> {
    let should_containerize = state.docker_capable == DockerCapability::Ready
        && state.config.container_mode != ContainerMode::ForceDisable;

    if should_containerize {
        state.image_manager.ensure_image().await?;
        container_spawn(build_id, agent_cli, state)
    } else {
        // Native PTY path — not handled here; caller falls back to existing path
        Ok(())
    }
}

/// Container-specific spawn — currently a placeholder.
///
/// # Errors
///
/// Always returns [`ContainerError::RelayNotImplemented`].
///
/// The WebSocket relay (browser ↔ container I/O) needs a separate design cycle.
/// Options:
/// - **A**: Container exposes TCP port; webshell connects via `tokio::net::TcpStream`
/// - **B**: Container connects back to webshell WebSocket endpoint (new route needed)
/// - **C**: `docker attach` to container's PTY (not natively supported for `docker run -d`)
fn container_spawn(
    _build_id: &str,
    _agent_cli: &str,
    _state: &AppState,
) -> Result<(), ContainerError> {
    tracing::warn!(target: "container", "container_spawn called but WebSocket relay not implemented");
    Err(ContainerError::RelayNotImplemented)
}
