// Provider product names (OpenAI, OpenRouter, LiteLLM, Together, Groq,
// Fireworks, Azure, Databricks, Anthropic, Vertex AI, etc.) appear in prose
// as proper nouns. They are not code identifiers and don't need backticks.
#![allow(clippy::doc_markdown)]

//! OpenAI-compatible Chat Completions completer.
//!
//! Covers OpenRouter, native OpenAI, LiteLLM proxy, Together, Groq, Fireworks,
//! Azure OpenAI, Databricks — anything that speaks `/v1/chat/completions`.
//!
//! Shares [`crate::agent::OpenAIFlavor`] with the agent HTTP streaming provider
//! and the lightsquad contract supervisor. Flavors differ only in default base
//! URL and env-var naming; the HTTP code path is identical.

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use std::time::{Duration, Instant};

use super::{Completion, CompletionError, LlmCompleter, model_class_from_name};
use crate::agent::OpenAIFlavor;
use crate::helix::generation::ModelClass;

const DEFAULT_TIMEOUT_SECS: u64 = 240;
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// OpenAI-compatible completer.
#[derive(Debug)]
pub struct OpenAICompatCompleter {
    flavor: OpenAIFlavor,
    base_url: String,
    api_key: SecretString,
    model: String,
    max_tokens: u32,
    temperature: f32,
    timeout: Duration,
    client: reqwest::Client,
    /// Optional referer header (OpenRouter uses this for attribution).
    referer: Option<String>,
    /// Optional X-Title header (OpenRouter uses this for attribution).
    title: Option<String>,
}

impl OpenAICompatCompleter {
    /// Build a completer from an explicit flavor, API key, and model. Uses the
    /// flavor's default base URL.
    ///
    /// # Errors
    ///
    /// [`CompletionError::Http`] on client-construction failure.
    pub fn new(
        flavor: OpenAIFlavor,
        api_key: SecretString,
        model: impl Into<String>,
    ) -> Result<Self, CompletionError> {
        let base = flavor.default_base_url();
        if base.is_empty() {
            return Err(CompletionError::MissingCredential(
                "OpenAIFlavor::Generic requires explicit base URL — use `with_base_url`".to_owned(),
            ));
        }
        Self::with_base(flavor, base.to_owned(), api_key, model)
    }

