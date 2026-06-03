//! Snapshot anchor for cache invalidation.
//!
//! Re-exports [`HelixSnapshotId`] from the gatekeeper module when the
//! `gatekeepers` feature is enabled (the common case). When it is not,
//! provides a minimal standalone definition so `soul-cache` compiles
//! without requiring `gatekeepers`.
//!
//! Consumers should use the re-export path
//! `lightarchitects::agent::cache::HelixSnapshotId` regardless of which
//! definition is active — the API surface is identical.

#[cfg(feature = "gatekeepers")]
pub use crate::agent::gatekeeper::HelixSnapshotId;

/// Replayability anchor for cache invalidation.
///
/// Changing the snapshot id invalidates the entire L1 + L2 cache namespace.
/// Two caches holding the same `HelixSnapshotId` are guaranteed to reflect
/// identical helix state (provided `canon_digest` is populated).
///
/// Used only when the `gatekeepers` feature is **not** active. When
/// `gatekeepers` is on, this type is provided by
/// [`crate::agent::gatekeeper::HelixSnapshotId`] (richer — includes
/// `assembled_at` + `canon_digest` + `helix_git_rev`).
#[cfg(not(feature = "gatekeepers"))]
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct HelixSnapshotId {
    /// UTC timestamp when the snapshot was recorded (ISO-8601 string).
    pub assembled_at: chrono::DateTime<chrono::Utc>,
    /// Optional SHA-256 hex digest of the canon directory tree.
    pub canon_digest: Option<String>,
    /// Optional helix git HEAD revision at assembly time.
    pub helix_git_rev: Option<String>,
}

#[cfg(not(feature = "gatekeepers"))]
impl HelixSnapshotId {
    /// Construct from a UTC timestamp only (tests / fallback paths).
    #[must_use]
    pub fn from_timestamp(assembled_at: chrono::DateTime<chrono::Utc>) -> Self {
        Self {
            assembled_at,
            canon_digest: None,
            helix_git_rev: None,
        }
    }
}
