// Product names (OpenRouter, OpenAI, LiteLLM, Together, Groq, Fireworks, Azure,
// Anthropic, Ollama, Llama, Sonnet, etc.) appear in prose throughout this
// module. Backticking each occurrence is noisy without aiding comprehension —
// these are well-known proper nouns, not code identifiers.
#![allow(clippy::doc_markdown)]

//! Multi-provider LLM completion for `Helix`-backed retrieval-augmented
//! generation.
//!
//! [`LlmCompleter`] is a focused, single-turn completion trait — no tools, no
//! schemas, no streaming. Pair it with [`crate::helix::generation::PromptPolicy`]
//! and [`crate::helix::generation::ContextStrategy`] to build a complete RAG
//! pipeline in a few lines.
//!
//! # Providers
//!
//! | Provider | Submodule | Covers |
//! |----------|-----------|--------|
//! | [`AnthropicCompleter`] | `anthropic` | `api.anthropic.com` direct |
//! | [`OpenAICompatCompleter`] | `openai_compat` | OpenRouter, OpenAI, LiteLLM, Together, Groq, Fireworks, Azure OpenAI, any `/v1/chat/completions` endpoint |
//! | [`OllamaCompleter`] | `ollama` | Local Ollama (`localhost:11434`) and Ollama Cloud |
//!
//! All three implementations:
//!
//! - Are async, `Send + Sync`, use [`reqwest::Client`].
//! - Declare [`ModelClass`] so callers can route via
//!   [`crate::helix::generation::optimal_strategy_for_intent_with_model_class`].
//! - Use `temperature = 0` by default (deterministic; can override via builder).
//! - Hold credentials as [`secrecy::SecretString`] (never logged).
//!
//! # Empirical guidance (LongMemEval-S 2026-05-27)
//!
//! - **Sonnet 4.6** via [`AnthropicCompleter`] is the canonical winning
//!   configuration. 0.858/500-Q accuracy with the `v3-winning` prompt bundle.
//! - **Llama 4 Scout** via [`OpenAICompatCompleter::openrouter`] achieved 0.620
//!   on the same study — the gap is concentrated in `Counting` + `FullContext`
//!   (-45.9pp) due to long-context attention quality. Use
//!   [`ModelClass::Cheap`] to disable `FullContext` for non-frontier models.
//!
//! # Example
//!
//! ```ignore
//! use lightarchitects::helix::generation::{
//!     KeywordIntentClassifier, IntentClassifier, PromptPolicy,
//!     optimal_strategy_for_intent_with_model_class, ContextStrategy,
//!     completer::{AnthropicCompleter, LlmCompleter},
//! };
//!
//! let completer = AnthropicCompleter::from_env("claude-sonnet-4-6")?;
//! let classifier = KeywordIntentClassifier;
//!
//! let intent = classifier.classify(query);
//! let strategy = optimal_strategy_for_intent_with_model_class(
//!     intent, completer.model_class()
//! );
//! // ... assemble context from helix per strategy ...
//! let policy = PromptPolicy::for_intent(intent);
//! let user_prompt = build_user_prompt(policy, snippets, query);
//! let completion = completer.complete(policy.system_prompt(), &user_prompt).await?;
//! println!("{} (took {}ms)", completion.text, completion.latency_ms);
//! ```

pub mod anthropic;
pub mod ollama;
pub mod openai_compat;

pub use crate::agent::OpenAIFlavor;
pub use anthropic::AnthropicCompleter;
pub use ollama::OllamaCompleter;
pub use openai_compat::OpenAICompatCompleter;

use async_trait::async_trait;

use super::ModelClass;

/// Result of a single-turn LLM completion call.
#[derive(Debug, Clone)]
pub struct Completion {
    /// Generated text from the assistant.
    pub text: String,
    /// Tokens in the prompt (best-effort; some providers return 0).
    pub input_tokens: u32,
    /// Tokens in the completion (best-effort; some providers return 0).
    pub output_tokens: u32,
    /// Wall-clock latency of the HTTP call.
    pub latency_ms: u64,
    /// Provider identifier (e.g. `"anthropic:claude-sonnet-4-6"`).
    pub provider: String,
}

/// Errors a completer can return. Variants are deliberately coarse — callers
/// retry on `Http` and `Timeout`, escalate on `Auth`, and treat the rest as
/// fatal.
#[derive(thiserror::Error, Debug)]
pub enum CompletionError {
    /// Network or provider HTTP error (5xx, transient connection issue).
    #[error("HTTP error: {0}")]
    Http(String),
    /// Authentication failed (401/403 — bad or expired API key).
    #[error("authentication failed: {0}")]
    Auth(String),
    /// Provider returned a successful HTTP status but the response body had
    /// no text content (e.g. content filter triggered).
    #[error("empty response from provider")]
    Empty,
    /// Request timed out before the provider returned a final answer.
    #[error("timeout after {seconds}s")]
    Timeout {
        /// Configured timeout in seconds.
        seconds: u64,
    },
    /// Provider returned a non-200, non-401 status with an error body.
    #[error("provider error: {0}")]
    Provider(String),
    /// JSON serialization / deserialization failed.
    #[error("serialization error: {0}")]
    Serde(String),
    /// API key was required but not configured (env var or arg missing).
    #[error("missing credential: {0}")]
    MissingCredential(String),
}

impl From<serde_json::Error> for CompletionError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serde(e.to_string())
    }
}

impl From<reqwest::Error> for CompletionError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Self::Timeout { seconds: 0 }
        } else if e.is_status() {
            let status_code = e.status().map_or(0, |s| s.as_u16());
            if status_code == 401 || status_code == 403 {
                Self::Auth(e.to_string())
            } else {
                Self::Http(e.to_string())
            }
        } else {
            Self::Http(e.to_string())
        }
    }
}

