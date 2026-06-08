//! `LiteLLMHttpDispatcher` — production [`OffloadDispatcher`] that POSTs to
//! an OpenAI-compatible `/chat/completions` endpoint.
//!
//! Wire format mirrors the Day 8 `examples/calibrate_pv_canon` harness:
//! `POST {base_url}/chat/completions` with `{model, messages, stream:false,
//! temperature}`; extracts `choices[0].message.content`.
//!
//! # Environment
//!
//! - `LA_LITELLM_BASE_URL` (default `http://localhost:11434/v1`)
//! - `LA_LITELLM_MODEL`    (default `glm-5.1:cloud`)
//! - `LA_LITELLM_API_KEY`  (default `ollama`)

use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;

use super::catalog::Pattern;
use super::laex_supervisor::OffloadDispatcher;

const DEFAULT_BASE_URL: &str = "http://localhost:11434/v1";
const DEFAULT_MODEL: &str = "glm-5.1:cloud";
const DEFAULT_API_KEY: &str = "ollama";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_TEMPERATURE: f64 = 0.1;

/// OpenAI-compatible chat completions dispatcher.
pub struct LiteLLMHttpDispatcher {
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: String,
    temperature: f64,
}

impl LiteLLMHttpDispatcher {
    /// Construct from environment variables with the documented defaults.
    ///
    /// # Errors
    ///
    /// Returns a string describing any `reqwest::Client` build failure.
    pub fn from_env() -> Result<Self, String> {
        let base_url =
            std::env::var("LA_LITELLM_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        let model = std::env::var("LA_LITELLM_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_owned());
        let api_key =
            std::env::var("LA_LITELLM_API_KEY").unwrap_or_else(|_| DEFAULT_API_KEY.to_owned());
        Self::with_config(
            base_url,
            model,
            api_key,
            DEFAULT_TEMPERATURE,
            DEFAULT_TIMEOUT,
        )
    }

    /// Construct with explicit configuration.
    ///
    /// # Errors
    ///
    /// Returns a string describing any `reqwest::Client` build failure.
    pub fn with_config(
        base_url: String,
        model: String,
        api_key: String,
        temperature: f64,
        per_call_timeout: Duration,
    ) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(per_call_timeout)
            .build()
            .map_err(|e| format!("reqwest client build: {e}"))?;
        Ok(Self {
            client,
            base_url,
            model,
            api_key,
            temperature,
        })
    }

    /// Construct using `catalog.default_model` (when set) for the model
    /// identifier, with env-var defaults for everything else.
    ///
    /// # Errors
    ///
    /// Propagates [`Self::from_env`] failures.
    pub fn from_catalog(
        catalog: &crate::agent::offload::catalog::OffloadCatalog,
    ) -> Result<Self, String> {
        let mut d = Self::from_env()?;
        if std::env::var("LA_LITELLM_MODEL").is_err() {
            if let Some(m) = catalog.default_model.as_deref() {
                m.clone_into(&mut d.model);
            }
        }
        Ok(d)
    }

    /// Borrow the resolved `model` (post-env-resolution).
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Borrow the resolved `base_url`.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[async_trait]
impl OffloadDispatcher for LiteLLMHttpDispatcher {
    async fn dispatch(&self, pattern: &Pattern, rendered_prompt: &str) -> Result<String, String> {
        let body = json!({
            "model": self.model,
            "messages": [{"role": "user", "content": rendered_prompt}],
            "stream": false,
            "temperature": self.temperature,
        });
        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("send to {} for pattern {}: {e}", self.base_url, pattern.id))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP {status}: {text}"));
        }
        let body_json: serde_json::Value =
            resp.json().await.map_err(|e| format!("decode body: {e}"))?;
        let content = body_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| format!("missing choices[0].message.content in response: {body_json}"))?
            .trim()
            .to_owned();
        Ok(content)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn from_env_uses_defaults_when_unset() {
        // Defaults must remain available even without env vars.
        // (We don't unset env here to avoid affecting other tests; we just
        // verify with_config + the constants.)
        let d = LiteLLMHttpDispatcher::with_config(
            DEFAULT_BASE_URL.to_owned(),
            DEFAULT_MODEL.to_owned(),
            DEFAULT_API_KEY.to_owned(),
            DEFAULT_TEMPERATURE,
            DEFAULT_TIMEOUT,
        )
        .unwrap();
        assert_eq!(d.base_url(), DEFAULT_BASE_URL);
        assert_eq!(d.model(), DEFAULT_MODEL);
    }

    #[test]
    fn with_config_accepts_custom_values() {
        let d = LiteLLMHttpDispatcher::with_config(
            "https://api.openai.com/v1".to_owned(),
            "gpt-4o-mini".to_owned(),
            "sk-test".to_owned(),
            0.2,
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(d.base_url(), "https://api.openai.com/v1");
        assert_eq!(d.model(), "gpt-4o-mini");
    }
}
