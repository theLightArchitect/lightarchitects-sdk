//! Ollama CLI/HTTP provider — [`OllamaCliProvider`] dispatches via `ollama run`
//! subprocess (batch) or the Ollama HTTP API (`spawn_streaming()`).
//!
//! # Streaming paths
//!
//! `spawn_streaming()` tries two HTTP endpoints in order:
//!
//! 1. `/v1/messages` — Anthropic-compatible SSE (Ollama ≥ 0.4). Parsed via
//!    [`LineSplitter`] + [`parse_sse_line`], identical to [`AnthropicHttpProvider`].
//! 2. `/api/chat` — Native Ollama NDJSON fallback (all versions). Parsed via
//!    [`LineSplitter`] + [`ollama_chat_to_provider_events`].
//!
//! # Security controls
//!
//! | Gate | Implementation |
//! |------|----------------|
//! | G1 content-plane | [`sanitize_for_dispatch`]: control chars rejected before subprocess exec |
//! | G10 subprocess hygiene | `kill_on_drop(true)` + `process_group(0)` + `tokio::time::timeout` |
//! | Registry guard | Model slug validated against `CLOUD_MODEL_REGISTRY` before dispatch |
//! | No shell interpolation | `Command::new("ollama")` with args as separate `Vec` items — execve(2) semantics |
//! | H1 HTTP streaming | Dual-path: `/v1/messages` SSE → `/api/chat` NDJSON fallback |
//! | S-auth | `Authorization: Bearer` via `SecretString`; never logged or exposed in spans |
//!
//! [`AnthropicHttpProvider`]: crate::agent::AnthropicHttpProvider

use std::collections::HashMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::stream::{BoxStream, StreamExt as _};
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{info, warn};

use super::cloud_models::{CostTier, lookup};
use super::error::OllamaError;
use super::messages_stream_parser::framing::LineSplitter;
use super::messages_stream_parser::sse::parse_sse_line;
use super::provider::{
    AgentResponse, LlmAgentProvider, ProviderCapabilities, ProviderError, ProviderEvent,
    SanitizedAgentRequest, SchemaMode, TokenUsage,
};
use super::tool_executor::ToolDefinition;
use super::translator::sanitize_prompt;

// ── Rate table (approximate Ollama Cloud pricing by CostTier) ──────────────────
// Exact per-model rates will be locked once Ollama publishes billing details.

const LOW_INPUT_USD_PER_M: f64 = 0.10;
const LOW_OUTPUT_USD_PER_M: f64 = 0.30;
const MEDIUM_INPUT_USD_PER_M: f64 = 0.50;
const MEDIUM_OUTPUT_USD_PER_M: f64 = 1.50;
const HIGH_INPUT_USD_PER_M: f64 = 2.00;
const HIGH_OUTPUT_USD_PER_M: f64 = 6.00;
const PREMIUM_INPUT_USD_PER_M: f64 = 5.00;
const PREMIUM_OUTPUT_USD_PER_M: f64 = 15.00;

/// HTTP timeout for Ollama streaming calls.
const OLLAMA_STREAM_TIMEOUT: Duration = Duration::from_secs(300);

// ── Provider struct ─────────────────────────────────────────────────────────────

/// Dispatches prompts via `ollama run` subprocess (batch) or the Ollama HTTP
/// API (`spawn_streaming()`).
///
/// Requires `ollama` on `PATH` and, for HTTP streaming, an Ollama server
/// reachable at `OLLAMA_HOST` (default `http://localhost:11434`).
///
/// # Example
///
/// ```rust,no_run
/// # use lightarchitects::agent::OllamaCliProvider;
/// let provider = OllamaCliProvider::new("glm-5.1:cloud", None).unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct OllamaCliProvider {
    /// Default model slug — must be present in `CLOUD_MODEL_REGISTRY`.
    pub default_model: String,
    /// Rate-table version tag written to AYIN spans for audit purposes.
    pub rate_table_version: &'static str,
    /// HTTP client for the Ollama REST API.
    client: reqwest::Client,
    /// Ollama server base URL (from `OLLAMA_HOST`; defaults to `https://ollama.com`
    /// for `:cloud` slugs or `http://localhost:11434` for local models).
    base_url: String,
    /// Bearer token for Ollama Cloud (`OLLAMA_API_KEY`). Stored as `SecretString`
    /// so the value is zeroed on drop and never captured in spans or logs.
    auth_token: Option<SecretString>,
}

