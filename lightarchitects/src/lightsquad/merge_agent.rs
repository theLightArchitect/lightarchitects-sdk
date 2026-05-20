//! Merge agent — serialises all ref-mutating git operations through a shared mutex.
//!
//! [`MergeAgent`] owns the merge and branch-cut contract for a lightsquad build.
//! All operations that mutate refs (`merge`, `branch -d`) acquire `ops_mutex`
//! (an `Arc<Mutex<()>>` shared with [`WorktreeManager`] and [`Coordinator`]).
//!
//! Read-only operations (`rev-parse`, `log --oneline`) bypass the mutex because
//! git is safe for concurrent reads.
//!
//! # Merge strategy
//!
//! `merge_task_to_feat` uses `--no-ff` (not `--ff-only`). After the first task
//! branch merges, the feat branch advances past the common ancestor; subsequent
//! task branches cannot be fast-forwarded. `--no-ff` is always correct here.
//!
//! # Lock contention
//!
//! `.git/index.lock` contention is transient — another git process holds it
//! briefly. [`MergeAgent`] retries up to [`MAX_RETRY_ATTEMPTS`] times with
//! jittered exponential backoff (base [`BASE_RETRY_MS`] ms, cap
//! [`MAX_RETRY_MS`] ms) before returning [`MergeError::RetryExhausted`].
//!
//! [`WorktreeManager`]: super::worktree_manager::WorktreeManager
//! [`Coordinator`]: super::types::Coordinator

use std::{
    path::{Component, PathBuf},
    sync::Arc,
    time::Duration,
};

use thiserror::Error;
use tokio::sync::Mutex;

// ── Retry constants ───────────────────────────────────────────────────────────

const MAX_RETRY_ATTEMPTS: u32 = 5;
const BASE_RETRY_MS: u64 = 50;
const MAX_RETRY_MS: u64 = 2_000;

/// Marker string in git stderr indicating `.git/index.lock` contention.
const LOCK_COLLISION_MARKER: &str = "Unable to create '";

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors from merge and branch operations.
#[derive(Debug, Error)]
pub enum MergeError {
    /// A merge conflict — the task branch could not auto-merge.
    #[error("merge conflict on branch: {branch}")]
    Conflict {
        /// The branch that could not be merged.
        branch: String,
    },
    /// `git` exited with a non-zero status.
    #[error("git operation failed (exit {code}): {stderr}")]
    GitFailed {
        /// Exit code.
        code: i32,
        /// Captured stderr.
        stderr: String,
    },
    /// `.git/index.lock` exists — transient contention on a single attempt.
    #[error("git lock contention — retry")]
    LockContention,
    /// All retry attempts exhausted due to persistent lock contention.
    #[error("git lock contention persisted after {attempts} retries")]
    RetryExhausted {
        /// Number of attempts made.
        attempts: u32,
    },
    /// The `git` binary could not be spawned.
    #[error("failed to spawn git: {0}")]
    Io(#[source] std::io::Error),
    /// `..` found in the branch name — rejected to prevent path traversal.
    #[error("path traversal rejected in branch name: {branch}")]
    PathTraversal {
        /// The rejected name.
        branch: String,
    },
}

// ── Retry policy ─────────────────────────────────────────────────────────────

/// Jittered exponential backoff for git lock contention.
#[derive(Debug, Clone)]
struct RetryPolicy {
    max_attempts: u32,
    base_ms: u64,
    max_ms: u64,
}

impl RetryPolicy {
    fn sleep_for(&self, attempt: u32) -> Duration {
        let exp_ms = self.base_ms.saturating_mul(1u64 << attempt.min(62));
        let capped = exp_ms.min(self.max_ms);
        // Add up to 25 % jitter to spread concurrent retriers.
        let jitter = (capped / 4).max(1);
        let ms = capped + (u64::from(attempt) % jitter);
        Duration::from_millis(ms)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: MAX_RETRY_ATTEMPTS,
            base_ms: BASE_RETRY_MS,
            max_ms: MAX_RETRY_MS,
        }
    }
}

