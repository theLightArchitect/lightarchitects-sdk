//! PAUSE/drain/resume state machine for lightsquad wave execution.
//!
//! Provides a [`PauseHandle`] that coordinates the three-state lifecycle of
//! a build: [`PauseState::Running`] → [`PauseState::Draining`] →
//! [`PauseState::Paused`] → [`PauseState::Running`].
//!
//! ## Design
//!
//! - **Draining**: no new tasks are dispatched; in-flight workers complete
//!   naturally. Triggered by an operator HITL signal.
//! - **Paused**: all workers have finished; the build is fully halted until
//!   an explicit [`PauseHandle::resume`] call.
//! - **Running**: normal dispatch; returned to by [`PauseHandle::resume`].
//!
//! [`PauseHandle`] is cheap to clone (one `Arc` per clone). All methods are
//! `async` and use a [`Mutex`](tokio::sync::Mutex) for state mutations and a
//! [`Notify`](tokio::sync::Notify) for wake-ups, so no busy-waiting occurs.
//!
//! ## Atomic writes
//!
//! This module also re-exports the [`atomic_write`] helper used by
//! crash-safe file operations across the lightsquad subsystem.

use std::path::Path;
use std::sync::Arc;

use tokio::sync::{Mutex, Notify};

// ─── State ────────────────────────────────────────────────────────────────────

/// The three states a lightsquad build can be in with respect to pausing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PauseState {
    /// Normal operation — tasks are dispatched freely.
    Running,
    /// No new tasks dispatched; in-flight workers complete naturally.
    Draining,
    /// All workers have drained; build is fully halted.
    Paused,
}

// ─── Handle ───────────────────────────────────────────────────────────────────

/// Build-level pause handle shared across all worker slots.
///
/// Clone is cheap — the handle wraps an `Arc` internally. All state mutations
/// are async and fully thread-safe.
#[derive(Clone, Debug)]
pub struct PauseHandle {
    state: Arc<Mutex<PauseState>>,
    resume_notify: Arc<Notify>,
}

impl PauseHandle {
    /// Create a new [`PauseHandle`] in the [`PauseState::Running`] state.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(PauseState::Running)),
            resume_notify: Arc::new(Notify::new()),
        }
    }

    /// Transition [`PauseState::Running`] → [`PauseState::Draining`].
    ///
    /// Idempotent: if the state is already [`PauseState::Draining`] or
    /// [`PauseState::Paused`] this is a no-op.
    pub async fn begin_drain(&self) {
        let mut guard = self.state.lock().await;
        if *guard == PauseState::Running {
            *guard = PauseState::Draining;
        }
    }

    /// Transition [`PauseState::Draining`] → [`PauseState::Paused`].
    ///
    /// Called by the wave dispatcher once the in-flight slot count reaches
    /// zero. If the state is not [`PauseState::Draining`] this is a no-op.
    pub async fn mark_paused(&self) {
        let mut guard = self.state.lock().await;
        if *guard == PauseState::Draining {
            *guard = PauseState::Paused;
        }
    }

    /// Transition [`PauseState::Paused`] → [`PauseState::Running`] and wake
    /// all tasks waiting in [`PauseHandle::wait_for_resume`].
    ///
    /// If the state is not [`PauseState::Paused`] this is a no-op.
    pub async fn resume(&self) {
        let mut guard = self.state.lock().await;
        if *guard == PauseState::Paused {
            *guard = PauseState::Running;
            // Drop before notifying so waiters don't re-lock against us.
            drop(guard);
            self.resume_notify.notify_waiters();
        }
    }

    /// Returns `true` when new task dispatch should be suppressed.
    ///
    /// This is `true` for both [`PauseState::Draining`] and
    /// [`PauseState::Paused`]; `false` only for [`PauseState::Running`].
    pub async fn is_draining(&self) -> bool {
        *self.state.lock().await != PauseState::Running
    }

    /// Suspend the caller until the state returns to [`PauseState::Running`].
    ///
    /// Uses [`Notify`](tokio::sync::Notify) internally — no busy-waiting.
    /// Returns immediately when the handle is already in
    /// [`PauseState::Running`].
    pub async fn wait_for_resume(&self) {
        loop {
            {
                let guard = self.state.lock().await;
                if *guard == PauseState::Running {
                    return;
                }
            }
            // Wait for a notification from resume().  Because Notify uses a
            // "lost wakeup" safe API we re-check the predicate after each
            // notification.
            self.resume_notify.notified().await;
        }
    }

    /// Return a snapshot of the current [`PauseState`] without blocking on
    /// any other operations.
    pub async fn current_state(&self) -> PauseState {
        self.state.lock().await.clone()
    }
}

impl Default for PauseHandle {
    /// Delegates to [`PauseHandle::new`].
    fn default() -> Self {
        Self::new()
    }
}

// ─── Atomic write ─────────────────────────────────────────────────────────────