impl OllamaCliProvider {
    /// Construct a new provider validated against the cloud model registry.
    ///
    /// Reads `OLLAMA_HOST` for the server base URL; falls back to
    /// `http://localhost:11434` when the variable is absent.
    ///
    /// The bearer token is passed explicitly by the caller — the provider does
    /// not read `OLLAMA_API_KEY` from the process environment.  Callers that
    /// need the env value should read it once at startup (eliminates the
    /// per-request TOCTOU race documented in webshell-la-native-backend merge
    /// gate `OLLAMA_API_KEY_TOCTOU`).
    ///
    /// # Errors
    ///
    /// Returns [`OllamaError::UnknownModel`] if `model_slug` is not in
    /// `CLOUD_MODEL_REGISTRY`.
    pub fn new(
        model_slug: impl Into<String>,
        auth_token: Option<SecretString>,
    ) -> Result<Self, OllamaError> {
        let slug = model_slug.into();
        if lookup(&slug).is_none() {
            return Err(OllamaError::UnknownModel(slug));
        }
        let is_cloud = slug.ends_with(":cloud");
        let default_host = if is_cloud {
            "https://ollama.com"
        } else {
            "http://localhost:11434"
        };
        let base_url = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| default_host.to_owned());
        Ok(Self {
            default_model: slug,
            rate_table_version: "2026-05-21",
            client: reqwest::Client::new(),
            base_url,
            auth_token,
        })
    }
}

// ── LlmAgentProvider impl ──────────────────────────────────────────────────────

