//! Error types for the MCP host.

use thiserror::Error;

/// All errors produced by the MCP host.
// Error variant fields are documented by their #[error("...")] display strings.
#[allow(missing_docs)]
#[derive(Debug, Error)]
pub enum McpHostError {
    /// Subprocess spawn or I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// MCP initialize handshake failed.
    #[error("MCP initialize failed for server '{name}': {reason}")]
    Initialize { name: String, reason: String },

    /// tools/list RPC failed.
    #[error("tools/list failed for server '{name}': {reason}")]
    ToolsList { name: String, reason: String },

    /// tools/call RPC failed.
    #[error("tool call '{tool}' on server '{name}' failed: {reason}")]
    ToolsCall {
        name: String,
        tool: String,
        reason: String,
    },

    /// Scope policy violation (path traversal, net host, tool not allowed).
    #[error("scope violation on server '{name}': {reason}")]
    Scope { name: String, reason: String },

    /// Named server not found in config.
    #[error("server '{name}' not found")]
    NotFound { name: String },

    /// Server exists but is not in Ready state.
    #[error("server '{name}' is not ready")]
    NotReady { name: String },

    /// Config parse or validation error.
    #[error("configuration error: {0}")]
    Config(String),

    /// JSON serialization error.
    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
}
