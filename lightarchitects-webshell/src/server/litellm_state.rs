//! Runtime-switchable `LiteLLM` provider configuration.
//!
//! [`LitellmConfig`] is stored in `AppState` behind `Arc<RwLock<_>>` so that
//! `POST /api/litellm/config` can update the active endpoint, API key, and
//! model without a server restart.
//!
//! Bootstrap order:
//!   1. `LitellmConfig::from_env()` — reads `LA_LITELLM_*` env vars at startup
//!   2. `POST /api/litellm/config` — operator overwrites at runtime via keychain
//!
//! All surfaces that need an [`OpenAICompatProvider`] should call
//! [`LitellmConfig::build_provider`] rather than reading env vars directly.

use chrono::{DateTime, Utc};
use lightarchitects::agent::openai_compat::OpenAICompatProvider;
use secrecy::{ExposeSecret, SecretString};

/// Active `LiteLLM` endpoint configuration.
#[derive(Debug, Clone)]
pub struct LitellmConfig {
    /// Base URL of the `LiteLLM` proxy (e.g. `http://localhost:4000`).
    pub base_url: String,
    /// API key forwarded as `Authorization: Bearer <key>`.
    pub api_key: SecretString,
    /// Model name routed by the proxy (e.g. `anthropic/claude-opus-4-7`).
    pub model: String,
    /// Wall-clock timestamp of the last write to this config.
    pub updated_at: DateTime<Utc>,
}

impl LitellmConfig {
    /// Construct from `LA_LITELLM_BASE_URL / API_KEY / MODEL` env vars.
    ///
    /// Falls back to sensible defaults so the server starts even when the
    /// vars are unset (useful in dev without a live `LiteLLM` proxy).
    #[must_use]
    pub fn from_env() -> Self {
        let base_url = std::env::var("LA_LITELLM_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:4000".to_owned());
        let api_key = SecretString::from(
            std::env::var("LA_LITELLM_API_KEY").unwrap_or_else(|_| "la-local-dev".to_owned()),
        );
        let model = std::env::var("LA_LITELLM_MODEL").unwrap_or_else(|_| "local-llama".to_owned());
        Self {
            base_url,
            api_key,
            model,
            updated_at: Utc::now(),
        }
    }

    /// Build an [`OpenAICompatProvider`] from the current config values.
    ///
    /// This is cheap — the wrapped `reqwest::Client` reuses its connection
    /// pool; only the wrapper struct is allocated.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the base URL is empty or the HTTP client cannot
    /// be constructed (extremely unlikely in practice).
    pub fn build_provider(&self) -> Result<OpenAICompatProvider, String> {
        OpenAICompatProvider::for_litellm(
            Some(self.base_url.clone()),
            self.api_key.expose_secret(),
            self.model.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_defaults_to_localhost() {
        // Don't override envs in this test — just verify the fallback compiles.
        let cfg = LitellmConfig::from_env();
        // If the real env vars are set they'll be used; otherwise defaults apply.
        assert!(!cfg.base_url.is_empty());
        assert!(!cfg.model.is_empty());
    }

    #[test]
    fn build_provider_succeeds_with_defaults() {
        let cfg = LitellmConfig {
            base_url: "http://localhost:4000".to_owned(),
            api_key: SecretString::from("test-key"),
            model: "local-llama".to_owned(),
            updated_at: Utc::now(),
        };
        assert!(cfg.build_provider().is_ok());
    }
}
