//! `SQLite` offline backend — re-exported from `lightarchitects-soul`.
//!
//! The implementation lives in the SDK crate so any consumer can use
//! [`SqliteBackend`] without depending on the full `soul-helix` server library.
//!
//! [`SqliteBackend`] is re-exported here so existing `lightarchitects_helix::SqliteBackend`
//! import paths continue to work without change.

pub use lightarchitects_soul::SqliteBackend;
