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

use std::{path::Path, sync::Arc, time::Instant};

use lightarchitects::{
    container_spawn::{ContainerPolicy, NetworkPolicy, SpawnPolicy},
    lightsquad::wave_dispatcher::WorkerSpec,
};

use crate::{
    container::{
        docker_cmd,
        spawner::{build_container_run_args, network_str},
        types::{ActiveContainerEntry, ContainerError, ContainerHandle, ContainerKind},
    },
    server::AppState,
};

// ── SERAPH C1 — pre-merge secret scan ────────────────────────────────────────

/// A secret pattern found in a staged diff (SERAPH T7 threat mitigation).
#[derive(Debug)]
pub struct SecretScanViolation {
    /// Short identifier for the matched denylist pattern.
    pub pattern_id: &'static str,
    /// File where the pattern was found, relative to the worktree root.
    pub file: std::path::PathBuf,
    /// 1-based line number within the `git show HEAD` diff output.
    pub line: u32,
    /// First 20 characters of the matched region followed by `"..."`.
    /// Never the full secret value.
    pub redacted_excerpt: String,
}

impl std::fmt::Display for SecretScanViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "secret pattern '{}' at {}:{} — {}",
            self.pattern_id,
            self.file.display(),
            self.line,
            self.redacted_excerpt,
        )
    }
}

/// SERAPH C1 — pre-merge secret scan.
///
/// Runs `git show HEAD --patch --format=` in `worktree` and scans every
/// added line for patterns from the T7 denylist:
/// `LA_LITELLM_API_KEY=`, `LITELLM_API_KEY=`, `API_KEY=`,
/// `sk-[A-Za-z0-9]{20,}`, `Bearer [A-Za-z0-9_-]{20,}`.
///
/// Returns the first [`SecretScanViolation`] found, or `Ok(())` if the diff
/// is clean. Fails open on git errors — a missing binary or empty worktree
/// never blocks a build unintentionally.
///
/// # Redaction
///
/// The `redacted_excerpt` in the returned violation contains only the first
/// 20 characters of the matched region followed by `"..."`. The full secret
/// value is never stored, logged, or transmitted.
///
/// # Errors
///
/// Returns [`Err(SecretScanViolation)`] when a denylist pattern is found in
/// the diff. All git I/O failures (missing binary, empty repo) return `Ok(())`
/// — the scan fails open rather than blocking a build on a git error.
pub fn scan_staged_diff_for_secrets(
    worktree: &Path,
    _branch: &str,
) -> Result<(), SecretScanViolation> {
    let Ok(output) = std::process::Command::new("git")
        .args(["show", "HEAD", "--patch", "--format="])
        .current_dir(worktree)
        .output()
    else {
        return Ok(());
    };

    if !output.status.success() {
        return Ok(());
    }

    let diff = String::from_utf8_lossy(&output.stdout);
    let mut current_file = std::path::PathBuf::new();

    for (idx, line) in diff.lines().enumerate() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            current_file = std::path::PathBuf::from(path);
            continue;
        }
        if !line.starts_with('+') || line.starts_with("+++") {
            continue;
        }
        let content = &line[1..];
        let line_num = u32::try_from(idx + 1).unwrap_or(u32::MAX);

        if let Some(v) = check_diff_line(content, &current_file, line_num) {
            return Err(v);
        }
    }

    Ok(())
}

/// Checks a single diff line (`+` prefix already stripped) against all
/// denylist patterns. Returns the first violation found, or `None`.
fn check_diff_line(content: &str, file: &Path, line_num: u32) -> Option<SecretScanViolation> {
    // Fixed key=value patterns — any occurrence is a violation.
    const FIXED: &[(&str, &str)] = &[
        ("LA_LITELLM_API_KEY", "LA_LITELLM_API_KEY="),
        ("LITELLM_API_KEY", "LITELLM_API_KEY="),
        ("API_KEY", "API_KEY="),
    ];
    for &(id, needle) in FIXED {
        if let Some(pos) = content.find(needle) {
            return Some(SecretScanViolation {
                pattern_id: id,
                file: file.to_path_buf(),
                line: line_num,
                redacted_excerpt: redact_at(content, pos),
            });
        }
    }

    // `sk-` followed by ≥20 alphanumeric chars (API key prefix pattern).
    if let Some(pos) = content.find("sk-") {
        let suffix = &content[pos + 3..];
        if suffix.len() >= 20 && suffix.chars().take(20).all(|c| c.is_ascii_alphanumeric()) {
            return Some(SecretScanViolation {
                pattern_id: "sk-prefix",
                file: file.to_path_buf(),
                line: line_num,
                redacted_excerpt: redact_at(content, pos),
            });
        }
    }

    // `Bearer ` followed by ≥20 alphanumeric-or-dash-or-underscore chars.
    if let Some(pos) = content.find("Bearer ") {
        let suffix = &content[pos + 7..];
        if suffix.len() >= 20
            && suffix
                .chars()
                .take(20)
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        {
            return Some(SecretScanViolation {
                pattern_id: "bearer-token",
                file: file.to_path_buf(),
                line: line_num,
                redacted_excerpt: redact_at(content, pos),
            });
        }
    }

    None
}

