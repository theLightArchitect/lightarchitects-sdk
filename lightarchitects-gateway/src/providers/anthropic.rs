//! Anthropic Messages API HTTP provider — native SSE streaming via `/v1/messages`.
//!
//! [`AnthropicHttpProvider`] implements [`lightarchitects::agent::LlmAgentProvider`]
//! with a streaming-first design: `spawn_streaming()` drives the SSE byte stream
//! through [`LineSplitter`] → [`parse_sse_line`] and yields [`ProviderEvent`]s.
//! `spawn()` delegates to `spawn_streaming()` and collects events into a batch
//! [`AgentResponse`].

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::stream::{BoxStream, StreamExt as _};
use lightarchitects::agent::messages_stream_parser::framing::LineSplitter;
use lightarchitects::agent::messages_stream_parser::sse::parse_sse_line;
use lightarchitects::agent::{
    AgentResponse, LlmAgentProvider, NullToolExecutor, ProviderCapabilities, ProviderError,
    ProviderEvent, SanitizedAgentRequest, SchemaMode, TokenUsage, ToolDefinition, ToolExecutor,
};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

use crate::llm::resolve_key;

/// HTTP timeout for a single SSE streaming call.
const STREAM_TIMEOUT: Duration = Duration::from_secs(300);

/// Anthropic `/v1/messages` streaming provider.
///
/// Calls the Anthropic Messages API with `"stream": true` and parses the
/// resulting SSE event stream into [`ProviderEvent`]s. Optionally holds a
/// [`ToolExecutor`] to include tool definitions in every request.
pub struct AnthropicHttpProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
    /// API key — held in a `Zeroizing` wrapper so it is wiped from heap on drop.
    api_key: Zeroizing<String>,
    /// Tool executor — [`NullToolExecutor`] when no tools are wired.
    tool_executor: Arc<dyn ToolExecutor>,
}

impl fmt::Debug for AnthropicHttpProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnthropicHttpProvider")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl AnthropicHttpProvider {
    /// Create a provider with an explicit model and API key.
    ///
    /// Uses [`NullToolExecutor`] — tools are disabled by default.
    /// Wrap with [`AnthropicHttpProvider::with_tools`] to attach an executor.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be constructed.
    pub fn new(model: impl Into<String>, api_key: impl Into<String>) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(STREAM_TIMEOUT)
            .build()
            .map_err(|e| format!("failed to build Anthropic HTTP client: {e}"))?;
        Ok(Self {
            client,
            base_url: "https://api.anthropic.com".to_owned(),
            model: model.into(),
            api_key: Zeroizing::new(api_key.into()),
            tool_executor: Arc::new(NullToolExecutor),
        })
    }

    /// Attach a [`ToolExecutor`] — tool definitions will be included in every
    /// `/v1/messages` request and the model may emit `tool_use` events.
    #[must_use]
    pub fn with_tools(mut self, executor: Arc<dyn ToolExecutor>) -> Self {
        self.tool_executor = executor;
        self
    }

    /// Construct from environment variables, reading:
    /// - `ANTHROPIC_API_KEY` (or `LA_LLM_API_KEY` fallback)
    /// - `LA_LLM_MODEL` (or default `"claude-sonnet-4-6"`)
    ///
    /// # Errors
    ///
    /// Returns an error if the API key is absent or the HTTP client fails.
    pub fn from_env() -> Result<Self, String> {
        let api_key = resolve_key("ANTHROPIC_API_KEY")
            .or_else(|| resolve_key("LA_LLM_API_KEY"))
            .ok_or_else(|| {
                "ANTHROPIC_API_KEY not set — cannot construct AnthropicHttpProvider".to_owned()
            })?;
        let model =
            std::env::var("LA_LLM_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_owned());
        Self::new(model, api_key)
    }

    /// Build the `/v1/messages` JSON request body for the given request.
    fn build_body(&self, req: &SanitizedAgentRequest) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": 8192_u32,
            "messages": [{"role": "user", "content": req.safe_prompt()}],
            "stream": true,
        });

        // Only set `system` when identity is non-empty — Anthropic rejects empty system prompts.
        if !req.safe_identity().is_empty() {
            body["system"] = serde_json::Value::String(req.safe_identity().to_owned());
        }

        let tool_defs: Vec<ToolDefinition> = self.tool_executor.tool_definitions();
        if !tool_defs.is_empty() {
            body["tools"] = serde_json::Value::Array(
                tool_defs
                    .iter()
                    .map(|def| {
                        serde_json::json!({
                            "name": def.name,
                            "description": def.description,
                            "input_schema": def.input_schema,
                        })
                    })
                    .collect(),
            );
        }

        body
    }
}

