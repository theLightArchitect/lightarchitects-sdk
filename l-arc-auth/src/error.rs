/// Errors from the l-arc-auth system.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// No API key found in the environment or at the expected file path.
    #[error("No API key found (checked LA_API_KEY env and {path})")]
    NoKeyFound {
        /// The file path that was checked for the key.
        path: String,
    },

    /// The validation endpoint rejected the key.
    #[error("Key validation failed: {0}")]
    ValidationFailed(String),

    /// An HTTP request to the validation endpoint failed.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// A filesystem operation failed (reading/writing cache or key files).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization or deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The key has been added to the server-side revocation list.
    #[error("Key has been revoked")]
    KeyRevoked,

    /// The grace period has been exhausted — no resets remain.
    #[error("Grace period exhausted (max {max} resets)")]
    GraceExhausted {
        /// The maximum number of grace resets that were allowed.
        max: u8,
    },

    /// The browser-based auth login flow failed.
    #[error("Auth login failed: {0}")]
    LoginFailed(String),

    /// The browser-based auth login flow timed out.
    #[error("Auth login timed out after {seconds}s")]
    LoginTimeout {
        /// How many seconds were waited before timing out.
        seconds: u64,
    },
}
