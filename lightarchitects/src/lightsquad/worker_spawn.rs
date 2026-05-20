//! Worker spawn — wraps `crate::agent::ClaudeCliProvider` for autonomous worker pool.
//!
//! Per canonical IRONCLAW PDF spec (7-Slot Agent Pool §):
//! ```text
//! claude --bare -p "{task_prompt}" --allowedTools "Read,Edit,Write,Bash" --output-format json
//! ```
//! - `--bare` skips CLAUDE.md auto-scan; context injected explicitly via `--append-system-prompt-file`
//! - `ANTHROPIC_API_KEY` set per worker tier (Sonnet / Haiku / Ollama Cloud)
//! - 3-5s startup overhead; negligible for tasks running 5-30 minutes
//! - 7 concurrent slots during peak wave execution
//! - Slot 1 becomes ReviewGate during gate cycle; other slots idle
//!
//! Worker tier allocation (peak):
//! - SLOT 1-2: Sonnet (complex impl)
//! - SLOT 3:   Ollama Cloud (qwen3-coder:480b or deepseek-v3.1:671b)
//! - SLOT 4-7: Haiku (simple edits, test boilerplate, formatting)
//!
//! Phase 3 implementation — wraps `crate::agent::ClaudeCliProvider` (already
//! implements subprocess spawn + G1 `sanitize_params`); adds slot allocator,
//! tier router (per `crate::lightsquad::decision_pipeline::ModelRouter`),
//! and result-channel routing back to `crate::lightsquad::wave_dispatcher`.
//!
//! Phase 1 stub — slot pool declared in Phase 3.

// ── WorkerHandle ────────────────────────────────────────────────────────────────

/// Handle to a running worker process.
///
/// Owns the subprocess `Child` handle and kills it on drop so that abandoned
/// workers (e.g. from a cancelled wave) don't accumulate as zombie processes.
///
/// # Drop behaviour
///
/// `Drop` calls [`tokio::process::Child::start_kill`] on a best-effort basis.
/// Errors are silently ignored — the process may have already exited cleanly.
/// No blocking wait is performed in `drop`; use [`WorkerHandle::wait`] before
/// dropping if you need the exit status.
pub struct WorkerHandle {
    /// Logical agent identifier (e.g. `"agent-abc123"`).
    pub agent_id: String,
    /// Task identifier this worker was spawned to execute.
    pub task_id: String,
    /// Worktree path the worker operates in.
    pub worktree: std::path::PathBuf,
    /// Underlying subprocess handle.  Wrapped in `Option` so `start_kill` can
    /// take ownership in `Drop` without requiring `&mut self` to be `Pin`ned.
    pub child: Option<tokio::process::Child>,
}

impl WorkerHandle {
    /// Wait for the worker process to exit and return its exit status.
    ///
    /// After this call the inner `Child` handle is consumed; subsequent `drop`
    /// is a no-op (the `Option` will be `None`).
    ///
    /// # Errors
    ///
    /// Returns an [`std::io::Error`] if waiting on the subprocess fails.
    pub async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        match self.child.as_mut() {
            Some(child) => child.wait().await,
            None => Err(std::io::Error::other("worker already consumed")),
        }
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Best-effort kill — ignore errors (process may have already exited).
            let _ = child.start_kill();
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Verify that a `WorkerHandle` with no child does not panic on drop.
    #[test]
    fn worker_handle_drop_with_no_child() {
        let handle = WorkerHandle {
            agent_id: "test-agent".to_owned(),
            task_id: "task-001".to_owned(),
            worktree: PathBuf::from("/tmp/test-worktree"),
            child: None,
        };
        drop(handle); // must not panic
    }
}
