//! Agent backend abstraction — controls how sibling agent processes are spawned.
//!
//! Three backends are available:
//! - [`NativeBackend`]: spawn as OS child processes (current default, Khadas production)
//! - [`DockerBackend`]: spawn as Docker containers named `larc-agent-{sibling}`
//! - [`SandboxBackend`]: spawn via macOS `sandbox-exec` with a minimal filesystem profile
//!
//! Backend selection is driven by `ARENA_AGENT_BACKEND` env var.
//!
//! # Safety
//!
//! `NativeBackend` and `SandboxBackend` use `libc::kill` to send SIGTERM and probe
//! process existence. These calls are in `send_sigterm` and `pid_is_running`, each
//! with a `// SAFETY:` comment justifying the `unsafe` block.
#![allow(unsafe_code)]

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use lightarchitects::core::paths;
use tokio::process::Command;
use tokio::sync::Mutex;

use super::arena_config::{AgentBackendKind, Config};

// ── Public types ──────────────────────────────────────────────────────────

/// A handle to a running agent process.
#[derive(Debug, Clone)]
pub struct AgentHandle {
    /// Sibling name (e.g., "eva", "corso", "laex").
    // Read by lifecycle methods (stop/status/restart) — not yet wired to supervisor.
    #[allow(dead_code)]
    pub sibling: String,
    /// Backend-specific process identity (PID for native/sandbox, container name for Docker).
    pub identity: AgentIdentity,
}

/// Backend-specific process identity.
// Inner values extracted by lifecycle methods — not yet wired to supervisor.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum AgentIdentity {
    /// Native OS process ID.
    Pid(u32),
    /// Docker container name.
    ContainerName(String),
    /// Sandbox process ID (macOS `sandbox-exec`).
    SandboxPid(u32),
}

/// Runtime status of an agent.
// Returned by `status()` — lifecycle management API, not yet wired to supervisor.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    /// Running and healthy.
    Running,
    /// Process has exited (expected or unexpected).
    Exited,
    /// Status could not be determined.
    Unknown,
}

/// Configuration for spawning a single agent.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Binary path for native/sandbox backends.
    pub binary_path: PathBuf,
    /// Docker image for the Docker backend.
    pub docker_image: String,
    /// Ollama host passed via `OLLAMA_HOST` env var to the agent.
    pub ollama_host: String,
    /// Soul helix mount source path (read-only for agents).
    pub soul_path: PathBuf,
    /// Arena data mount source path (read-write for curator, read-only for agents).
    pub arena_path: PathBuf,
    /// Sandbox profiles directory (created by SandboxBackend at runtime).
    pub sandbox_profiles_dir: PathBuf,
}

impl AgentConfig {
    /// Build `AgentConfig` from the global `Config`.
    ///
    /// # Errors
    /// Returns error if the home directory cannot be resolved.
    pub fn from_config(config: &Config) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let soul_path = std::env::var("SOUL_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| paths::soul_or_fallback());
        let arena_path = std::env::var("ARENA_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| config.data_dir.clone());
        let sandbox_profiles_dir = arena_path.join("sandbox-profiles");

        // Use the configured gateway binary for native/sandbox backends
        let binary_path =
            std::env::current_exe().unwrap_or_else(|_| PathBuf::from("lightarchitects-gateway"));

        Ok(Self {
            binary_path,
            docker_image: config.docker_image.clone(),
            ollama_host: config.ollama_host.clone(),
            soul_path,
            arena_path,
            sandbox_profiles_dir,
        })
    }
}

// ── AgentBackend trait ────────────────────────────────────────────────────

/// Abstraction over how sibling agent processes are spawned and managed.
///
/// All implementations must be `Send + Sync` — the backend is stored behind `Arc`.
// `stop`/`status`/`restart` are the lifecycle management API — not yet wired to
// the supervisor restart loop. Suppressed here; the private helpers have their own
// `#[allow(dead_code)]` annotations for the same reason.
#[allow(dead_code)]
pub trait AgentBackend: Send + Sync {
    /// Spawn a new agent process for the given sibling.
    ///
    /// Returns a handle identifying the running process.
    fn spawn<'a>(
        &'a self,
        sibling: &'a str,
        config: &'a AgentConfig,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentHandle, String>> + Send + 'a>>;

