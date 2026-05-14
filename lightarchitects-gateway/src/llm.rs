//! LLM client — supports Ollama, OpenAI-compatible, and Anthropic APIs.
//!
//! Backend is selected by `LLM_BACKEND` env var:
//! - `ollama` (default): calls `POST /api/generate` on Ollama
//! - `openai`: calls `POST /v1/chat/completions` on any OpenAI-compatible API
//!   (`HuggingFace` Inference Endpoints, vLLM, llama-server, etc.)
//! - `anthropic` | `claude`: calls `POST /v1/messages` on Anthropic
//!
//! When `LLM_BACKEND=openai`, reads `LLM_API_URL` and `LLM_API_KEY`.
//! This enables the Arena to run on Light Architect Genesis instead of Ollama models.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;
use tokio::time;
use zeroize::Zeroizing;

/// HTTP timeout for LLM generation (cloud models may be slow).
const LLM_TIMEOUT: Duration = Duration::from_secs(120);

/// Maximum response length to accept (prevents runaway generation).
const MAX_RESPONSE_LEN: usize = 65_536;

/// Backend protocol for LLM calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmBackend {
    /// Ollama `/api/generate` protocol.
    Ollama,
    /// `OpenAI`-compatible `/v1/chat/completions` protocol.
    /// Used for `HuggingFace` Inference Endpoints, vLLM, llama-server.
    OpenAi,
    /// Anthropic Messages API (`/v1/messages`).
    Anthropic,
}

/// Multi-backend LLM client with serialization semaphore.
///
/// Contains a shared semaphore (`permits = 1`) that serializes all LLM calls.
/// On 16 GB Khadas, concurrent requests cause queue collisions and OOM.
/// Every caller — heartbeats, curator, agent loop — acquires the permit
/// before making a request, so only one is in-flight at a time.
pub struct LlmClient {
    client: reqwest::Client,
    base_url: String,
    default_model: String,
    backend: LlmBackend,
    /// API key for OpenAI-compatible backends (Bearer token).
    /// Wrapped in `Zeroizing` so the key is wiped from heap on drop.
    api_key: Option<Zeroizing<String>>,
    /// Single-permit semaphore — serializes LLM access across all tasks.
    semaphore: Arc<Semaphore>,
    /// Ollama availability flag, maintained by a background heartbeat task.
    /// `false` when Ollama has failed `HEARTBEAT_FAILURE_THRESHOLD` consecutive
    /// health checks — `generate_with_model` returns an error immediately instead
    /// of blocking the semaphore waiting for a server that is down.
    /// Always `true` for the `OpenAi` backend (no heartbeat spawned).
    ollama_available: Arc<AtomicBool>,
}

impl LlmClient {
    /// Create from environment variables.
    ///
    /// Reads:
    /// - `LLM_BACKEND`: `ollama` (default) or `openai`
    /// - `OLLAMA_BASE_URL` / `LLM_API_URL`: endpoint URL
    /// - `OLLAMA_MODEL` / `LLM_MODEL`: model name
    /// - `LLM_API_KEY`: Bearer token (`OpenAI` backend only)
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be constructed.
    pub fn from_env() -> Result<Arc<Self>, String> {
        let backend_str = std::env::var("LLM_BACKEND").unwrap_or_else(|_| "ollama".into());
        let backend = match backend_str.to_lowercase().as_str() {
            "openai" | "hf" | "vllm" | "llamacpp" => LlmBackend::OpenAi,
            "anthropic" | "claude" => LlmBackend::Anthropic,
            _ => LlmBackend::Ollama,
        };

        let (base_url, default_model) = match backend {
            LlmBackend::Ollama => {
                let url = std::env::var("OLLAMA_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:11434".into());
                let model = std::env::var("OLLAMA_MODEL")
                    .or_else(|_| std::env::var("LLM_MODEL"))
                    .unwrap_or_else(|_| "nemotron-3-super:cloud".into());
                (url, model)
            }
            LlmBackend::OpenAi => {
                let url = std::env::var("LLM_API_URL")
                    .or_else(|_| std::env::var("EXODUS_ENDPOINT_URL"))
                    .unwrap_or_else(|_| "http://localhost:8080".into());
                let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "genesis".into());
                (url, model)
            }
            LlmBackend::Anthropic => {
                let url = "https://api.anthropic.com".to_owned();
                let model =
                    std::env::var("LLM_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".into());
                (url, model)
            }
        };

