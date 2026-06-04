//! Direct HTTP provider for the **Google AI Studio** Gemini API
//! (`generativelanguage.googleapis.com`), NOT production Vertex AI.
//!
//! Implements [`LlmAgentProvider`] calling the Gemini `generateContent` endpoint
//! against `generativelanguage.googleapis.com/v1beta`. Auth is API-key.
//!
//! # Naming note (2026-06-04 rename)
//!
//! This module was previously named `vertex.rs` / `GoogleAiStudioProvider`. The
//! original name was a misnomer: production Vertex AI lives at
//! `{region}-aiplatform.googleapis.com` and uses `OAuth2` service-account auth.
//! Real Vertex AI contracts now live separately (see
//! `provider.llm.vertex-ai-gemini` and `provider.llm.vertex-ai-claude`); a
//! dedicated Rust impl for real Vertex is a follow-up build. Until then, this
//! module honestly targets Google AI Studio under its true name.
//!
//! Handles `tool_use` multi-turn loops with G1 sanitization on every
//! `functionResponse` block before re-submission.
//!
//! # Security invariants
//!
//! API key resolved via Keychain only in release (see [`super::auth`]).
//! Chain depth enforced against [`MAX_CHAIN_DEPTH`].

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use secrecy::ExposeSecret as _;
use serde_json::{Value, json};
use tracing::{info, warn};

use crate::agent::{
    AgentResponse, LlmAgentProvider, MAX_CHAIN_DEPTH, ProviderCapabilities, ProviderError,
    SanitizedAgentRequest, SchemaMode, TokenUsage, sanitize_params,
};

use super::auth::resolve_google_ai_studio_key;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Gemini generateContent endpoint template; `{model}` is substituted.
///
/// Auth is sent as `x-goog-api-key` header — never in the URL so the key
/// cannot appear in server access logs, reverse-proxy logs, or error messages.
const API_TEMPLATE: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent";

/// Input cost per 1 000 tokens for `gemini-1.5-pro`.
const COST_INPUT_PER_K: f64 = 0.00125;
/// Output cost per 1 000 tokens for `gemini-1.5-pro`.
const COST_OUTPUT_PER_K: f64 = 0.005;

// ── Provider ──────────────────────────────────────────────────────────────────

/// Direct HTTP provider for the Vertex AI / Gemini `generateContent` API.
pub struct GoogleAiStudioProvider {
    http: Client,
    /// Gemini model identifier (e.g. `"gemini-1.5-pro"`).
    model: String,
    /// Maximum output tokens per call.
    max_tokens: u32,
}

