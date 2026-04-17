//! Unified error type for [`SoulDb`][crate::soul::SoulDb] operations.
//!
//! `SoulError` is the single error type that offline-tier users encounter.
//! It wraps [`StorageError`][crate::soul::storage::StorageError] (always) and
//! [`PipelineError`][crate::soul::pipeline::error::PipelineError] (when the
//! `search` feature is enabled).
//!
//! MCP client operations ([`SoulClient`][crate::soul::SoulClient]) use
//! [`SdkError`][crate::core::SdkError] — a separate type for a
//! separate tier.

use thiserror::Error;

/// Unified error for offline helix operations.
///
/// Returned by all [`SoulDb`][crate::soul::SoulDb] methods. Users of the MCP
/// client tier ([`SoulClient`][crate::soul::SoulClient]) see
/// [`crate::core::SdkError`] instead.
#[derive(Debug, Error)]
pub enum SoulError {
    /// A storage backend operation failed.
    #[error(transparent)]
    Storage(#[from] crate::soul::storage::StorageError),

    /// A retrieval pipeline operation failed (requires `search` feature).
    #[cfg(feature = "search")]
    #[error(transparent)]
    Pipeline(#[from] crate::soul::pipeline::error::PipelineError),
}
