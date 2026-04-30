//! In-memory registry of active dispatch handles.
//!
//! [`DispatchRegistry`] is stored inside `AppState` behind an
//! `Arc<tokio::sync::Mutex<DispatchRegistry>>`.  We use `Mutex` rather than
//! `RwLock` because every operation (insert, remove, lookup) involves a
//! short critical section — no long-held read guards that would benefit from
//! `RwLock` parallelism (MED M-4).

use std::collections::HashMap;

use tokio::sync::broadcast;

use super::types::{AgentState, DispatchId, DomainAgent};

/// Per-agent tracking within a dispatch.
#[derive(Debug, Clone)]
pub struct AgentHandle {
    /// Current lifecycle state.
    pub state: AgentState,
    /// Task join handle — `None` if not yet spawned or already reaped.
    ///
    /// Stored as an `Option` so the registry can be cloned cheaply (clone
    /// drops the handle, leaving `None`).  The actual `JoinHandle<()>` is
    /// consumed by the executor when the task completes.
    pub task_id: Option<String>,
}

/// Shared handle to a live dispatch.
#[derive(Debug)]
pub struct DispatchHandle {
    /// Agents involved and their current states.
    pub agents: HashMap<DomainAgent, AgentHandle>,
    /// Broadcast sender — call `.subscribe()` to attach an SSE receiver.
    pub broadcast_tx: broadcast::Sender<crate::dispatch::types::DispatchEvent>,
    /// Wall-clock start time (Unix ms) for elapsed calculation.
    pub started_ms: u64,
    /// Whether this is a dry-run (no filesystem writes).
    pub dry: bool,
}

impl DispatchHandle {
    /// Create a new handle with an empty agent map.
    #[must_use]
    pub fn new(
        agents: Vec<DomainAgent>,
        broadcast_tx: broadcast::Sender<crate::dispatch::types::DispatchEvent>,
        dry: bool,
    ) -> Self {
        // SAFE: Unix epoch in ms is ~1.7×10¹² now; u64 max is ~1.8×10¹⁹ (overflows year 292M).
        #[allow(clippy::cast_possible_truncation)]
        let started_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0) as u64;

        let agent_map = agents
            .into_iter()
            .map(|a| {
                (
                    a,
                    AgentHandle {
                        state: AgentState::Pending,
                        task_id: None,
                    },
                )
            })
            .collect();

        Self {
            agents: agent_map,
            broadcast_tx,
            started_ms,
            dry,
        }
    }
}

/// Registry of active dispatches.
///
/// Keyed by [`DispatchId`]. Access is mediated by the
/// `Arc<Mutex<DispatchRegistry>>` stored in `AppState`.
#[derive(Debug, Default)]
pub struct DispatchRegistry {
    active: HashMap<DispatchId, DispatchHandle>,
}

impl DispatchRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            active: HashMap::new(),
        }
    }

    /// Insert a new dispatch handle.  Returns `false` and leaves the
    /// registry unchanged if `id` is already registered.
    pub fn insert(&mut self, id: DispatchId, handle: DispatchHandle) -> bool {
        use std::collections::hash_map::Entry;
        match self.active.entry(id) {
            Entry::Occupied(_) => false,
            Entry::Vacant(e) => {
                e.insert(handle);
                true
            }
        }
    }

    /// Remove and return a dispatch handle.
    pub fn remove(&mut self, id: &DispatchId) -> Option<DispatchHandle> {
        self.active.remove(id)
    }

    /// Return a reference to the broadcast sender for `id`, if active.
    #[must_use]
    pub fn broadcast_tx(
        &self,
        id: &DispatchId,
    ) -> Option<&broadcast::Sender<crate::dispatch::types::DispatchEvent>> {
        self.active.get(id).map(|h| &h.broadcast_tx)
    }

    /// Return `true` if `id` is currently active.
    #[must_use]
    pub fn contains(&self, id: &DispatchId) -> bool {
        self.active.contains_key(id)
    }

    /// Number of active dispatches.
    #[must_use]
    pub fn len(&self) -> usize {
        self.active.len()
    }

    /// `true` when no dispatches are active.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }
}
