//! Storage backend abstractions — re-exported from `lightarchitects-soul`.
//!
//! [`StorageBackend`], [`EntryFilter`], [`StorageError`], and the storage
//! entry type all live in the SDK crate so any downstream consumer can
//! depend on the interface without pulling in the full `soul-helix` server
//! library.
//!
//! This module re-exports the canonical SDK types under the names that
//! `soul-helix` previously defined internally, preserving backward
//! compatibility for all callers that use `lightarchitects_helix::storage::*` paths.
//!
//! # Type Aliases
//!
//! - [`HelixEntry`] = [`lightarchitects_soul::StorageEntry`]
//! - [`SearchHit`] = [`lightarchitects_soul::StorageSearchHit`]
//!
//! All other types are re-exported directly with unchanged names.

// ── Direct re-exports (names unchanged) ──────────────────────────────────────

pub use lightarchitects_soul::{
    EntryFilter, StorageBackend, StorageBackendKind, StorageConfig, StorageError,
};

// ── Backward-compat type aliases ─────────────────────────────────────────────

/// Flat storage representation of a helix entry.
///
/// Previously defined in this module; now a type alias for
/// [`lightarchitects_soul::StorageEntry`] for backward compatibility.
/// New code should use [`lightarchitects_soul::StorageEntry`] directly.
pub type HelixEntry = lightarchitects_soul::StorageEntry;

/// A single matching line from full-text storage search.
///
/// Previously defined in this module; now a type alias for
/// [`lightarchitects_soul::StorageSearchHit`] for backward compatibility.
/// New code should use [`lightarchitects_soul::StorageSearchHit`] directly.
pub type SearchHit = lightarchitects_soul::StorageSearchHit;
