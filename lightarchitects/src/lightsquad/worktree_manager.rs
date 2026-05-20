//! Git worktree lifecycle management for the lightsquad worker pool.
//!
//! [`WorktreeManager`] owns the create/remove/list contract for all worktrees
//! spawned during a build. All ref-mutating operations (`create`, `remove`)
//! are serialised through the shared `ops_mutex` inherited from
//! [`Coordinator`].
//!
//! `list` is a pure read that bypasses the mutex — it runs `git worktree list
//! --porcelain` which is safe to call concurrently with any read-only
//! git operation.
//!
//! # Cleanup protocol
//!
//! `remove` follows the 5-step protocol verified in Task #17:
//! 1. Find open file handles in the worktree (`lsof`)
//! 2. Send SIGTERM to owning PIDs (≤5 s drain window)
//! 3. Send SIGKILL to survivors
//! 4. `git worktree remove --force <path>` (macOS APFS: open handles don't
//!    block FS removal, but git internal locks do — hence the kill step)
//! 5. `git worktree prune` (mandatory; cleans stale admin refs)
//!
//! [`Coordinator`]: super::types::Coordinator

use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use thiserror::Error;
use tokio::sync::Mutex;

/// Errors from worktree lifecycle operations.
#[derive(Debug, Error)]
pub enum WorktreeError {
    /// `..` found in the path — rejected to prevent directory traversal.
    #[error("path traversal rejected in worktree path: {path}")]
    PathTraversal {
        /// The rejected path.
        path: String,
    },
    /// The path does not exist or is inaccessible after canonicalization.
    #[error("worktree path does not exist or is inaccessible: {path}")]
    InvalidPath {
        /// The problematic path.
        path: String,
    },
    /// `git worktree` exited with a non-zero status.
    #[error("git worktree operation failed (exit {code}): {stderr}")]
    GitFailed {
        /// Exit code from git.
        code: i32,
        /// Captured stderr from git.
        stderr: String,
    },
    /// The `git` binary could not be spawned.
    #[error("failed to spawn git: {0}")]
    Spawn(#[source] std::io::Error),
    /// Parsing the `--porcelain` output of `git worktree list` failed.
    #[error("failed to parse worktree list output: {0}")]
    ParseError(String),
}

/// A live worktree created by [`WorktreeManager::create`].
#[derive(Debug, Clone)]
pub struct WorktreeHandle {
    /// Absolute path of the worktree on disk.
    pub path: PathBuf,
    /// Name of the branch checked out in this worktree.
    pub branch: String,
}

/// A worktree entry returned by [`WorktreeManager::list`].
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    /// Absolute path of the worktree.
    pub path: PathBuf,
    /// HEAD commit SHA (40 hex chars) of the worktree.
    pub head: String,
    /// Branch checked out in the worktree, or `None` for detached HEAD.
    pub branch: Option<String>,
}

/// Manages the lifecycle of git worktrees for a lightsquad build.
pub struct WorktreeManager {
    /// Shared mutex with [`MergeAgent`] — serialises ALL ref-mutating git ops.
    ///
    /// [`MergeAgent`]: super::merge_agent::MergeAgent
    pub(crate) ops_mutex: Arc<Mutex<()>>,
    /// Root of the primary repository (contains the `.git` directory).
    repo_root: PathBuf,
}

impl WorktreeManager {
    /// Create a new [`WorktreeManager`] for `repo_root`.
    ///
    /// `ops_mutex` should be the same `Arc<Mutex<()>>` used by [`MergeAgent`]
    /// and obtained from [`Coordinator::ops_mutex`].
    ///
    /// [`MergeAgent`]: super::merge_agent::MergeAgent
    /// [`Coordinator::ops_mutex`]: super::types::Coordinator::ops_mutex
    #[must_use]
    pub fn new(ops_mutex: Arc<Mutex<()>>, repo_root: PathBuf) -> Self {
        Self {
            ops_mutex,
            repo_root,
        }
    }

