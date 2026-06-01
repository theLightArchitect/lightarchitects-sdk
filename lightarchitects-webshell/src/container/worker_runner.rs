//! Worker-task container launcher.
//!
//! Spawns a sandboxed Docker container for each autonomous wave task.
//! The container runs the `lightarchitects --bare` agent CLI; `LiteLLM`
//! env vars are forwarded via `docker run -e` so the in-container agent
//! can reach the proxy without touching host network state.
//!
//! # Lifecycle
//!
//! ```text
//! spawn_worker_container(spec, state)
//!   └─ PolicyStore.tighten_for_build (per-task override)
//!   └─ semaphore.try_acquire_owned()
//!   └─ build_container_run_args (shared helper from spawner)
//!   └─ docker run -d ...
//!   └─ active_containers.insert(ContainerKind::WorkerTask)
//!
//! await_worker_exit(id)
//!   └─ docker wait <id>  (blocks until exit)
//!
//! reap_worker(id, state)
//!   └─ active_containers.remove(id) + semaphore.add_permits(1)
//!   └─ docker stop + docker rm -f (fire-and-forget)
//! ```

use std::{sync::Arc, time::Instant};

use lightarchitects::{
    container_spawn::{ContainerPolicy, SpawnPolicy},
    lightsquad::wave_dispatcher::WorkerSpec,
};

use crate::{
    container::{
        docker_cmd,
        spawner::build_container_run_args,
        types::{ActiveContainerEntry, ContainerError, ContainerHandle, ContainerKind},
    },
    server::AppState,
};

/// Exit status from a completed worker container.
#[derive(Debug, Clone)]
pub struct WorkerOutcome {
    /// Process exit code from `docker wait`.
    pub exit_code: i32,
}

/// Spawn a sandboxed container to execute `spec`.
///
/// Applies the per-task `policy_override` (tightening only) on top of the
/// system [`ContainerPolicy`], acquires a semaphore slot, then runs
/// `docker run -d` with the composed args.
///
/// On success, inserts a [`ContainerKind::WorkerTask`] entry into
/// `state.active_containers`; the paired [`reap_worker`] removes it.
///
/// # Errors
///
/// - [`ContainerError::ConcurrencyCapExceeded`] if no semaphore slot is free.
/// - [`ContainerError::PolicyError`] if the per-task override violates tightening rules.
/// - [`ContainerError::Io`] on `docker run` failure.
pub async fn spawn_worker_container(
    spec: &WorkerSpec,
    state: &AppState,
) -> Result<ContainerHandle, ContainerError> {
    // M10: single load — snapshot policy once at entry.
    let base_policy: Arc<ContainerPolicy> = state.policy.load_full();

    // Apply per-task tightening override if present.
    let effective_policy: Arc<ContainerPolicy> = if let Some(ref ov) = spec.task.policy_override {
        let override_cp = override_to_container_policy(ov, &base_policy);
        state
            .policy_store
            .tighten_for_build(&override_cp)
            .map_err(|e| ContainerError::PolicyError(e.to_string()))?
    } else {
        Arc::clone(&base_policy)
    };

    // Acquire semaphore before docker run.
    let permit = state
        .policy_semaphore
        .clone()
        .try_acquire_owned()
        .map_err(|_| ContainerError::ConcurrencyCapExceeded)?;

    let container_name = format!(
        "la-worker-{task}-w{wave}",
        task = sanitize_id(&spec.task.id),
        wave = spec.wave_index,
    );

    let (full_args, _seccomp) =
        build_container_run_args(&effective_policy, &container_name).await?;

    let full_arg_refs: Vec<&str> = full_args.iter().map(String::as_str).collect();
    let output = docker_cmd::run_detached(&full_arg_refs)
        .await
        .map_err(ContainerError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ContainerError::Io(std::io::Error::other(format!(
            "docker run (worker) failed: {stderr}"
        ))));
    }

    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let relay_url = String::new(); // worker containers have no relay URL

    // Forget permit ONLY after docker run succeeds (H1 pattern).
    let iso_mode = effective_policy.iso_mode;
    permit.forget();

    let entry = ActiveContainerEntry {
        kind: ContainerKind::WorkerTask {
            task_id: spec.task.id.clone(),
            wave_index: spec.wave_index,
        },
        started_at: Instant::now(),
        policy_snapshot_iso_mode: iso_mode,
    };

    let inserted = state
        .active_containers
        .write()
        .map(|mut g| {
            g.insert(container_id.clone(), entry);
        })
        .is_ok();

    if !inserted {
        // Rollback: return semaphore slot + kill container.
        state.policy_semaphore.add_permits(1);
        let id = container_id.clone();
        drop(tokio::spawn(async move {
            docker_cmd::stop(&id).await;
            docker_cmd::rm_force(&[&id]).await;
        }));
        return Err(ContainerError::Io(std::io::Error::other(
            "active_containers lock poisoned during worker spawn",
        )));
    }

    tracing::info!(
        target: "container",
        container_id = %container_id,
        container_name = %container_name,
        task_id = %spec.task.id,
        wave_index = spec.wave_index,
        ?iso_mode,
        "worker container spawned"
    );

    Ok(ContainerHandle {
        container_id,
        relay_url,
    })
}