/// Single-turn LLM completion. Implementations:
///
/// - MUST issue exactly one HTTP request per `complete()` call.
/// - MUST NOT retry inside the trait (callers wrap with their own retry).
/// - MUST set `temperature = 0` unless overridden via constructor.
/// - MUST report `input_tokens` / `output_tokens` when the provider reports
///   them; zero is acceptable when the provider doesn't.
#[async_trait]
pub trait LlmCompleter: Send + Sync {
    /// Stable provider identifier in `"<provider>:<model>"` format
    /// (e.g. `"anthropic:claude-sonnet-4-6"`, `"openrouter:anthropic/claude-sonnet-4.6"`,
    /// `"ollama:qwen2.5:32b"`).
    fn name(&self) -> String;

    /// Capability tier of the underlying model. Used by
    /// [`super::optimal_strategy_for_intent_with_model_class`] to gate
    /// `FullContext` strategy.
    fn model_class(&self) -> ModelClass;

    /// Issue a single completion call.
    ///
    /// # Errors
    ///
    /// Returns [`CompletionError`] on HTTP failure, auth failure, timeout,
    /// or empty response.
    async fn complete(&self, system: &str, user: &str) -> Result<Completion, CompletionError>;
}

/// Best-guess capability classification from a model name string.
///
/// Recognises the known model families (Anthropic Sonnet/Opus/Haiku, OpenAI
/// GPT-5/4o/nano, Google Gemini 2.5 Pro/Flash/Flash-Lite, Meta Llama 4 variants,
/// DeepSeek, Qwen). Returns [`ModelClass::Cheap`] for any model under ~13B
/// parameters or any unrecognised model.
///
/// Used by provider constructors that don't take a `ModelClass` explicitly.
#[must_use]
pub fn model_class_from_name(model: &str) -> ModelClass {
    let m = model.to_lowercase();

    // Explicit Cheap markers — checked FIRST so that variants like
    // `gemini-2.5-flash-lite` are correctly demoted from the broader
    // `gemini-2.5-flash` MidTier match below.
    if m.contains("-lite") || m.contains("-nano") {
        return ModelClass::Cheap;
    }

    // Frontier — strongest tier. Long-context attention proven.
    if m.contains("opus-4")
        || m.contains("claude-opus-4")
        || m.contains("sonnet-4-6")
        || m.contains("sonnet-4.6")
        || (m.contains("gpt-5") && !m.contains("mini"))
        || m.contains("gemini-2.5-pro")
        || (m.contains("o3") && !m.contains("mini"))
        || m.contains("llama-3.3:70b")
        || m.contains("llama3.3:70b")
        || m.contains("kimi-k2.5")
    {
        return ModelClass::Frontier;
    }

    // Mid-tier — capable but with measurable long-context degradation.
    if m.contains("sonnet-4-5")
        || m.contains("sonnet-4.5")
        || m.contains("sonnet-4")
        || m.contains("gpt-5-mini")
        || m.contains("gpt-4o-mini")
        || m.contains("gpt-4-turbo")
        || m.contains("gemini-2.5-flash")
        || m.contains("gemini-2.0-flash")
        || m.contains("haiku-4-5")
        || m.contains("haiku-4.5")
        || m.contains("deepseek-v4")
        || m.contains("qwen3.5:32b")
        || m.contains("qwen2.5:32b")
        || m.contains("llama-4-maverick")
        || m.contains("nemotron-3-super")
    {
        return ModelClass::MidTier;
    }

    // Cheap — small models or known-collapse on long-context.
    // Default fallback also lands here.
    ModelClass::Cheap
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn model_class_anthropic() {
        assert_eq!(
            model_class_from_name("claude-sonnet-4-6"),
            ModelClass::Frontier
        );
        assert_eq!(
            model_class_from_name("claude-sonnet-4-5"),
            ModelClass::MidTier
        );
        assert_eq!(
            model_class_from_name("claude-haiku-4-5-20251001"),
            ModelClass::MidTier
        );
    }

    #[test]
    fn model_class_openai() {
        assert_eq!(model_class_from_name("gpt-5"), ModelClass::Frontier);
        assert_eq!(model_class_from_name("gpt-5-mini"), ModelClass::MidTier);
        assert_eq!(model_class_from_name("gpt-4.1-nano"), ModelClass::Cheap);
    }

    #[test]
    fn model_class_google() {
        assert_eq!(
            model_class_from_name("gemini-2.5-pro"),
            ModelClass::Frontier
        );
        assert_eq!(
            model_class_from_name("gemini-2.5-flash"),
            ModelClass::MidTier
        );
        assert_eq!(
            model_class_from_name("gemini-2.5-flash-lite"),
            ModelClass::Cheap
        );
    }

    #[test]
    fn model_class_ollama_local() {
        assert_eq!(model_class_from_name("llama3.3:70b"), ModelClass::Frontier);
        assert_eq!(model_class_from_name("qwen2.5:32b"), ModelClass::MidTier);
        assert_eq!(model_class_from_name("phi-3.5-mini"), ModelClass::Cheap);
    }

    #[test]
    fn model_class_unknown_is_cheap() {
        assert_eq!(model_class_from_name("some-tiny-model"), ModelClass::Cheap);
    }

    #[test]
    fn completion_error_from_reqwest_timeout() {
        // Hard to construct reqwest::Error directly in tests — just validate
        // serde_json -> CompletionError works as the other From path.
        let bad: Result<u32, serde_json::Error> = serde_json::from_str("not json");
        let err: CompletionError = bad.unwrap_err().into();
        match err {
            CompletionError::Serde(_) => {}
            other => panic!("expected Serde, got {other:?}"),
        }
    }
}