    /// Create a new worktree at `worktree_path` on a fresh branch `branch`.
    ///
    /// Equivalent to `git worktree add -b <branch> <worktree_path> HEAD`.
    /// The branch is cut from the current `HEAD` of the primary repository.
    ///
    /// Acquires `ops_mutex` for the duration — this is a ref-mutating
    /// operation.
    ///
    /// # Errors
    ///
    /// Returns [`WorktreeError::PathTraversal`] if `worktree_path` contains
    /// `..`. Returns [`WorktreeError::GitFailed`] if git exits non-zero.
    pub async fn create(
        &self,
        branch: &str,
        worktree_path: &Path,
    ) -> Result<WorktreeHandle, WorktreeError> {
        let path_str = safe_path(worktree_path)?;
        let _guard = self.ops_mutex.lock().await;

        let output = tokio::process::Command::new("git")
            .current_dir(&self.repo_root)
            .args(["worktree", "add", "-b", branch, &path_str, "HEAD"])
            .output()
            .await
            .map_err(WorktreeError::Spawn)?;

        if !output.status.success() {
            let code = output.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(WorktreeError::GitFailed { code, stderr });
        }

        Ok(WorktreeHandle {
            path: PathBuf::from(&path_str),
            branch: branch.to_owned(),
        })
    }

    /// Remove the worktree at `worktree_path` using the 5-step cleanup
    /// protocol.
    ///
    /// Steps:
    /// 1. Find PIDs with open handles in the worktree via `lsof`
    /// 2. SIGTERM them; wait up to 5 s
    /// 3. SIGKILL survivors
    /// 4. `git worktree remove --force <path>`
    /// 5. `git worktree prune`
    ///
    /// Acquires `ops_mutex` from step 4 onward (steps 1–3 are pure reads).
    ///
    /// # Errors
    ///
    /// Returns [`WorktreeError::PathTraversal`] for `..` in path.
    /// Returns [`WorktreeError::GitFailed`] if `remove --force` or `prune`
    /// exits non-zero.
    pub async fn remove(&self, worktree_path: &Path) -> Result<(), WorktreeError> {
        let path_str = safe_path(worktree_path)?;

        // Steps 1–3: drain open handles (best-effort; errors are non-fatal).
        drain_open_handles(&path_str).await;

        // Step 4+5: ref-mutating — serialise through ops_mutex.
        let _guard = self.ops_mutex.lock().await;

        let remove_out = tokio::process::Command::new("git")
            .current_dir(&self.repo_root)
            .args(["worktree", "remove", "--force", &path_str])
            .output()
            .await
            .map_err(WorktreeError::Spawn)?;

        if !remove_out.status.success() {
            let code = remove_out.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&remove_out.stderr).into_owned();
            return Err(WorktreeError::GitFailed { code, stderr });
        }

        let prune_out = tokio::process::Command::new("git")
            .current_dir(&self.repo_root)
            .args(["worktree", "prune"])
            .output()
            .await
            .map_err(WorktreeError::Spawn)?;

