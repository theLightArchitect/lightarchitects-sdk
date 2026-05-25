//! Worker spawn — slot allocator, tier router, and subprocess handle lifecycle.
//!
//! Per canonical IRONCLAW PDF spec (7-Slot Agent Pool):
//!
//! ```text
//! Worker tier allocation (peak — 7 concurrent slots):
//!   SLOT 1-3:  OllamaCloud  — qwen3-coder:480b-cloud via Ollama Cloud /api/chat
//!   SLOT 4-7:  ClaudeCli    — claude --bare -p <prompt> subprocess
//! ```
//!
//! # TierRouter
//!
//! [`TierRouter::tier_for_slot`] maps a 1-based slot index to a [`WorkerTier`].
//! The router is pure (no I/O, no state) so it is trivially testable.
//!
//! The concrete provider construction happens in `lightsquad_bridge::make_worker`
//! — the router itself is provider-agnostic.
//!
//! # Phase 3 scope
//!
//! - [`WorkerTier`] enum.
//! - [`TierRouter`] with [`TierRouter::tier_for_slot`] and
//!   [`TierRouter::is_ollama_slot`] helpers.
//! - [`WorkerHandle`] struct (Phase 1 stub, unchanged).

// ── WorkerTier ────────────────────────────────────────────────────────────────

/// Provider tier assigned to a worker slot.
///
/// Matches the slot allocation table in the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerTier {
    /// Slots 1-3: Ollama Cloud (`qwen3-coder:480b-cloud` via `/api/chat` NDJSON).
    OllamaCloud,
    /// Slots 4-7: Claude CLI subprocess (`claude --bare -p <prompt>`).
    ClaudeCli,
}

// ── TierRouter ────────────────────────────────────────────────────────────────

/// Pure mapping from 1-based slot index to [`WorkerTier`].
///
/// The router carries no state; all methods are associated functions.
pub struct TierRouter;

impl TierRouter {
    /// Map a 1-based slot index to its [`WorkerTier`].
    ///
    /// Slots 1-3 → [`WorkerTier::OllamaCloud`].
    /// Slots 4-7 → [`WorkerTier::ClaudeCli`].
    ///
    /// Out-of-range slots (0 or >7) fall through to [`WorkerTier::ClaudeCli`]
    /// so the pool degrades gracefully under misconfiguration rather than
    /// panicking.
    #[must_use]
    pub fn tier_for_slot(slot: usize) -> WorkerTier {
        match slot {
            1..=3 => WorkerTier::OllamaCloud,
            _ => WorkerTier::ClaudeCli,
        }
    }

    /// Returns `true` if `slot` is assigned to the Ollama Cloud tier.
    #[must_use]
    pub fn is_ollama_slot(slot: usize) -> bool {
        matches!(Self::tier_for_slot(slot), WorkerTier::OllamaCloud)
    }
}

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
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── TierRouter ────────────────────────────────────────────────────────────────

    #[test]
    fn slots_1_to_3_are_ollama() {
        for slot in 1..=3 {
            assert_eq!(
                TierRouter::tier_for_slot(slot),
                WorkerTier::OllamaCloud,
                "slot {slot} must be OllamaCloud"
            );
            assert!(
                TierRouter::is_ollama_slot(slot),
                "is_ollama_slot({slot}) must be true"
            );
        }
    }

    #[test]
    fn slots_4_to_7_are_claude_cli() {
        for slot in 4..=7 {
            assert_eq!(
                TierRouter::tier_for_slot(slot),
                WorkerTier::ClaudeCli,
                "slot {slot} must be ClaudeCli"
            );
            assert!(
                !TierRouter::is_ollama_slot(slot),
                "is_ollama_slot({slot}) must be false"
            );
        }
    }

    #[test]
    fn slot_0_falls_through_to_claude_cli() {
        assert_eq!(TierRouter::tier_for_slot(0), WorkerTier::ClaudeCli);
    }

    #[test]
    fn slot_8_falls_through_to_claude_cli() {
        assert_eq!(TierRouter::tier_for_slot(8), WorkerTier::ClaudeCli);
    }

    // ── WorkerHandle ──────────────────────────────────────────────────────────────

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
