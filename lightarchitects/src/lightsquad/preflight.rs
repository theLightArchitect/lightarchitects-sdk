//! 7-step LASDLC preflight checklist — Phase 4 scope: steps 1–3 and 6.
//!
//! Steps implemented in this phase:
//!
//! | Step | Name | Impl |
//! |------|------|------|
//! | 1 | Plan integrity | [`check_plan`] — unique IDs, non-empty tasks |
//! | 2 | Dependency graph | [`check_deps`] — topological sort, cycle detection, ID resolution |
//! | 3 | Repository safety | [`check_repo`] — clean working tree, no branch collision |
//! | 6 | Build state init | [`init_build_state`] — seed `SharedState` via `Coordinator` |
//!
//! Steps 4 (disk), 5 (API keys), and 7 (dry-run + APPROVE) land in Phase 5.
//!
//! # Design
//!
//! Every check returns a typed [`PreflightError`] rather than a free-form
//! string so that callers can pattern-match on specific failure classes.
//! All checks are synchronous (steps 1–2) or async (steps 3, 6).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use thiserror::Error;

use super::types::{BuildStatus, Coordinator, Task, TaskStatus};

/// Preflight check failure.
#[derive(Debug, Error)]
pub enum PreflightError {
    /// The task list is empty — nothing to build.
    #[error("task list is empty")]
    NoTasks,
    /// A task ID appears more than once.
    #[error("duplicate task ID: {id}")]
    DuplicateTaskId {
        /// The duplicated ID.
        id: String,
    },
    /// A `depends_on` entry references an ID that does not exist.
    #[error("task '{task}' depends on unknown ID '{dep}'")]
    UnknownDependency {
        /// Task that declares the bad dependency.
        task: String,
        /// The unresolvable dependency ID.
        dep: String,
    },
    /// The dependency graph has a cycle (autonomous progress is impossible).
    #[error("dependency cycle detected involving task: {task}")]
    DependencyCycle {
        /// One task in the cycle.
        task: String,
    },
    /// The git working tree is dirty (uncommitted or untracked changes).
    #[error("repository working tree is dirty — commit or stash changes first")]
    DirtyWorkingTree,
    /// The feat branch already exists and would collide with this build.
    #[error("branch '{branch}' already exists — remove it before starting")]
    BranchCollision {
        /// The colliding branch name.
        branch: String,
    },
    /// `git` could not be executed.
    #[error("failed to spawn git: {0}")]
    Spawn(#[source] std::io::Error),
    /// `git` exited non-zero unexpectedly.
    #[error("git check failed (exit {code}): {stderr}")]
    GitFailed {
        /// Exit code.
        code: i32,
        /// Captured stderr.
        stderr: String,
    },
}

// ── Step 1: Plan integrity ────────────────────────────────────────────────────

/// Verify task list integrity: non-empty, unique IDs.
///
/// Does not touch the filesystem. Call before any git operation.
///
/// # Errors
///
/// Returns [`PreflightError::NoTasks`] for an empty list, or
/// [`PreflightError::DuplicateTaskId`] for a repeated ID.
pub fn check_plan(tasks: &[Task]) -> Result<(), PreflightError> {
    if tasks.is_empty() {
        return Err(PreflightError::NoTasks);
    }
    let mut seen = HashSet::new();
    for task in tasks {
        if !seen.insert(&task.id) {
            return Err(PreflightError::DuplicateTaskId {
                id: task.id.clone(),
            });
        }
    }
    Ok(())
}

// ── Step 2: Dependency graph validation ──────────────────────────────────────

/// Validate the task dependency graph: all `depends_on` IDs exist, no cycles.
///
/// Uses Kahn's algorithm (BFS-based topological sort). A cycle is detected
/// when fewer nodes are processed than the total task count.
///
/// # Errors
///
/// Returns [`PreflightError::UnknownDependency`] if a dep ID is not found in
/// `tasks`, or [`PreflightError::DependencyCycle`] if a cycle is detected.
pub fn check_deps(tasks: &[Task]) -> Result<Vec<String>, PreflightError> {
    let ids: HashSet<&str> = tasks.iter().map(|t| t.id.as_str()).collect();

    // Validate all dep references resolve.
    for task in tasks {
        for dep in &task.depends_on {
            if !ids.contains(dep.as_str()) {
                return Err(PreflightError::UnknownDependency {
                    task: task.id.clone(),
                    dep: dep.clone(),
                });
            }
        }
    }

    // Kahn's algorithm: build in-degree map and adjacency list.
    let mut in_degree: HashMap<&str, usize> = tasks.iter().map(|t| (t.id.as_str(), 0)).collect();
    let mut dependents: HashMap<&str, Vec<&str>> =
        tasks.iter().map(|t| (t.id.as_str(), Vec::new())).collect();

    for task in tasks {
        in_degree
            .entry(task.id.as_str())
            .and_modify(|d| *d += task.depends_on.len());
        for dep in &task.depends_on {
            dependents
                .entry(dep.as_str())
                .or_default()
                .push(task.id.as_str());
        }
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter_map(|(&id, &d)| if d == 0 { Some(id) } else { None })
        .collect();
    let mut order = Vec::with_capacity(tasks.len());

    while let Some(id) = queue.pop_front() {
        order.push(id.to_owned());
        if let Some(deps) = dependents.get(id) {
            for &dependent in deps {
                let degree = in_degree.entry(dependent).or_default();
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    queue.push_back(dependent);
                }
            }
        }
    }

    if order.len() < tasks.len() {
        // Find a task still with in_degree > 0 for the error message.
        let cycle_task = in_degree
            .iter()
            .find(|&(_, &d)| d > 0)
            .map_or("unknown", |(&id, _)| id);
        return Err(PreflightError::DependencyCycle {
            task: cycle_task.to_owned(),
        });
    }

    Ok(order)
}

// ── Step 3: Repository safety ─────────────────────────────────────────────────

/// Verify git repository safety: clean working tree, no branch collision.
///
/// Does NOT check remote sync (that is G1/G2 in the LASDLC pre-flight script;
/// `check_repo` is the in-process soft check for common accidents).
///
/// # Errors
///
/// Returns [`PreflightError::DirtyWorkingTree`] if `git status --porcelain`
/// produces output, or [`PreflightError::BranchCollision`] if `feat_branch`
/// already exists.
pub async fn check_repo(repo_root: &Path, feat_branch: &str) -> Result<(), PreflightError> {
    check_clean_tree(repo_root).await?;
    check_no_branch_collision(repo_root, feat_branch).await?;
    Ok(())
}

async fn check_clean_tree(repo_root: &Path) -> Result<(), PreflightError> {
    let output = tokio::process::Command::new("git")
        .current_dir(repo_root)
        .args(["status", "--porcelain"])
        .output()
        .await
        .map_err(PreflightError::Spawn)?;

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(PreflightError::GitFailed { code, stderr });
    }

