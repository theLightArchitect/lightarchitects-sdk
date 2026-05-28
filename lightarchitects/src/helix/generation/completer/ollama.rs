// Product names (Ollama, Llama, Qwen, etc.) appear in prose as proper nouns.
#![allow(clippy::doc_markdown)]

//! Ollama completer — local (`localhost:11434`) and Ollama Cloud.
//!
//! Uses `POST {base_url}/api/chat` with `stream: false`. Sets a large
//! `num_ctx` by default (131_072) so the v8 full-context multi-session
//! pattern works against capable local models (e.g. `qwen2.5:32b`,
//! `llama3.3:70b`); callers can override via [`OllamaCompleter::with_num_ctx`].
//!
//! The cloud variant adds a Bearer auth header. Local Ollama on macOS does
//! not require authentication by default.

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use std::time::{Duration, Instant};

use super::{Completion, CompletionError, LlmCompleter, model_class_from_name};
use crate::helix::generation::ModelClass;

const DEFAULT_LOCAL_BASE_URL: &str = "http://localhost:11434";
const DEFAULT_CLOUD_BASE_URL: &str = "https://ollama.com";
const DEFAULT_TIMEOUT_SECS: u64 = 600; // local models can be slow
const DEFAULT_NUM_CTX: u32 = 131_072; // 128K — enough for v8 FullContext

/// Ollama Chat API completer.
#[derive(Debug)]
pub struct OllamaCompleter {
    base_url: String,
    api_key: Option<SecretString>,
    model: String,
    temperature: f32,
    num_ctx: u32,
    timeout: Duration,
    client: reqwest::Client,
}

impl OllamaCompleter {
    /// Build a local Ollama completer (`http://localhost:11434`).
    ///
    /// # Errors
    ///
    /// [`CompletionError::Http`] on client construction failure.
    pub fn local(model: impl Into<String>) -> Result<Self, CompletionError> {
        Self::custom(DEFAULT_LOCAL_BASE_URL, model, None)
    }

    /// Build an Ollama Cloud completer (`https://ollama.com`) with API key.
    ///
    /// # Errors
    ///
    /// See [`Self::custom`].
    pub fn cloud(api_key: SecretString, model: impl Into<String>) -> Result<Self, CompletionError> {
        Self::custom(DEFAULT_CLOUD_BASE_URL, model, Some(api_key))
    }

    /// Build a completer with an explicit base URL (e.g. a remote LAN host or
    /// reverse proxy).
    ///
    /// # Errors
    ///
    /// [`CompletionError::Http`] on client construction failure.
    pub fn custom(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<SecretString>,
    ) -> Result<Self, CompletionError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| CompletionError::Http(e.to_string()))?;
        Ok(Self {
            base_url: base_url.into(),
            api_key,
            model: model.into(),
            temperature: 0.0,
            num_ctx: DEFAULT_NUM_CTX,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            client,
        })
    }

    /// Build cloud from the `OLLAMA_API_KEY` environment variable.
    ///
    /// # Errors
    ///
    /// [`CompletionError::MissingCredential`] if env var unset/empty.
    pub fn cloud_from_env(model: impl Into<String>) -> Result<Self, CompletionError> {
        let key = std::env::var("OLLAMA_API_KEY")
            .map_err(|_| CompletionError::MissingCredential("OLLAMA_API_KEY".to_owned()))?;
        if key.is_empty() {
            return Err(CompletionError::MissingCredential(
                "OLLAMA_API_KEY is empty".to_owned(),
            ));
        }
        Self::cloud(SecretString::from(key), model)
    }

    /// Override the default `temperature = 0`.
    #[must_use]
    pub fn with_temperature(mut self, t: f32) -> Self {
        self.temperature = t;
        self
    }

    /// Override the default `num_ctx = 131_072` (128K). Most local models
    /// support 32K-128K natively; some support 256K+.
    #[must_use]
    pub fn with_num_ctx(mut self, n: u32) -> Self {
        self.num_ctx = n;
        self
    }

    /// Override the default request timeout (600s — local generation can be slow).
    ///
    /// # Errors
    ///
    /// [`CompletionError::Http`] on client rebuild failure.
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
impl LlmCompleter for OllamaCompleter {
    fn name(&self) -> String {
        let location = if self.api_key.is_some() {
            "ollama-cloud"
        } else {
            "ollama"
        };
        format!("{location}:{}", self.model)
    }

    fn model_class(&self) -> ModelClass {
        model_class_from_name(&self.model)
    }

    async fn complete(&self, system: &str, user: &str) -> Result<Completion, CompletionError> {
        let body = json!({
            "model": self.model,
            "stream": false,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "options": {
                "temperature": self.temperature,
                "num_ctx": self.num_ctx,
            }
        });

        let mut req = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .header("content-type", "application/json");
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key.expose_secret());
        }

        let started = Instant::now();
        let resp = req.json(&body).send().await?;
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
        let text = payload["message"]["content"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(std::borrow::ToOwned::to_owned)
            .ok_or(CompletionError::Empty)?;

        // Ollama reports prompt_eval_count and eval_count for input/output tokens.
        let input_tokens = payload["prompt_eval_count"].as_u64().unwrap_or(0);
        let output_tokens = payload["eval_count"].as_u64().unwrap_or(0);

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
    fn local_name_uses_ollama_prefix() {
        let c = OllamaCompleter::local("qwen2.5:32b").unwrap();
        assert_eq!(c.name(), "ollama:qwen2.5:32b");
    }

    #[test]
    fn cloud_name_uses_cloud_prefix() {
        let c = OllamaCompleter::cloud(SecretString::from("k"), "kimi-k2.5:cloud").unwrap();
        assert_eq!(c.name(), "ollama-cloud:kimi-k2.5:cloud");
    }

    #[test]
    fn model_class_routes_through_name() {
        let llama_big = OllamaCompleter::local("llama3.3:70b").unwrap();
        assert_eq!(llama_big.model_class(), ModelClass::Frontier);

        let qwen_mid = OllamaCompleter::local("qwen2.5:32b").unwrap();
        assert_eq!(qwen_mid.model_class(), ModelClass::MidTier);

        let phi_small = OllamaCompleter::local("phi-3.5-mini").unwrap();
        assert_eq!(phi_small.model_class(), ModelClass::Cheap);
    }

    #[test]
    fn default_num_ctx_fits_full_context() {
        let c = OllamaCompleter::local("qwen2.5:32b").unwrap();
        assert!(c.num_ctx >= 124_000); // our v8 full-context fits comfortably
    }

    #[test]
    fn builder_chain_compiles() {
        let c = OllamaCompleter::local("qwen2.5:32b")
            .unwrap()
            .with_temperature(0.5)
            .with_num_ctx(32_768);
        assert!(c.temperature > 0.0);
        assert_eq!(c.num_ctx, 32_768);
    }

    #[test]
    fn custom_base_url() {
        let c = OllamaCompleter::custom("http://10.0.0.5:11434", "llama3.3:70b", None);
        assert!(c.is_ok());
        let c = c.unwrap();
        assert_eq!(c.base_url, "http://10.0.0.5:11434");
        assert!(c.api_key.is_none());
    }
}
