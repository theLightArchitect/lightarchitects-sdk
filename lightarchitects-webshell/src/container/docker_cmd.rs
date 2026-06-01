//! Private Docker CLI wrapper with per-operation timeouts.
//!
//! All Docker subprocess calls in the container subsystem route through this
//! module. Timeouts are defined once here — callers never interact with
//! `tokio::time::timeout` or raw `tokio::process::Command` directly.

use std::{process::Stdio, sync::OnceLock, time::Duration};

use tokio::{process::Command, time::timeout};

// ── Docker binary resolution ─────────────────────────────────────────────────

/// Resolves the Docker binary path once at first call and caches it.
///
/// `LaunchAgent` environments inherit a stripped PATH (`/usr/bin:/bin:/usr/sbin:/sbin`)
/// that excludes `/usr/local/bin` and `/opt/homebrew/bin`, so `Command::new(docker_bin())`
/// silently fails. Probe common install locations before falling back to `"docker"`.
pub(crate) fn docker_bin() -> &'static str {
    static DOCKER: OnceLock<String> = OnceLock::new();
    DOCKER.get_or_init(|| {
        // Check DOCKER_PATH env override first (for testing / non-standard installs).
        if let Ok(p) = std::env::var("DOCKER_PATH") {
            if std::path::Path::new(&p).exists() {
                return p;
            }
        }
        // Probe common install locations in order of prevalence on macOS + Linux.
        for candidate in &[
            "/usr/local/bin/docker",
            "/opt/homebrew/bin/docker",
            "/usr/bin/docker",
            "/snap/bin/docker",
        ] {
            if std::path::Path::new(candidate).exists() {
                return (*candidate).to_owned();
            }
        }
        "docker".to_owned()
    })
}

// ── Timeout budget per operation ────────────────────────────────────────────

const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
const INSPECT_TIMEOUT: Duration = Duration::from_secs(5);
const PS_TIMEOUT: Duration = Duration::from_secs(10);
const RM_TIMEOUT: Duration = Duration::from_secs(10);
const STOP_TIMEOUT: Duration = Duration::from_secs(10);
const RUN_TIMEOUT: Duration = Duration::from_secs(15);
const PULL_TIMEOUT: Duration = Duration::from_secs(120);
const BUILD_TIMEOUT: Duration = Duration::from_secs(300);

// ── Public API ───────────────────────────────────────────────────────────────

/// Returns the Docker server version string, or `None` if the daemon is
/// unreachable or the call times out.
pub(crate) async fn version() -> Option<String> {
    let out = timeout(
        VERSION_TIMEOUT,
        Command::new(docker_bin())
            .args(["version", "--format", "{{.Server.Version}}"])
            .output(),
    )
    .await
    .ok()?
    .ok()?;

    if out.status.success() {
        let ver = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        Some(ver)
    } else {
        None
    }
}

/// Returns `true` if `image` exists in the local Docker image store.
pub(crate) async fn image_exists(image: &str) -> bool {
    timeout(
        INSPECT_TIMEOUT,
        Command::new(docker_bin())
            .args(["image", "inspect", image])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
    .await
    .is_ok_and(|r| r.is_ok_and(|s| s.success()))
}

/// Pulls `image` from the registry. Returns the exit status, or an I/O error
/// if the daemon is unreachable or the pull exceeds [`PULL_TIMEOUT`].
pub(crate) async fn pull(image: &str) -> std::io::Result<std::process::ExitStatus> {
    timeout(
        PULL_TIMEOUT,
        Command::new(docker_bin())
            .args(["pull", image])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
    .await
    .map_err(|_| {
        tracing::warn!(target: "container", image, "docker pull timed out");
        std::io::Error::other("docker pull timed out")
    })?
}

/// Builds an image tagged `tag` from the build context at `context_path`.
/// Returns the exit status, or an I/O error on timeout.
pub(crate) async fn build(
    tag: &str,
    context_path: &str,
) -> std::io::Result<std::process::ExitStatus> {
    timeout(
        BUILD_TIMEOUT,
        Command::new(docker_bin())
            .args(["build", "-t", tag, context_path])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
    .await
    .map_err(|_| {
        tracing::warn!(target: "container", tag, "docker build timed out");
        std::io::Error::other("docker build timed out")
    })?
}

/// Runs a container in detached mode (`-d`) with the given extra args.
/// Returns the full output (stdout = container ID on success).
pub(crate) async fn run_detached(args: &[&str]) -> std::io::Result<std::process::Output> {
    let mut cmd_args = vec!["run", "-d"];
    cmd_args.extend_from_slice(args);

    timeout(
        RUN_TIMEOUT,
        Command::new(docker_bin())
            .args(&cmd_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| std::io::Error::other("docker run timed out"))?
}

/// Runs `docker run --rm hello-world` to verify spawn permission.
/// Returns `true` on success, `false` on permission denial or timeout.
pub(crate) async fn check_run_permission() -> bool {
    timeout(
        RUN_TIMEOUT,
        Command::new(docker_bin())
            .args(["run", "--rm", "hello-world"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
    .await
    .is_ok_and(|r| r.is_ok_and(|s| s.success()))
}

/// Stops a container with a 3-second grace period. Fire-and-forget safe.
pub(crate) async fn stop(container_id: &str) {
    let _ = timeout(
        STOP_TIMEOUT,
        Command::new(docker_bin())
            .args(["stop", "--time", "3", container_id])
            .output(),
    )
    .await;
}

/// Force-removes containers by ID. Fire-and-forget safe.
pub(crate) async fn rm_force(ids: &[&str]) {
    if ids.is_empty() {
        return;
    }
    let mut args = vec!["rm", "-f"];
    args.extend_from_slice(ids);
    let _ = timeout(RM_TIMEOUT, Command::new(docker_bin()).args(&args).output()).await;
}

/// Lists running container IDs matching `label` (e.g. `"managed-by=la-hitl"`).
/// Returns an empty vec on timeout or daemon error.
pub(crate) async fn ps_running_with_label(label: &str) -> Vec<String> {
    let result = timeout(
        PS_TIMEOUT,
        Command::new(docker_bin())
            .args([
                "ps",
                "-q",
                "--filter",
                &format!("label={label}"),
                "--filter",
                "status=running",
            ])
            .output(),
    )
    .await;

    match result {
        Ok(Ok(out)) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
        Ok(_) => {
            tracing::warn!(target: "container", label, "docker ps (running) failed");
            vec![]
        }
        Err(_) => {
            tracing::warn!(target: "container", label, "docker ps (running) timed out");
            vec![]
        }
    }
}

/// Lists exited container IDs matching `label` (e.g. `"managed-by=la-hitl"`).
/// Returns an empty vec on timeout or daemon error.
pub(crate) async fn ps_exited_with_label(label: &str) -> Vec<String> {
    let result = timeout(
        PS_TIMEOUT,
        Command::new(docker_bin())
            .args([
                "ps",
                "-a",
                "-q",
                "--filter",
                &format!("label={label}"),
                "--filter",
                "status=exited",
            ])
            .output(),
    )
    .await;

    match result {
        Ok(Ok(out)) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
        Ok(_) => {
            tracing::warn!(target: "container", label, "docker ps failed");
            vec![]
        }
        Err(_) => {
            tracing::warn!(target: "container", label, "docker ps timed out — daemon unreachable");
            vec![]
        }
    }
}
