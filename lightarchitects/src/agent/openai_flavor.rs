#![allow(clippy::doc_markdown)] // product names (OpenAI, OpenRouter, LiteLLM, RunPod) throughout
//! Canonical [`OpenAIFlavor`] enum — shared by the agentic-loop HTTP provider
//! ([`super::openai_compat`]) and the helix generation completers.
//!
//! This module is **not** feature-gated so it is available to both the helix
//! RAG layer (unconditional) and the `loops-core`-gated agent layer.

/// Provider flavor for OpenAI-compatible `/chat/completions` endpoints.
///
/// All flavors share the same HTTP request shape. They differ only in their
/// default base URL and the canonical environment variable used for the API
/// key. This enum is the single source of truth for all OpenAI-compatible
/// provider configuration in the SDK.
///
/// # Provider matrix
///
/// | Variant | Default base URL | Key env var |
/// |---------|-----------------|-------------|
/// | [`OpenAi`] | `https://api.openai.com/v1` | `OPENAI_API_KEY` |
/// | [`OpenRouter`] | `https://openrouter.ai/api/v1` | `OPENROUTER_API_KEY` |
/// | [`LiteLLM`] | `http://localhost:4000/v1` | `LITELLM_API_KEY` |
/// | [`Generic`] | *(none — caller must supply)* | *(none)* |
///
/// [`OpenAi`]: OpenAIFlavor::OpenAi
/// [`OpenRouter`]: OpenAIFlavor::OpenRouter
/// [`LiteLLM`]: OpenAIFlavor::LiteLLM
/// [`Generic`]: OpenAIFlavor::Generic
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpenAIFlavor {
    /// Native `OpenAI` API (`https://api.openai.com/v1`).
    ///
    /// Env var: `OPENAI_API_KEY`.
    OpenAi,

    /// `OpenRouter` (`https://openrouter.ai/api/v1`) — routes to 200+ models
    /// including Anthropic, Google, `OpenAI`, and Meta.
    ///
    /// Model names use the `provider/model` format:
    /// `anthropic/claude-sonnet-4.6`, `openai/gpt-5`, `google/gemini-2.5-flash`.
    ///
    /// Env var: `OPENROUTER_API_KEY`.
    ///
    /// `OpenRouter` accepts optional app-attribution headers (`HTTP-Referer`,
    /// `X-Title`) that improve rate-limit treatment. Use
    /// [`OpenAIFlavor::needs_openrouter_headers`] to check.
    OpenRouter,

    /// `LiteLLM` proxy (`http://localhost:4000/v1` by default).
    ///
    /// Translates the `OpenAI` wire format to provider-specific APIs including
    /// Vertex AI, Bedrock, Azure `OpenAI`, and Anthropic.
    ///
    /// Env var: `LITELLM_API_KEY`.
    LiteLLM,

    /// Generic `OpenAI`-compatible endpoint. No default base URL.
    ///
    /// Use for Together AI, Groq, Fireworks, Azure `OpenAI`, Databricks,
    /// RunPod vLLM, or any other `OpenAI`-shape endpoint not covered above.
    /// The caller **must** supply an explicit `base_url`.
    ///
    /// No canonical env var — credentials must be provided explicitly or via
    /// the caller's own convention.
    #[default]
    Generic,
}

impl OpenAIFlavor {
    /// Short identifier used in telemetry labels and provider names.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::OpenRouter => "openrouter",
            Self::LiteLLM => "litellm",
            Self::Generic => "openai-compat",
        }
    }

    /// Default base URL for this flavor.
    ///
    /// Returns an empty string for [`Self::Generic`] — callers must supply
    /// an explicit URL when using the `Generic` variant.
    #[must_use]
    pub const fn default_base_url(self) -> &'static str {
        match self {
            Self::OpenAi => "https://api.openai.com/v1",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::LiteLLM => "http://localhost:4000/v1",
            Self::Generic => "",
        }
    }

    /// Well-known environment variable for this flavor's API key.
    ///
    /// Returns an empty string for [`Self::Generic`] — there is no universal
    /// convention for generic endpoint credentials; callers provide the key
    /// explicitly or via their own env var.
    #[must_use]
    pub const fn default_api_key_env(self) -> &'static str {
        match self {
            Self::OpenAi => "OPENAI_API_KEY",
            Self::OpenRouter => "OPENROUTER_API_KEY",
            Self::LiteLLM => "LITELLM_API_KEY",
            Self::Generic => "",
        }
    }

    /// Returns `true` for flavors that accept OpenRouter app-attribution
    /// headers (`HTTP-Referer`, `X-Title`).
    ///
    /// These headers are optional but improve rate-limit treatment on
    /// OpenRouter endpoints per their published documentation.
    #[must_use]
    pub const fn needs_openrouter_headers(self) -> bool {
        matches!(self, Self::OpenRouter)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn as_str_values() {
        assert_eq!(OpenAIFlavor::OpenAi.as_str(), "openai");
        assert_eq!(OpenAIFlavor::OpenRouter.as_str(), "openrouter");
        assert_eq!(OpenAIFlavor::LiteLLM.as_str(), "litellm");
        assert_eq!(OpenAIFlavor::Generic.as_str(), "openai-compat");
    }

    #[test]
    fn default_base_url_generic_is_empty() {
        assert_eq!(OpenAIFlavor::Generic.default_base_url(), "");
    }

    #[test]
    fn default_api_key_env_generic_is_empty() {
        assert_eq!(OpenAIFlavor::Generic.default_api_key_env(), "");
    }

    #[test]
    fn openrouter_needs_headers() {
        assert!(OpenAIFlavor::OpenRouter.needs_openrouter_headers());
        assert!(!OpenAIFlavor::OpenAi.needs_openrouter_headers());
        assert!(!OpenAIFlavor::Generic.needs_openrouter_headers());
    }

    #[test]
    fn default_is_generic() {
        assert_eq!(OpenAIFlavor::default(), OpenAIFlavor::Generic);
    }
}
