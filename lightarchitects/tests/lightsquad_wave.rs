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
    wave_dispatcher::{SLOT_CAPACITY, WorkerSpec, dispatch_wave, read_slot_capacity},
    worktree_manager::WorktreeManager,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
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
        file_ownership: vec![],
        concurrency_safe: false,
        context_tiers: vec![],
        prompt: format!("implement {id}"),
    }
}

fn make_task_with_dep(id: &str, dep: &str) -> Task {
    Task {
        id: id.to_owned(),
        branch: format!("task/build/{id}"),
        depends_on: vec![dep.to_owned()],
        file_ownership: vec![],
        concurrency_safe: false,
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
        0,
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
        0,
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

// ── Dual-pool concurrency — peak counter integration tests ──────────────────

/// Concurrent peak-counting mock worker for the dual-pool tests.
///
/// Each invocation:
/// 1. Atomically increments the active counter and bumps the peak if needed
/// 2. Holds the slot for `hold_ms` so peak overlap accumulates beyond the
///    serialized worktree-creation ramp
/// 3. Makes an empty commit so the merge agent has something to merge (it
///    handles empty commits via fast-forward, no conflicts)
/// 4. Decrements the active counter on exit
///
/// The `Arc<AtomicUsize>` pair (`active`, `peak`) is shared across all
/// concurrent worker invocations.
async fn peak_counting_worker(
    spec: WorkerSpec,
    active: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    hold_ms: u64,
) -> Result<(), String> {
    let cur = active.fetch_add(1, Ordering::SeqCst) + 1;
    peak.fetch_max(cur, Ordering::SeqCst);

    tokio::time::sleep(Duration::from_millis(hold_ms)).await;

    let status = tokio::process::Command::new("git")
        .current_dir(&spec.worktree_path)
        .args([
            "-c",
            "user.email=peak@test",
            "-c",
            "user.name=PeakMock",
            "commit",
            "--allow-empty",
            "-m",
            &format!("peak-task {}", spec.task.id),
        ])
        .status()
        .await
        .map_err(|e| format!("git spawn: {e}"))?;

    active.fetch_sub(1, Ordering::SeqCst);

    if status.success() {
        Ok(())
    } else {
        Err(format!("git commit failed for {}", spec.task.id))
    }
}

/// Empirical proof that `concurrency_safe = true` tasks fan out **beyond**
/// `SLOT_CAPACITY = 7`. Dispatches 12 safe tasks with a 1-second worker
/// hold and asserts peak observed concurrency exceeds the write-task cap.
///
/// Math: worktree-create is ~50 ms × 12 = ~600 ms of serialized ramp; the
/// first worker spawned at t=50 ms holds until t=1050 ms. By t=600 ms all
/// 12 are spawned and still holding, so peak should reach 12 (≤ read cap).
#[tokio::test]
async fn dispatch_wave_safe_tasks_peak_exceeds_write_slot_capacity() {
    const N_SAFE: usize = 12;
    const HOLD_MS: u64 = 1_000;

    let repo_dir = TempDir::new().unwrap();
    let wt_dir = TempDir::new().unwrap();
    let repo = repo_dir.path().to_path_buf();
    let wt_root = wt_dir.path().to_path_buf();
    let feat_branch = "feat/test-safe-fanout";

    init_repo(&repo).await;
    checkout_branch(&repo, feat_branch).await;

    // 12 safe tasks (no deps between them, so they're all ready immediately).
    let tasks: Vec<Task> = (0..N_SAFE)
        .map(|i| {
            let mut t = make_task(&format!("safe-{i:02}"));
            t.concurrency_safe = true;
            t
        })
        .collect();

    let coordinator = Coordinator::new();
    init_build_state(&coordinator, "test-safe-fanout", &tasks).await;

    let ops_mutex = coordinator.ops_mutex.clone();
    let wm = WorktreeManager::new(ops_mutex.clone(), repo.clone());
    let ma = MergeAgent::new(ops_mutex, repo.clone());

    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));

    let active_for_worker = Arc::clone(&active);
    let peak_for_worker = Arc::clone(&peak);

    let result = dispatch_wave(
        0,
        &tasks,
        &coordinator,
        &wm,
        &ma,
        feat_branch,
        &wt_root,
        move |spec| {
            let active = Arc::clone(&active_for_worker);
            let peak = Arc::clone(&peak_for_worker);
            async move { peak_counting_worker(spec, active, peak, HOLD_MS).await }
        },
    )
    .await;

    assert!(result.is_ok(), "dispatch_wave failed: {result:?}");
    let summary = result.unwrap();
    assert_eq!(
        summary.succeeded as usize, N_SAFE,
        "all safe tasks should succeed"
    );
    assert_eq!(summary.failed, 0);

    let peak_observed = peak.load(Ordering::SeqCst);
    let read_cap = read_slot_capacity();

    // ── The actual proof ────────────────────────────────────────────────
    // Safe tasks MUST be able to peak above SLOT_CAPACITY (7). If they
    // don't, the dual-pool split is broken.
    assert!(
        peak_observed > SLOT_CAPACITY,
        "expected peak concurrency > SLOT_CAPACITY ({SLOT_CAPACITY}); got {peak_observed}. \
         This means the dual-pool dispatch is treating safe tasks as if \
         they shared the write pool.",
    );

    // Peak must respect the read-pool cap.
    assert!(
        peak_observed <= read_cap,
        "peak {peak_observed} exceeds read pool cap {read_cap}; dispatcher bug",
    );

    eprintln!(
        "[safe-fanout] N={N_SAFE} safe tasks, hold={HOLD_MS}ms, observed peak concurrency={peak_observed} (SLOT_CAPACITY={SLOT_CAPACITY}, read_cap={read_cap})"
    );
}