impl GoogleAiStudioProvider {
    /// Create a provider targeting `model` with the given per-call token cap.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Internal`] if `reqwest::Client` cannot be built.
    pub fn new(model: impl Into<String>, max_tokens: u32) -> Result<Self, ProviderError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| ProviderError::Internal(format!("reqwest client: {e}")))?;
        Ok(Self {
            http,
            model: model.into(),
            max_tokens,
        })
    }

    /// Sanitize a Gemini `functionResponse` part before re-submission.
    ///
    /// Applies G1 sanitization to every string leaf inside the response object
    /// to close the same prompt-injection vector addressed in
    /// `AnthropicHttpProvider::sanitize_tool_result`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::ParamSanitizationFailed`] on G1 rejection.
    fn sanitize_function_response(value: &Value) -> Result<Value, ProviderError> {
        match value {
            Value::String(s) => {
                let (_, clean) = sanitize_params("functionResponse", s)?;
                Ok(Value::String(clean))
            }
            Value::Array(arr) => {
                let sanitized: Result<Vec<Value>, _> =
                    arr.iter().map(Self::sanitize_function_response).collect();
                Ok(Value::Array(sanitized?))
            }
            Value::Object(map) => {
                let sanitized: Result<serde_json::Map<String, Value>, _> = map
                    .iter()
                    .map(|(k, v)| Self::sanitize_function_response(v).map(|sv| (k.clone(), sv)))
                    .collect();
                Ok(Value::Object(sanitized?))
            }
            other => Ok(other.clone()),
        }
    }

    /// Execute the Gemini `generateContent` loop.
    ///
    /// Returns `(final_text, total_input_tokens, total_output_tokens)`.
    // Line count: inherent verbosity from JSON mapping + error conversion in a
    // stateful protocol loop — same rationale as AnthropicHttpProvider::call_messages.
    #[allow(clippy::too_many_lines)]
    async fn call_generate(
        &self,
        mut contents: Vec<Value>,
        system_instruction: &str,
        max_turns: u32,
        api_key: &str,
    ) -> Result<(String, u32, u32), ProviderError> {
        let url = API_TEMPLATE.replace("{model}", &self.model);

        let mut total_in: u32 = 0;
        let mut total_out: u32 = 0;
        let mut turns_used: u32 = 0;

        loop {
            if turns_used >= max_turns {
                return Err(ProviderError::TurnsExceeded { cap: max_turns });
            }

            let body = json!({
                "system_instruction": {
                    "parts": [{"text": system_instruction}]
                },
                "contents": contents,
                "generationConfig": {
                    "maxOutputTokens": self.max_tokens,
                },
            });

            let resp = self
                .http
                .post(&url)
                .header("content-type", "application/json")
                .header("x-goog-api-key", api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| ProviderError::Internal(format!("http send: {}", e.without_url())))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(ProviderError::Internal(format!(
                    "Google AI Studio API {status}: {text}"
                )));
            }

            let parsed: Value = resp.json().await.map_err(|e| {
                ProviderError::Internal(format!("json decode: {}", e.without_url()))
            })?;

            if let Some(usage) = parsed.get("usageMetadata") {
                total_in = total_in.saturating_add(
                    u32::try_from(
                        usage
                            .get("promptTokenCount")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                    )
                    .unwrap_or(u32::MAX),
                );
                total_out = total_out.saturating_add(
                    u32::try_from(
                        usage
                            .get("candidatesTokenCount")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                    )
                    .unwrap_or(u32::MAX),
                );
            }

            turns_used += 1;

            info!(
                gen_ai.request.model = %self.model,
                gen_ai.usage.input_tokens = total_in,
                gen_ai.usage.output_tokens = total_out,
                turns = turns_used,
                "Google AI Studio API call"
            );

            let candidate = parsed
                .get("candidates")
                .and_then(|c| c.get(0))
                .cloned()
                .unwrap_or(Value::Null);

            let finish_reason = candidate
                .get("finishReason")
                .and_then(Value::as_str)
                .unwrap_or("STOP");

            let parts = candidate
                .get("content")
                .and_then(|c| c.get("parts"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            if finish_reason == "STOP" {
                let text = parts
                    .iter()
                    .find(|p| p.get("text").is_some())
                    .and_then(|p| p.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                return Ok((text, total_in, total_out));
            }

            // Handle function calls (tool_use equivalent in Gemini).
            let func_calls: Vec<&Value> = parts
                .iter()
                .filter(|p| p.get("functionCall").is_some())
                .collect();

            if func_calls.is_empty() {
                warn!(%finish_reason, "no function calls but not STOP; treating as end");
                let text = parts
                    .iter()
                    .find(|p| p.get("text").is_some())
                    .and_then(|p| p.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                return Ok((text, total_in, total_out));
            }

            // Push model turn + function responses with G1 sanitization.
            contents.push(json!({
                "role": "model",
                "parts": parts
            }));

            let mut response_parts: Vec<Value> = Vec::new();
            for fc in func_calls {
                let Some(call) = fc.get("functionCall") else {
                    continue;
                };
                let name = call
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let raw_response =
                    json!({"output": format!("Function '{name}' not available in this provider.")});
                let sanitized = Self::sanitize_function_response(&raw_response)?;
                response_parts.push(json!({
                    "functionResponse": {
                        "name": name,
                        "response": sanitized,
                    }
                }));
            }

            contents.push(json!({
                "role": "user",
                "parts": response_parts,
            }));
        }
    }
}

// ── LlmAgentProvider ──────────────────────────────────────────────────────────

#[async_trait]
impl LlmAgentProvider for GoogleAiStudioProvider {
    fn name(&self) -> &'static str {
        "vertex-http"
    }

    async fn spawn(&self, req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
        let inner = req.request();

        if inner.chain_depth >= MAX_CHAIN_DEPTH {
            return Err(ProviderError::ChainDepthExceeded {
                depth: inner.chain_depth,
            });
        }

        let api_key_secret = resolve_google_ai_studio_key()?;
        let api_key = api_key_secret.expose_secret();

        let contents = vec![json!({
            "role": "user",
            "parts": [{"text": req.safe_prompt()}],
        })];

        let (text, tokens_in, tokens_out) = self
            .call_generate(contents, req.safe_identity(), inner.max_turns, api_key)
            .await?;

        let cost_usd = self.estimate_cost(tokens_in, tokens_out);

        if cost_usd > inner.max_budget_usd {
            return Err(ProviderError::BudgetExceeded {
                cap_usd: inner.max_budget_usd,
                actual_usd: cost_usd,
            });
        }

        let output = if inner.schema.is_some() {
            serde_json::from_str(&text).unwrap_or_else(|_| json!({"text": text}))
        } else {
            json!({"text": text})
        };

        Ok(AgentResponse {
            output,
            turns_used: 1,
            cost_usd,
            tokens: TokenUsage {
                input: tokens_in,
                output: tokens_out,
            },
            provider_attrs: [
                ("gen_ai.request.model".into(), json!(self.model)),
                ("gen_ai.usage.input_tokens".into(), json!(tokens_in)),
                ("gen_ai.usage.output_tokens".into(), json!(tokens_out)),
            ]
            .into(),
            retry_count: 0,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            schema_enforcement: SchemaMode::BestEffort,
            native_budget_cap: false,
            native_turn_cap: false,
            auth_inherits_session: false,
        }
    }

    fn estimate_cost(&self, input_tokens: u32, max_output_tokens: u32) -> f64 {
        let in_k = f64::from(input_tokens) / 1000.0;
        let out_k = f64::from(max_output_tokens) / 1000.0;
        in_k * COST_INPUT_PER_K + out_k * COST_OUTPUT_PER_K
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_string_passes_clean() {
        let clean = Value::String("answer: 42".into());
        let out = GoogleAiStudioProvider::sanitize_function_response(&clean).unwrap();
        assert_eq!(out, clean);
    }

    #[test]
    fn sanitize_nested_object_passes_clean() {
        let obj = json!({"result": {"value": "ok"}});
        let out = GoogleAiStudioProvider::sanitize_function_response(&obj).unwrap();
        assert_eq!(out, obj);
    }

    #[test]
    fn sanitize_null_passes_through() {
        let out = GoogleAiStudioProvider::sanitize_function_response(&Value::Null).unwrap();
        assert_eq!(out, Value::Null);
    }

    #[test]
    fn estimate_cost_is_positive() {
        let p = GoogleAiStudioProvider::new("gemini-1.5-pro", 4096).unwrap();
        assert!(p.estimate_cost(1000, 1000) > 0.0);
    }

    #[test]
    fn provider_name() {
        let p = GoogleAiStudioProvider::new("gemini-1.5-pro", 4096).unwrap();
        assert_eq!(p.name(), "vertex-http");
    }
}