#[async_trait]
impl LlmAgentProvider for AnthropicHttpProvider {
    fn name(&self) -> &'static str {
        "anthropic-http"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            schema_enforcement: SchemaMode::BestEffort,
            native_budget_cap: false,
            native_turn_cap: false,
            auth_inherits_session: false,
        }
    }

    /// Estimate USD cost using Anthropic claude-sonnet-4-6 public rate table.
    ///
    /// Rates: $3 / `MTok` input · $15 / `MTok` output (as of 2026-05).
    fn estimate_cost(&self, input_tokens: u32, max_output_tokens: u32) -> f64 {
        let input_cost = f64::from(input_tokens) * 3.0 / 1_000_000.0;
        let output_cost = f64::from(max_output_tokens) * 15.0 / 1_000_000.0;
        input_cost + output_cost
    }

    async fn spawn_streaming(
        &self,
        req: SanitizedAgentRequest,
    ) -> Result<BoxStream<'static, ProviderEvent>, ProviderError> {
        let url = format!("{}/v1/messages", self.base_url);
        let body = self.build_body(&req);

        // Clone the key into a local Zeroizing wrapper for the async block.
        let api_key_val = Zeroizing::new(self.api_key.as_str().to_owned());

        let response = self
            .client
            .post(&url)
            .header("x-api-key", api_key_val.as_str())
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                ProviderError::Internal(format!("Anthropic /v1/messages HTTP error: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            // `.text()` consumes the response — safe since we already checked status.
            let text = response.text().await.unwrap_or_default();
            return Err(ProviderError::Internal(format!(
                "Anthropic /v1/messages returned {status}: {text}"
            )));
        }

        let (tx, rx) = mpsc::channel::<ProviderEvent>(64);

        // Background task: byte stream → line splitter → SSE parser → channel.
        let byte_stream = response.bytes_stream();
        tokio::spawn(async move {
            let mut splitter = LineSplitter::new();
            tokio::pin!(byte_stream);

            'outer: while let Some(chunk) = byte_stream.next().await {
                let bytes = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::error!(provider = "anthropic-http", "SSE read error: {e}");
                        break 'outer;
                    }
                };

                let lines = match splitter.push_bytes(&bytes) {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!(provider = "anthropic-http", "SSE framing error: {e}");
                        break 'outer;
                    }
                };

                for line in lines {
                    match parse_sse_line(&line) {
                        Ok(Some(ev)) => {
                            if tx.send(ev).await.is_err() {
                                // Receiver was dropped — caller cancelled; stop silently.
                                return;
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            tracing::warn!(provider = "anthropic-http", "SSE parse error: {e}");
                        }
                    }
                }
            }

            // Flush any partial final line (rare but possible on abrupt stream end).
            if let Some(line) = splitter.flush() {
                if let Ok(Some(ev)) = parse_sse_line(&line) {
                    let _ = tx.send(ev).await;
                }
            }
        });

        // Convert channel receiver to a `BoxStream` via `unfold`.
        let stream =
            futures_util::stream::unfold(
                rx,
                |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
            );

        Ok(stream.boxed())
    }

    /// Batch invocation: drives `spawn_streaming` and collects events.
    async fn spawn(&self, req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
        let stream = self.spawn_streaming(req).await?;
        tokio::pin!(stream);

        let mut text = String::new();
        let mut model_name = self.model.clone();
        let mut input_tokens = 0_u32;
        let mut output_tokens = 0_u32;
        let mut stop_reason = "end_turn".to_owned();

        while let Some(ev) = stream.next().await {
            match ev {
                ProviderEvent::MessageStart {
                    model: m,
                    input_tokens: i,
                } => {
                    model_name = m;
                    input_tokens = i;
                }
                ProviderEvent::TextDelta { text: t, .. } => {
                    text.push_str(&t);
                }
                ProviderEvent::MessageDelta {
                    stop_reason: sr,
                    output_tokens: o,
                } => {
                    stop_reason = sr;
                    output_tokens = o;
                }
                _ => {}
            }
        }

        let cost = self.estimate_cost(input_tokens, output_tokens);
        let mut attrs = HashMap::new();
        attrs.insert("model".to_owned(), serde_json::Value::String(model_name));
        attrs.insert(
            "stop_reason".to_owned(),
            serde_json::Value::String(stop_reason),
        );

        Ok(AgentResponse {
            output: serde_json::Value::String(text),
            turns_used: 1,
            cost_usd: cost,
            tokens: TokenUsage {
                input: input_tokens,
                output: output_tokens,
            },
            provider_attrs: attrs,
            retry_count: 0,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn new_builds_successfully() {
        let provider = AnthropicHttpProvider::new("claude-sonnet-4-6", "test-key").unwrap();
        assert_eq!(provider.name(), "anthropic-http");
    }

    #[test]
    fn estimate_cost_is_non_negative() {
        let provider = AnthropicHttpProvider::new("claude-sonnet-4-6", "key").unwrap();
        let cost = provider.estimate_cost(1000, 500);
        assert!(cost > 0.0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn estimate_cost_zero_tokens() {
        let provider = AnthropicHttpProvider::new("claude-sonnet-4-6", "key").unwrap();
        // Zero tokens → zero cost: exact integer arithmetic, no float rounding.
        let cost = provider.estimate_cost(0, 0);
        assert_eq!(cost, 0.0_f64);
    }

    #[test]
    fn build_body_sets_stream_true() {
        use lightarchitects::agent::AgentRequest;

        let provider = AnthropicHttpProvider::new("claude-sonnet-4-6", "key").unwrap();
        let req = AgentRequest {
            sibling_identity: "test system".to_owned(),
            user_prompt: "hello".to_owned(),
            schema: None,
            allowed_tools: vec![],
            max_turns: 1,
            max_budget_usd: 1.0,
            model_hint: None,
            parent_span_id: None,
            chain_origin: None,
            chain_depth: 0,
            aud: None,
        }
        .sanitize()
        .unwrap();

        let body = provider.build_body(&req);
        assert_eq!(body["stream"], serde_json::Value::Bool(true));
        assert!(body["system"].as_str().is_some());
    }

    #[test]
    fn build_body_omits_system_when_empty_identity() {
        use lightarchitects::agent::AgentRequest;

        let provider = AnthropicHttpProvider::new("claude-sonnet-4-6", "key").unwrap();
        let req = AgentRequest {
            sibling_identity: String::new(),
            user_prompt: "hi".to_owned(),
            schema: None,
            allowed_tools: vec![],
            max_turns: 1,
            max_budget_usd: 1.0,
            model_hint: None,
            parent_span_id: None,
            chain_origin: None,
            chain_depth: 0,
            aud: None,
        }
        .sanitize()
        .unwrap();

        let body = provider.build_body(&req);
        assert!(body.get("system").is_none());
    }

    #[test]
    fn capabilities_returns_best_effort_schema() {
        let provider = AnthropicHttpProvider::new("claude-sonnet-4-6", "key").unwrap();
        assert_eq!(
            provider.capabilities().schema_enforcement,
            SchemaMode::BestEffort
        );
    }
}