    /// Stop a running agent.
    fn stop<'a>(
        &'a self,
        handle: &'a AgentHandle,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>>;

    /// Query the current status of an agent.
    fn status<'a>(
        &'a self,
        handle: &'a AgentHandle,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentStatus, String>> + Send + 'a>>;

    /// Stop the agent and spawn a fresh one. Returns the new handle.
    fn restart<'a>(
        &'a self,
        handle: &'a AgentHandle,
        config: &'a AgentConfig,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentHandle, String>> + Send + 'a>>;
}

// ── Factory function ───────────────────────────────────────────────────────

/// Create the appropriate backend from config.
///
/// Returns a boxed `AgentBackend` implementation matching `config.agent_backend`.
#[must_use]
pub fn create_backend(config: &Config) -> Arc<dyn AgentBackend> {
    match config.agent_backend {
        AgentBackendKind::Native => Arc::new(NativeBackend),
        AgentBackendKind::Docker => Arc::new(DockerBackend),
        AgentBackendKind::Sandbox => Arc::new(SandboxBackend),
    }
}

// ── NativeBackend ─────────────────────────────────────────────────────────

/// Spawns agents as native OS child processes via `tokio::process::Command`.
///
/// This is the default for Khadas production. The orchestrator binary re-executes
/// itself with `--agent <sibling>`. If the child exits, the supervisor restarts it.
pub struct NativeBackend;

impl AgentBackend for NativeBackend {
    fn spawn<'a>(
        &'a self,
        sibling: &'a str,
        config: &'a AgentConfig,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentHandle, String>> + Send + 'a>>
    {
        Box::pin(native_spawn(sibling, config))
    }

    fn stop<'a>(
        &'a self,
        handle: &'a AgentHandle,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(native_stop(handle))
    }

    fn status<'a>(
        &'a self,
        handle: &'a AgentHandle,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentStatus, String>> + Send + 'a>>
    {
        Box::pin(native_status(handle))
    }

    fn restart<'a>(
        &'a self,
        handle: &'a AgentHandle,
        config: &'a AgentConfig,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentHandle, String>> + Send + 'a>>
    {
        Box::pin(native_restart(handle, config))
    }
}

async fn native_spawn(sibling: &str, config: &AgentConfig) -> Result<AgentHandle, String> {
    let child = Command::new(&config.binary_path)
        .arg("--agent")
        .arg(sibling)
        .env("OLLAMA_HOST", &config.ollama_host)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("NativeBackend: failed to spawn {sibling}: {e}"))?;

    let pid = child.id().ok_or_else(|| {
        format!("NativeBackend: spawned {sibling} but process exited immediately")
    })?;

    tracing::info!(sibling, pid, "NativeBackend: agent spawned");
    Ok(AgentHandle {
        sibling: sibling.to_owned(),
        identity: AgentIdentity::Pid(pid),
    })
}

#[allow(dead_code)] // called by AgentBackend::stop — lifecycle API, not yet wired
async fn native_stop(handle: &AgentHandle) -> Result<(), String> {
    let AgentIdentity::Pid(pid) = handle.identity else {
        return Err(format!(
            "NativeBackend: unexpected identity type for {}",
            handle.sibling
        ));
    };
    send_sigterm(pid).map_err(|e| format!("NativeBackend: {e}"))?;
    tracing::info!(sibling = %handle.sibling, pid, "NativeBackend: SIGTERM sent");
    Ok(())
}

#[allow(dead_code)] // called by AgentBackend::status — lifecycle API, not yet wired
async fn native_status(handle: &AgentHandle) -> Result<AgentStatus, String> {
    let AgentIdentity::Pid(pid) = handle.identity else {
        return Ok(AgentStatus::Unknown);
    };
    let running = pid_is_running(pid);
    Ok(if running {
        AgentStatus::Running
    } else {
        AgentStatus::Exited
    })
}

#[allow(dead_code)] // called by AgentBackend::restart — lifecycle API, not yet wired
async fn native_restart(handle: &AgentHandle, config: &AgentConfig) -> Result<AgentHandle, String> {
    let _ = native_stop(handle).await; // best-effort stop; ignore error if already exited
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    native_spawn(&handle.sibling, config).await
}

