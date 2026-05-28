// Product names (Anthropic, Claude, Sonnet, Opus, etc.) appear in prose;
// they are proper nouns, not code identifiers, so doc_markdown is suppressed.
#![allow(clippy::doc_markdown)]

//! Direct Anthropic API completer (`api.anthropic.com`).

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use std::time::{Duration, Instant};

use super::{Completion, CompletionError, LlmCompleter, model_class_from_name};
use crate::helix::generation::ModelClass;

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_API_VERSION: &str = "2023-06-01";
const DEFAULT_TIMEOUT_SECS: u64 = 240;
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Anthropic Messages API completer.
///
/// Uses `POST {base_url}/v1/messages` with `x-api-key` auth and the
/// `anthropic-version` header. Sets `temperature = 0` by default; override
/// with [`AnthropicCompleter::with_temperature`].
#[derive(Debug)]
pub struct AnthropicCompleter {
    base_url: String,
    api_version: String,
    api_key: SecretString,
    model: String,
    max_tokens: u32,
    temperature: f32,
    timeout: Duration,
    client: reqwest::Client,
}

impl AnthropicCompleter {
    /// Build a completer with an explicit API key and model.
    ///
    /// # Errors
    ///
    /// Returns [`CompletionError::Http`] if the underlying `reqwest::Client`
    /// cannot be constructed (e.g. TLS config failure).
    pub fn new(api_key: SecretString, model: impl Into<String>) -> Result<Self, CompletionError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| CompletionError::Http(e.to_string()))?;
        Ok(Self {
            base_url: DEFAULT_BASE_URL.to_owned(),
            api_version: DEFAULT_API_VERSION.to_owned(),
            api_key,
            model: model.into(),
            max_tokens: DEFAULT_MAX_TOKENS,
            temperature: 0.0,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            client,
        })
    }

    /// Build a completer using the `ANTHROPIC_API_KEY` environment variable.
    ///
    /// # Errors
    ///
    /// [`CompletionError::MissingCredential`] if the env var is unset or empty;
    /// [`CompletionError::Http`] on client-construction failure.
    pub fn from_env(model: impl Into<String>) -> Result<Self, CompletionError> {
        let key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| CompletionError::MissingCredential("ANTHROPIC_API_KEY".to_owned()))?;
        if key.is_empty() {
            return Err(CompletionError::MissingCredential(
                "ANTHROPIC_API_KEY is empty".to_owned(),
            ));
        }
        Self::new(SecretString::from(key), model)
    }

    /// Override the default `temperature = 0`.
    #[must_use]
    pub fn with_temperature(mut self, t: f32) -> Self {
        self.temperature = t;
        self
    }

    /// Override the default `max_tokens = 4096`.
    #[must_use]
    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n;
        self
    }

    /// Override the default base URL (e.g. for proxies or beta endpoints).
    #[must_use]
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Override the default request timeout.
    ///
    /// # Errors
    ///
    /// [`CompletionError::Http`] if the underlying client cannot be rebuilt
    /// with the new timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self, CompletionError> {
        self.timeout = timeout;
        self.client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| CompletionError::Http(e.to_string()))?;
        Ok(self)
    }
}

#[async_trait]
impl LlmCompleter for AnthropicCompleter {
    fn name(&self) -> String {
        format!("anthropic:{}", self.model)
    }

    fn model_class(&self) -> ModelClass {
        model_class_from_name(&self.model)
    }

    async fn complete(&self, system: &str, user: &str) -> Result<Completion, CompletionError> {
        let body = json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "temperature": self.temperature,
            "system": system,
            "messages": [{"role": "user", "content": user}],
        });

        let started = Instant::now();
        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", self.api_key.expose_secret())
            .header("anthropic-version", &self.api_version)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                401 | 403 => CompletionError::Auth(body),
                _ => CompletionError::Provider(format!("HTTP {status}: {body}")),
            });
        }

        let payload: serde_json::Value = resp.json().await?;
        let text = payload["content"]
            .as_array()
            .and_then(|blocks| {
                blocks
                    .iter()
                    .find_map(|b| b["text"].as_str().map(std::borrow::ToOwned::to_owned))
            })
            .filter(|s| !s.is_empty())
            .ok_or(CompletionError::Empty)?;
        let usage = &payload["usage"];
        let input_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
        let output_tokens = usage["output_tokens"].as_u64().unwrap_or(0);

        Ok(Completion {
            text,
            input_tokens: u32::try_from(input_tokens).unwrap_or(u32::MAX),
            output_tokens: u32::try_from(output_tokens).unwrap_or(u32::MAX),
            latency_ms,
            provider: self.name(),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn name_includes_model() {
        let c = AnthropicCompleter::new(SecretString::from("k"), "claude-sonnet-4-6").unwrap();
        assert_eq!(c.name(), "anthropic:claude-sonnet-4-6");
    }

    #[test]
    fn model_class_routes_through_name() {
        let c = AnthropicCompleter::new(SecretString::from("k"), "claude-sonnet-4-6").unwrap();
        assert_eq!(c.model_class(), ModelClass::Frontier);

        let c5 = AnthropicCompleter::new(SecretString::from("k"), "claude-sonnet-4-5").unwrap();
        assert_eq!(c5.model_class(), ModelClass::MidTier);
    }

    #[test]
    fn explicit_key_constructor_works() {
        // Rust 2024 + workspace `unsafe_code = "deny"` blocks env::set_var/remove_var
        // (see CLAUDE.md). Verify the explicit-key path; from_env() is exercised
        // implicitly when the env var is set in the host environment.
        let c = AnthropicCompleter::new(SecretString::from("test-key"), "claude-sonnet-4-6");
        assert!(c.is_ok());
    }

    #[test]
    fn builder_chain_compiles() {
        let c = AnthropicCompleter::new(SecretString::from("k"), "claude-sonnet-4-6")
            .unwrap()
            .with_temperature(0.5)
            .with_max_tokens(1024);
        assert!(c.temperature > 0.0);
        assert_eq!(c.max_tokens, 1024);
    }
}
