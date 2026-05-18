//! Debounced broadcast of `WebEvent::GitForestUpdate` events.
//!
//! Phase 4 route handlers call [`broadcast_update`] when the branch topology
//! changes.  A 250ms debounce window (`DEBOUNCE_MS`) coalesces rapid-fire
//! updates (e.g. multiple commits landing in quick succession) into a single
//! broadcast per repo, avoiding flooding connected SSE clients.
//!
//! Phase 2 scaffold: the broadcaster is wired but no route calls it yet.
//! Phase 4 adds the three backend routes that call it.

use std::sync::Arc;

use tokio::{
    sync::broadcast,
    time::{self, Duration},
};
use tracing::debug;

use super::BranchNode;
use crate::events::WebEvent;

/// Debounce window per repo.  API-canon-audit S6 (iter-7) specifies 250ms.
const DEBOUNCE_MS: u64 = 250;

/// Emit a `GitForestUpdate` event on `event_tx` for `repo` with the given
/// `root` topology.
///
/// The call is fire-and-forget (returns `bool` via `.is_ok()` to avoid
/// propagating `broadcast::SendError<WebEvent>` which triggers
/// `clippy::result_large_err`).
pub fn broadcast_update(
    event_tx: &broadcast::Sender<WebEvent>,
    repo: String,
    root: BranchNode,
) -> bool {
    event_tx
        .send(WebEvent::GitForestUpdate { repo, root })
        .is_ok()
}

/// Spawn a debounced broadcast task.
///
/// The caller holds the `broadcast::Sender`; this helper clones it and
/// spawns a `tokio::task` that sleeps for `DEBOUNCE_MS` then emits.
/// Callers that want coalescing should cancel the previous handle before
/// spawning a new one (see `DebouncedBroadcaster` below).
pub fn spawn_debounced_broadcast(
    event_tx: broadcast::Sender<WebEvent>,
    repo: String,
    root: BranchNode,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        time::sleep(Duration::from_millis(DEBOUNCE_MS)).await;
        let sent = broadcast_update(&event_tx, repo.clone(), root);
        debug!(
            target: "gitforest.broadcaster",
            repo = %repo,
            sent = sent,
            "debounced GitForestUpdate emitted",
        );
    })
}

/// State for per-repo debounced broadcasting.
///
/// Holds the last-scheduled task handle so callers can cancel it before
/// scheduling a replacement, achieving a trailing-edge debounce.
pub struct DebouncedBroadcaster {
    event_tx: broadcast::Sender<WebEvent>,
    pending: Arc<std::sync::Mutex<Option<tokio::task::AbortHandle>>>,
}

impl DebouncedBroadcaster {
    /// Create a new broadcaster wrapping the given channel sender.
    pub fn new(event_tx: broadcast::Sender<WebEvent>) -> Self {
        Self {
            event_tx,
            pending: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Schedule a debounced broadcast.  Cancels any pending broadcast for
    /// the same broadcaster instance before scheduling the new one.
    pub fn schedule(&self, repo: String, root: BranchNode) {
        let mut guard = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(handle) = guard.take() {
            handle.abort();
        }
        let handle = spawn_debounced_broadcast(self.event_tx.clone(), repo, root);
        *guard = Some(handle.abort_handle());
    }
}
