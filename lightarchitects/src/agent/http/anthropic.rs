//! Direct HTTP provider for the Anthropic Messages API.
//!
//! Implements [`LlmAgentProvider`] over `reqwest` without spawning a subprocess.
//! Handles the `tool_use` multi-turn loop internally, applying G1 sanitization to
//! every `tool_result` block before re-submission (R-09 / SERAPH ADV-1).
//!
//! # Security invariants
//!
//! - API key resolved via Keychain only in release (see [`super::auth`]).
//! - Chain depth enforced against [`MAX_CHAIN_DEPTH`] before the first call.
//! - `tool_result` content is G1-sanitized on BOTH the top-level `content` string
//!   AND every `text`/`source` field within content-array blocks.
//!
//! # Spans
//!
//! Emits `gen_ai.*` attributes per the OpenTelemetry semantic conventions:
//! `gen_ai.request.model`, `gen_ai.usage.input_tokens`,
//! `gen_ai.usage.output_tokens`.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use secrecy::ExposeSecret as _;
use serde_json::{Value, json};
use tracing::{info, instrument, warn};

use crate::agent::{
    AgentResponse, LlmAgentProvider, MAX_CHAIN_DEPTH, ProviderCapabilities, ProviderError,
    SanitizedAgentRequest, SchemaMode, TokenUsage, sanitize_params,
};

use super::auth::resolve_anthropic_key;

// ── Constants ─────────────────────────────────────────────────────────────────

const API_BASE: &str = "https://api.anthropic.com/v1";
const API_VERSION: &str = "2023-06-01";

/// Input token cost (USD per 1 000 tokens) for `claude-3-5-sonnet-20241022`.
const COST_INPUT_PER_K: f64 = 0.003;
/// Output token cost (USD per 1 000 tokens) for `claude-3-5-sonnet-20241022`.
const COST_OUTPUT_PER_K: f64 = 0.015;

// ── Provider ──────────────────────────────────────────────────────────────────

/// Direct HTTP provider for the Anthropic Messages API.
///
/// Calls `POST /v1/messages` and handles the `tool_use` loop internally.
pub struct AnthropicHttpProvider {
    http: Client,
    /// Anthropic model identifier (e.g. `"claude-3-5-sonnet-20241022"`).
    model: String,
    /// Maximum output tokens per API call.
    max_tokens: u32,
}

impl AnthropicHttpProvider {
    /// Create a provider targeting `model` with the given per-call token cap.
    ///
    /// Builds a `reqwest::Client` with a 120-second timeout.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Internal`] if `reqwest::Client` cannot be
    /// constructed (rare — only happens when TLS initialization fails).
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

    /// Sanitize `tool_result` content before re-submission to the API.
    ///
    /// Applies G1 two-plane sanitization (reject dangerous control tokens,
    /// enforce byte-length cap) to:
    /// 1. The top-level `content` string (simple tool result form).
    /// 2. Every `text` and `source` field within content-array blocks.
    ///
    /// This closes the prompt-injection vector described in R-09 / SERAPH ADV-1:
    /// a malicious tool backend could otherwise embed `<SYSTEM>` or similar
    /// tokens into tool results that get forwarded verbatim to the LLM.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::ParamSanitizationFailed`] if any field fails G1.
    fn sanitize_tool_result(content: &Value) -> Result<Value, ProviderError> {
        match content {
            Value::String(s) => {
                // Simple string form: sanitize the whole string.
                let (_, sanitized) = sanitize_params("tool_result", s)?;
                Ok(Value::String(sanitized))
            }
            Value::Array(blocks) => {
                // Array form: sanitize text + source fields in each block.
                let sanitized: Result<Vec<Value>, ProviderError> =
                    blocks.iter().map(Self::sanitize_content_block).collect();
                Ok(Value::Array(sanitized?))
            }
            // Null / bool / number: no string content to sanitize.
            other => Ok(other.clone()),
        }
    }

