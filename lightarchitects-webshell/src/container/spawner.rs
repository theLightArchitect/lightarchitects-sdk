//! Session spawner — transparent router between container and native PTY paths.

use std::path::Path;

use lightarchitects::container_spawn::ContainerPolicy;

use crate::container::{
    docker_cmd,
    types::{ContainerError, ContainerHandle, ContainerMode, DockerCapability},
};
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
/// Returns an error if the container path is selected and `docker run` fails,
/// the image is not in the allowlist, or the concurrent cap is exceeded.
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

/// Container-specific spawn using the active [`ContainerPolicy`].
///
/// # Security
///
/// Image name is validated against [`ALLOWED_IMAGES`] before any Docker call.
/// Concurrent container count is enforced via the shared semaphore in [`AppState`].
/// The policy snapshot is taken once at entry (`M10` idiom — single `ArcSwap` load).
///
/// # Errors
///
/// - [`ContainerError::ConcurrencyCapExceeded`] when no semaphore permit is available.
/// - [`ContainerError::PolicyError`] when the policy produces invalid docker args.
/// - [`ContainerError::Io`] on image allowlist violation or `docker run` failure.
async fn container_spawn(
    build_id: &str,
    _agent_cli: &str,
    _cwd: &Path,
    state: &AppState,
) -> Result<ContainerHandle, ContainerError> {
    // M10: SINGLE-LOAD — snapshot policy ONCE at function entry; use throughout.
    let policy: std::sync::Arc<ContainerPolicy> = state.policy.load_full();

    let image = std::env::var("LA_AGENT_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE_NAME.to_owned());
    if !ALLOWED_IMAGES.contains(&image.as_str()) {
        return Err(ContainerError::Io(std::io::Error::other(format!(
            "image '{image}' is not in the container allowlist"
        ))));
    }

    // Acquire semaphore BEFORE docker run — enforces max_concurrent cap atomically.
    // The permit is held until docker run either succeeds (permit forgotten) or
    // fails (permit dropped automatically → slot returned).
    let permit = state
        .policy_semaphore
        .clone()
        .try_acquire_owned()
        .map_err(|_| ContainerError::ConcurrencyCapExceeded)?;

    // For Hardened/Airgapped: resolve seccomp profile to a private temp file.
    // `_seccomp` keeps the NamedTempFile alive (and path valid) until after
    // `run_detached` returns; it is deleted on drop.
    let _seccomp;
    let policy_for_args;
    if policy.iso_mode.requires_read_only_root() {
        let tmp = super::seccomp_resolver::write_seccomp_profile().map_err(ContainerError::Io)?;
        let mut p = (*policy).clone();
        p.seccomp_profile_path = Some(tmp.path().to_path_buf());
        _seccomp = Some(tmp);
        policy_for_args = p;
    } else {
        _seccomp = None;
        policy_for_args = (*policy).clone();
    }

    let docker_args = policy_for_args
        .build_docker_args()
        .map_err(|e| ContainerError::PolicyError(e.to_string()))?;

    let container_name = format!("la-{}", sanitize_build_id(build_id));

    // Compose full arg list: policy-derived args + --name <name> + image.
    let mut full_args: Vec<&str> = docker_args.iter().map(String::as_str).collect();
    full_args.extend_from_slice(&["--name", &container_name, &image]);

    let output = docker_cmd::run_detached(&full_args)
        .await
        .map_err(ContainerError::Io)?;

    if !output.status.success() {
        // permit dropped here → semaphore slot automatically returned.
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ContainerError::Io(std::io::Error::other(format!(
            "docker run failed: {stderr}"
        ))));
    }

    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let relay_url = format!("/api/terminal/container/{container_id}");

    // H1 fix: forget the permit ONLY after docker run succeeds, then insert into
    // active_containers. The relay's ContainerDropGuard will call add_permits(1)
    // on drop, returning the slot. If active_containers write fails (poisoned lock),
    // we add the permit back manually before returning the error.
    permit.forget();

    let inserted = state
        .active_containers
        .write()
        .map(|mut g| {
            g.insert(container_id.clone(), std::time::Instant::now());
        })
        .is_ok();

    if !inserted {
        // Rollback: manually return the forgotten permit slot, then kill the container.
        state.policy_semaphore.add_permits(1);
        let id = container_id.clone();
        drop(tokio::spawn(async move {
            docker_cmd::stop(&id).await;
            docker_cmd::rm_force(&[&id]).await;
        }));
        return Err(ContainerError::Io(std::io::Error::other(
            "active_containers lock poisoned during spawn",
        )));
    }

    tracing::info!(
        target: "container",
        container_id = %container_id,
        container_name = %container_name,
        iso_mode = ?policy.iso_mode,
        "container spawned"
    );

    Ok(ContainerHandle {
        container_id,
        relay_url,
    })
}

/// Sanitizes a build ID for use in a Docker container name.
///
/// Docker container names allow `[a-zA-Z0-9_.-]`. Strips everything else.
fn sanitize_build_id(build_id: &str) -> String {
    build_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        .take(48)
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
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

    /// Airgapped network negative-control: `--network=none` containers cannot
    /// reach external IPs.  Gated behind `AIRGAPPED_E2E=1` so it only runs in
    /// environments with a live Docker daemon.  Proves P2 Northstar predicate:
    /// "Airgapped container curl `https://1.1.1.1` → exit code != 0."
    ///
    /// Run manually: `AIRGAPPED_E2E=1 cargo test -p lightarchitects-webshell airgapped_network`
    #[test]
    fn airgapped_network_blocks_outbound_traffic() {
        if std::env::var("AIRGAPPED_E2E").is_err() {
            return; // skip when Docker not explicitly requested
        }

        let out = std::process::Command::new("docker")
            .args([
                "run",
                "--rm",
                "--network",
                "none",
                // alpine:latest has `ping` via busybox; no curl/wget needed
                "alpine",
                "ping",
                "-c",
                "1",
                "-W",
                "2",
                "1.1.1.1",
            ])
            .output()
            .expect("docker must be on $PATH for AIRGAPPED_E2E test");

        assert!(
            !out.status.success(),
            "Airgapped container (--network=none) should NOT be able to reach 1.1.1.1, \
             but ping exited successfully — network isolation is broken"
        );
    }
}