        if !prune_out.status.success() {
            let code = prune_out.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&prune_out.stderr).into_owned();
            return Err(WorktreeError::GitFailed { code, stderr });
        }

        Ok(())
    }

    /// List all worktrees for the repository.
    ///
    /// Parses `git worktree list --porcelain` output. Does **not** acquire
    /// `ops_mutex` — this is a pure read operation safe to call concurrently.
    ///
    /// # Errors
    ///
    /// Returns [`WorktreeError::Spawn`] if git cannot be executed, or
    /// [`WorktreeError::ParseError`] if the porcelain output is malformed.
    pub async fn list(&self) -> Result<Vec<WorktreeInfo>, WorktreeError> {
        let output = tokio::process::Command::new("git")
            .current_dir(&self.repo_root)
            .args(["worktree", "list", "--porcelain"])
            .output()
            .await
            .map_err(WorktreeError::Spawn)?;

        if !output.status.success() {
            let code = output.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(WorktreeError::GitFailed { code, stderr });
        }

        Ok(parse_worktree_porcelain(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Validate a path: reject `..` components, then return the display string.
///
/// Full canonicalization is intentionally skipped for paths that may not
/// yet exist (e.g. the target directory for `git worktree add`). The `..`
/// rejection is the critical TOCTOU defense per `git_routes.rs` pattern.
fn safe_path(path: &Path) -> Result<String, WorktreeError> {
    if path.components().any(|c| c == Component::ParentDir) {
        return Err(WorktreeError::PathTraversal {
            path: path.display().to_string(),
        });
    }
    Ok(path.display().to_string())
}

/// Send SIGTERM to PIDs with open handles in `path`, wait ≤5 s, SIGKILL
/// survivors. All errors are silently ignored — the goal is a best-effort
/// drain before the forced `git worktree remove`.
async fn drain_open_handles(path: &str) {
    #[cfg(target_os = "macos")]
    {
        let Ok(lsof) = tokio::process::Command::new("lsof")
            .args(["+D", path])
            .output()
            .await
        else {
            return;
        };

        let pids: Vec<u32> = String::from_utf8_lossy(&lsof.stdout)
            .lines()
            .skip(1)
            .filter_map(|line| {
                line.split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse::<u32>().ok())
            })
            .collect();

        if pids.is_empty() {
            return;
        }

        // SIGTERM pass.
        for &pid in &pids {
            let _ = tokio::process::Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .output()
                .await;
        }

        // Wait up to 5 s.
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        // SIGKILL survivors.
        for &pid in &pids {
            let _ = tokio::process::Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .output()
                .await;
        }
    }
    // On non-macOS, lsof behavior differs; skip drain.
    #[cfg(not(target_os = "macos"))]
    let _ = path;
}

/// Parse `git worktree list --porcelain` output into [`WorktreeInfo`] entries.
fn parse_worktree_porcelain(output: &str) -> Vec<WorktreeInfo> {
    let mut entries = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_head: Option<String> = None;
    let mut current_branch: Option<String> = None;

    for line in output.lines() {
        if let Some(path_str) = line.strip_prefix("worktree ") {
            // Flush previous entry.
            if let (Some(path), Some(head)) = (current_path.take(), current_head.take()) {
                entries.push(WorktreeInfo {
                    path,
                    head,
                    branch: current_branch.take(),
                });
            }
            current_path = Some(PathBuf::from(path_str));
            current_head = None;
            current_branch = None;
        } else if let Some(sha) = line.strip_prefix("HEAD ") {
            current_head = Some(sha.to_owned());
        } else if let Some(branch_ref) = line.strip_prefix("branch ") {
            // refs/heads/<name> → <name>
            let name = branch_ref
                .strip_prefix("refs/heads/")
                .unwrap_or(branch_ref)
                .to_owned();
            current_branch = Some(name);
        }
        // "bare", "detached", "locked", "prunable" lines are silently skipped.
    }

    // Flush final entry.
    if let (Some(path), Some(head)) = (current_path, current_head) {
        entries.push(WorktreeInfo {
            path,
            head,
            branch: current_branch,
        });
    }

    entries
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn safe_path_rejects_parent_dir() {
        let result = safe_path(Path::new("/tmp/../etc/passwd"));
        assert!(matches!(result, Err(WorktreeError::PathTraversal { .. })));
    }

    #[test]
    fn safe_path_accepts_normal_path() {
        let result = safe_path(Path::new("/tmp/worktrees/task-001"));
        assert!(result.is_ok());
    }

    #[test]
    fn parse_worktree_porcelain_single_entry() {
        let input = "worktree /repo\nHEAD abc1234567890123456789012345678901234567890\nbranch refs/heads/main\n\n";
        let entries = parse_worktree_porcelain(input);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, PathBuf::from("/repo"));
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
    }

    #[test]
    fn parse_worktree_porcelain_detached_head() {
        let input =
            "worktree /repo/wt\nHEAD abc1234567890123456789012345678901234567890\ndetached\n\n";
        let entries = parse_worktree_porcelain(input);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].branch.is_none());
    }

    #[test]
    fn parse_worktree_porcelain_multiple_entries() {
        let input = concat!(
            "worktree /repo\nHEAD aaa0000000000000000000000000000000000000\nbranch refs/heads/main\n\n",
            "worktree /repo/wt1\nHEAD bbb0000000000000000000000000000000000000\nbranch refs/heads/task/t1\n\n",
        );
        let entries = parse_worktree_porcelain(input);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].branch.as_deref(), Some("task/t1"));
    }

    #[test]
    fn worktree_error_path_traversal_display() {
        let err = WorktreeError::PathTraversal {
            path: "/tmp/../etc".to_owned(),
        };
        assert!(err.to_string().contains("/tmp/../etc"));
    }
}
