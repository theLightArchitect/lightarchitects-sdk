//! Fleet error type.
//!
//! All three variants carry HTTP response code semantics for the webshell layer:
//! - [`FleetError::NotFound`]  → HTTP 404
//! - [`FleetError::Parse`]     → HTTP 422 Unprocessable Entity
//! - [`FleetError::Io`]        → HTTP 500 Internal Server Error
//!
//! The HTTP mapping must be kept complete — all variants must be covered in the
//! webshell `IntoResponse` impl (Builders Cookbook: "Multi-variant errors need
//! complete HTTP maps").

/// Errors produced by fleet module operations.
#[derive(Debug, thiserror::Error)]
pub enum FleetError {
    /// JSONL session file not found at the given path.
    ///
    /// # HTTP
    /// Maps to **404 Not Found**.
    #[error("JSONL file not found: {path}")]
    NotFound {
        /// Absolute path that was searched.
        path: String,
    },

    /// JSON parse failure while processing a JSONL record.
    ///
    /// # HTTP
    /// Maps to **422 Unprocessable Entity**.
    #[error("JSONL parse error: {0}")]
    Parse(#[from] serde_json::Error),

    /// Underlying I/O failure (file open, read, directory scan).
    ///
    /// # HTTP
    /// Maps to **500 Internal Server Error**.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_display() {
        let e = FleetError::NotFound {
            path: "/tmp/foo.jsonl".into(),
        };
        assert!(e.to_string().contains("/tmp/foo.jsonl"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn parse_error_from_serde() {
        let raw = serde_json::from_str::<serde_json::Value>("{bad json").unwrap_err();
        let e: FleetError = raw.into();
        assert!(matches!(e, FleetError::Parse(_)));
    }

    #[test]
    fn io_error_from_std() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let e: FleetError = io_err.into();
        assert!(matches!(e, FleetError::Io(_)));
    }
}