    if !output.stdout.is_empty() {
        return Err(PreflightError::DirtyWorkingTree);
    }
    Ok(())
}

async fn check_no_branch_collision(
    repo_root: &Path,
    feat_branch: &str,
) -> Result<(), PreflightError> {
    let output = tokio::process::Command::new("git")
        .current_dir(repo_root)
        .args(["branch", "--list", feat_branch])
        .output()
        .await
        .map_err(PreflightError::Spawn)?;

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(PreflightError::GitFailed { code, stderr });
    }

    if !output.stdout.is_empty() {
        return Err(PreflightError::BranchCollision {
            branch: feat_branch.to_owned(),
        });
    }
    Ok(())
}

// ── Step 6: Build state initialization ───────────────────────────────────────

/// Seed `SharedState` via `coordinator`: mark all tasks `Pending`, set build
/// status `Pending`, arm the `Notify` for the wave dispatcher.
///
/// This is the in-process half of step 6; the full step (canon loading, prompt
/// cache warm, decision ledger init) requires runtime deps wired in Phase 5.
pub async fn init_build_state(coordinator: &Coordinator, codename: &str, tasks: &[Task]) {
    let mut state = coordinator.state.write().await;
    for task in tasks {
        state.tasks.insert(task.id.clone(), TaskStatus::Pending);
    }
    state
        .builds
        .insert(codename.to_owned(), BuildStatus::Pending);
    drop(state);
    coordinator.notify.notify_waiters();
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::lightsquad::types::ContextTier;

    fn task(id: &str, deps: &[&str]) -> Task {
        Task {
            id: id.to_owned(),
            branch: format!("task/{id}"),
            depends_on: deps.iter().map(|s| (*s).to_owned()).collect(),
            file_ownership: vec![],
            concurrency_safe: false,
            context_tiers: vec![ContextTier {
                tier: "T1".to_owned(),
                label: "test".to_owned(),
                files: vec![],
                token_estimate: 0,
            }],
            prompt: "test".to_owned(),
            policy_override: None,
        }
    }

    // ── check_plan ────────────────────────────────────────────────────────────

    #[test]
    fn check_plan_empty_is_error() {
        assert!(matches!(check_plan(&[]), Err(PreflightError::NoTasks)));
    }

    #[test]
    fn check_plan_duplicate_id_is_error() {
        let tasks = vec![task("t1", &[]), task("t1", &[])];
        assert!(matches!(
            check_plan(&tasks),
            Err(PreflightError::DuplicateTaskId { .. })
        ));
    }

    #[test]
    fn check_plan_valid_passes() {
        let tasks = vec![task("t1", &[]), task("t2", &[])];
        assert!(check_plan(&tasks).is_ok());
    }

    // ── check_deps ────────────────────────────────────────────────────────────

    #[test]
    fn check_deps_unknown_dep_is_error() {
        let tasks = vec![task("t1", &["missing"])];
        assert!(matches!(
            check_deps(&tasks),
            Err(PreflightError::UnknownDependency { .. })
        ));
    }

    #[test]
    fn check_deps_cycle_is_error() {
        let tasks = vec![task("t1", &["t2"]), task("t2", &["t1"])];
        assert!(matches!(
            check_deps(&tasks),
            Err(PreflightError::DependencyCycle { .. })
        ));
    }

    #[test]
    fn check_deps_linear_chain_is_ok() {
        let tasks = vec![task("t1", &[]), task("t2", &["t1"]), task("t3", &["t2"])];
        let order = check_deps(&tasks).unwrap();
        // t1 must precede t2, t2 must precede t3.
        let pos: HashMap<_, _> = order
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        assert!(pos["t1"] < pos["t2"]);
        assert!(pos["t2"] < pos["t3"]);
    }

    #[test]
    fn check_deps_diamond_is_ok() {
        let tasks = vec![
            task("t1", &[]),
            task("t2", &["t1"]),
            task("t3", &["t1"]),
            task("t4", &["t2", "t3"]),
        ];
        assert!(check_deps(&tasks).is_ok());
    }

    // ── init_build_state ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn init_build_state_seeds_tasks_pending() {
        let coord = Coordinator::new();
        let tasks = vec![task("t1", &[]), task("t2", &[])];
        init_build_state(&coord, "test-build", &tasks).await;

        let state = coord.state.read().await;
        assert_eq!(state.tasks.get("t1"), Some(&TaskStatus::Pending));
        assert_eq!(state.tasks.get("t2"), Some(&TaskStatus::Pending));
        assert_eq!(state.builds.get("test-build"), Some(&BuildStatus::Pending));
    }
}
