//! Oracle client — dispatches to multiple models in parallel, collects verdicts.

use std::time::{Duration, Instant};

use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;
use tracing::{info, warn};

use crate::models::{self, KeySource, ModelConfig, ModelId, ModelRole, OracleMode};
use crate::verdict::{Finding, FindingStatus, OracleVerdict};

/// Errors from the oracle client.
#[derive(Debug, Error)]
pub enum OracleError {
    /// Failed to build the HTTP client.
    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),
    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),
}

/// Builder for [`OracleClient`].
pub struct OracleClientBuilder {
    ollama_endpoint: String,
    timeout: Duration,
}

impl OracleClientBuilder {
    /// Set the Ollama Cloud endpoint (default: `http://localhost:11434`).
    #[must_use]
    pub fn ollama_endpoint(mut self, endpoint: &str) -> Self {
        self.ollama_endpoint = endpoint.to_string();
        self
    }

    /// Set the per-model timeout (default: 180 seconds).
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Build the client.
    ///
    /// # Errors
    ///
    /// Returns [`OracleError::Http`] if the HTTP client cannot be constructed.
    pub fn build(self) -> Result<OracleClient, OracleError> {
        let http = Client::builder().timeout(self.timeout).build()?;

        let configs = models::default_configs(&self.ollama_endpoint);

        Ok(OracleClient { http, configs })
    }
}

/// Multi-model mathematical oracle client.
///
/// Dispatches prompts to multiple AI models in parallel and collects
/// their responses for consensus analysis.
pub struct OracleClient {
    http: Client,
    configs: Vec<ModelConfig>,
}

impl OracleClient {
    /// Create a new builder with default settings.
    #[must_use]
    pub fn builder() -> OracleClientBuilder {
        OracleClientBuilder {
            ollama_endpoint: "http://localhost:11434".to_string(),
            timeout: Duration::from_secs(180),
        }
    }

    /// Start building a query.
    #[must_use]
    pub fn query(&self, prompt: &str) -> OracleQuery<'_> {
        OracleQuery {
            client: self,
            prompt: prompt.to_string(),
            mode: OracleMode::Prove,
            models: None,
        }
    }

    /// Get configs for the specified models.
    fn configs_for(&self, ids: &[ModelId]) -> Vec<&ModelConfig> {
        ids.iter()
            .filter_map(|id| self.configs.iter().find(|c| c.id == *id))
            .collect()
    }
}

/// A query being built against the oracle.
pub struct OracleQuery<'a> {
    client: &'a OracleClient,
    prompt: String,
    mode: OracleMode,
    models: Option<Vec<ModelId>>,
}

impl OracleQuery<'_> {
    /// Set the oracle mode (default: [`OracleMode::Prove`]).
    #[must_use]
    pub fn mode(mut self, mode: OracleMode) -> Self {
        self.mode = mode;
        self
    }

    /// Override the model set (ignores mode default).
    #[must_use]
    pub fn models(mut self, models: Vec<ModelId>) -> Self {
        self.models = Some(models);
        self.mode = OracleMode::Custom;
        self
    }

    /// Execute the query — dispatches to all selected models in parallel.
    ///
    /// # Errors
    ///
    /// Returns [`OracleError`] if no models are configured. Individual model
    /// failures are captured as [`FindingStatus::Error`] in the verdict, not
    /// as top-level errors.
    pub async fn call(self) -> Result<OracleVerdict, OracleError> {
        let model_ids = self.models.unwrap_or_else(|| ModelId::for_mode(self.mode));
        if model_ids.is_empty() {
            return Err(OracleError::Config(
                "No models selected. Use .mode() or .models() to specify.".into(),
            ));
        }

        let configs = self.client.configs_for(&model_ids);
        let total = configs.len();

        info!(
            models = ?model_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "Dispatching oracle query to {total} models"
        );

        let start = Instant::now();

        // Dispatch all models in parallel
        let mut handles = Vec::with_capacity(configs.len());
        for config in &configs {
            let http = self.client.http.clone();
            let prompt = self.prompt.clone();
            let full_prompt = format!("{}{}", config.prompt_prefix, prompt);
            let endpoint = config.endpoint.clone();
            let model_name = config.model_name.to_string();
            let max_tokens = config.max_tokens;
            let key = resolve_key(&config.key_source);
            let id = config.id;
            let role = config.role;
            let display = config.display.to_string();

            let handle = tokio::spawn(async move {
                let t0 = Instant::now();
                let result = call_single_model(
                    &http,
                    &endpoint,
                    &model_name,
                    &full_prompt,
                    max_tokens,
                    key.as_deref(),
                )
                .await;
                let elapsed = t0.elapsed();

                match result {
                    Ok((content, tokens_in, tokens_out)) => Finding {
                        model: id,
                        role,
                        display,
                        status: FindingStatus::Ok,
                        content,
                        elapsed,
                        tokens_in,
                        tokens_out,
                    },
                    Err(e) => {
                        warn!(model = %id, error = %e, "Model call failed");
                        Finding {
                            model: id,
                            role,
                            display,
                            status: FindingStatus::Error(e.to_string()),
                            content: String::new(),
                            elapsed,
                            tokens_in: 0,
                            tokens_out: 0,
                        }
                    }
                }
            });
            handles.push(handle);
        }

        // Collect results
        let mut findings = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.await {
                Ok(finding) => findings.push(finding),
                Err(e) => warn!(error = %e, "Task join error"),
            }
        }

        // Sort by role priority: formal_proof first
        findings.sort_by_key(|f| match f.role {
            ModelRole::FormalProof => 0,
            ModelRole::Derivation => 1,
            ModelRole::Numerical => 2,
            ModelRole::Reasoning => 3,
        });

        let models_ok = findings
            .iter()
            .filter(|f| f.status == FindingStatus::Ok)
            .count();
        let consensus = OracleVerdict::compute_consensus(&findings);
        let total_elapsed = start.elapsed();

        Ok(OracleVerdict {
            prompt: self.prompt,
            findings,
            consensus,
            total_elapsed,
            models_ok,
            models_total: total,
        })
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// OpenAI-compatible chat completion response.
#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

