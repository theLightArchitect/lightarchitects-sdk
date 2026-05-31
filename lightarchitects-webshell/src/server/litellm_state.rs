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

use axum::{Json, extract::State, http::StatusCode};
use chrono::{DateTime, Utc};
use lightarchitects::agent::openai_compat::OpenAICompatProvider;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tokio::task::spawn_blocking;

use crate::server::AppState;

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

impl Default for LitellmConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            api_key: SecretString::from(String::new()),
            model: String::new(),
            updated_at: DateTime::UNIX_EPOCH,
        }
    }
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

/// Request body for `POST /api/litellm/config`.
#[derive(Debug, Deserialize)]
pub struct ConfigUpdateRequest {
    /// `LiteLLM` proxy base URL (e.g. `http://localhost:4000`).
    pub base_url: String,
    /// API key forwarded as `Bearer` token.
    pub api_key: String,
    /// Model name (e.g. `anthropic/claude-opus-4-7`).
    pub model: String,
}

/// Response body for `GET /api/litellm/config`.
#[derive(Debug, Serialize)]
pub struct ConfigStatusResponse {
    /// Active `LiteLLM` proxy base URL.
    pub base_url: String,
    /// Active model name routed by the proxy.
    pub model: String,
    /// Whether an API key is currently stored in the Keychain.
    pub has_key: bool,
    /// Timestamp of the last config update.
    pub updated_at: DateTime<Utc>,
}

/// `POST /api/litellm/config` — store key in keychain + update `AppState`.
///
/// Stores the API key in the macOS Keychain (`la-litellm-credential`) and
/// writes all three fields to `AppState.litellm_config` atomically.
///
/// # Errors
///
/// Returns 400 if `api_key` is empty; 500 if the keychain write fails.
pub async fn update_config(
    State(state): State<AppState>,
    Json(req): Json<ConfigUpdateRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    if req.api_key.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "api_key must not be empty".to_owned(),
        ));
    }

    let key = req.api_key.clone();
    spawn_blocking(move || {
        crate::auth::credential::keychain::keychain_set(
            crate::auth::credential::litellm::KEYCHAIN_SERVICE,
            &key,
        )
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut cfg = state.litellm_config.write().await;
    cfg.base_url = req.base_url;
    cfg.api_key = SecretString::from(req.api_key);
    cfg.model = req.model;
    cfg.updated_at = Utc::now();

    tracing::info!(
        target: "litellm.config",
        base_url = %cfg.base_url,
        model = %cfg.model,
        "LiteLLM config updated by operator"
    );

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/litellm/config` — return current config (key redacted).
pub async fn get_config(State(state): State<AppState>) -> Json<ConfigStatusResponse> {
    let cfg = state.litellm_config.read().await;
    let has_key = !cfg.api_key.expose_secret().is_empty();
    Json(ConfigStatusResponse {
        base_url: cfg.base_url.clone(),
        model: cfg.model.clone(),
        has_key,
        updated_at: cfg.updated_at,
    })
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
