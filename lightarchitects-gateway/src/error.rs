//! Error types for the `lightarchitects-gateway` crate.

use lightarchitects::core::handler::HandlerError;
use thiserror::Error;

/// Top-level gateway error, covering all failure modes.
#[derive(Debug, Error)]
pub enum GatewayError {
    /// Configuration load or parse error.
    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    /// An I/O error from reading stdin or writing stdout.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization or deserialization failure.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// A required parameter was missing from the tool call.
    #[error("missing required parameter: {0}")]
    MissingParam(&'static str),

    /// The requested tool is not registered in this gateway.
    #[error("unknown tool: {0}")]
    UnknownTool(String),

    /// A file operation (read/write/edit) failed with a descriptive message.
    #[error("file error: {0}")]
    File(String),

    /// A subprocess (bash/search/glob) failed with a descriptive message.
    #[error("subprocess error: {0}")]
    Subprocess(String),

    /// The `old_string` was not found in the target file.
    #[error("edit error: string not found in file")]
    EditNotFound,

    /// The `old_string` matches more than once and `replace_all` is false.
    #[error("edit error: string is not unique ({count} matches found)")]
    EditNotUnique {
        /// Number of occurrences found.
        count: usize,
    },

    /// A governance policy (trust or scope) blocked the agent action.
    #[error("governance error for agent '{agent}': {reason}")]
    Governance {
        /// Name of the agent whose action was blocked.
        agent: String,
        /// Human-readable explanation of the policy violation.
        reason: String,
    },

    /// An agent is not enabled in the gateway config.
    #[error("agent '{0}' is not enabled in config")]
    AgentNotEnabled(String),

    /// An agent binary could not be spawned (process spawn failure).
    #[error("failed to spawn agent '{agent}': {reason}")]
    SpawnFailed {
        /// Agent name.
        agent: String,
        /// Underlying error description.
        reason: String,
    },

    /// A conductor operation failed.
    #[error("conductor error: {0}")]
    Conductor(String),

    /// A parameter value is present but invalid (wrong type, unknown variant, etc.).
    #[error("invalid parameter: {0}")]
    InvalidParam(String),

    /// An internal operation failed with a descriptive message.
    #[error("internal error: {0}")]
    Internal(String),

    /// An MCP protocol exchange with an agent failed.
    #[error("MCP protocol error for agent '{agent}': {reason}")]
    McpProtocol {
        /// Agent name.
        agent: String,
        /// Error description.
        reason: String,
    },
}

/// Configuration-specific error variants.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The config file could not be read.
    #[error("cannot read config file at '{path}': {source}")]
    ReadFile {
        /// Path that could not be read.
        path: String,
        /// Underlying I/O error.
        source: std::io::Error,
    },

    /// The TOML content could not be parsed.
    #[error("cannot parse config TOML: {0}")]
    ParseToml(#[from] toml::de::Error),

    /// The HOME environment variable is not set.
    #[error("HOME environment variable is not set")]
    NoHome,
}

impl From<HandlerError> for GatewayError {
    /// Map an in-process handler error to the appropriate gateway error.
    ///
    /// This mapping preserves the most specific gateway error variant for each
    /// handler error, so that downstream consumers (MCP error responses, AYIN
    /// observability events) see the same error categories regardless of whether
    /// the call went through the spawner or the inline handler.
    fn from(err: HandlerError) -> Self {
        match err {
            HandlerError::UnknownAction { action, .. } => Self::UnknownTool(action),
            HandlerError::InvalidParams { message, .. } => Self::InvalidParam(message),
            // Both NotInitialized and ServiceError map to McpProtocol —
            // they represent the same class of error as a subprocess MCP
            // protocol failure.
            HandlerError::NotInitialized { handler, message }
            | HandlerError::ServiceError {
                handler, message, ..
            } => Self::McpProtocol {
                agent: handler,
                reason: message,
            },
            HandlerError::Internal { message, .. } => Self::Internal(message),
        }
    }
}
