//! Wave dispatcher — fan tasks out to per-worktree workers.
//!
//! Phase 3 implementation:
//! - Tokio `JoinSet` for concurrent worker handles
//! - Per-task git worktree created at dispatch (`task/{build}/{task}` branch)
//! - Worker subprocess spawned via `crate::agent::ClaudeCliProvider`
//! - Wave completes when all task handles join (success or fail)
//! - Critical-path-aware scheduling (per AdaptOrch arXiv 2602.16873)
//!
//! Phase 1 stub — types declared in Phase 3.
