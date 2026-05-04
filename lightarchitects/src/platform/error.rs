//! Error type for the platform HTTP client.

use thiserror::Error;

/// Errors returned by [`super::PlatformClient`].
#[derive(Debug, Error)]
pub enum PlatformError {
    /// Network failure or unexpected HTTP status code.
    #[error("HTTP error: {0}")]
    Http(String),

    /// The requested resource does not exist (HTTP 404).
    #[error("not found: {0}")]
    NotFound(String),

    /// Request rate-limited by the server (HTTP 429).
    #[error("rate limited — retry after {retry_after_secs}s")]
    RateLimited {
        /// Seconds to wait before retrying, taken from the `Retry-After` header.
        retry_after_secs: u64,
    },

    /// Response body could not be deserialized into the expected type.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Client was configured incorrectly (e.g. empty base URL).
    #[error("config error: {0}")]
    Config(String),
}