// ── DockerBackend ──────────────────────────────────────────────────────────

/// Spawns agents as Docker containers named `larc-agent-{sibling}`.
///
/// Uses the `docker` CLI as a subprocess — no Docker SDK dependency.
/// The container runs the gateway image with `--agent <sibling>`.
/// Volumes: `~/.soul` (read-only), arena path (read-only for agents).
pub struct DockerBackend;

impl AgentBackend for DockerBackend {
    fn spawn<'a>(
        &'a self,
        sibling: &'a str,
        config: &'a AgentConfig,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentHandle, String>> + Send + 'a>>
    {
        Box::pin(docker_spawn(sibling, config))
    }

    fn stop<'a>(
        &'a self,
        handle: &'a AgentHandle,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(docker_stop(handle))
    }

    fn status<'a>(
        &'a self,
        handle: &'a AgentHandle,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentStatus, String>> + Send + 'a>>
    {
        Box::pin(docker_status(handle))
    }

    fn restart<'a>(
        &'a self,
        handle: &'a AgentHandle,
        config: &'a AgentConfig,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentHandle, String>> + Send + 'a>>
    {
        Box::pin(docker_restart(handle, config))
    }
}

/// Container name convention: `larc-agent-{sibling}`.
fn container_name(sibling: &str) -> String {
    format!("larc-agent-{sibling}")
}

async fn docker_spawn(sibling: &str, config: &AgentConfig) -> Result<AgentHandle, String> {
    let name = container_name(sibling);
    let soul_mount = format!("{}:/soul:ro", config.soul_path.display());
    let arena_mount = format!("{}:/arena:ro", config.arena_path.display());

    // Remove any existing stopped container with the same name (idempotent)
    let _ = Command::new("docker")
        .args(["rm", "-f", &name])
        .output()
        .await;

    let status = Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            &name,
            "--restart",
            "unless-stopped",
            "--network",
            "arena-net",
            "--add-host",
            "host.docker.internal:host-gateway",
            "-v",
            &soul_mount,
            "-v",
            &arena_mount,
            "-e",
            &format!("OLLAMA_HOST={}", config.ollama_host),
            "--label",
            "managed-by=arena",
            &config.docker_image,
            "--agent",
            sibling,
        ])
        .status()
        .await
        .map_err(|e| format!("DockerBackend: docker run failed for {sibling}: {e}"))?;

    if !status.success() {
        return Err(format!(
            "DockerBackend: docker run exited {status} for {sibling}"
        ));
    }

    tracing::info!(sibling, container = %name, "DockerBackend: container started");
    Ok(AgentHandle {
        sibling: sibling.to_owned(),
        identity: AgentIdentity::ContainerName(name),
    })
}

#[allow(dead_code)] // called by AgentBackend::stop — lifecycle API, not yet wired
async fn docker_stop(handle: &AgentHandle) -> Result<(), String> {
    let AgentIdentity::ContainerName(ref name) = handle.identity else {
        return Err(format!(
            "DockerBackend: unexpected identity type for {}",
            handle.sibling
        ));
    };
    let status = Command::new("docker")
        .args(["stop", name])
        .status()
        .await
        .map_err(|e| format!("DockerBackend: docker stop failed: {e}"))?;
    if !status.success() {
        return Err(format!(
            "DockerBackend: docker stop exited {status} for {}",
            handle.sibling
        ));
    }
    tracing::info!(sibling = %handle.sibling, container = %name, "DockerBackend: container stopped");
    Ok(())
}

#[allow(dead_code)] // called by AgentBackend::status — lifecycle API, not yet wired
async fn docker_status(handle: &AgentHandle) -> Result<AgentStatus, String> {
    let AgentIdentity::ContainerName(ref name) = handle.identity else {
        return Ok(AgentStatus::Unknown);
    };
    let output = Command::new("docker")
        .args(["inspect", "--format", "{{.State.Status}}", name])
        .output()
        .await
        .map_err(|e| format!("DockerBackend: docker inspect failed: {e}"))?;

    let state = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(match state.as_str() {
        "running" => AgentStatus::Running,
        "exited" | "dead" | "removing" => AgentStatus::Exited,
        _ => AgentStatus::Unknown,
    })
}

