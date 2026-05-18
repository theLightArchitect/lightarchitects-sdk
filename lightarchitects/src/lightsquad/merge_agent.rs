//! Merge agent — serializes all git2 operations to a single shared `.git` store.
//!
//! Phase 3 implementation:
//! - `Arc<Mutex<()>>` scope-limited to ref-mutating operations (branch cuts, merges, worktree create/remove)
//! - Read-only ops (rev-parse, log, blame) bypass the mutex
//! - Jittered exponential backoff on git2 lock contention
//! - Zero LLM calls — pure `git2` + `std::process::Command`
//!
//! Per canonical IRONCLAW PDF spec (Git Strategy §):
//! > "All merge operations, branch cuts, and worktree creates/removes
//! > serialize through the Rust orchestrator. No two git processes
//! > run concurrently against the same repository."
//!
//! Phase 1 stub — types declared in Phase 3.
