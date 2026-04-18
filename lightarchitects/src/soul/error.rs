//! Unified error type for [`SoulDb`][lightarchitects::soul::SoulDb] operations.
//!
//! `SoulError` is the single error type that offline-tier users encounter.
//! It wraps [`StorageError`][lightarchitects::soul::storage::StorageError] (always) and
//! [`PipelineError`][lightarchitects::soul::pipeline::error::PipelineError] (when the
//! `search` feature is enabled).
//!
//! MCP client operations ([`SoulClient`][lightarchitects::soul::SoulClient]) use
//! [`SdkError`][lightarchitects::core::SdkError] — a separate type for a
//! separate tier.

use thiserror::Error;

/// Unified error for offline helix operations.
///
/// Returned by all [`SoulDb`][lightarchitects::soul::SoulDb] methods. Users of the MCP
/// client tier ([`SoulClient`][lightarchitects::soul::SoulClient]) see
/// [`lightarchitects::core::SdkError`] instead.
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
