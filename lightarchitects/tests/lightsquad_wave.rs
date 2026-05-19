//! Integration tests for lightsquad wave dispatch.
//!
//! Verifies the full create-worktree → worker-commit → merge-back-to-feat
//! cycle against a real (in-process tempdir) git repository, using a mock
//! worker that does `git add + commit` instead of invoking Claude CLI.
//!
//! # Setup
//!
//! Each test creates an isolated `TempDir` repo:
//! ```text
//! <repo>/           ← primary repo (on feat/<codename>)
//! <worktrees>/      ← separate dir; per-task worktrees land here
//! ```
//!
//! The mock worker writes a unique file (`<task_id>.txt`) and commits it in
//! the task's worktree, then returns `Ok(())`. `dispatch_wave` merges the
//! result back to `feat/<codename>` via `MergeAgent`.

#![cfg(feature = "lightsquad")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use lightarchitects::lightsquad::{
    merge_agent::MergeAgent,
    preflight::{check_deps, check_plan, init_build_state},
    types::{BuildStatus, Coordinator, Task, TaskStatus},
    wave_dispatcher::{WorkerSpec, dispatch_wave},
    worktree_manager::WorktreeManager,
};
use tempfile::TempDir;

// ── Git helpers ───────────────────────────────────────────────────────────────

async fn git(dir: &Path, args: &[&str]) {
    let status = tokio::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .await
        .expect("git spawn failed");
    assert!(status.success(), "git {args:?} failed");
}

/// Bootstrap a minimal git repo: init, configure identity, empty initial commit.
async fn init_repo(dir: &Path) {
    git(dir, &["init", "-b", "main"]).await;
    git(dir, &["config", "user.email", "test@lightsquad.test"]).await;
    git(dir, &["config", "user.name", "Lightsquad Test"]).await;
    git(dir, &["commit", "--allow-empty", "-m", "init"]).await;
}

/// Create and checkout a branch.
async fn checkout_branch(dir: &Path, branch: &str) {
    git(dir, &["checkout", "-b", branch]).await;
}

fn make_task(id: &str) -> Task {
    Task {
        id: id.to_owned(),
        branch: format!("task/build/{id}"),
        depends_on: vec![],
        context_tiers: vec![],
        prompt: format!("implement {id}"),
    }
}

fn make_task_with_dep(id: &str, dep: &str) -> Task {
    Task {
        id: id.to_owned(),
        branch: format!("task/build/{id}"),
        depends_on: vec![dep.to_owned()],
        context_tiers: vec![],
        prompt: format!("implement {id}"),
    }
}

// ── Preflight unit tests (fast, no git) ───────────────────────────────────────

#[test]
fn preflight_check_plan_rejects_empty() {
    assert!(check_plan(&[]).is_err());
}

#[test]
fn preflight_check_deps_detects_cycle() {
    let tasks = vec![make_task_with_dep("a", "b"), make_task_with_dep("b", "a")];
    assert!(check_deps(&tasks).is_err());
}

#[test]
fn preflight_check_deps_resolves_diamond() {
    let tasks = vec![
        make_task("root"),
        make_task_with_dep("left", "root"),
        make_task_with_dep("right", "root"),
        {
            let mut t = make_task("tip");
            t.depends_on = vec!["left".to_owned(), "right".to_owned()];
            t
        },
    ];
    let order = check_deps(&tasks).unwrap();
    // root must come before left and right; left+right before tip.
    let pos: std::collections::HashMap<_, _> = order
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    assert!(pos["root"] < pos["left"]);
    assert!(pos["root"] < pos["right"]);
    assert!(pos["left"] < pos["tip"]);
    assert!(pos["right"] < pos["tip"]);
}

// ── Wave dispatch integration tests ──────────────────────────────────────────

