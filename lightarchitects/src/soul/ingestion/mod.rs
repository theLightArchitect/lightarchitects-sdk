//! Ingestion helpers for the SOUL offline storage layer.
//!
//! Provides two entry points for loading vault content into a
//! [`StorageBackend`][crate::soul::storage::StorageBackend]:
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`markdown::from_markdown`] | Parse a single markdown file into a [`StorageEntry`][crate::soul::storage::StorageEntry] |
//! | [`vault::load_directory`] | Stream all `*.md` files from a directory tree |
//! | [`vault::ingest_directory`] | Walk + write all entries to a backend in one call |
//!
//! # Feature Gate
//!
//! This module requires the `ingestion` feature:
//!
//! ```toml
//! lightarchitects-soul = { version = "0.1", features = ["ingestion"] }
//! ```

/// Markdown parsing — [`from_markdown`][markdown::from_markdown].
pub mod markdown;

/// Vault directory walker — [`load_directory`][vault::load_directory] and
/// [`ingest_directory`][vault::ingest_directory].
pub mod vault;

// ── Convenience re-exports ────────────────────────────────────────────────────

pub use markdown::from_markdown;
pub use vault::{MAX_ENTRIES, ingest_directory, load_directory};
