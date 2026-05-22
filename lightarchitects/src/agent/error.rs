//! Error types for Ollama-based agent providers.

/// Errors produced by [`super::ollama::OllamaCliProvider`].
///
/// Every variant carries an HTTP status code via [`OllamaError::http_status_code`]
/// so webshell route handlers can produce correctly-coded responses without
/// pattern-matching on the error string.
#[derive(Debug, thiserror::Error)]
pub enum OllamaError {
    /// The requested model slug is not in the cloud model registry.
    #[error("model not in registry: {0}")]
    UnknownModel(String),

    /// The `ollama` CLI binary was not found on `PATH`.
    #[error("ollama CLI not found on PATH")]
    CliMissing,

    /// Spawning the `ollama` subprocess failed (I/O error).
    #[error("ollama subprocess spawn failed: {0}")]
    SpawnFailed(#[from] std::io::Error),

    /// The subprocess exited with a non-zero status code.
    #[error("ollama subprocess exited with non-zero status: {0}")]
    NonZeroExit(i32),

    /// The prompt was rejected by sanitization — contains forbidden characters.
    #[error("prompt sanitization rejected input")]
    PromptInvalid,

    /// Subprocess stdout could not be parsed as the expected output format.
    #[error("output parse failed: {0}")]
    OutputParse(String),

    /// Estimated cost exceeds the caller-supplied ceiling before dispatch.
    #[error("cost ceiling exceeded: actual={actual:.4} > ceiling={ceiling:.4}")]
    CostExceeded {
        /// Estimated cost that would be incurred (USD).
        actual: f64,
        /// Configured cost ceiling (USD).
        ceiling: f64,
    },
}

impl OllamaError {
    /// HTTP status code for this error variant.
    ///
    /// Used by webshell route handlers to set `StatusCode` without
    /// pattern-matching on error strings.
    ///
    /// | Variant | Code | Reason |
    /// |---------|------|--------|
    /// | `UnknownModel` | 400 | Caller supplied an invalid slug |
    /// | `PromptInvalid` | 400 | Caller supplied a forbidden prompt |
    /// | `CostExceeded` | 402 | Budget cap would be breached |
    /// | `CliMissing` | 503 | Infrastructure missing — retry after install |
    /// | `SpawnFailed` | 500 | Unexpected process-spawn failure |
    /// | `NonZeroExit` | 500 | Subprocess internal failure |
    /// | `OutputParse` | 500 | Subprocess produced unparseable output |
    #[must_use]
    pub fn http_status_code(&self) -> u16 {
        match self {
            Self::UnknownModel(_) | Self::PromptInvalid => 400,
            Self::CostExceeded { .. } => 402,
            Self::CliMissing => 503,
            Self::SpawnFailed(_) | Self::NonZeroExit(_) | Self::OutputParse(_) => 500,
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_variants_have_non_zero_http_status_code() {
        let variants: &[OllamaError] = &[
            OllamaError::UnknownModel("x".to_owned()),
            OllamaError::CliMissing,
            OllamaError::NonZeroExit(1),
            OllamaError::PromptInvalid,
            OllamaError::OutputParse("bad".to_owned()),
            OllamaError::CostExceeded {
                actual: 1.0,
                ceiling: 0.5,
            },
        ];
        for v in variants {
            let code = v.http_status_code();
            assert!(
                (400..600).contains(&code),
                "expected 4xx/5xx, got {code} for {v}"
            );
        }
    }

    #[test]
    fn unknown_model_is_400() {
        assert_eq!(
            OllamaError::UnknownModel("x".to_owned()).http_status_code(),
            400
        );
    }

    #[test]
    fn prompt_invalid_is_400() {
        assert_eq!(OllamaError::PromptInvalid.http_status_code(), 400);
    }

    #[test]
    fn cost_exceeded_is_402() {
        assert_eq!(
            OllamaError::CostExceeded {
                actual: 1.0,
                ceiling: 0.5
            }
            .http_status_code(),
            402
        );
    }

    #[test]
    fn cli_missing_is_503() {
        assert_eq!(OllamaError::CliMissing.http_status_code(), 503);
    }
}