/// 3 independent tasks in a single wave — all should succeed and merge.
#[tokio::test]
async fn dispatch_wave_three_independent_tasks_all_succeed() {
    let repo_dir = TempDir::new().unwrap();
    let wt_dir = TempDir::new().unwrap();
    let repo = repo_dir.path().to_path_buf();
    let wt_root = wt_dir.path().to_path_buf();
    let feat_branch = "feat/test-three-tasks";

    init_repo(&repo).await;
    checkout_branch(&repo, feat_branch).await;

    let tasks: Vec<Task> = ["t1", "t2", "t3"].iter().map(|id| make_task(id)).collect();

    let coordinator = Coordinator::new();
    init_build_state(&coordinator, "test-three-tasks", &tasks).await;

    let ops_mutex = coordinator.ops_mutex.clone();
    let wm = WorktreeManager::new(ops_mutex.clone(), repo.clone());
    let ma = MergeAgent::new(ops_mutex, repo.clone());

    let result = dispatch_wave(
        &tasks,
        &coordinator,
        &wm,
        &ma,
        feat_branch,
        &wt_root,
        mock_worker,
    )
    .await;

    assert!(result.is_ok(), "dispatch_wave failed: {result:?}");
    let summary = result.unwrap();
    assert_eq!(summary.succeeded, 3);
    assert_eq!(summary.failed, 0);

    // Verify all tasks are Complete in shared state.
    let state = coordinator.state.read().await;
    for task in &tasks {
        assert_eq!(
            state.tasks.get(&task.id),
            Some(&TaskStatus::Complete),
            "task {} not Complete",
            task.id
        );
    }

    // Verify build result is seeded in state (Program.run sets it; here just check tasks).
    drop(state);
}

/// `init_build_state` seeds Pending and sets build to Pending.
#[tokio::test]
async fn init_build_state_seeds_coordinator() {
    let tasks = vec![make_task("x"), make_task("y")];
    let coord = Coordinator::new();
    init_build_state(&coord, "my-build", &tasks).await;

    let state = coord.state.read().await;
    assert_eq!(state.tasks.get("x"), Some(&TaskStatus::Pending));
    assert_eq!(state.tasks.get("y"), Some(&TaskStatus::Pending));
    assert_eq!(state.builds.get("my-build"), Some(&BuildStatus::Pending));
}

/// A wave with a failing worker sets the task Failed and returns [`WaveError`].
#[tokio::test]
async fn dispatch_wave_failing_worker_marks_task_failed() {
    let repo_dir = TempDir::new().unwrap();
    let wt_dir = TempDir::new().unwrap();
    let repo = repo_dir.path().to_path_buf();
    let wt_root = wt_dir.path().to_path_buf();
    let feat_branch = "feat/test-fail";

    init_repo(&repo).await;
    checkout_branch(&repo, feat_branch).await;

    let tasks = vec![make_task("fail-task")];

    let coordinator = Coordinator::new();
    init_build_state(&coordinator, "test-fail", &tasks).await;

    let ops_mutex = coordinator.ops_mutex.clone();
    let wm = WorktreeManager::new(ops_mutex.clone(), repo.clone());
    let ma = MergeAgent::new(ops_mutex, repo.clone());

    let result = dispatch_wave(
        &tasks,
        &coordinator,
        &wm,
        &ma,
        feat_branch,
        &wt_root,
        |_spec| async { Err("intentional test failure".to_owned()) },
    )
    .await;

    assert!(result.is_err(), "expected WaveError but got Ok");

    let state = coordinator.state.read().await;
    assert_eq!(
        state.tasks.get("fail-task"),
        Some(&TaskStatus::Failed),
        "failed task should be marked Failed"
    );
}

// ── Mock worker ───────────────────────────────────────────────────────────────

/// Mock worker: writes `<task_id>.txt` in the worktree and commits it.
async fn mock_worker(spec: WorkerSpec) -> Result<(), String> {
    let task_id = spec.task.id.clone();
    let wt = &spec.worktree_path;

    // Write a unique file so the commit has real content.
    std::fs::write(wt.join(format!("{task_id}.txt")), task_id.as_bytes())
        .map_err(|e| format!("write file: {e}"))?;

    // Stage.
    tokio::process::Command::new("git")
        .current_dir(wt)
        .args(["add", "."])
        .status()
        .await
        .map_err(|e| format!("git add spawn: {e}"))?;

    // Commit.
    let status = tokio::process::Command::new("git")
        .current_dir(wt)
        .args([
            "-c",
            "user.email=mock@test",
            "-c",
            "user.name=Mock",
            "commit",
            "-m",
            &format!("task {task_id} complete"),
        ])
        .status()
        .await
        .map_err(|e| format!("git commit spawn: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("git commit failed for {task_id}"))
    }
}
