//! Snapshot serialization and deserialization for `CanvasState`.
//!
//! A snapshot is a complete, integrity-stamped copy of `CanvasState` at a
//! specific `snapshot_seq`. Callers use it for: (a) SSE reconnect replay,
//! (b) periodic persistence to `snapshots/<seq>.json`, and (c) gap recovery
//! when UI SSE consumers detect a sequence gap.

use crate::types::CanvasState;
use serde::{Deserialize, Serialize};

/// A stamped snapshot of `CanvasState` at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// The `snapshot_seq` value from `CanvasState` at capture time.
    pub snapshot_seq: u64,
    /// The full canvas state. Deserializing and calling `reduce()` again from
    /// this state will produce identical results for the same event sequence.
    pub state: CanvasState,
    /// Schema version for forward-compatibility.
    pub schema_version: u32,
}

impl Snapshot {
    /// Current snapshot schema version.
    pub const SCHEMA_VERSION: u32 = 1;

    /// Capture the current `CanvasState` as a snapshot.
    pub fn capture(state: &CanvasState) -> Self {
        Self {
            snapshot_seq: state.snapshot_seq,
            state: state.clone(),
            schema_version: Self::SCHEMA_VERSION,
        }
    }

    /// Serialize to a JSON byte vector for persistence.
    ///
    /// Returns an error if serialization fails (practically infallible for
    /// well-formed `CanvasState`).
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserialize from a JSON byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    /// Extract the `CanvasState`, consuming this snapshot.
    pub fn into_state(self) -> CanvasState {
        self.state
    }
}