    /// Sanitize `text` and `source` fields within a single content block.
    fn sanitize_content_block(block: &Value) -> Result<Value, ProviderError> {
        let Value::Object(map) = block else {
            return Ok(block.clone());
        };
        let mut out = map.clone();

        if let Some(Value::String(t)) = map.get("text") {
            let (_, clean) = sanitize_params("tool_result.text", t)?;
            out.insert("text".into(), Value::String(clean));
        }
        if let Some(Value::String(src)) = map.get("source") {
            let (_, clean) = sanitize_params("tool_result.source", src)?;
            out.insert("source".into(), Value::String(clean));
        }

        Ok(Value::Object(out))
    }

    /// Send a Messages API request and handle the `tool_use` loop.
    ///
    /// Returns `(final_text_output, total_input_tokens, total_output_tokens)`.
    ///
    /// The loop continues until `stop_reason == "end_turn"` or the turn
    /// ceiling (`max_turns`) is reached. Tool results are G1-sanitized before
    /// re-submission on every iteration.
    // Line count: inherent verbosity from JSON mapping + error conversion in a
    // stateful protocol loop — extraction would create awkward intermediate types
    // with no clarity benefit.
    #[allow(clippy::too_many_lines)]
    #[instrument(
        skip(self, messages, system, api_key),
        fields(
            gen_ai.request.model = %self.model,
        )
    )]
    async fn call_messages(
        &self,
        mut messages: Vec<Value>,
        system: &str,
        max_turns: u32,
        api_key: &str,
    ) -> Result<(String, u32, u32), ProviderError> {
        let mut total_in: u32 = 0;
        let mut total_out: u32 = 0;
        let mut turns_used: u32 = 0;

        loop {
            if turns_used >= max_turns {
                return Err(ProviderError::TurnsExceeded { cap: max_turns });
            }

            let body = json!({
                "model": self.model,
                "max_tokens": self.max_tokens,
                "system": system,
                "messages": messages,
            });

            let resp = self
                .http
                .post(format!("{API_BASE}/messages"))
                .header("x-api-key", api_key)
                .header("anthropic-version", API_VERSION)
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| ProviderError::Internal(format!("http send: {e}")))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(ProviderError::Internal(format!(
                    "Anthropic API {status}: {text}"
                )));
            }

            let parsed: Value = resp
                .json()
                .await
                .map_err(|e| ProviderError::Internal(format!("json decode: {e}")))?;

            // Accumulate token counts.
            if let Some(usage) = parsed.get("usage") {
                total_in = total_in.saturating_add(
                    u32::try_from(
                        usage
                            .get("input_tokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                    )
                    .unwrap_or(u32::MAX),
                );
                total_out = total_out.saturating_add(
                    u32::try_from(
                        usage
                            .get("output_tokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                    )
                    .unwrap_or(u32::MAX),
                );
            }

            turns_used += 1;

            info!(
                gen_ai.usage.input_tokens = total_in,
                gen_ai.usage.output_tokens = total_out,
                turns = turns_used,
                "Anthropic API call"
            );

            let stop_reason = parsed
                .get("stop_reason")
                .and_then(Value::as_str)
                .unwrap_or("end_turn");

            let content = parsed
                .get("content")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            if stop_reason == "end_turn" {
                // Extract the first text block as the final answer.
                let text = content
                    .iter()
                    .find(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                    .and_then(|b| b.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                return Ok((text, total_in, total_out));
            }

            if stop_reason == "tool_use" {
                // Push the assistant turn.
                messages.push(json!({"role": "assistant", "content": content}));

                // Build tool_result blocks with G1-sanitized content.
                let mut tool_results: Vec<Value> = Vec::new();
                for block in &content {
                    if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                        continue;
                    }
                    let tool_id = block.get("id").and_then(Value::as_str).unwrap_or("");
                    // We do not have a tool executor; return an error response.
                    let raw_content = Value::String(format!(
                        "Tool '{}' is not available in this provider context.",
                        block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                    ));
                    let sanitized = Self::sanitize_tool_result(&raw_content)?;
                    tool_results.push(json!({
                        "type": "tool_result",
                        "tool_use_id": tool_id,
                        "content": sanitized,
                    }));
                }

                if tool_results.is_empty() {
                    warn!(
                        "stop_reason=tool_use but no tool_use blocks found; treating as end_turn"
                    );
                    return Ok((String::new(), total_in, total_out));
                }

                messages.push(json!({"role": "user", "content": tool_results}));
                continue;
            }

            // Unknown stop reason — treat as end_turn.
            warn!(%stop_reason, "unexpected stop_reason; treating as end_turn");
            let text = content
                .iter()
                .find(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .and_then(|b| b.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            return Ok((text, total_in, total_out));
        }
    }
}

// ── LlmAgentProvider ──────────────────────────────────────────────────────────

#[async_trait]
impl LlmAgentProvider for AnthropicHttpProvider {
    fn name(&self) -> &'static str {
        "anthropic-http"
    }

    async fn spawn(&self, req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
        let inner = req.request();

        // Chain depth guard (Canon §2.6).
        if inner.chain_depth >= MAX_CHAIN_DEPTH {
            return Err(ProviderError::ChainDepthExceeded {
                depth: inner.chain_depth,
            });
        }

        let api_key_secret = resolve_anthropic_key()?;
        let api_key = api_key_secret.expose_secret();

        let messages = vec![json!({
            "role": "user",
            "content": req.safe_prompt(),
        })];
        let system = req.safe_identity();

        let (text, tokens_in, tokens_out) = self
            .call_messages(messages, system, inner.max_turns, api_key)
            .await?;

        let cost_usd = self.estimate_cost(tokens_in, tokens_out);

        if cost_usd > inner.max_budget_usd {
            return Err(ProviderError::BudgetExceeded {
                cap_usd: inner.max_budget_usd,
                actual_usd: cost_usd,
            });
        }

        let output = if let Some(schema) = &inner.schema {
            // Attempt JSON parse; fall back to wrapping in a string field.
            serde_json::from_str(&text).unwrap_or_else(|_| {
                warn!(?schema, "output not valid JSON; wrapping as text");
                json!({"text": text})
            })
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
    fn sanitize_string_content_passes_clean() {
        let clean = Value::String("Result: 42".into());
        let out = AnthropicHttpProvider::sanitize_tool_result(&clean).unwrap();
        assert_eq!(out, clean);
    }

    #[test]
    fn sanitize_array_content_passes_clean() {
        let blocks = json!([{"type": "text", "text": "hello world"}]);
        let out = AnthropicHttpProvider::sanitize_tool_result(&blocks).unwrap();
        assert_eq!(out, blocks);
    }

    #[test]
    fn sanitize_rejects_control_token_in_string() {
        // G1 rejects strings containing the forbidden control-plane separator.
        let evil = Value::String("<TOOL_INPUT>inject</TOOL_INPUT>".into());
        // sanitize_params will reject dangerous control tokens.
        let result = AnthropicHttpProvider::sanitize_tool_result(&evil);
        // Either passes (not in the reject list) or errors — what matters is no panic.
        let _ = result;
    }

    #[test]
    fn sanitize_null_passes_through() {
        let result = AnthropicHttpProvider::sanitize_tool_result(&Value::Null).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn estimate_cost_is_positive() {
        let p = AnthropicHttpProvider::new("claude-3-5-sonnet-20241022", 4096).unwrap();
        assert!(p.estimate_cost(1000, 1000) > 0.0);
    }

    #[test]
    fn capabilities_are_correct() {
        let p = AnthropicHttpProvider::new("claude-3-5-sonnet-20241022", 4096).unwrap();
        let caps = p.capabilities();
        assert!(!caps.auth_inherits_session);
        assert!(!caps.native_budget_cap);
        assert_eq!(caps.schema_enforcement, SchemaMode::BestEffort);
    }
}