/// Single choice.
#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

/// Message content.
#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

/// Token usage.
#[derive(Debug, Deserialize)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

/// Call a single model endpoint with streaming.
///
/// Uses SSE streaming (`stream: true`) to avoid HTTP timeouts on long proofs.
/// Parses `data: {...}` lines, concatenating `delta.content` chunks.
/// Falls back to non-streaming if the response isn't SSE-formatted.
async fn call_single_model(
    http: &Client,
    endpoint: &str,
    model_name: &str,
    prompt: &str,
    max_tokens: u32,
    api_key: Option<&str>,
) -> Result<(String, u32, u32), reqwest::Error> {
    let body = serde_json::json!({
        "model": model_name,
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": max_tokens,
        "temperature": 0.3,
        "stream": true,
    });

    let mut req = http
        .post(format!("{endpoint}/chat/completions"))
        .json(&body);

    if let Some(key) = api_key {
        if !key.is_empty() {
            req = req.bearer_auth(key);
        }
    }

    let resp = req.send().await?;
    let full_body = resp.text().await?;

    // Try SSE streaming format first: lines starting with "data: "
    let mut content = String::new();
    let mut tokens_in: u32 = 0;
    let mut tokens_out: u32 = 0;
    let mut found_sse = false;

    for line in full_body.lines() {
        let trimmed = line.trim();
        if trimmed == "data: [DONE]" {
            break;
        }
        if let Some(json_str) = trimmed.strip_prefix("data: ") {
            found_sse = true;
            let Ok(chunk) = serde_json::from_str::<serde_json::Value>(json_str) else {
                continue;
            };
            // Extract delta content
            if let Some(delta) = chunk
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("delta"))
                .and_then(|d| d.get("content"))
                .and_then(serde_json::Value::as_str)
            {
                content.push_str(delta);
            }
            // Extract usage from final chunk (some providers include it)
            if let Some(usage) = chunk.get("usage") {
                tokens_in = u32::try_from(
                    usage
                        .get("prompt_tokens")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0),
                )
                .unwrap_or(u32::MAX);
                tokens_out = u32::try_from(
                    usage
                        .get("completion_tokens")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0),
                )
                .unwrap_or(u32::MAX);
            }
        }
    }

    // Fallback: if no SSE lines found, parse as regular JSON response
    if !found_sse {
        if let Ok(resp_json) = serde_json::from_str::<ChatResponse>(&full_body) {
            content = resp_json
                .choices
                .first()
                .and_then(|c| c.message.content.clone())
                .unwrap_or_default();
            if let Some(usage) = resp_json.usage {
                tokens_in = usage.prompt_tokens;
                tokens_out = usage.completion_tokens;
            }
        }
    }

    Ok((content, tokens_in, tokens_out))
}

/// Resolve an API key from its source.
fn resolve_key(source: &KeySource) -> Option<String> {
    match source {
        KeySource::None => None,
        #[cfg(target_os = "macos")]
        KeySource::Keychain { account, service } => keychain_get(account, service),
        KeySource::EnvVar(var) => std::env::var(var).ok(),
    }
}

/// Read a password from the macOS Keychain.
#[cfg(target_os = "macos")]
fn keychain_get(account: &str, service: &str) -> Option<String> {
    use std::process::Command;
    let output = Command::new("security")
        .args(["find-generic-password", "-a", account, "-s", service, "-w"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}
