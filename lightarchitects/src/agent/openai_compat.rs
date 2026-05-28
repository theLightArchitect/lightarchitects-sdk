#![allow(clippy::doc_markdown)] // product names (OpenAI, OpenRouter, RunPod, Fireworks, etc.) throughout
//! OpenAI-compatible HTTP streaming provider.
//!
//! [`OpenAICompatProvider`] implements [`LlmAgentProvider`] against any endpoint
//! that speaks the OpenAI `/chat/completions` SSE wire format — including RunPod
//! vLLM workers, Together AI, Fireworks, OpenRouter, native OpenAI, and
//! self-hosted vLLM.
//!
//! # Configuration — generic endpoint (env vars)
//!
//! | Variable | Required | Description |
//! |----------|----------|-------------|
//! | `LA_OPENAI_COMPAT_BASE_URL` | Yes | Base URL ending **without** trailing slash (e.g. `https://api.runpod.ai/v2/<id>/openai/v1`) |
//! | `LA_OPENAI_COMPAT_API_KEY` | Yes | Bearer token |
//! | `LA_OPENAI_COMPAT_MODEL` | No | Model name; defaults to `"default"` |
//!
//! # Flavor-aware constructors
//!
//! For named providers use the convenience constructors:
//!
//! ```no_run
//! # use lightarchitects::agent::openai_compat::OpenAICompatProvider;
//! // OpenRouter
//! let p = OpenAICompatProvider::for_openrouter("sk-or-…", "anthropic/claude-sonnet-4.6").unwrap();
//! // Native OpenAI
//! let p = OpenAICompatProvider::for_openai("sk-…", "gpt-5").unwrap();
//! ```
//!
//! # Security gates
//!
//! - G1: accepts only [`SanitizedAgentRequest`] (compile-time proof of sanitization).
//! - S-auth: API key stored in [`Zeroizing`] wrapper — zeroed on drop, never logged.
//! - Hard timeout: [`STREAM_TIMEOUT`] (5 min) on the entire SSE stream.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::stream::{BoxStream, StreamExt as _};
use zeroize::Zeroizing;

use crate::agent::messages_stream_parser::framing::LineSplitter;
use crate::agent::messages_stream_parser::openai_sse::{OpenAiStreamState, parse_openai_sse_line};
use crate::agent::{
    AgentResponse, LlmAgentProvider, NullToolExecutor, OpenAIFlavor, ProviderCapabilities,
    ProviderError, ProviderEvent, SanitizedAgentRequest, SchemaMode, TokenUsage, ToolDefinition,
    ToolExecutor,
};

/// Hard timeout for a single streaming call.
const STREAM_TIMEOUT: Duration = Duration::from_secs(300);

/// Default input token rate: $0.80 / `MTok` (H100 vLLM mid-size model baseline).
const DEFAULT_INPUT_USD_PER_M: f64 = 0.80;

/// Default output token rate: $1.60 / `MTok`.
const DEFAULT_OUTPUT_USD_PER_M: f64 = 1.60;

/// OpenRouter app-attribution referer sent when `flavor == OpenRouter`.
const OPENROUTER_REFERER: &str = "https://github.com/TheLightArchitects/lightarchitects-sdk";

/// OpenRouter app-attribution title sent when `flavor == OpenRouter`.
const OPENROUTER_TITLE: &str = "Light Architects";

/// OpenAI-compatible HTTP streaming provider.
///
/// Calls `POST <base_url>/chat/completions` with `"stream": true` and parses
/// the SSE response via [`OpenAiStreamState`] into [`ProviderEvent`]s.
pub struct OpenAICompatProvider {
    client: reqwest::Client,
    /// Provider flavor — controls default URL, env-var naming, and
    /// optional attribution headers.
    flavor: OpenAIFlavor,
    /// Base URL — no trailing slash. `/chat/completions` is appended at call time.
    base_url: String,
    model: String,
    /// Bearer token — zeroed on drop.
    api_key: Zeroizing<String>,
    tool_executor: Arc<dyn ToolExecutor>,
    input_usd_per_m: f64,
    output_usd_per_m: f64,
}

