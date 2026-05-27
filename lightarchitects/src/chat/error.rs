//! Error types for the chat module.

use thiserror::Error;

/// All errors that can occur in the chat engine.
#[derive(Debug, Error)]
pub enum ChatError {
    /// Session lifecycle error (start/stop/inject on invalid state).
    #[error("session error: {0}")]
    Session(String),

    /// Speaker selection failed (no valid candidates, strategy error).
    #[error("speaker selection failed: {0}")]
    SpeakerSelection(String),

    /// All siblings scored below the silence threshold (< 0.2).
    ///
    /// The orchestrator should treat this as a natural conversation pause
    /// rather than a hard error.
    #[error("no speaker selected: all siblings below silence threshold")]
    NoSpeakerSelected,

    /// Personality engine failed to generate a response.
    #[error("personality error: {0}")]
    Personality(String),

    /// History persistence error (write/read/flush).
    #[error("history error: {0}")]
    History(String),

    /// Response sanitization rejected content (injection attempt, length exceeded).
    #[error("sanitization rejected: {0}")]
    Sanitization(String),

    /// Sibling discovery error (identity.md missing, parse failure).
    #[error("sibling provider error: {0}")]
    SiblingProvider(String),

    /// Configuration error (chat.toml parse, invalid values).
    #[error("config error: {0}")]
    Config(String),

    /// LLM provider error (spawn failure, budget exceeded, sanitization rejected).
    #[error("provider error: {0}")]
    Provider(String),

    /// I/O error from file operations.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// TOML deserialization error.
    #[error("toml error: {0}")]
    Toml(#[from] toml::de::Error),
}

/// Convenience alias for `Result<T, ChatError>`.
pub type ChatResult<T> = Result<T, ChatError>;