    /// Build a completer with an explicit base URL (useful for `Generic` or for
    /// overriding the flavor default).
    ///
    /// # Errors
    ///
    /// [`CompletionError::Http`] on client-construction failure.
    pub fn with_base(
        flavor: OpenAIFlavor,
        base_url: String,
        api_key: SecretString,
        model: impl Into<String>,
    ) -> Result<Self, CompletionError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| CompletionError::Http(e.to_string()))?;
        Ok(Self {
            flavor,
            base_url,
            api_key,
            model: model.into(),
            max_tokens: DEFAULT_MAX_TOKENS,
            temperature: 0.0,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            client,
            referer: None,
            title: None,
        })
    }

    /// Convenience constructor: OpenRouter with API key + model.
    ///
    /// # Errors
    ///
    /// See [`Self::new`].
    pub fn openrouter(
        api_key: SecretString,
        model: impl Into<String>,
    ) -> Result<Self, CompletionError> {
        Self::new(OpenAIFlavor::OpenRouter, api_key, model)
    }

    /// Convenience constructor: native OpenAI with API key + model.
    ///
    /// # Errors
    ///
    /// See [`Self::new`].
    pub fn openai(
        api_key: SecretString,
        model: impl Into<String>,
    ) -> Result<Self, CompletionError> {
        Self::new(OpenAIFlavor::OpenAi, api_key, model)
    }

    /// Build from the flavor's environment variable.
    ///
    /// # Errors
    ///
    /// [`CompletionError::MissingCredential`] if the env var is unset/empty;
    /// see also [`Self::new`].
    pub fn from_env(
        flavor: OpenAIFlavor,
        model: impl Into<String>,
    ) -> Result<Self, CompletionError> {
        let env_var = flavor.default_api_key_env();
        if env_var.is_empty() {
            return Err(CompletionError::MissingCredential(
                "OpenAIFlavor::Generic has no default env var — provide the API key explicitly via with_base()".to_owned(),
            ));
        }
        let key = std::env::var(env_var)
            .map_err(|_| CompletionError::MissingCredential(env_var.to_owned()))?;
        if key.is_empty() {
            return Err(CompletionError::MissingCredential(format!(
                "{env_var} is empty"
            )));
        }
        Self::new(flavor, SecretString::from(key), model)
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

    /// Set `HTTP-Referer` header (OpenRouter attribution).
    #[must_use]
    pub fn with_referer(mut self, referer: impl Into<String>) -> Self {
        self.referer = Some(referer.into());
        self
    }

    /// Set `X-Title` header (OpenRouter attribution).
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Override the default request timeout.
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
impl LlmCompleter for OpenAICompatCompleter {
    fn name(&self) -> String {
        format!("{}:{}", self.flavor.as_str(), self.model)
    }

    fn model_class(&self) -> ModelClass {
        // OpenRouter model names are typically "provider/model"; strip the
        // provider prefix before classification so e.g. "anthropic/claude-sonnet-4.6"
        // routes the same as "claude-sonnet-4.6".
        let raw = self.model.split('/').next_back().unwrap_or(&self.model);
        model_class_from_name(raw)
    }

    async fn complete(&self, system: &str, user: &str) -> Result<Completion, CompletionError> {
        let body = json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "temperature": self.temperature,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
        });

        let mut req = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(self.api_key.expose_secret())
            .header("content-type", "application/json");
        if let Some(r) = &self.referer {
            req = req.header("HTTP-Referer", r);
        }
        if let Some(t) = &self.title {
            req = req.header("X-Title", t);
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
        let text = payload["choices"][0]["message"]["content"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(std::borrow::ToOwned::to_owned)
            .ok_or(CompletionError::Empty)?;

        let usage = &payload["usage"];
        let input_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0);
        let output_tokens = usage["completion_tokens"].as_u64().unwrap_or(0);

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
    fn openrouter_name_includes_model() {
        let c = OpenAICompatCompleter::openrouter(
            SecretString::from("k"),
            "anthropic/claude-sonnet-4.6",
        )
        .unwrap();
        assert_eq!(c.name(), "openrouter:anthropic/claude-sonnet-4.6");
    }

    #[test]
    fn openrouter_strips_provider_prefix_for_classification() {
        let c = OpenAICompatCompleter::openrouter(
            SecretString::from("k"),
            "anthropic/claude-sonnet-4.6",
        )
        .unwrap();
        assert_eq!(c.model_class(), ModelClass::Frontier);

        let llama =
            OpenAICompatCompleter::openrouter(SecretString::from("k"), "meta-llama/llama-4-scout")
                .unwrap();
        assert_eq!(llama.model_class(), ModelClass::Cheap);
    }

    #[test]
    fn openai_native_constructor() {
        let c = OpenAICompatCompleter::openai(SecretString::from("k"), "gpt-5").unwrap();
        assert_eq!(c.name(), "openai:gpt-5");
        assert_eq!(c.model_class(), ModelClass::Frontier);
    }

    #[test]
    fn generic_flavor_requires_explicit_base() {
        let err =
            OpenAICompatCompleter::new(OpenAIFlavor::Generic, SecretString::from("k"), "any-model");
        assert!(matches!(err, Err(CompletionError::MissingCredential(_))));
    }

    #[test]
    fn generic_with_explicit_base_works() {
        let c = OpenAICompatCompleter::with_base(
            OpenAIFlavor::Generic,
            "https://my-proxy.example/v1".to_owned(),
            SecretString::from("k"),
            "some-model",
        );
        assert!(c.is_ok());
    }

    #[test]
    fn with_referer_and_title_set_headers() {
        let c = OpenAICompatCompleter::openrouter(SecretString::from("k"), "test")
            .unwrap()
            .with_referer("https://lightarchitects.io")
            .with_title("SOUL helix");
        assert_eq!(c.referer.as_deref(), Some("https://lightarchitects.io"));
        assert_eq!(c.title.as_deref(), Some("SOUL helix"));
    }
}