impl fmt::Debug for OpenAICompatProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAICompatProvider")
            .field("flavor", &self.flavor)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl OpenAICompatProvider {
    /// Construct a provider with an explicit flavor, credentials, and base URL.
    ///
    /// Uses the flavor's `default_base_url()` when `base_url` is `None`.
    /// [`OpenAIFlavor::Generic`] requires an explicit `base_url`.
    ///
    /// # Errors
    ///
    /// Returns an error if `base_url` is empty for `Generic` flavor, or if the
    /// HTTP client cannot be built.
    pub fn with_flavor(
        flavor: OpenAIFlavor,
        base_url: Option<impl Into<String>>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, String> {
        let resolved_url =
            base_url.map_or_else(|| flavor.default_base_url().to_owned(), Into::into);

        if resolved_url.is_empty() {
            return Err(format!(
                "OpenAIFlavor::{flavor:?} requires an explicit base_url — none was supplied"
            ));
        }

        let client = reqwest::Client::builder()
            .timeout(STREAM_TIMEOUT)
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;

        Ok(Self {
            client,
            flavor,
            base_url: resolved_url.trim_end_matches('/').to_owned(),
            model: model.into(),
            api_key: Zeroizing::new(api_key.into()),
            tool_executor: Arc::new(NullToolExecutor),
            input_usd_per_m: DEFAULT_INPUT_USD_PER_M,
            output_usd_per_m: DEFAULT_OUTPUT_USD_PER_M,
        })
    }

    /// Construct a generic provider with explicit credentials and default H100 rate table.
    ///
    /// Uses [`NullToolExecutor`] — attach tools with [`with_tools`].
    ///
    /// [`with_tools`]: OpenAICompatProvider::with_tools
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn new(
        model: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self, String> {
        Self::with_flavor(OpenAIFlavor::Generic, Some(base_url.into()), api_key, model)
    }

    /// Construct an OpenRouter-flavored provider.
    ///
    /// Uses `https://openrouter.ai/api/v1` as the base URL and attaches the
    /// required `HTTP-Referer` and `X-Title` attribution headers per OpenRouter
    /// documentation.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn for_openrouter(
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, String> {
        Self::with_flavor(OpenAIFlavor::OpenRouter, None::<String>, api_key, model)
    }

    /// Construct a native OpenAI-flavored provider.
    ///
    /// Uses `https://api.openai.com/v1` as the base URL.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn for_openai(
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, String> {
        Self::with_flavor(OpenAIFlavor::OpenAi, None::<String>, api_key, model)
    }

    /// Construct a LiteLLM-flavored provider.
    ///
    /// Uses `http://localhost:4000/v1` as the default base URL (override with
    /// an explicit `base_url`).
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn for_litellm(
        base_url: Option<impl Into<String>>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, String> {
        Self::with_flavor(OpenAIFlavor::LiteLLM, base_url, api_key, model)
    }

    /// Construct from environment variables.
    ///
    /// Reads `LA_OPENAI_COMPAT_BASE_URL`, `LA_OPENAI_COMPAT_API_KEY`, and
    /// optionally `LA_OPENAI_COMPAT_MODEL` (defaults to `"default"`).
    ///
    /// # Errors
    ///
    /// Returns an error if `LA_OPENAI_COMPAT_BASE_URL` or
    /// `LA_OPENAI_COMPAT_API_KEY` are absent or if the HTTP client fails.
    pub fn from_env() -> Result<Self, String> {
        let base_url = std::env::var("LA_OPENAI_COMPAT_BASE_URL").map_err(|_| {
            "LA_OPENAI_COMPAT_BASE_URL not set — cannot construct OpenAICompatProvider".to_owned()
        })?;
        let api_key = std::env::var("LA_OPENAI_COMPAT_API_KEY").map_err(|_| {
            "LA_OPENAI_COMPAT_API_KEY not set — cannot construct OpenAICompatProvider".to_owned()
        })?;
        let model =
            std::env::var("LA_OPENAI_COMPAT_MODEL").unwrap_or_else(|_| "default".to_owned());
        Self::new(model, base_url, api_key)
    }

    /// Construct from a named flavor's environment variables.
    ///
    /// Uses `LIGHTSQUAD_SUPERVISOR_BASE_URL` → `<FLAVOR>_BASE_URL` → flavor
    /// default for the URL, and `LIGHTSQUAD_SUPERVISOR_API_KEY` →
    /// `<FLAVOR_API_KEY_ENV>` for the key.
    ///
    /// # Errors
    ///
    /// Returns an error if required env vars are absent, or if the base URL
    /// is empty for a `Generic` flavor.
    pub fn from_env_with_flavor(flavor: OpenAIFlavor) -> Result<Self, String> {
        let base_url = std::env::var("LA_OPENAI_COMPAT_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| flavor.default_base_url().to_owned());

        let model =
            std::env::var("LA_OPENAI_COMPAT_MODEL").unwrap_or_else(|_| "default".to_owned());

        let api_key_env = flavor.default_api_key_env();
        let api_key = std::env::var("LA_OPENAI_COMPAT_API_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                if api_key_env.is_empty() {
                    None
                } else {
                    std::env::var(api_key_env).ok().filter(|s| !s.is_empty())
                }
            })
            .ok_or_else(|| {
                format!(
                    "no API key found: set LA_OPENAI_COMPAT_API_KEY{}",
                    if api_key_env.is_empty() {
                        String::new()
                    } else {
                        format!(" or {api_key_env}")
                    }
                )
            })?;

        Self::with_flavor(flavor, Some(base_url), api_key, model)
    }

    /// Attach a [`ToolExecutor`].
    ///
    /// Tool definitions are forwarded in the OpenAI `"tools"` array with
    /// `"type": "function"`. Requires `ENABLE_AUTO_TOOL_CHOICE=true` on the
    /// vLLM endpoint side.
    #[must_use]
    pub fn with_tools(mut self, executor: Arc<dyn ToolExecutor>) -> Self {
        self.tool_executor = executor;
        self
    }

    /// Override the per-token rate table (USD per million tokens).
    ///
    /// Call this when the deployed model has known published rates that differ
    /// from the conservative H100 defaults.
    #[must_use]
    pub fn with_rates(mut self, input_usd_per_m: f64, output_usd_per_m: f64) -> Self {
        self.input_usd_per_m = input_usd_per_m;
        self.output_usd_per_m = output_usd_per_m;
        self
    }

    /// Build the `/chat/completions` JSON request body.
    fn build_body(&self, req: &SanitizedAgentRequest) -> serde_json::Value {
        // System prompt goes as the first message when non-empty.
        let mut messages: Vec<serde_json::Value> = Vec::new();
        if !req.safe_identity().is_empty() {
            messages.push(serde_json::json!({
                "role": "system",
                "content": req.safe_identity(),
            }));
        }
        messages.push(serde_json::json!({
            "role": "user",
            "content": req.safe_prompt(),
        }));

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": 8192_u32,
            "messages": messages,
            "stream": true,
            // Request usage stats on the final chunk (supported by vLLM ≥ 0.4).
            "stream_options": { "include_usage": true },
        });

        // Structured output enforcement — forwarded when the caller supplies a schema.
        if let Some(schema) = req.request().schema.as_ref() {
            body["response_format"] = serde_json::json!({
                "type": "json_schema",
                "json_schema": {
                    "name": "response",
                    "strict": true,
                    "schema": schema,
                }
            });
        }

        // Attach tool definitions in OpenAI format.
        let tool_defs: Vec<ToolDefinition> = self.tool_executor.tool_definitions();
        if !tool_defs.is_empty() {
            body["tools"] = serde_json::Value::Array(
                tool_defs
                    .iter()
                    .map(|def| {
                        serde_json::json!({
                            "type": "function",
                            "function": {
                                "name": def.name,
                                "description": def.description,
                                "parameters": def.input_schema,
                            },
                        })
                    })
                    .collect(),
            );
            body["tool_choice"] = serde_json::Value::String("auto".to_owned());
        }

        body
    }
}

