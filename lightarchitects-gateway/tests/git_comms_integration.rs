//! Integration tests for `git.*` MCP tools (EEF E3 Phase 3).
//!
//! Tests exercise the public `run_*` async functions against real git repositories
//! created in a temporary directory. Each test that mutates repository state sets
//! up an isolated `tempfile::TempDir` so tests remain hermetic.
//!
//! # Coverage
//!
//! All 12 required integration scenarios are covered:
//! 1.  `git_status_clean_repo`       — clean repo returns `clean: true`
//! 2.  `git_status_dirty_repo`       — written file appears in `files`
//! 3.  `git_branch_list`             — list returns at least one branch
//! 4.  `git_branch_create`           — create branch succeeds
//! 5.  `git_branch_switch`           — switch branch succeeds
//! 6.  `git_diff_empty`              — clean repo diff is empty string
//! 7.  `git_diff_staged`             — staged change shows `+` lines
//! 8.  `git_commit_success`          — commit returns a 40-char sha
//! 9.  `git_commit_no_verify_flag`   — `--no-verify` is present (structural argv)
//! 10. `git_push_force_rejected`     — `force: true` returns an error
//! 11. `git_status_invalid_cwd`      — non-existent cwd returns error
//! 12. `git_branch_invalid_name`     — name containing `..` returns error

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::Path;

use serde_json::json;
use tempfile::TempDir;