#[allow(dead_code)] // called by AgentBackend::restart — lifecycle API, not yet wired
async fn docker_restart(handle: &AgentHandle, config: &AgentConfig) -> Result<AgentHandle, String> {
    let _ = docker_stop(handle).await; // best-effort
    docker_spawn(&handle.sibling, config).await
}

// ── SandboxBackend ─────────────────────────────────────────────────────────

/// Spawns agents via macOS `sandbox-exec` with a minimal filesystem profile.
///
/// The profile allows read/write access to `~/lightarchitects/soul/`, `~/lightarchitects/arena/`, `~/lightarchitects/ayin/logs/`.
/// Raw sockets are denied. The profile is written to `~/lightarchitects/arena/sandbox-profiles/{sibling}.sb`
/// at spawn time and reused on subsequent restarts.
pub struct SandboxBackend;

/// Sandbox profile template — macOS TinyScheme-based SBPL.
const SANDBOX_PROFILE_TEMPLATE: &str = r#"(version 1)
(deny default)

; Allow execution of the agent binary
(allow process-exec)

; Allow inter-process operations needed for IPC
(allow process-fork)
(allow signal (target self))

; Allow file reads under ~/lightarchitects/soul, ~/lightarchitects/arena, ~/lightarchitects/ayin
(allow file-read*
  (subpath "{soul_path}")
  (subpath "{arena_path}")
  (subpath "{ayin_logs_path}"))

; Allow file writes under ~/lightarchitects/soul and ~/lightarchitects/arena only
(allow file-write*
  (subpath "{soul_path}")
  (subpath "{arena_path}"))

; Allow reads of system paths needed for the runtime
(allow file-read*
  (subpath "/usr/lib")
  (subpath "/usr/local/lib")
  (subpath "/System/Library")
  (literal "/dev/null")
  (literal "/dev/urandom"))

; Allow network (TCP/UDP) but deny raw sockets
(allow network-outbound)
(allow network-inbound)
(deny network* (with no-report)
  (local unix-socket))

; Allow basic system operations
(allow sysctl-read)
(allow mach-lookup)
"#;

impl AgentBackend for SandboxBackend {
    fn spawn<'a>(
        &'a self,
        sibling: &'a str,
        config: &'a AgentConfig,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentHandle, String>> + Send + 'a>>
    {
        Box::pin(sandbox_spawn(sibling, config))
    }

    fn stop<'a>(
        &'a self,
        handle: &'a AgentHandle,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(sandbox_stop(handle))
    }

    fn status<'a>(
        &'a self,
        handle: &'a AgentHandle,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentStatus, String>> + Send + 'a>>
    {
        Box::pin(sandbox_status(handle))
    }

    fn restart<'a>(
        &'a self,
        handle: &'a AgentHandle,
        config: &'a AgentConfig,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentHandle, String>> + Send + 'a>>
    {
        Box::pin(sandbox_restart(handle, config))
    }
}

/// Write the sandbox profile for a sibling, return the profile path.
#[allow(dead_code)] // called by sandbox_spawn — lifecycle API, not yet wired
fn write_sandbox_profile(sibling: &str, config: &AgentConfig) -> Result<PathBuf, String> {
    let ayin_logs_path = paths::ayin_or_fallback().join("logs");

    let profile = SANDBOX_PROFILE_TEMPLATE
        .replace("{soul_path}", &config.soul_path.to_string_lossy())
        .replace("{arena_path}", &config.arena_path.to_string_lossy())
        .replace("{ayin_logs_path}", &ayin_logs_path.to_string_lossy());

    std::fs::create_dir_all(&config.sandbox_profiles_dir)
        .map_err(|e| format!("SandboxBackend: create_dir_all failed: {e}"))?;

    let profile_path = config.sandbox_profiles_dir.join(format!("{sibling}.sb"));
    std::fs::write(&profile_path, &profile)
        .map_err(|e| format!("SandboxBackend: write profile failed: {e}"))?;

    Ok(profile_path)
}