        let api_key = match backend {
            LlmBackend::Anthropic => resolve_key("ANTHROPIC_API_KEY")
                .or_else(|| resolve_key("LLM_API_KEY"))
                .map(Zeroizing::new),
            LlmBackend::OpenAi => resolve_key("LLM_API_KEY")
                .or_else(|| resolve_key("EXODUS_HF_TOKEN"))
                .or_else(|| resolve_key("OPENAI_API_KEY"))
                .map(Zeroizing::new),
            LlmBackend::Ollama => resolve_key("OLLAMA_API_KEY").map(Zeroizing::new),
        };

        Self::build_sync(base_url, default_model, backend, api_key)
    }

    /// Create an LLM client with an explicit backend, model, and optional API key.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be constructed.
    pub fn with_backend(
        backend: LlmBackend,
        model: &str,
        api_key: Option<String>,
    ) -> Result<Arc<Self>, String> {
        let (base_url, default_model) = match backend {
            LlmBackend::Ollama => {
                let url = std::env::var("OLLAMA_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:11434".into());
                (url, model.to_owned())
            }
            LlmBackend::OpenAi => {
                let url = std::env::var("LLM_API_URL")
                    .or_else(|_| std::env::var("EXODUS_ENDPOINT_URL"))
                    .unwrap_or_else(|_| "http://localhost:8080".into());
                (url, model.to_owned())
            }
            LlmBackend::Anthropic => {
                let url = "https://api.anthropic.com".to_owned();
                (url, model.to_owned())
            }
        };
        let api_key = api_key.map(Zeroizing::new);
        Self::build_sync(base_url, default_model, backend, api_key)
    }

    fn build_sync(
        base_url: String,
        default_model: String,
        backend: LlmBackend,
        api_key: Option<Zeroizing<String>>,
    ) -> Result<Arc<Self>, String> {
        let client = reqwest::Client::builder()
            .timeout(LLM_TIMEOUT)
            .build()
            .map_err(|e| format!("failed to build LLM HTTP client: {e}"))?;

        tracing::info!(
            base_url = %base_url,
            model = %default_model,
            backend = ?backend,
            has_api_key = api_key.is_some(),
            "LLM client initialized (semaphore=1)"
        );

        let ollama_available = Arc::new(AtomicBool::new(true));
        let client_arc = Arc::new(Self {
            client,
            base_url,
            default_model,
            backend,
            api_key,
            semaphore: Arc::new(Semaphore::new(1)),
            ollama_available: Arc::clone(&ollama_available),
        });

        if matches!(client_arc.backend, LlmBackend::Ollama) {
            let heartbeat = Arc::clone(&client_arc);
            tokio::spawn(run_ollama_heartbeat(heartbeat));
        }

        Ok(client_arc)
    }

    /// Generate text from a prompt using the default model.
    ///
    /// # Errors
    /// Returns an error if the LLM call fails (network, timeout, or non-2xx status).
    pub async fn generate(&self, prompt: &str) -> Result<String, String> {
        self.generate_with_model(&self.default_model, prompt).await
    }

    /// Generate text with a specific model override.
    ///
    /// Acquires the LLM semaphore before calling the backend.
    /// Routes to Ollama or OpenAI-compatible API based on config.
    ///
    /// # Errors
    /// Returns an error if the LLM call fails (network, timeout, non-2xx status,
    /// or the Ollama circuit is open).
    pub async fn generate_with_model(&self, model: &str, prompt: &str) -> Result<String, String> {
        if matches!(self.backend, LlmBackend::Ollama)
            && !self.ollama_available.load(Ordering::Relaxed)
        {
            return Err("Ollama is currently unreachable (circuit open — failing fast)".into());
        }

        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| format!("LLM semaphore closed: {e}"))?;

        match self.backend {
            LlmBackend::Ollama => self.generate_ollama(model, prompt).await,
            LlmBackend::OpenAi => self.generate_openai(model, prompt).await,
            LlmBackend::Anthropic => self.generate_anthropic(model, prompt).await,
        }
    }

    /// Generate via Ollama `/api/generate` with streaming.
    ///
    /// Streaming keeps the HTTP connection alive during long generations,
    /// preventing timeout-based truncation when `num_predict` is large.
    /// Each line is NDJSON: `{"response": "chunk", "done": false}`.
    /// The final line has `"done": true` with token counts.
    async fn generate_ollama(&self, model: &str, prompt: &str) -> Result<String, String> {
        let start = Instant::now();
        let url = format!("{}/api/generate", self.base_url);

        let body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": true,
            "options": {
                "temperature": 0.7,
                "num_predict": 8192,
                "num_ctx": 32768,
            }
        });

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("LLM HTTP error: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("LLM returned {status}: {text}"));
        }

        // Read streaming NDJSON response — concatenate all response chunks
        let mut text = String::new();
        let mut final_meta: Option<serde_json::Value> = None;
        let full_body = response
            .text()
            .await
            .map_err(|e| format!("LLM stream read error: {e}"))?;

        for line in full_body.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(chunk) = serde_json::from_str::<serde_json::Value>(trimmed) else {
                continue;
            };
            if let Some(tok) = chunk.get("response").and_then(serde_json::Value::as_str) {
                text.push_str(tok);
            }
            if chunk.get("done").and_then(serde_json::Value::as_bool) == Some(true) {
                final_meta = Some(chunk);
            }

            if text.len() > MAX_RESPONSE_LEN {
                break;
            }
        }

        if let Some(ref meta) = final_meta {
            Self::log_completion(model, &start, meta, text.len());
        } else {
            tracing::info!(
                model,
                backend = "ollama",
                duration_ms = start.elapsed().as_millis(),
                response_len = text.len(),
                "LLM stream complete (no final metadata)"
            );
        }
        Ok(Self::truncate_response(text))
    }

    /// Generate via `OpenAI`-compatible `/v1/chat/completions`.
    ///
    /// Converts the raw prompt into a chat message and extracts the
    /// response content. Handles thinking models that return
    /// `reasoning_content` separately from `content`.
    async fn generate_openai(&self, model: &str, prompt: &str) -> Result<String, String> {
        let start = Instant::now();
        let url = format!("{}/v1/chat/completions", self.base_url);

        let body = serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.7,
            "max_tokens": 8192,
        });

        let mut request = self.client.post(&url).json(&body);
        if let Some(ref key) = self.api_key {
            // Build bearer string in a Zeroizing wrapper so the local copy is
            // wiped from heap when it goes out of scope.
            let bearer = Zeroizing::new(format!("Bearer {}", key.as_str()));
            request = request.header("Authorization", bearer.as_str());
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("LLM HTTP error: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("LLM returned {status}: {text}"));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("LLM response parse error: {e}"))?;

        // Extract content — handle thinking models with reasoning_content
        let message = json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"));

        let content = message
            .and_then(|m| m.get("content"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");

        let reasoning = message
            .and_then(|m| m.get("reasoning_content"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");

        // Prefer content; fall back to reasoning only when content is empty.
        // Never concatenate — reasoning_content is internal chain-of-thought
        // that must not leak into user-facing output.
        let text = if !content.is_empty() {
            content.to_owned()
        } else if !reasoning.is_empty() {
            reasoning.to_owned()
        } else {
            String::new()
        };

        let usage = json.get("usage");
        let input_tokens = usage
            .and_then(|u| u.get("prompt_tokens"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let output_tokens = usage
            .and_then(|u| u.get("completion_tokens"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);

        tracing::info!(
            model,
            backend = "openai",
            duration_ms = start.elapsed().as_millis(),
            input_tokens,
            output_tokens,
            response_len = text.len(),
            "LLM call complete"
        );

        Ok(Self::truncate_response(text))
    }

    /// Generate via Anthropic Messages API (`/v1/messages`).
    async fn generate_anthropic(&self, model: &str, prompt: &str) -> Result<String, String> {
        let start = Instant::now();
        let url = format!("{}/v1/messages", self.base_url);

        let body = serde_json::json!({
            "model": model,
            "max_tokens": 8192,
            "messages": [{"role": "user", "content": prompt}],
        });

        let mut request = self.client.post(&url).json(&body);
        if let Some(ref key) = self.api_key {
            request = request
                .header("x-api-key", key.as_str())
                .header("anthropic-version", "2023-06-01");
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("Anthropic HTTP error: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("Anthropic returned {status}: {text}"));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Anthropic response parse error: {e}"))?;

        // Extract text from content blocks
        let text = json
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| {
                arr.iter().find(|block| {
                    block.get("type") == Some(&serde_json::Value::String("text".to_owned()))
                })
            })
            .and_then(|block| block.get("text"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned();

        let usage = json.get("usage");
        let input_tokens = usage
            .and_then(|u| u.get("input_tokens"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let output_tokens = usage
            .and_then(|u| u.get("output_tokens"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);

        tracing::info!(
            model,
            backend = "anthropic",
            duration_ms = start.elapsed().as_millis(),
            input_tokens,
            output_tokens,
            response_len = text.len(),
            "LLM call complete"
        );

        Ok(Self::truncate_response(text))
    }

    /// Log completion metrics for Ollama backend.
    fn log_completion(model: &str, start: &Instant, json: &serde_json::Value, len: usize) {
        let input_tokens = json
            .get("prompt_eval_count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let output_tokens = json
            .get("eval_count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        tracing::info!(
            model,
            backend = "ollama",
            duration_ms = start.elapsed().as_millis(),
            input_tokens,
            output_tokens,
            response_len = len,
            "LLM call complete"
        );
    }

    /// Truncate response to max length.
    fn truncate_response(text: String) -> String {
        if text.len() > MAX_RESPONSE_LEN {
            text[..MAX_RESPONSE_LEN].to_owned()
        } else {
            text
        }
    }

    /// Check if the LLM server is reachable.
    pub async fn health_check(&self) -> bool {
        let url = match self.backend {
            LlmBackend::Ollama => format!("{}/api/tags", self.base_url),
            LlmBackend::OpenAi => format!("{}/health", self.base_url),
            LlmBackend::Anthropic => return true, // No public health endpoint; fail open
        };
        matches!(self.client.get(&url).send().await, Ok(r) if r.status().is_success())
    }
}

/// Resolve an API key by checking all storage sources.
///
/// Priority: environment variable → `~/.lightarchitects/keys.toml` → OS keyring.
pub fn resolve_key(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let home = std::env::var_os("HOME")?;
            let path = std::path::PathBuf::from(home)
                .join(".lightarchitects")
                .join("keys.toml");
            let content = std::fs::read_to_string(&path).ok()?;
            let keys: std::collections::HashMap<String, String> = toml::from_str(&content).ok()?;
            keys.get(name).cloned().filter(|s| !s.is_empty())
        })
        .or_else(|| {
            let entry = keyring::Entry::new("lightarchitects", name).ok()?;
            entry.get_password().ok().filter(|s| !s.is_empty())
        })
}

/// Interval between Ollama heartbeat probes.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Number of consecutive heartbeat failures before the circuit opens.
const HEARTBEAT_FAILURE_THRESHOLD: u32 = 3;

/// Background task: probes Ollama liveness and updates `ollama_available`.
///
/// Opens the circuit (sets flag to `false`) after `HEARTBEAT_FAILURE_THRESHOLD`
/// consecutive failures, preventing callers from blocking the semaphore while
/// Ollama is down.  Closes the circuit on the next successful probe.
async fn run_ollama_heartbeat(client: Arc<LlmClient>) {
    let mut consecutive_failures: u32 = 0;
    loop {
        time::sleep(HEARTBEAT_INTERVAL).await;
        if client.health_check().await {
            if consecutive_failures > 0 {
                tracing::info!("Ollama heartbeat recovered — circuit closed");
            }
            consecutive_failures = 0;
            client.ollama_available.store(true, Ordering::Relaxed);
        } else {
            consecutive_failures = consecutive_failures.saturating_add(1);
            if consecutive_failures == HEARTBEAT_FAILURE_THRESHOLD {
                tracing::warn!(
                    failures = consecutive_failures,
                    "Ollama unreachable — circuit open (fail-fast enabled)"
                );
            }
            if consecutive_failures >= HEARTBEAT_FAILURE_THRESHOLD {
                client.ollama_available.store(false, Ordering::Relaxed);
            }
        }
    }
}
