//! Error types for the `lightarchitects` SDK.
//!
//! The SDK uses a three-tier error hierarchy:
//!
//! - [`SdkError`] — top-level error returned by all public API methods
//! - [`TransportError`] — failures at the stdio / process layer
//! - [`ProtocolError`] — failures at the JSON-RPC / MCP framing layer
//! - [`ToolError`] — failures reported by the remote MCP tool itself

use thiserror::Error;

/// Top-level SDK error, covering all failure modes.
///
/// Sibling clients return `Result<T, SdkError>` from every fallible method.
#[derive(Debug, Error)]
pub enum SdkError {
    /// Stdio transport failure (I/O, timeout, or process error).
    #[error("transport: {0}")]
    Transport(#[from] TransportError),

    /// Protocol-level failure (framing, JSON-RPC error, or unexpected response).
    #[error("protocol: {0}")]
    Protocol(#[from] ProtocolError),

    /// Remote tool returned a logical error (not a transport or protocol failure).
    #[error("tool: {0}")]
    Tool(#[from] ToolError),

    /// Serialization or deserialization failure.
    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),

    /// SDK or client configuration error.
    #[error("config: {0}")]
    Config(String),

    /// Scope constraint violated — the requested target, tool, or domain
    /// failed SDK-side validation before dispatch.
    ///
    /// The message does **not** include the rejected target verbatim.
    /// The raw target is logged at `WARN` level with a hash for audit trails.
    #[error("scope violation: {0}")]
    ScopeViolation(String),

    /// Authentication failure — no key, revoked key, or exhausted grace period.
    ///
    /// Returned by [`lightarchitects::core::StdioTransport::connect`] when an [`lightarchitects::auth::AuthChecker`]
    /// is present and the auth check returns a hard failure. No subprocess is
    /// spawned when this error is returned.
    #[error("auth: {0}")]
    Auth(String),
}

/// Errors at the stdio / process layer.
///
/// Only [`TransportError::Timeout`] and [`TransportError::Io`] are retried by
/// [`lightarchitects::core::config::RetryConfig`]. [`TransportError::ProcessSpawn`] and
/// [`TransportError::ProcessExit`] are terminal — retrying the same binary will
/// not help.
#[derive(Debug, Error)]
pub enum TransportError {
    /// An I/O error from reading or writing the child process stdio.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The call exceeded the configured timeout.
    #[error("timed out after {secs}s")]
    Timeout {
        /// Seconds elapsed before the timeout fired.
        secs: u64,
    },

    /// Failed to spawn the sibling binary.
    #[error("failed to spawn '{binary}': {source}")]
    ProcessSpawn {
        /// Path to the binary that could not be spawned.
        binary: String,
        /// Underlying OS error.
        source: std::io::Error,
    },

    /// The sibling process exited unexpectedly.
    #[error("process exited with status {status:?}")]
    ProcessExit {
        /// Exit status (`None` means killed by signal).
        status: Option<i32>,
    },
}

impl TransportError {
    /// Returns `true` if retrying this error may succeed.
    ///
    /// Only [`TransportError::Io`] and [`TransportError::Timeout`] are
    /// considered retryable. Spawn failures and unexpected exits are terminal.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Io(_) | Self::Timeout { .. })
    }
}

/// Errors at the JSON-RPC / MCP framing layer.
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// The response could not be parsed as valid JSON.
    #[error("malformed JSON: {0}")]
    MalformedJson(String),

    /// The response did not match the expected JSON-RPC structure.
    #[error("unexpected response shape: {0}")]
    UnexpectedShape(String),

    /// The remote returned a JSON-RPC error object.
    #[error("RPC error {code}: {message}")]
    RpcError {
        /// JSON-RPC error code.
        code: i64,
        /// Error message from the remote.
        message: String,
    },

    /// The response `id` did not match the sent request `id`.
    #[error("id mismatch: sent {sent}, received {received}")]
    IdMismatch {
        /// The `id` sent in the request.
        sent: u64,
        /// The `id` received in the response.
        received: u64,
    },

    /// Response exceeded [`lightarchitects::core::constants::MAX_RESPONSE_BYTES`].
    #[error("response exceeded maximum size ({max_bytes} bytes)")]
    ResponseTooLarge {
        /// The configured maximum in bytes.
        max_bytes: usize,
    },
}

/// An error returned by a remote MCP tool (logical, not transport).
///
/// Tool errors are **not retried** — they represent the tool's intentional
/// failure response, not a transient infrastructure problem.
#[derive(Debug, Error)]
#[error("tool '{tool}' failed: {message}")]
pub struct ToolError {
    /// Name of the tool that failed.
    pub tool: String,
    /// Human-readable error message from the tool.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_and_timeout_are_retryable() {
        let io = TransportError::Io(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
        assert!(io.is_retryable());
        let timeout = TransportError::Timeout { secs: 30 };
        assert!(timeout.is_retryable());
    }

    #[test]
    fn spawn_and_exit_are_not_retryable() {
        let spawn = TransportError::ProcessSpawn {
            binary: "test".to_owned(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        };
        assert!(!spawn.is_retryable());
        let exit = TransportError::ProcessExit { status: Some(1) };
        assert!(!exit.is_retryable());
    }

    #[test]
    fn sdk_error_from_transport() {
        let transport_err = TransportError::Timeout { secs: 5 };
        let sdk_err = SdkError::from(transport_err);
        assert!(matches!(
            sdk_err,
            SdkError::Transport(TransportError::Timeout { .. })
        ));
    }
}