#[async_trait]
impl LlmAgentProvider for OpenAICompatProvider {
    fn name(&self) -> &'static str {
        self.flavor.as_str()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            schema_enforcement: SchemaMode::BestEffort,
            native_budget_cap: false,
            native_turn_cap: false,
            // Caller supplies the API key explicitly — no session inheritance.
            auth_inherits_session: false,
        }
    }

    /// Estimate USD cost at the configured per-token rates.
    fn estimate_cost(&self, input_tokens: u32, max_output_tokens: u32) -> f64 {
        (f64::from(input_tokens) / 1_000_000.0 * self.input_usd_per_m)
            + (f64::from(max_output_tokens) / 1_000_000.0 * self.output_usd_per_m)
    }

    async fn spawn_streaming(
        &self,
        req: SanitizedAgentRequest,
    ) -> Result<BoxStream<'static, ProviderEvent>, ProviderError> {
        let url = format!("{}/chat/completions", self.base_url);
        let body = self.build_body(&req);

        // Clone key into a local Zeroizing wrapper for the async block.
        let api_key_val = Zeroizing::new(self.api_key.as_str().to_owned());
        let needs_or_headers = self.flavor.needs_openrouter_headers();

        let mut request_builder = self
            .client
            .post(&url)
            .bearer_auth(api_key_val.as_str())
            .header("content-type", "application/json")
            .header("accept", "text/event-stream");

        // OpenRouter app-attribution headers — optional but improve rate-limit
        // treatment per OpenRouter documentation.
        if needs_or_headers {
            request_builder = request_builder
                .header("HTTP-Referer", OPENROUTER_REFERER)
                .header("X-Title", OPENROUTER_TITLE);
        }

        let response = request_builder.json(&body).send().await.map_err(|e| {
            ProviderError::Internal(format!("OpenAI-compat /chat/completions HTTP error: {e}"))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();

            // 401/403 → AuthFailure so callers can surface credential errors distinctly.
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(ProviderError::AuthFailure(format!(
                    "OpenAI-compat endpoint returned {status}: {text}"
                )));
            }
            return Err(ProviderError::Internal(format!(
                "OpenAI-compat /chat/completions returned {status}: {text}"
            )));
        }

        let (tx, rx) = tokio::sync::mpsc::channel::<ProviderEvent>(64);

        let byte_stream = response.bytes_stream();
        // Background task: bytes → line splitter → OpenAI SSE parser → state mapper → channel.
        tokio::spawn(async move {
            let mut splitter = LineSplitter::new();
            let mut state = OpenAiStreamState::new();
            tokio::pin!(byte_stream);

            'outer: while let Some(chunk) = byte_stream.next().await {
                let bytes = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::error!(provider = "openai-compat", "SSE read error: {e}");
                        break 'outer;
                    }
                };

                let lines = match splitter.push_bytes(&bytes) {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!(provider = "openai-compat", "SSE framing error: {e}");
                        break 'outer;
                    }
                };

                for line in lines {
                    match parse_openai_sse_line(&line) {
                        Ok(Some(raw_chunk)) => {
                            for ev in state.apply(raw_chunk) {
                                if tx.send(ev).await.is_err() {
                                    // Receiver dropped — caller cancelled; stop silently.
                                    return;
                                }
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            tracing::warn!(provider = "openai-compat", "SSE parse error: {e}");
                        }
                    }
                }
            }

            // Flush partial final line (rare on abrupt stream end).
            if let Some(line) = splitter.flush() {
                if let Ok(Some(raw_chunk)) = parse_openai_sse_line(&line) {
                    for ev in state.apply(raw_chunk) {
                        let _ = tx.send(ev).await;
                    }
                }
            }

            // Guarantee the stream always closes with MessageStart + MessageStop
            // even if the endpoint produced no chunks with a model field.
            // finalize() also emits any buffered MessageDelta with final token counts.
            for ev in state.finalize() {
                let _ = tx.send(ev).await;
            }
            let _ = tx.send(ProviderEvent::MessageStop).await;
        });

        let stream =
            futures_util::stream::unfold(
                rx,
                |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
            );

        Ok(stream.boxed())
    }

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
                ProviderEvent::TextDelta { text: t, .. } => text.push_str(&t),
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_provider() -> OpenAICompatProvider {
        OpenAICompatProvider::new(
            "Qwen/Qwen2.5-Coder-32B-Instruct",
            "https://api.runpod.ai/v2/abc123/openai/v1",
            "rp_test_key",
        )
        .unwrap()
    }

    #[test]
    fn new_builds_successfully() {
        let p = make_provider();
        // Generic flavor → "openai-compat"
        assert_eq!(p.name(), "openai-compat");
    }

    #[test]
    fn for_openrouter_sets_flavor_and_url() {
        let p = OpenAICompatProvider::for_openrouter("sk-or-test", "anthropic/claude-sonnet-4.6")
            .unwrap();
        assert_eq!(p.name(), "openrouter");
        assert!(p.base_url.contains("openrouter.ai"));
    }

    #[test]
    fn for_openai_sets_flavor_and_url() {
        let p = OpenAICompatProvider::for_openai("sk-test", "gpt-5").unwrap();
        assert_eq!(p.name(), "openai");
        assert!(p.base_url.contains("openai.com"));
    }

    #[test]
    fn for_litellm_uses_default_url() {
        let p =
            OpenAICompatProvider::for_litellm(None::<String>, "litellm-key", "llama-3.3").unwrap();
        assert_eq!(p.name(), "litellm");
        assert!(p.base_url.contains("localhost:4000"));
    }

    #[test]
    fn with_flavor_generic_requires_explicit_url() {
        let err = OpenAICompatProvider::with_flavor(
            OpenAIFlavor::Generic,
            None::<String>,
            "key",
            "model",
        );
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("requires an explicit base_url"));
    }

    #[test]
    fn trailing_slash_stripped_from_base_url() {
        let p = OpenAICompatProvider::new("m", "https://example.com/v1/", "key").unwrap();
        assert_eq!(p.base_url, "https://example.com/v1");
    }

    #[test]
    fn estimate_cost_zero_tokens() {
        let p = make_provider();
        assert!(p.estimate_cost(0, 0) < f64::EPSILON);
    }

    #[test]
    fn estimate_cost_positive() {
        let p = make_provider();
        assert!(p.estimate_cost(1_000, 500) > 0.0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn with_rates_overrides_defaults() {
        let p = make_provider().with_rates(2.0, 4.0);
        // 1M input tokens at $2/MTok = $2.00
        assert_eq!(p.estimate_cost(1_000_000, 0), 2.0_f64);
    }

    #[test]
    fn capabilities_no_native_caps() {
        let p = make_provider();
        let caps = p.capabilities();
        assert!(!caps.native_budget_cap);
        assert!(!caps.native_turn_cap);
        assert!(!caps.auth_inherits_session);
        assert_eq!(caps.schema_enforcement, SchemaMode::BestEffort);
    }

    #[test]
    fn build_body_stream_true_and_include_usage() {
        use crate::agent::AgentRequest;

        let p = make_provider();
        let req = AgentRequest {
            sibling_identity: "system prompt".to_owned(),
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
            conversation_history: Vec::new(),
            tool_definitions: Vec::new(),
        }
        .sanitize()
        .unwrap();

        let body = p.build_body(&req);
        assert_eq!(body["stream"], serde_json::Value::Bool(true));
        assert_eq!(
            body["stream_options"]["include_usage"],
            serde_json::Value::Bool(true)
        );
    }

    #[test]
    fn build_body_wires_schema_as_response_format() {
        use crate::agent::AgentRequest;

        let p = make_provider();
        let schema = serde_json::json!({
            "type": "object",
            "properties": { "answer": { "type": "string" } },
            "required": ["answer"]
        });
        let req = AgentRequest {
            sibling_identity: String::new(),
            user_prompt: "q".to_owned(),
            schema: Some(schema.clone()),
            allowed_tools: vec![],
            max_turns: 1,
            max_budget_usd: 1.0,
            model_hint: None,
            parent_span_id: None,
            chain_origin: None,
            chain_depth: 0,
            aud: None,
            conversation_history: Vec::new(),
            tool_definitions: Vec::new(),
        }
        .sanitize()
        .unwrap();

        let body = p.build_body(&req);
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        assert_eq!(body["response_format"]["json_schema"]["schema"], schema);
    }

    #[test]
    fn build_body_no_response_format_when_schema_absent() {
        use crate::agent::AgentRequest;

        let p = make_provider();
        let req = AgentRequest {
            sibling_identity: String::new(),
            user_prompt: "q".to_owned(),
            schema: None,
            allowed_tools: vec![],
            max_turns: 1,
            max_budget_usd: 1.0,
            model_hint: None,
            parent_span_id: None,
            chain_origin: None,
            chain_depth: 0,
            aud: None,
            conversation_history: Vec::new(),
            tool_definitions: Vec::new(),
        }
        .sanitize()
        .unwrap();

        let body = p.build_body(&req);
        assert!(body.get("response_format").is_none());
    }

    #[test]
    fn build_body_system_prompt_as_first_message() {
        use crate::agent::AgentRequest;

        let p = make_provider();
        let req = AgentRequest {
            sibling_identity: "be helpful".to_owned(),
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
            conversation_history: Vec::new(),
            tool_definitions: Vec::new(),
        }
        .sanitize()
        .unwrap();

        let body = p.build_body(&req);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "be helpful");
        assert_eq!(msgs[1]["role"], "user");
    }

    #[test]
    fn build_body_omits_system_when_empty_identity() {
        use crate::agent::AgentRequest;

        let p = make_provider();
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
            conversation_history: Vec::new(),
            tool_definitions: Vec::new(),
        }
        .sanitize()
        .unwrap();

        let body = p.build_body(&req);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn build_body_tools_formatted_as_openai_functions() {
        use crate::agent::AgentRequest;

        // Provide a minimal ToolExecutor stub via NullToolExecutor override.
        struct OneToolExecutor;
        #[async_trait::async_trait]
        impl ToolExecutor for OneToolExecutor {
            fn tool_definitions(&self) -> Vec<ToolDefinition> {
                vec![ToolDefinition {
                    name: "bash".to_owned(),
                    description: "Run a shell command".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": { "command": { "type": "string" } },
                        "required": ["command"]
                    }),
                }]
            }
            async fn execute(
                &self,
                _id: &str,
                _name: &str,
                _input: serde_json::Value,
            ) -> Result<crate::agent::ToolOutput, crate::agent::ToolError> {
                Err(crate::agent::ToolError::ToolsNotAvailable)
            }
        }

        let p = make_provider().with_tools(Arc::new(OneToolExecutor));
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
            conversation_history: Vec::new(),
            tool_definitions: Vec::new(),
        }
        .sanitize()
        .unwrap();

        let body = p.build_body(&req);
        let tools = body["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "bash");
        assert_eq!(body["tool_choice"], "auto");
    }

    #[test]
    #[allow(unsafe_code)]
    fn from_env_missing_base_url_returns_error() {
        // Ensure the env var is absent; guard against CI pollution.
        // SAFETY: this test binary is single-threaded at this point; no other
        // threads concurrently read LA_OPENAI_COMPAT_BASE_URL.
        unsafe {
            std::env::remove_var("LA_OPENAI_COMPAT_BASE_URL");
        }
        let err = OpenAICompatProvider::from_env().unwrap_err();
        assert!(err.contains("LA_OPENAI_COMPAT_BASE_URL"));
    }
}