/// Wait for a worker container to exit and return its exit code.
///
/// Runs `docker wait <id>` which blocks until the container stops.
///
/// # Errors
///
/// Returns [`ContainerError::Io`] if the `docker wait` command fails to spawn
/// or produces non-UTF-8 output.
pub async fn await_worker_exit(container_id: &str) -> Result<WorkerOutcome, ContainerError> {
    let output = tokio::process::Command::new("docker")
        .args(["wait", container_id])
        .output()
        .await
        .map_err(ContainerError::Io)?;

    let raw = String::from_utf8_lossy(&output.stdout);
    let exit_code: i32 = raw.trim().parse().unwrap_or(1);

    Ok(WorkerOutcome { exit_code })
}

/// Clean up a completed worker container.
///
/// Removes the container from `active_containers`, returns the semaphore
/// slot, and runs `docker stop` + `docker rm -f` in a background task.
pub fn reap_worker(container_id: &str, state: &AppState) {
    // Return semaphore slot.
    state.policy_semaphore.add_permits(1);

    // Remove from registry.
    if let Ok(mut g) = state.active_containers.write() {
        g.remove(container_id);
    }

    // Fire-and-forget cleanup.
    let id = container_id.to_owned();
    drop(tokio::spawn(async move {
        docker_cmd::stop(&id).await;
        docker_cmd::rm_force(&[&id]).await;
        tracing::info!(container_id = %id, "worker container reaped");
    }));
}

/// Build a [`ContainerPolicy`] from a per-task override layered on `base`.
fn override_to_container_policy(
    ov: &lightarchitects::lightsquad::types::TaskPolicyOverride,
    base: &ContainerPolicy,
) -> ContainerPolicy {
    let mut p = base.clone();
    if let Some(iso) = ov.iso_mode {
        p.iso_mode = iso;
    }
    if let Some(net) = ov.network {
        p.network = net;
    }
    if let Some(mb) = ov.memory_mb {
        p.resources.memory_mb = mb;
    }
    if let Some(cpus) = ov.cpus {
        p.resources.cpus = cpus;
    }
    p
}

/// Sanitise a task ID for use in a Docker container name.
fn sanitize_id(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        .take(32)
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_slashes_and_spaces() {
        assert_eq!(sanitize_id("my task/id here"), "mytaskidhere");
    }

    #[test]
    fn sanitize_allows_valid_chars() {
        assert_eq!(sanitize_id("task-01_v2.0"), "task-01_v2.0");
    }

    #[test]
    fn sanitize_truncates_at_32() {
        let long = "a".repeat(50);
        assert_eq!(sanitize_id(&long).len(), 32);
    }
}