/// Control test — `concurrency_safe = false` tasks must still respect the
/// write pool cap of `SLOT_CAPACITY = 7`. Same setup as the safe test but
/// with the flag flipped. Asserts peak ≤ 7.
#[tokio::test]
async fn dispatch_wave_unsafe_tasks_peak_respects_write_slot_capacity() {
    const N_UNSAFE: usize = 12;
    const HOLD_MS: u64 = 1_000;

    let repo_dir = TempDir::new().unwrap();
    let wt_dir = TempDir::new().unwrap();
    let repo = repo_dir.path().to_path_buf();
    let wt_root = wt_dir.path().to_path_buf();
    let feat_branch = "feat/test-unsafe-cap";

    init_repo(&repo).await;
    checkout_branch(&repo, feat_branch).await;

    // Default is concurrency_safe = false — no field tweak needed.
    let tasks: Vec<Task> = (0..N_UNSAFE)
        .map(|i| make_task(&format!("unsafe-{i:02}")))
        .collect();

    let coordinator = Coordinator::new();
    init_build_state(&coordinator, "test-unsafe-cap", &tasks).await;

    let ops_mutex = coordinator.ops_mutex.clone();
    let wm = WorktreeManager::new(ops_mutex.clone(), repo.clone());
    let ma = MergeAgent::new(ops_mutex, repo.clone());

    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));

    let active_for_worker = Arc::clone(&active);
    let peak_for_worker = Arc::clone(&peak);

    let result = dispatch_wave(
        0,
        &tasks,
        &coordinator,
        &wm,
        &ma,
        feat_branch,
        &wt_root,
        move |spec| {
            let active = Arc::clone(&active_for_worker);
            let peak = Arc::clone(&peak_for_worker);
            async move { peak_counting_worker(spec, active, peak, HOLD_MS).await }
        },
    )
    .await;

    assert!(result.is_ok(), "dispatch_wave failed: {result:?}");
    let summary = result.unwrap();
    assert_eq!(summary.succeeded as usize, N_UNSAFE);

    let peak_observed = peak.load(Ordering::SeqCst);

    // ── The control ─────────────────────────────────────────────────────
    // Unsafe (default) tasks MUST cap at SLOT_CAPACITY = 7. If this fails,
    // the original write-pool semantics are broken.
    assert!(
        peak_observed <= SLOT_CAPACITY,
        "expected peak ≤ SLOT_CAPACITY ({SLOT_CAPACITY}); got {peak_observed}. Write pool cap is broken.",
    );

    eprintln!(
        "[unsafe-cap] N={N_UNSAFE} unsafe tasks, hold={HOLD_MS}ms, observed peak concurrency={peak_observed} (SLOT_CAPACITY={SLOT_CAPACITY})"
    );
}

/// Mixed wave — half safe, half unsafe — must respect BOTH pools
/// independently. With 8 safe + 8 unsafe ready, peak total active should
/// reach approximately `SLOT_CAPACITY + min(N_SAFE, read_cap)` once the
/// ramp completes, demonstrating the two pools coexist.
#[tokio::test]
async fn dispatch_wave_mixed_safe_and_unsafe_respect_independent_pools() {
    const N_SAFE: usize = 8;
    const N_UNSAFE: usize = 8;
    const HOLD_MS: u64 = 1_000;

    let repo_dir = TempDir::new().unwrap();
    let wt_dir = TempDir::new().unwrap();
    let repo = repo_dir.path().to_path_buf();
    let wt_root = wt_dir.path().to_path_buf();
    let feat_branch = "feat/test-mixed-pools";

    init_repo(&repo).await;
    checkout_branch(&repo, feat_branch).await;

    let mut tasks: Vec<Task> = Vec::new();
    for i in 0..N_SAFE {
        let mut t = make_task(&format!("safe-{i:02}"));
        t.concurrency_safe = true;
        tasks.push(t);
    }
    for i in 0..N_UNSAFE {
        tasks.push(make_task(&format!("unsafe-{i:02}")));
    }

    let coordinator = Coordinator::new();
    init_build_state(&coordinator, "test-mixed-pools", &tasks).await;

    let ops_mutex = coordinator.ops_mutex.clone();
    let wm = WorktreeManager::new(ops_mutex.clone(), repo.clone());
    let ma = MergeAgent::new(ops_mutex, repo.clone());

    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));

    let active_for_worker = Arc::clone(&active);
    let peak_for_worker = Arc::clone(&peak);

    let result = dispatch_wave(
        0,
        &tasks,
        &coordinator,
        &wm,
        &ma,
        feat_branch,
        &wt_root,
        move |spec| {
            let active = Arc::clone(&active_for_worker);
            let peak = Arc::clone(&peak_for_worker);
            async move { peak_counting_worker(spec, active, peak, HOLD_MS).await }
        },
    )
    .await;

    assert!(result.is_ok(), "dispatch_wave failed: {result:?}");
    let summary = result.unwrap();
    assert_eq!(summary.succeeded as usize, N_SAFE + N_UNSAFE);

    let peak_observed = peak.load(Ordering::SeqCst);

    // Peak must exceed SLOT_CAPACITY — at minimum, the 7 unsafe slots
    // plus at least one safe slot must coexist.
    assert!(
        peak_observed > SLOT_CAPACITY,
        "mixed wave should burst above SLOT_CAPACITY ({SLOT_CAPACITY}); got {peak_observed}",
    );

    eprintln!(
        "[mixed-pools] N_SAFE={N_SAFE} + N_UNSAFE={N_UNSAFE}, hold={HOLD_MS}ms, observed peak={peak_observed} (SLOT_CAPACITY={SLOT_CAPACITY})"
    );
}