#[async_trait]
impl LlmAgentProvider for OllamaCliProvider {
    fn name(&self) -> &'static str {
        "ollama-cli"
    }

    /// Dispatch a one-shot prompt via `ollama run <model>`.
    ///
    /// The effective model is `model_hint` when provided, otherwise
    /// `default_model`. Both must be present in `CLOUD_MODEL_REGISTRY`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Internal`] for unknown model, missing CLI, or
    /// subprocess failure. Returns [`ProviderError::SubprocessTimeout`] when the
    /// wall-clock budget is exceeded.
    async fn spawn(&self, req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
        let inner = req.request();
        let model = inner.model_hint.as_deref().unwrap_or(&self.default_model);

        let model_meta = lookup(model).ok_or_else(|| {
            ProviderError::Internal(format!(
                "ollama-cli: unknown model '{model}' (not in CLOUD_MODEL_REGISTRY)"
            ))
        })?;

        let prompt = req.safe_prompt();
        sanitize_prompt(prompt).map_err(|_| ProviderError::ParamSanitizationFailed {
            param_name: "user_prompt".to_owned(),
            reason: "prompt contains characters forbidden by translator sanitizer".to_owned(),
        })?;

        let t0 = Instant::now();
        let raw = dispatch_ollama(prompt, model, inner.max_turns).await?;
        let latency_ms = u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX);

        let input_tokens = u32::try_from(prompt.len() / 4).unwrap_or(u32::MAX);
        let output_tokens = u32::try_from(raw.len() / 4).unwrap_or(u32::MAX);
        let cost = cost_for_tier(model_meta.cost_tier, input_tokens, output_tokens);

        let mut attrs = HashMap::new();
        attrs.insert(
            "model.family".to_owned(),
            Value::String(model_meta.family.to_owned()),
        );
        attrs.insert(
            "model.provider_org".to_owned(),
            Value::String(model_meta.provider_org.to_owned()),
        );
        attrs.insert(
            "model.cost_tier".to_owned(),
            Value::String(model_meta.cost_tier.as_str().to_owned()),
        );
        attrs.insert(
            "agent.provider".to_owned(),
            Value::String("ollama-cli".to_owned()),
        );
        attrs.insert("latency_ms".to_owned(), Value::Number(latency_ms.into()));

        info!(
            provider = "ollama-cli",
            model,
            family = model_meta.family,
            cost_tier = model_meta.cost_tier.as_str(),
            input_tokens,
            output_tokens,
            cost_usd = cost,
            latency_ms,
            "ollama agent call completed"
        );

        Ok(AgentResponse {
            output: Value::String(raw),
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

    /// Stream via Ollama HTTP API with automatic path selection.
    ///
    /// Tries `/v1/messages` (Anthropic-compat SSE) first; on 404 or connection
    /// failure falls back to `/api/chat` (native Ollama NDJSON).
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Internal`] when both paths fail or a non-404
    /// error response is received from `/v1/messages`.
    async fn spawn_streaming(
        &self,
        req: SanitizedAgentRequest,
    ) -> Result<BoxStream<'static, ProviderEvent>, ProviderError> {
        let inner = req.request();
        let model = inner
            .model_hint
            .as_deref()
            .unwrap_or(&self.default_model)
            .to_owned();
        let prompt = req.safe_prompt().to_owned();
        let identity = req.safe_identity().to_owned();
        let client = self.client.clone();
        let base_url = self.base_url.clone();
        // Build messages slice: history (if any) + current user turn.
        // Empty prompt means the user turn is already the last entry in history
        // (agentic loop iteration 2+: tool results absorbed into history).
        let mut msgs = req.request().conversation_history.clone();
        if !prompt.is_empty() {
            msgs.push(serde_json::json!({"role": "user", "content": prompt}));
        }
        let system = if identity.is_empty() {
            None
        } else {
            Some(identity.as_str())
        };
        let tools = if req.request().tool_definitions.is_empty() {
            None
        } else {
            Some(req.request().tool_definitions.as_slice())
        };
        let bearer = self
            .auth_token
            .as_ref()
            .map(|t| format!("Bearer {}", t.expose_secret()));

        // Primary path: /v1/messages (Anthropic-compat SSE, Ollama ≥ 0.4)
        let v1_url = format!("{base_url}/v1/messages");
        let v1_body = build_v1_messages_body(&model, &msgs, system, tools);
        let mut v1_req = client
            .post(&v1_url)
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .timeout(OLLAMA_STREAM_TIMEOUT);
        if let Some(ref token) = bearer {
            v1_req = v1_req.header("authorization", token);
        }
        let v1_result = v1_req.json(&v1_body).send().await;

        match v1_result {
            Ok(resp) if resp.status().is_success() => {
                return Ok(sse_response_to_stream(resp));
            }
            Ok(resp) if resp.status().as_u16() == 404 => {
                warn!(
                    provider = "ollama-http",
                    "Ollama /v1/messages not available (404), falling back to /api/chat"
                );
            }
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(ProviderError::Internal(format!(
                    "Ollama /v1/messages returned {status}: {text}"
                )));
            }
            Err(e) => {
                warn!(
                    provider = "ollama-http",
                    err = %e,
                    "Ollama /v1/messages request failed, falling back to /api/chat"
                );
            }
        }

        // Fallback path: /api/chat (native Ollama NDJSON, all versions)
        let chat_url = format!("{base_url}/api/chat");
        let chat_body = build_api_chat_body(&model, &msgs, system, tools);
        let mut chat_req = client
            .post(&chat_url)
            .header("content-type", "application/json")
            .timeout(OLLAMA_STREAM_TIMEOUT);
        if let Some(ref token) = bearer {
            chat_req = chat_req.header("authorization", token);
        }
        let chat_resp = chat_req.json(&chat_body).send().await.map_err(|e| {
            ProviderError::Internal(format!("Ollama /api/chat request failed: {e}"))
        })?;

        if !chat_resp.status().is_success() {
            let status = chat_resp.status();
            let text = chat_resp.text().await.unwrap_or_default();
            return Err(ProviderError::Internal(format!(
                "Ollama /api/chat returned {status}: {text}"
            )));
        }

        Ok(ndjson_response_to_stream(model, chat_resp))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            // Phase 2: plain text output, no JSON schema enforcement.
            // Phase 3 (ADK layer) upgrades this to BestEffort.
            schema_enforcement: SchemaMode::None,
            native_budget_cap: false,
            native_turn_cap: false,
            auth_inherits_session: false,
        }
    }

    fn estimate_cost(&self, input_tokens: u32, max_output_tokens: u32) -> f64 {
        let tier = lookup(&self.default_model).map_or(CostTier::Medium, |m| m.cost_tier);
        cost_for_tier(tier, input_tokens, max_output_tokens)
    }
}

// ── HTTP streaming helpers ─────────────────────────────────────────────────────

/// Build a `/v1/messages` (Anthropic-compat) request body for Ollama.
///
/// `messages` is a pre-assembled slice of `{"role","content"}` objects already
/// containing history turns followed by the current user message.
/// `tools` are serialized in Anthropic format: `{"name","description","input_schema"}`.
fn build_v1_messages_body(
    model: &str,
    messages: &[serde_json::Value],
    system: Option<&str>,
    tools: Option<&[ToolDefinition]>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": 8192_u32,
        "messages": messages,
        "stream": true,
    });
    if let Some(s) = system {
        body["system"] = serde_json::Value::String(s.to_owned());
    }
    if let Some(defs) = tools {
        let arr: Vec<serde_json::Value> = defs
            .iter()
            .map(|t| serde_json::json!({"name": t.name, "description": t.description, "input_schema": t.input_schema}))
            .collect();
        body["tools"] = serde_json::Value::Array(arr);
    }
    body
}

/// Build a `/api/chat` (native Ollama / Ollama Cloud) request body.
///
/// `messages` is pre-assembled; system prompt is prepended as a `role:system`
/// entry when present. Tools follow `OpenAI` function-calling format.
fn build_api_chat_body(
    model: &str,
    messages: &[serde_json::Value],
    system: Option<&str>,
    tools: Option<&[ToolDefinition]>,
) -> serde_json::Value {
    let mut full_messages: Vec<serde_json::Value> = Vec::new();
    if let Some(s) = system {
        full_messages.push(serde_json::json!({"role": "system", "content": s}));
    }
    full_messages.extend_from_slice(messages);
    let mut body = serde_json::json!({
        "model": model,
        "messages": full_messages,
        "stream": true,
        "options": {"num_ctx": 131_072_u32},
    });
    if let Some(defs) = tools {
        let arr: Vec<serde_json::Value> = defs
            .iter()
            .map(|t| serde_json::json!({"type":"function","function":{"name":t.name,"description":t.description,"parameters":t.input_schema}}))
            .collect();
        body["tools"] = serde_json::Value::Array(arr);
    }
    body
}

/// Convert a successful `/v1/messages` SSE response into a `ProviderEvent` stream.
///
/// Spawns a background task that feeds bytes through [`LineSplitter`] +
/// [`parse_sse_line`]; the same pipeline used by `AnthropicHttpProvider`.
fn sse_response_to_stream(response: reqwest::Response) -> BoxStream<'static, ProviderEvent> {
    let (tx, rx) = mpsc::channel::<ProviderEvent>(64);

    tokio::spawn(async move {
        let mut splitter = LineSplitter::new();
        let byte_stream = response.bytes_stream();
        tokio::pin!(byte_stream);

        'outer: while let Some(chunk) = byte_stream.next().await {
            let bytes = match chunk {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!(provider = "ollama-http", "SSE read error: {e}");
                    break 'outer;
                }
            };

            let lines = match splitter.push_bytes(&bytes) {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!(provider = "ollama-http", "SSE framing error: {e}");
                    break 'outer;
                }
            };

            for line in lines {
                match parse_sse_line(&line) {
                    Ok(Some(ev)) => {
                        if tx.send(ev).await.is_err() {
                            return;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(provider = "ollama-http", "SSE parse error: {e}");
                    }
                }
            }
        }

        if let Some(line) = splitter.flush() {
            if let Ok(Some(ev)) = parse_sse_line(&line) {
                let _ = tx.send(ev).await;
            }
        }
    });

    futures_util::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    )
    .boxed()
}

/// Convert a successful `/api/chat` NDJSON response into a `ProviderEvent` stream.
///
/// Emits a synthetic `MessageStart` + `ContentBlockStart` at the start (the
/// native `/api/chat` format has no equivalent), then maps each NDJSON line via
/// [`ollama_chat_to_provider_events`], and closes with `ContentBlockStop` +
/// `MessageDelta` + `MessageStop`.
fn ndjson_response_to_stream(
    model: String,
    response: reqwest::Response,
) -> BoxStream<'static, ProviderEvent> {
    let (tx, rx) = mpsc::channel::<ProviderEvent>(64);

    tokio::spawn(async move {
        // Synthetic open events — /api/chat has no equivalent of message_start.
        if tx
            .send(ProviderEvent::MessageStart {
                model: model.clone(),
                input_tokens: 0,
            })
            .await
            .is_err()
        {
            return;
        }
        if tx
            .send(ProviderEvent::ContentBlockStart {
                index: 0,
                block_type: "text".to_owned(),
                tool_use_id: None,
                tool_name: None,
            })
            .await
            .is_err()
        {
            return;
        }

        let mut splitter = LineSplitter::new();
        let byte_stream = response.bytes_stream();
        tokio::pin!(byte_stream);

        let mut prompt_tokens: u32 = 0;
        let mut output_tokens: u32 = 0;
        let mut stop_reason = "end_turn".to_owned();

        'outer: while let Some(chunk) = byte_stream.next().await {
            let bytes = match chunk {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!(provider = "ollama-http", "NDJSON read error: {e}");
                    break 'outer;
                }
            };

            let lines = match splitter.push_bytes(&bytes) {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!(provider = "ollama-http", "NDJSON framing error: {e}");
                    break 'outer;
                }
            };

            for line in lines {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<serde_json::Value>(&line) {
                    Ok(json) => {
                        let events = ollama_chat_to_provider_events(
                            &json,
                            &mut prompt_tokens,
                            &mut output_tokens,
                            &mut stop_reason,
                        );
                        for ev in events {
                            if tx.send(ev).await.is_err() {
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(provider = "ollama-http", line = %line, "NDJSON parse error: {e}");
                    }
                }
            }
        }

        // Synthetic close events.
        let _ = tx.send(ProviderEvent::ContentBlockStop { index: 0 }).await;
        let _ = tx
            .send(ProviderEvent::MessageDelta {
                stop_reason,
                output_tokens,
            })
            .await;
        let _ = tx.send(ProviderEvent::MessageStop).await;
    });

    futures_util::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    )
    .boxed()
}

/// Translate one Ollama `/api/chat` NDJSON line into zero or more [`ProviderEvent`]s.
///
/// Wire format:
/// - Streaming line: `{"message":{"content":"…"},"done":false}`
/// - Final line: `{"done":true,"done_reason":"stop","eval_count":N,"prompt_eval_count":M}`
///
/// Updates `prompt_tokens`, `output_tokens`, and `stop_reason` in-place from
/// the final `done:true` line; callers emit the closing `MessageDelta` + `MessageStop`
/// themselves after the loop completes.
pub(super) fn ollama_chat_to_provider_events(
    json: &serde_json::Value,
    prompt_tokens: &mut u32,
    output_tokens: &mut u32,
    stop_reason: &mut String,
) -> Vec<ProviderEvent> {
    if json["done"].as_bool().unwrap_or(false) {
        if let Some(n) = json["prompt_eval_count"].as_u64() {
            *prompt_tokens = u32::try_from(n).unwrap_or(u32::MAX);
        }
        if let Some(n) = json["eval_count"].as_u64() {
            *output_tokens = u32::try_from(n).unwrap_or(u32::MAX);
        }
        if let Some(r) = json["done_reason"].as_str() {
            *stop_reason = match r {
                "stop" => "end_turn".to_owned(),
                "length" => "max_tokens".to_owned(),
                other => other.to_owned(),
            };
        }
        vec![] // Closing events emitted by the caller after the loop.
    } else if let Some(content) = json["message"]["content"].as_str() {
        if content.is_empty() {
            vec![]
        } else {
            vec![ProviderEvent::TextDelta {
                index: 0,
                text: content.to_owned(),
            }]
        }
    } else {
        vec![]
    }
}

// ── Subprocess helper ──────────────────────────────────────────────────────────

/// Invoke `ollama run <model> <prompt>` and return trimmed stdout.
///
/// Uses `kill_on_drop(true)` and a `process_group(0)` so that `killpg` on
/// timeout reaches any grandchild processes the ollama CLI may spawn.
async fn dispatch_ollama(
    prompt: &str,
    model: &str,
    max_turns: u32,
) -> Result<String, ProviderError> {
    let timeout_dur = Duration::from_secs(u64::from(max_turns) * 120 + 30);

    // execve(2) semantics — no shell; args are separate Vec items.
    // model is validated against CLOUD_MODEL_REGISTRY (known slugs) before this call.
    let mut cmd = tokio::process::Command::new("ollama");
    cmd.args(["run", model, prompt])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    #[cfg(unix)]
    cmd.process_group(0);

    let child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            warn!(provider = "ollama-cli", "ollama binary not found on PATH");
            ProviderError::Internal("ollama CLI not found on PATH".to_owned())
        } else {
            warn!(provider = "ollama-cli", err = %e, "subprocess spawn failed");
            ProviderError::Internal(format!("ollama spawn failed: {e}"))
        }
    })?;

    let pgid = child.id();

    let result = tokio::time::timeout(timeout_dur, child.wait_with_output()).await;

    match result {
        Ok(Ok(out)) => {
            if !out.stderr.is_empty() {
                warn!(
                    provider = "ollama-cli",
                    stderr_bytes = out.stderr.len(),
                    "subprocess wrote to stderr"
                );
            }
            if !out.status.success() {
                let code = out.status.code().unwrap_or(-1);
                warn!(provider = "ollama-cli", exit_code = code, "non-zero exit");
                return Err(ProviderError::Internal(format!(
                    "ollama exited with status {code}"
                )));
            }
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
        }
        Ok(Err(e)) => {
            warn!(provider = "ollama-cli", err = %e, "wait_with_output failed");
            Err(ProviderError::Internal("subprocess I/O error".to_owned()))
        }
        Err(_elapsed) => {
            #[cfg(unix)]
            if let Some(pid) = pgid {
                // SAFETY: killpg is async-signal-safe. `pid` is a valid u32 from the OS,
                // bounded by PID_MAX (≤4_194_304 on Linux, 99_999 on macOS) — well within
                // i32::MAX, so the cast cannot wrap. Negative return value means the group
                // already exited; safe to ignore.
                #[allow(unsafe_code, clippy::cast_possible_wrap)]
                unsafe {
                    libc::killpg(pid as libc::pid_t, libc::SIGKILL);
                }
            }
            warn!(
                provider = "ollama-cli",
                timeout_secs = timeout_dur.as_secs(),
                "subprocess timed out; process group killed"
            );
            Err(ProviderError::SubprocessTimeout {
                used_turns: 0,
                used_budget_usd: 0.0,
            })
        }
    }
}

/// Compute estimated cost in USD from token counts and a cost tier.
fn cost_for_tier(tier: CostTier, input_tokens: u32, output_tokens: u32) -> f64 {
    let (in_rate, out_rate) = match tier {
        CostTier::Low => (LOW_INPUT_USD_PER_M, LOW_OUTPUT_USD_PER_M),
        CostTier::Medium => (MEDIUM_INPUT_USD_PER_M, MEDIUM_OUTPUT_USD_PER_M),
        CostTier::High => (HIGH_INPUT_USD_PER_M, HIGH_OUTPUT_USD_PER_M),
        CostTier::Premium => (PREMIUM_INPUT_USD_PER_M, PREMIUM_OUTPUT_USD_PER_M),
    };
    (f64::from(input_tokens) / 1_000_000.0 * in_rate)
        + (f64::from(output_tokens) / 1_000_000.0 * out_rate)
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::agent::cloud_models::CLOUD_MODEL_REGISTRY;
    use serde_json::json;

    // ── Provider struct ──

    #[test]
    fn provider_new_valid_slug_succeeds() {
        let p = OllamaCliProvider::new("glm-5.1:cloud", None).unwrap();
        assert_eq!(p.default_model, "glm-5.1:cloud");
        assert_eq!(p.name(), "ollama-cli");
    }

    #[test]
    fn provider_new_unknown_slug_fails() {
        let err = OllamaCliProvider::new("not-a-model:cloud", None).unwrap_err();
        assert!(
            matches!(err, OllamaError::UnknownModel(ref s) if s == "not-a-model:cloud"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn provider_base_url_field_is_string() {
        // Verify the struct exposes base_url as a non-empty String.
        // Env-var mutation is not tested here to avoid unsafe-code lint and
        // parallel-test races; the logic is a trivial env::var-or-default.
        let p = OllamaCliProvider::new("glm-5.1:cloud", None).unwrap();
        assert!(!p.base_url.is_empty());
        assert!(
            p.base_url.starts_with("http"),
            "base_url must be an HTTP URL, got: {}",
            p.base_url
        );
    }

    #[test]
    fn capabilities_reports_no_native_enforcement() {
        let p = OllamaCliProvider::new("glm-5.1:cloud", None).unwrap();
        let caps = p.capabilities();
        assert!(!caps.native_budget_cap);
        assert!(!caps.native_turn_cap);
        assert!(!caps.auth_inherits_session);
        assert_eq!(caps.schema_enforcement, SchemaMode::None);
    }

    #[test]
    fn estimate_cost_non_negative_for_all_registry_models() {
        for m in CLOUD_MODEL_REGISTRY {
            let p = OllamaCliProvider::new(m.slug, None).unwrap();
            let cost = p.estimate_cost(1_000, 500);
            assert!(cost >= 0.0, "estimate_cost negative for '{}'", m.slug);
        }
    }

    #[test]
    fn low_tier_cheaper_than_premium_tier() {
        let low = cost_for_tier(CostTier::Low, 100_000, 100_000);
        let premium = cost_for_tier(CostTier::Premium, 100_000, 100_000);
        assert!(low < premium);
    }

    // ── Auth token wiring (Phase-10 TOCTOU close) ──
    //
    // Provider holds auth_token verbatim from the constructor arg; no internal
    // env read. These tests pin both the absent (None) and present (Some)
    // shapes against the `Option<SecretString>` field so a future regression
    // (e.g. reintroducing the env::var read) breaks the build.

    #[test]
    fn provider_new_with_none_token_stores_none() {
        let p = OllamaCliProvider::new("glm-5.1:cloud", None).unwrap();
        assert!(
            p.auth_token.is_none(),
            "provider must store None auth_token when constructed with None"
        );
    }

    #[test]
    fn provider_new_with_some_token_stores_token() {
        let p = OllamaCliProvider::new(
            "glm-5.1:cloud",
            Some(SecretString::from("test-bearer-pin-do-not-leak")),
        )
        .unwrap();
        let token = p.auth_token.as_ref().expect("Some token stored");
        // ExposeSecret only used here in test code under explicit assertion.
        assert_eq!(
            token.expose_secret(),
            "test-bearer-pin-do-not-leak",
            "stored token bytes must match constructor arg verbatim"
        );
    }

    // ── build_v1_messages_body ──

    fn user_msg(content: &str) -> serde_json::Value {
        json!({"role": "user", "content": content})
    }

    fn assistant_msg(content: &str) -> serde_json::Value {
        json!({"role": "assistant", "content": content})
    }

    #[test]
    fn v1_body_contains_required_fields() {
        let msgs = vec![user_msg("hello")];
        let body = build_v1_messages_body("qwen3:4b", &msgs, None, None);
        assert_eq!(body["model"], "qwen3:4b");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["content"], "hello");
        assert_eq!(body["messages"][0]["role"], "user");
        assert!(body["system"].is_null(), "None system → no system field");
    }

    #[test]
    fn v1_body_includes_system_when_some() {
        let msgs = vec![user_msg("hi")];
        let body = build_v1_messages_body("qwen3:4b", &msgs, Some("You are helpful."), None);
        assert_eq!(body["system"], "You are helpful.");
    }

    #[test]
    fn v1_body_multi_turn_history_preserved() {
        let msgs = vec![
            user_msg("turn1"),
            assistant_msg("reply1"),
            user_msg("turn2"),
        ];
        let body = build_v1_messages_body("qwen3:4b", &msgs, None, None);
        let arr = body["messages"].as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[2]["content"], "turn2");
    }

    #[test]
    fn v1_body_tools_in_anthropic_format() {
        let tools = vec![ToolDefinition {
            name: "read".to_owned(),
            description: "Read a file".to_owned(),
            input_schema: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
        }];
        let msgs = vec![user_msg("hi")];
        let body = build_v1_messages_body("qwen3:4b", &msgs, None, Some(&tools));
        let t = &body["tools"][0];
        assert_eq!(t["name"], "read");
        assert_eq!(t["description"], "Read a file");
        assert!(!t["input_schema"].is_null());
        assert!(t["type"].is_null(), "v1 format has no 'type' wrapper");
    }

    #[test]
    fn v1_body_no_tools_field_when_none() {
        let msgs = vec![user_msg("hi")];
        let body = build_v1_messages_body("qwen3:4b", &msgs, None, None);
        assert!(body["tools"].is_null());
    }

    // ── build_api_chat_body ──

    #[test]
    fn chat_body_stream_true_and_num_ctx() {
        let msgs = vec![user_msg("hello")];
        let body = build_api_chat_body("qwen3:4b", &msgs, None, None);
        assert_eq!(body["stream"], true);
        assert_eq!(body["model"], "qwen3:4b");
        assert_eq!(body["options"]["num_ctx"], 131_072_u32);
        let arr = body["messages"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["role"], "user");
    }

    #[test]
    fn chat_body_prepends_system_message() {
        let msgs = vec![user_msg("hi")];
        let body = build_api_chat_body("qwen3:4b", &msgs, Some("sys prompt"), None);
        let arr = body["messages"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["role"], "system");
        assert_eq!(arr[0]["content"], "sys prompt");
        assert_eq!(arr[1]["role"], "user");
    }

    #[test]
    fn chat_body_multi_turn_history_after_system() {
        let msgs = vec![user_msg("q1"), assistant_msg("a1"), user_msg("q2")];
        let body = build_api_chat_body("qwen3:4b", &msgs, Some("sys"), None);
        let arr = body["messages"].as_array().unwrap();
        // system + 3 history = 4
        assert_eq!(arr.len(), 4);
        assert_eq!(arr[0]["role"], "system");
        assert_eq!(arr[3]["content"], "q2");
    }

    #[test]
    fn chat_body_tools_in_openai_format() {
        let tools = vec![ToolDefinition {
            name: "read".to_owned(),
            description: "Read a file".to_owned(),
            input_schema: json!({"type": "object"}),
        }];
        let msgs = vec![user_msg("hi")];
        let body = build_api_chat_body("qwen3:4b", &msgs, None, Some(&tools));
        let t = &body["tools"][0];
        assert_eq!(t["type"], "function");
        assert_eq!(t["function"]["name"], "read");
        assert_eq!(t["function"]["description"], "Read a file");
    }

    #[test]
    fn chat_body_no_tools_field_when_none() {
        let msgs = vec![user_msg("hi")];
        let body = build_api_chat_body("qwen3:4b", &msgs, None, None);
        assert!(body["tools"].is_null());
    }

    // ── ollama_chat_to_provider_events ──

    #[test]
    fn ndjson_streaming_line_yields_text_delta() {
        let mut pt = 0_u32;
        let mut ot = 0_u32;
        let mut sr = "end_turn".to_owned();
        let line = json!({
            "model": "qwen3:4b",
            "message": {"role": "assistant", "content": "Hello"},
            "done": false
        });
        let events = ollama_chat_to_provider_events(&line, &mut pt, &mut ot, &mut sr);
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], ProviderEvent::TextDelta { index: 0, text } if text == "Hello")
        );
        assert_eq!(pt, 0, "prompt_tokens not set on non-done line");
    }

    #[test]
    fn ndjson_empty_content_yields_no_events() {
        let mut pt = 0_u32;
        let mut ot = 0_u32;
        let mut sr = "end_turn".to_owned();
        let line = json!({"message": {"content": ""}, "done": false});
        let events = ollama_chat_to_provider_events(&line, &mut pt, &mut ot, &mut sr);
        assert!(events.is_empty());
    }

    #[test]
    fn ndjson_done_line_updates_counters() {
        let mut pt = 0_u32;
        let mut ot = 0_u32;
        let mut sr = "end_turn".to_owned();
        let line = json!({
            "model": "qwen3:4b",
            "done": true,
            "done_reason": "stop",
            "eval_count": 42,
            "prompt_eval_count": 10
        });
        let events = ollama_chat_to_provider_events(&line, &mut pt, &mut ot, &mut sr);
        assert!(
            events.is_empty(),
            "done line yields no events (caller emits close)"
        );
        assert_eq!(pt, 10);
        assert_eq!(ot, 42);
        assert_eq!(sr, "end_turn", "Ollama 'stop' maps to 'end_turn'");
    }

    #[test]
    fn ndjson_done_length_maps_to_max_tokens() {
        let mut pt = 0_u32;
        let mut ot = 0_u32;
        let mut sr = "end_turn".to_owned();
        let line = json!({
            "done": true,
            "done_reason": "length",
            "eval_count": 0,
            "prompt_eval_count": 0
        });
        ollama_chat_to_provider_events(&line, &mut pt, &mut ot, &mut sr);
        assert_eq!(sr, "max_tokens");
    }

    #[test]
    fn ndjson_missing_done_field_treated_as_false() {
        let mut pt = 0_u32;
        let mut ot = 0_u32;
        let mut sr = "end_turn".to_owned();
        let line = json!({"message": {"content": "hi"}});
        let events = ollama_chat_to_provider_events(&line, &mut pt, &mut ot, &mut sr);
        // Should produce TextDelta since done defaults to false.
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ProviderEvent::TextDelta { text, .. } if text == "hi"));
    }

    // ── W3.5: history prepending ──────────────────────────────────────────────

    #[test]
    fn history_prepended_in_api_chat_body() {
        let history = vec![
            json!({"role": "user", "content": "turn 1"}),
            json!({"role": "assistant", "content": "response 1"}),
        ];
        let mut msgs = history.clone();
        msgs.push(json!({"role": "user", "content": "turn 2"}));
        let body = build_api_chat_body("m", &msgs, None, None);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["content"], "turn 1");
        assert_eq!(messages[1]["content"], "response 1");
        assert_eq!(messages[2]["content"], "turn 2");
    }

    #[test]
    fn history_prepended_in_v1_messages_body() {
        let history = vec![
            json!({"role": "user", "content": "q"}),
            json!({"role": "assistant", "content": "a"}),
        ];
        let mut msgs = history.clone();
        msgs.push(json!({"role": "user", "content": "follow-up"}));
        let body = build_v1_messages_body("m", &msgs, None, None);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2]["content"], "follow-up");
    }

    #[test]
    fn single_turn_no_history() {
        let msgs = vec![json!({"role": "user", "content": "hello"})];
        let body = build_api_chat_body("m", &msgs, None, None);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
    }
}
