//! Storage backend abstractions — re-exported from `lightarchitects-soul`.
//!
//! [`StorageBackend`], [`EntryFilter`], [`StorageError`], and the storage
//! entry type all live in the SDK crate so any downstream consumer can
//! depend on the interface without pulling in the full `soul-helix` server
//! library.
//!
//! This module re-exports the canonical SDK types under the names that
//! `soul-helix` previously defined internally, preserving backward
//! compatibility for all callers that use `lightarchitects::helix::storage::*` paths.
//!
//! # Type Aliases
//!
//! - [`HelixEntry`] = [`lightarchitects::soul::StorageEntry`]
//! - [`SearchHit`] = [`lightarchitects::soul::StorageSearchHit`]
//!
//! All other types are re-exported directly with unchanged names.

// ── Direct re-exports (names unchanged) ──────────────────────────────────────

pub use crate::soul::{
    EntryFilter, StorageBackend, StorageBackendKind, StorageConfig, StorageError,
};

// ── Backward-compat type aliases ─────────────────────────────────────────────

/// Flat storage representation of a helix entry.
///
/// Previously defined in this module; now a type alias for
/// [`lightarchitects::soul::StorageEntry`] for backward compatibility.
/// New code should use [`lightarchitects::soul::StorageEntry`] directly.
pub type HelixEntry = crate::soul::StorageEntry;

/// A single matching line from full-text storage search.
///
/// Previously defined in this module; now a type alias for
/// [`lightarchitects::soul::StorageSearchHit`] for backward compatibility.
/// New code should use [`lightarchitects::soul::StorageSearchHit`] directly.
pub type SearchHit = crate::soul::StorageSearchHit;