// ── MergeAgent ────────────────────────────────────────────────────────────────

/// Serialises all ref-mutating git operations for a lightsquad build.
pub struct MergeAgent {
    /// Shared with [`WorktreeManager`] — only one holder at a time can mutate refs.
    ///
    /// [`WorktreeManager`]: super::worktree_manager::WorktreeManager
    pub(crate) ops_mutex: Arc<Mutex<()>>,
    /// Root of the primary repository (contains `.git`).
    repo_root: PathBuf,
    retry_policy: RetryPolicy,
}

impl MergeAgent {
    /// Create a new [`MergeAgent`] for `repo_root`.
    ///
    /// `ops_mutex` must be the same `Arc<Mutex<()>>` used by
    /// [`WorktreeManager`] (obtained from [`Coordinator::ops_mutex`]).
    ///
    /// [`WorktreeManager`]: super::worktree_manager::WorktreeManager
    /// [`Coordinator::ops_mutex`]: super::types::Coordinator::ops_mutex
    #[must_use]
    pub fn new(ops_mutex: Arc<Mutex<()>>, repo_root: PathBuf) -> Self {
        Self {
            ops_mutex,
            repo_root,
            retry_policy: RetryPolicy::default(),
        }
    }

    /// Merge `task_branch` into `feat_branch` using `--no-ff`.
    ///
    /// Steps:
    /// 1. Validate both branch names (reject `..`).
    /// 2. Acquire `ops_mutex`.
    /// 3. `git checkout <feat_branch>`
    /// 4. `git merge --no-ff --no-edit <task_branch>` with retry on lock contention.
    ///
    /// # Errors
    ///
    /// Returns [`MergeError::Conflict`] on merge conflicts,
    /// [`MergeError::GitFailed`] on other non-zero exits, and
    /// [`MergeError::RetryExhausted`] if lock contention persists.
    pub async fn merge_task_to_feat(
        &self,
        task_branch: &str,
        feat_branch: &str,
    ) -> Result<(), MergeError> {
        validate_branch_name(task_branch)?;
        validate_branch_name(feat_branch)?;

        let _guard = self.ops_mutex.lock().await;

        // Checkout feat branch first.
        run_git(&self.repo_root, &["checkout", feat_branch])
            .await
            .map_err(|e| match e {
                MergeError::LockContention => MergeError::LockContention,
                other => other,
            })?;

        // Merge with retry on lock contention.
        let task_branch = task_branch.to_owned();
        let repo_root = self.repo_root.clone();
        let policy = self.retry_policy.clone();

        let mut last_err = MergeError::LockContention;
        for attempt in 0..policy.max_attempts {
            match run_git(&repo_root, &["merge", "--no-ff", "--no-edit", &task_branch]).await {
                Ok(()) => return Ok(()),
                Err(MergeError::LockContention) => {
                    last_err = MergeError::LockContention;
                    tokio::time::sleep(policy.sleep_for(attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }

        // Exhausted retries.
        let _ = last_err;
        Err(MergeError::RetryExhausted {
            attempts: policy.max_attempts,
        })
    }

    /// Delete `branch` after a successful merge.
    ///
    /// Acquires `ops_mutex`. Non-fatal if the branch doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns [`MergeError::GitFailed`] if `git branch -d` exits non-zero
    /// for a reason other than "branch not found".
    pub async fn delete_branch(&self, branch: &str) -> Result<(), MergeError> {
        validate_branch_name(branch)?;
        let _guard = self.ops_mutex.lock().await;
        match run_git(&self.repo_root, &["branch", "-d", branch]).await {
            Ok(()) => Ok(()),
            Err(MergeError::GitFailed { stderr, .. })
                if stderr.contains("not found") || stderr.contains("no branch named") =>
            {
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Return the tree SHA of `HEAD` without acquiring the mutex.
    ///
    /// Safe to call concurrently — `rev-parse` is a pure read.
    ///
    /// # Errors
    ///
    /// Returns [`MergeError::GitFailed`] if git exits non-zero.
    pub async fn rev_parse_head_tree(&self) -> Result<String, MergeError> {
        let output = tokio::process::Command::new("git")
            .current_dir(&self.repo_root)
            .args(["rev-parse", "HEAD^{tree}"])
            .output()
            .await
            .map_err(MergeError::Io)?;

        if !output.status.success() {
            let code = output.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(MergeError::GitFailed { code, stderr });
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Run `git <args>` in `repo_root` once; classify exit codes.
///
/// Returns [`MergeError::LockContention`] for transient `.git/index.lock`
/// contention, [`MergeError::Conflict`] for merge conflicts, and
/// [`MergeError::GitFailed`] for all other non-zero exits.
async fn run_git(repo_root: &PathBuf, args: &[&str]) -> Result<(), MergeError> {
    let output = tokio::process::Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .await
        .map_err(MergeError::Io)?;

    if output.status.success() {
        return Ok(());
    }

    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    if stderr.contains(LOCK_COLLISION_MARKER) {
        return Err(MergeError::LockContention);
    }

    if code == 1 && stderr.contains("CONFLICT") {
        let branch = args.last().copied().unwrap_or("unknown").to_owned();
        return Err(MergeError::Conflict { branch });
    }

    Err(MergeError::GitFailed { code, stderr })
}

/// Reject branch names containing `..` (path traversal vector).
fn validate_branch_name(name: &str) -> Result<(), MergeError> {
    // `..` in a branch name is invalid git syntax AND a traversal risk.
    if std::path::Path::new(name)
        .components()
        .any(|c| c == Component::ParentDir)
        || name.contains("..")
    {
        return Err(MergeError::PathTraversal {
            branch: name.to_owned(),
        });
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn validate_branch_name_rejects_dotdot() {
        assert!(matches!(
            validate_branch_name("feat/../etc"),
            Err(MergeError::PathTraversal { .. })
        ));
    }

    #[test]
    fn validate_branch_name_rejects_bare_dotdot() {
        assert!(matches!(
            validate_branch_name(".."),
            Err(MergeError::PathTraversal { .. })
        ));
    }

    #[test]
    fn validate_branch_name_accepts_normal() {
        assert!(validate_branch_name("feat/ironclaw-spine").is_ok());
        assert!(validate_branch_name("task/build/task-001").is_ok());
    }

    #[test]
    fn retry_policy_sleep_increases() {
        let p = RetryPolicy::default();
        let d0 = p.sleep_for(0);
        let d1 = p.sleep_for(1);
        let d2 = p.sleep_for(2);
        assert!(d1 > d0, "backoff must grow");
        assert!(d2 > d1, "backoff must grow");
        assert!(d2.as_millis() <= u128::from(MAX_RETRY_MS) + 100, "capped");
    }

    #[test]
    fn retry_policy_respects_cap() {
        let p = RetryPolicy::default();
        let large = p.sleep_for(30);
        assert!(large.as_millis() <= u128::from(MAX_RETRY_MS) + 600);
    }

    #[test]
    fn merge_error_conflict_display() {
        let e = MergeError::Conflict {
            branch: "task/t1".to_owned(),
        };
        assert!(e.to_string().contains("task/t1"));
    }

    #[test]
    fn merge_error_retry_exhausted_display() {
        let e = MergeError::RetryExhausted { attempts: 5 };
        assert!(e.to_string().contains('5'));
    }

    #[test]
    fn merge_error_lock_contention_classified() {
        let stderr = "error: Unable to create '/repo/.git/index.lock': File exists.".to_owned();
        assert!(stderr.contains(LOCK_COLLISION_MARKER));
    }

    #[test]
    fn new_merge_agent_is_constructible() {
        let mutex = Arc::new(Mutex::new(()));
        let agent = MergeAgent::new(mutex, PathBuf::from("/tmp/test-repo"));
        assert_eq!(agent.repo_root, PathBuf::from("/tmp/test-repo"));
    }
}