/// Returns the first 20 characters of `s[pos..]` followed by `"..."`.
/// Never returns more than 20 characters of potential secret material.
fn redact_at(s: &str, pos: usize) -> String {
    let excerpt = &s[pos..];
    if excerpt.len() > 20 {
        format!("{}...", &excerpt[..20])
    } else {
        format!("{excerpt}...")
    }
}

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

    // SERAPH C2 (T8): worker tasks always run on la-worker-bridge for namespace
    // isolation from PTY session containers, regardless of the system base policy.
    let mut worker_policy = (*effective_policy).clone();
    worker_policy.network = NetworkPolicy::WorkerBridge;
    let effective_policy = Arc::new(worker_policy);

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
    let network_policy_at_spawn = network_str(effective_policy.network);
    permit.forget();

    let entry = ActiveContainerEntry {
        kind: ContainerKind::WorkerTask {
            task_id: spec.task.id.clone(),
            wave_index: spec.wave_index,
        },
        started_at: Instant::now(),
        policy_snapshot_iso_mode: iso_mode,
        network_policy_at_spawn,
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
    let output = tokio::process::Command::new(crate::container::docker_cmd::docker_bin())
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

    // ── SERAPH C1 — scan_staged_diff_for_secrets unit tests ──────────────────

    #[test]
    fn check_diff_line_detects_la_litellm_key() {
        let v = check_diff_line(
            "  export LA_LITELLM_API_KEY=sk-realvalue123",
            std::path::Path::new(".env"),
            7,
        );
        assert!(v.is_some());
        let v = v.unwrap();
        assert_eq!(v.pattern_id, "LA_LITELLM_API_KEY");
        assert!(v.redacted_excerpt.ends_with("..."));
    }

    #[test]
    fn check_diff_line_detects_litellm_key() {
        let v = check_diff_line(
            "LITELLM_API_KEY=verysecretvalue",
            std::path::Path::new("cfg.py"),
            1,
        );
        assert!(v.is_some());
        assert_eq!(v.unwrap().pattern_id, "LITELLM_API_KEY");
    }

    #[test]
    fn check_diff_line_detects_sk_prefix_with_long_suffix() {
        let v = check_diff_line(
            "const KEY = \"sk-abcdefghijklmnopqrstuvwxyz\";",
            std::path::Path::new("src/lib.rs"),
            42,
        );
        assert!(v.is_some());
        assert_eq!(v.unwrap().pattern_id, "sk-prefix");
    }

    #[test]
    fn check_diff_line_ignores_short_sk_suffix() {
        let v = check_diff_line(
            "let x = \"sk-short\";",
            std::path::Path::new("src/lib.rs"),
            1,
        );
        assert!(v.is_none(), "sk- with < 20 chars should not trigger");
    }

    #[test]
    fn check_diff_line_detects_bearer_with_long_token() {
        let v = check_diff_line(
            "Authorization: Bearer abcdefghijklmnopqrstuvwxyz",
            std::path::Path::new("http.rs"),
            5,
        );
        assert!(v.is_some());
        assert_eq!(v.unwrap().pattern_id, "bearer-token");
    }

    #[test]
    fn check_diff_line_ignores_short_bearer() {
        let v = check_diff_line(
            "Authorization: Bearer tooshort",
            std::path::Path::new("x.rs"),
            1,
        );
        assert!(v.is_none(), "Bearer with < 20 chars should not trigger");
    }

    #[test]
    fn redact_at_truncates_long_string() {
        let s = "ABC123456789012345678901234567890";
        let r = redact_at(s, 0);
        assert!(r.ends_with("..."), "should end with ...");
        assert_eq!(r.len(), 23, "20 chars + 3 for ...");
    }

    #[test]
    fn redact_at_short_string_gets_ellipsis() {
        let r = redact_at("short", 0);
        assert!(r.ends_with("..."));
    }

    /// SERAPH T7 regression: planting a fake `LiteLLM` key in a diff line triggers
    /// the scan.
    #[test]
    fn scan_staged_detects_litellm_key_in_diff_line() {
        let violation = check_diff_line(
            "LA_LITELLM_API_KEY=sk-fake12345verylongsecretvalue",
            std::path::Path::new("config.env"),
            1,
        );
        assert!(violation.is_some(), "T7 regression: secret not detected");
        let v = violation.unwrap();
        // Redacted excerpt must NOT contain more than 20 chars of the secret.
        assert!(
            v.redacted_excerpt.len() <= 23,
            "redacted excerpt leaks too much: {}",
            v.redacted_excerpt
        );
    }

    #[test]
    fn scan_staged_diff_returns_ok_for_non_existent_path() {
        // Non-existent worktree — should fail open, not panic.
        let result =
            scan_staged_diff_for_secrets(std::path::Path::new("/tmp/does-not-exist-xyz"), "main");
        assert!(result.is_ok(), "should fail open on git error");
    }
}
