//! Single AgentRunner worker lifecycle for the delivery arena.
//!
//! Phase 3 implementation:
//! - `run(task_spec: TaskSpec) -> Result<TaskResult, WorkerError>`
//! - Creates worktree via `Command::new("git") worktree add -b <branch> <path> HEAD`
//!   (gix has no worktree-add primitive; git2 lacks --force/--detach parity — Task #17 Q1)
//! - Constructs `AgentRunner` via fail-closed builder (Phase 2A §5.5.1):
//!   MUST set `permission_matrix` or returns `BuilderError::MissingPermissionMatrix`
//! - Sets `W3C_TRACEPARENT` env var before spawn (observability-canon §1.1)
//! - Strips `ANTHROPIC_API_KEY` before spawn (memory: webshell_anthropic_key_strip)
//! - Cleanup: lsof → kill -TERM → wait ≤5 s → kill -KILL → worktree remove --force → prune