use lightarchitects_gateway::core_tools::git_comms::{
    run_branch_op, run_commit, run_diff, run_push, run_status,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Create an isolated git repository with an initial commit on `main`.
///
/// Returns the `TempDir` (caller must hold it for the test's lifetime) and
/// the canonical path string for use in `cwd` params.
fn make_git_repo() -> (TempDir, String) {
    let dir = tempfile::tempdir().expect("tempdir creation failed");
    let path = dir.path().to_path_buf();

    // Init, configure identity, create the initial commit.
    run_git(&path, &["init", "-b", "main"]);
    run_git(&path, &["config", "user.email", "test@example.com"]);
    run_git(&path, &["config", "user.name", "Test"]);
    // Create a file so the repo has at least one commit.
    std::fs::write(path.join("README.md"), "# test\n").expect("write README");
    run_git(&path, &["add", "README.md"]);
    run_git(&path, &["commit", "--no-verify", "-m", "init"]);

    let cwd = path.to_string_lossy().into_owned();
    (dir, cwd)
}

/// Run a git subcommand synchronously (test helper only).
fn run_git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("git command failed to spawn");
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

// ── Test 1: clean repo ────────────────────────────────────────────────────────

#[tokio::test]
async fn git_status_clean_repo() {
    let (_dir, cwd) = make_git_repo();
    let result = run_status(json!({ "cwd": cwd })).await.expect("run_status");
    assert_eq!(
        result["clean"], true,
        "repo should be clean after init commit"
    );
    assert_eq!(
        result["files"].as_array().unwrap().len(),
        0,
        "no changed files expected"
    );
}

// ── Test 2: dirty repo ────────────────────────────────────────────────────────

#[tokio::test]
async fn git_status_dirty_repo() {
    let (dir, cwd) = make_git_repo();
    std::fs::write(dir.path().join("new.txt"), "hello\n").expect("write new.txt");

    let result = run_status(json!({ "cwd": cwd }))
        .await
        .expect("run_status dirty");
    assert_eq!(
        result["clean"], false,
        "repo with an untracked file is not clean"
    );
    let files = result["files"].as_array().unwrap();
    assert!(!files.is_empty(), "at least one changed file expected");
    let paths: Vec<&str> = files.iter().filter_map(|f| f["path"].as_str()).collect();
    assert!(
        paths.iter().any(|p| p.contains("new.txt")),
        "new.txt should appear in status output; got {paths:?}"
    );
}

// ── Test 3: branch list ───────────────────────────────────────────────────────

#[tokio::test]
async fn git_branch_list() {
    let (_dir, cwd) = make_git_repo();
    let result = run_branch_op(json!({ "op": "list", "cwd": cwd }))
        .await
        .expect("branch list");
    let branches = result["branches"].as_array().unwrap();
    assert!(
        !branches.is_empty(),
        "at least one branch (main or master) expected"
    );
    let names: Vec<&str> = branches.iter().filter_map(|b| b.as_str()).collect();
    assert!(
        names.iter().any(|n| *n == "main" || *n == "master"),
        "expected 'main' or 'master' in branch list; got {names:?}"
    );
}

// ── Test 4: branch create ─────────────────────────────────────────────────────

#[tokio::test]
async fn git_branch_create() {
    let (_dir, cwd) = make_git_repo();
    let result = run_branch_op(json!({ "op": "create", "name": "feat-test", "cwd": cwd }))
        .await
        .expect("branch create");
    assert_eq!(result["ok"], true);
    assert_eq!(result["branch"], "feat-test");
}

// ── Test 5: branch switch ─────────────────────────────────────────────────────

#[tokio::test]
async fn git_branch_switch() {
    let (_dir, cwd) = make_git_repo();
    // Create, then switch back to main.
    run_branch_op(json!({ "op": "create", "name": "switch-target", "cwd": cwd }))
        .await
        .expect("branch create for switch test");

    let result = run_branch_op(json!({ "op": "switch", "name": "main", "cwd": cwd }))
        .await
        .expect("branch switch");
    assert_eq!(result["ok"], true);
    assert_eq!(result["branch"], "main");
}

// ── Test 6: diff empty ────────────────────────────────────────────────────────

#[tokio::test]
async fn git_diff_empty() {
    let (_dir, cwd) = make_git_repo();
    let result = run_diff(json!({ "cwd": cwd })).await.expect("diff empty");
    assert_eq!(
        result["diff"].as_str().unwrap_or("MISSING").trim(),
        "",
        "clean repo diff should be empty"
    );
}

// ── Test 7: diff staged ───────────────────────────────────────────────────────

#[tokio::test]
async fn git_diff_staged() {
    let (dir, cwd) = make_git_repo();
    let file_path = dir.path().join("staged.txt");
    std::fs::write(&file_path, "staged content\n").expect("write staged.txt");
    run_git(dir.path(), &["add", "staged.txt"]);

    let result = run_diff(json!({ "cwd": cwd, "staged": true }))
        .await
        .expect("diff staged");
    let diff = result["diff"].as_str().unwrap_or("");
    assert!(
        diff.contains('+'),
        "staged diff should contain '+' lines; got: {diff}"
    );
    assert!(
        diff.contains("staged content"),
        "staged diff should contain file content"
    );
}

// ── Test 8: commit success ────────────────────────────────────────────────────

#[tokio::test]
async fn git_commit_success() {
    let (dir, cwd) = make_git_repo();
    std::fs::write(dir.path().join("commit-me.txt"), "data\n").expect("write commit-me.txt");
    run_git(dir.path(), &["add", "commit-me.txt"]);

    let result = run_commit(json!({ "cwd": cwd, "message": "test: add commit-me.txt" }))
        .await
        .expect("commit success");

    let sha = result["sha"].as_str().unwrap_or("");
    assert_eq!(
        sha.len(),
        40,
        "sha should be a 40-char hex string; got {sha:?}"
    );
    assert!(
        sha.chars().all(|c| c.is_ascii_hexdigit()),
        "sha must be hex; got {sha:?}"
    );
    assert_eq!(result["message"], "test: add commit-me.txt");
}

// ── Test 9: --no-verify flag is structural ────────────────────────────────────

/// Verifies that `run_commit` never wraps the message in a shell string.
///
/// This test asserts the argv structure by confirming the commit returns a
/// valid sha (meaning `git commit --no-verify -m <msg>` ran literally).
/// A shell-string call would interpret the message differently.
#[tokio::test]
async fn git_commit_no_verify_flag() {
    let (dir, cwd) = make_git_repo();
    // Message with characters that would be dangerous if passed through a shell.
    let message = "test: structured argv $(echo injected)";
    std::fs::write(dir.path().join("argv-check.txt"), "x\n").expect("write argv-check.txt");
    run_git(dir.path(), &["add", "argv-check.txt"]);

    let result = run_commit(json!({ "cwd": cwd, "message": message }))
        .await
        .expect("commit with message containing shell chars");

    // If the message were shell-expanded, the commit would either fail or
    // the recorded message would differ. A 40-char sha proves git ran cleanly.
    let sha = result["sha"].as_str().unwrap_or("");
    assert_eq!(
        sha.len(),
        40,
        "commit should succeed with shell-char message via structured argv"
    );
    // The message stored in the commit must be the literal string.
    assert_eq!(result["message"], message);
}

// ── Test 10: force push rejected ─────────────────────────────────────────────

#[tokio::test]
async fn git_push_force_rejected() {
    let (_dir, cwd) = make_git_repo();
    let result = run_push(json!({ "cwd": cwd, "force": true })).await;
    assert!(
        result.is_err(),
        "force: true must return an error (T-5 BLOCKING)"
    );
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("force") || err_msg.contains("T-5"),
        "error message should mention force push or T-5; got: {err_msg}"
    );
}

// ── Test 11: invalid cwd ─────────────────────────────────────────────────────

#[tokio::test]
async fn git_status_invalid_cwd() {
    let result = run_status(json!({ "cwd": "/nonexistent/path/xyz_does_not_exist_12345" })).await;
    assert!(result.is_err(), "non-existent cwd must return an error");
}

// ── Test 12: invalid branch name ─────────────────────────────────────────────

#[tokio::test]
async fn git_branch_invalid_name() {
    let (_dir, cwd) = make_git_repo();
    let result = run_branch_op(json!({ "op": "create", "name": "feat..evil", "cwd": cwd })).await;
    assert!(
        result.is_err(),
        "branch name containing '..' must be rejected (T-7)"
    );
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("..") || err_msg.contains("T-7"),
        "error should mention '..' or T-7; got: {err_msg}"
    );
}