/// Atomically write `data` to `path` via a `.tmp` sibling file and rename.
///
/// The write sequence is:
/// 1. Create parent directories (if absent).
/// 2. Write `data` to `<path>.tmp`, flush, and `fsync`.
/// 3. Rename `<path>.tmp` → `path` (atomic on POSIX).
///
/// A crash between step 2 and 3 leaves the original `path` unchanged; the
/// stale `.tmp` sibling is silently overwritten on the next call.
///
/// # Errors
///
/// Returns [`std::io::Error`] if directory creation, the file write, the
/// `fsync`, or the rename fails.
pub fn atomic_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::fs::{self, OpenOptions};
    use std::io::Write as _;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = {
        let mut p = path.as_os_str().to_owned();
        p.push(".tmp");
        std::path::PathBuf::from(p)
    };

    {
        let mut tmp = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)?;

        tmp.write_all(data)?;
        tmp.flush()?;
        tmp.sync_all()?;
    }

    fs::rename(&tmp_path, path)?;
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ── State transition tests ────────────────────────────────────────────────

    #[tokio::test]
    async fn new_state_is_running() {
        let h = PauseHandle::new();
        assert_eq!(h.current_state().await, PauseState::Running);
    }

    #[tokio::test]
    async fn begin_drain_transitions_to_draining() {
        let h = PauseHandle::new();
        h.begin_drain().await;
        assert_eq!(h.current_state().await, PauseState::Draining);
    }

    #[tokio::test]
    async fn begin_drain_is_idempotent_when_draining() {
        let h = PauseHandle::new();
        h.begin_drain().await;
        h.begin_drain().await; // second call must not panic or change state
        assert_eq!(h.current_state().await, PauseState::Draining);
    }

    #[tokio::test]
    async fn begin_drain_is_noop_when_paused() {
        let h = PauseHandle::new();
        h.begin_drain().await;
        h.mark_paused().await;
        h.begin_drain().await; // must not move Paused → Draining
        assert_eq!(h.current_state().await, PauseState::Paused);
    }

    #[tokio::test]
    async fn mark_paused_after_drain() {
        let h = PauseHandle::new();
        h.begin_drain().await;
        h.mark_paused().await;
        assert_eq!(h.current_state().await, PauseState::Paused);
    }

    #[tokio::test]
    async fn mark_paused_is_noop_from_running() {
        let h = PauseHandle::new();
        h.mark_paused().await; // must not transition Running → Paused
        assert_eq!(h.current_state().await, PauseState::Running);
    }

    #[tokio::test]
    async fn resume_returns_to_running() {
        let h = PauseHandle::new();
        h.begin_drain().await;
        h.mark_paused().await;
        h.resume().await;
        assert_eq!(h.current_state().await, PauseState::Running);
    }

    #[tokio::test]
    async fn is_draining_semantics() {
        let h = PauseHandle::new();
        assert!(!h.is_draining().await, "Running should not be draining");

        h.begin_drain().await;
        assert!(h.is_draining().await, "Draining should report is_draining");

        h.mark_paused().await;
        assert!(
            h.is_draining().await,
            "Paused should also suppress dispatch"
        );

        h.resume().await;
        assert!(!h.is_draining().await, "Running again after resume");
    }

    #[tokio::test]
    async fn wait_for_resume_returns_immediately_when_running() {
        let h = PauseHandle::new();
        // Must complete without blocking.
        tokio::time::timeout(std::time::Duration::from_millis(100), h.wait_for_resume())
            .await
            .expect("wait_for_resume should return immediately when Running");
    }

    #[tokio::test]
    async fn wait_for_resume_unblocks_after_resume() {
        let h = PauseHandle::new();
        h.begin_drain().await;
        h.mark_paused().await;

        let h2 = h.clone();
        let waiter = tokio::spawn(async move {
            h2.wait_for_resume().await;
        });

        // Give the waiter a moment to block.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        h.resume().await;

        tokio::time::timeout(std::time::Duration::from_millis(200), waiter)
            .await
            .expect("waiter should unblock after resume")
            .expect("waiter task should not panic");
    }

    #[tokio::test]
    async fn default_impl_is_running() {
        let h = PauseHandle::default();
        assert_eq!(h.current_state().await, PauseState::Running);
    }

    // ── Clone isolation ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn clone_shares_state() {
        let h1 = PauseHandle::new();
        let h2 = h1.clone();

        h1.begin_drain().await;
        assert_eq!(h2.current_state().await, PauseState::Draining);
    }

    // ── atomic_write tests ────────────────────────────────────────────────────

    #[test]
    fn atomic_write_creates_file_and_parents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/c/data.bin");
        atomic_write(&path, b"hello world").unwrap();
        assert!(path.exists());
        assert_eq!(std::fs::read(&path).unwrap(), b"hello world");
    }

    #[test]
    fn atomic_write_overwrite_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.bin");
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
    }

    #[test]
    fn atomic_write_leaves_no_tmp_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clean.bin");
        atomic_write(&path, b"data").unwrap();
        let tmp = dir.path().join("clean.bin.tmp");
        assert!(!tmp.exists(), ".tmp sibling should be removed after rename");
    }
}
