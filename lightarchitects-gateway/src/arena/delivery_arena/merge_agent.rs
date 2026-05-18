//! Serialised git operations for the delivery arena.
//!
//! Phase 3 implementation: `MergeAgent { ops_mutex: Arc<Mutex<()>>, retry_policy: RetryPolicy }`.
//!
//! # Design invariants (Task #17, iter-7)
//!
//! - Mutex scope = REF-MUTATING OPS ONLY (worktree add/remove, branch create, merge, commit).
//!   Pure reads bypass the mutex — gating reads single-threads the wave dispatcher.
//! - All mutating git ops via `tokio::process::Command::new("git")`.
//! - `git2` carve-outs (read-only, require `spawn_blocking`):
//!   `Repository::head()` + `branch()` for in-process fast paths.
//! - Retry policy: jittered exponential, base 50 ms, cap 2 s, max 5 attempts.
//!   Handles `.git/index.lock` collision (exit-128).
