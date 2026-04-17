//! Unified error type for [`SoulDb`][crate::SoulDb] operations.
//!
//! `SoulError` is the single error type that offline-tier users encounter.
//! It wraps [`StorageError`][crate::storage::StorageError] (always) and
//! [`PipelineError`][crate::pipeline::error::PipelineError] (when the
//! `search` feature is enabled).
//!
//! MCP client operations ([`SoulClient`][crate::SoulClient]) use
//! [`SdkError`][lightarchitects_core::SdkError] — a separate type for a
//! separate tier.

use thiserror::Error;

/// Unified error for offline helix operations.
///
/// Returned by all [`SoulDb`][crate::SoulDb] methods. Users of the MCP
/// client tier ([`SoulClient`][crate::SoulClient]) see
/// [`lightarchitects_core::SdkError`] instead.
#[derive(Debug, Error)]
pub enum SoulError {
    /// A storage backend operation failed.
    #[error(transparent)]
    Storage(#[from] crate::storage::StorageError),

    /// A retrieval pipeline operation failed (requires `search` feature).
    #[cfg(feature = "search")]
    #[error(transparent)]
    Pipeline(#[from] crate::pipeline::error::PipelineError),
}