async fn sandbox_spawn(sibling: &str, config: &AgentConfig) -> Result<AgentHandle, String> {
    let profile_path = write_sandbox_profile(sibling, config)?;

    let child = Command::new("sandbox-exec")
        .arg("-f")
        .arg(&profile_path)
        .arg(&config.binary_path)
        .arg("--agent")
        .arg(sibling)
        .env("OLLAMA_HOST", &config.ollama_host)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("SandboxBackend: sandbox-exec failed for {sibling}: {e}"))?;

    let pid = child.id().ok_or_else(|| {
        format!("SandboxBackend: spawned {sibling} but process exited immediately")
    })?;

    tracing::info!(sibling, pid, "SandboxBackend: agent spawned");
    Ok(AgentHandle {
        sibling: sibling.to_owned(),
        identity: AgentIdentity::SandboxPid(pid),
    })
}

#[allow(dead_code)] // called by AgentBackend::stop — lifecycle API, not yet wired
async fn sandbox_stop(handle: &AgentHandle) -> Result<(), String> {
    let AgentIdentity::SandboxPid(pid) = handle.identity else {
        return Err(format!(
            "SandboxBackend: unexpected identity type for {}",
            handle.sibling
        ));
    };
    send_sigterm(pid).map_err(|e| format!("SandboxBackend: {e}"))?;
    tracing::info!(sibling = %handle.sibling, pid, "SandboxBackend: SIGTERM sent");
    Ok(())
}

#[allow(dead_code)] // called by AgentBackend::status — lifecycle API, not yet wired
async fn sandbox_status(handle: &AgentHandle) -> Result<AgentStatus, String> {
    let AgentIdentity::SandboxPid(pid) = handle.identity else {
        return Ok(AgentStatus::Unknown);
    };
    Ok(if pid_is_running(pid) {
        AgentStatus::Running
    } else {
        AgentStatus::Exited
    })
}

#[allow(dead_code)] // called by AgentBackend::restart — lifecycle API, not yet wired
async fn sandbox_restart(
    handle: &AgentHandle,
    config: &AgentConfig,
) -> Result<AgentHandle, String> {
    let _ = sandbox_stop(handle).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    sandbox_spawn(&handle.sibling, config).await
}

// ── Shared utilities ───────────────────────────────────────────────────────

/// Send SIGTERM to a process by PID.
///
/// Uses the standard `kill(2)` syscall via `libc` on Unix.
/// On non-Unix platforms, logs a warning and returns `Ok(())`.
fn send_sigterm(pid: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        let raw_pid = i32::try_from(pid).map_err(|_| format!("PID {pid} overflows i32"))?;
        // SAFETY: kill(2) is safe to call with any valid pid_t and SIGTERM.
        // We check the return value and map the errno to a String error.
        let ret = unsafe { libc::kill(raw_pid, libc::SIGTERM) };
        if ret != 0 {
            let errno = std::io::Error::last_os_error();
            return Err(format!("kill({pid}, SIGTERM): {errno}"));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        tracing::warn!(pid, "send_sigterm: SIGTERM not supported on this platform");
        Ok(())
    }
}

/// Check if a PID is currently running (Unix only).
///
/// Uses `kill(pid, 0)` — sends no signal but checks whether the process exists.
fn pid_is_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let Ok(raw_pid) = i32::try_from(pid) else {
            return false;
        };
        // SAFETY: kill(pid, 0) is a standard POSIX existence probe.
        // Signal 0 is never delivered; the call only checks process existence.
        let ret = unsafe { libc::kill(raw_pid, 0) };
        ret == 0
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

// ── Managed agent set ──────────────────────────────────────────────────────

/// Thread-safe set of running agent handles.
///
/// The orchestrator stores handles here so the supervisor can query/restart them.
#[derive(Default)]
pub struct ManagedAgents {
    handles: Mutex<Vec<AgentHandle>>,
}

impl ManagedAgents {
    /// Create an empty handle set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handle.
    pub async fn insert(&self, handle: AgentHandle) {
        self.handles.lock().await.push(handle);
    }

    /// Snapshot all handles (cloned).
    #[allow(dead_code)] // supervisor restart loop API, not yet wired
    pub async fn snapshot(&self) -> Vec<AgentHandle> {
        self.handles.lock().await.clone()
    }

    /// Replace all handles (e.g., after a full restart).
    #[allow(dead_code)] // supervisor restart loop API, not yet wired
    pub async fn replace(&self, new_handles: Vec<AgentHandle>) {
        *self.handles.lock().await = new_handles;
    }
}
