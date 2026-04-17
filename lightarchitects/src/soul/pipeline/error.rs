//! Pipeline error types.
//!
//! [`PipelineError`] covers all failure modes in the hybrid retrieval pipeline,
//! wrapping storage and embedding errors as root causes.

use thiserror::Error;

// ============================================================================
// PipelineError
// ============================================================================

/// Error type for all retrieval pipeline operations.
#[derive(Debug, Error)]
pub enum PipelineError {
    /// A storage backend operation failed.
    #[error("storage error: {0}")]
    Storage(#[from] crate::soul::storage::StorageError),

    /// An embedding provider operation failed.
    #[error("embedding error: {0}")]
    Embedding(#[from] crate::soul::embedding::EmbeddingError),

    /// The query was invalid (empty, malformed, etc.).
    #[error("invalid query: {0}")]
    InvalidQuery(String),
}

/// Result type for retrieval pipeline operations.
pub type PipelineResult<T> = Result<T, PipelineError>;
