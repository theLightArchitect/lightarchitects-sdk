//! Shared LiteLLM provider factory for all sibling handlers and skill dispatch.
//!
//! All provider coupling routes through this module so callers need only
//! configure `LA_LITELLM_*` env vars — no per-handler provider selection.
//!
//! # Environment variables
//!
//! | Var | Default | Description |
//! |-----|---------|-------------|
//! | `LA_LITELLM_BASE_URL` | `http://localhost:4000` | LiteLLM proxy base URL |
//! | `LA_LITELLM_API_KEY` | `la-local-dev` | Bearer token for the proxy |
//! | `LA_LITELLM_MODEL` | `anthropic/claude-sonnet-4-6` | Model string sent to the proxy |
//!
//! # Legacy mapping
//!
//! When `LA_LITELLM_MODEL` is absent, the legacy `LA_LLM` + `LA_MODEL` pair is
//! mapped to a LiteLLM model prefix so existing deployments keep working:
//!
//! | `LA_LLM` | `LA_MODEL` | LiteLLM model |
//! |----------|-----------|---------------|
//! | `ollama` | `llama3` | `ollama/llama3` |
//! | `anthropic` | `claude-opus-4-7` | `anthropic/claude-opus-4-7` |
//! | `claude` / (default) | — | `anthropic/claude-sonnet-4-6` |

use lightarchitects::agent::openai_compat::OpenAICompatProvider;

/// Build an `OpenAICompatProvider` pointing at the LiteLLM proxy.
///
/// Reads the `LA_LITELLM_*` env vars; falls back to legacy `LA_LLM`/`LA_MODEL`
/// mapping for the model name.
///
/// # Errors
///
/// Returns an error if `reqwest` cannot build an HTTP client (system-level
/// failure; essentially never fires with a functioning TLS stack).
pub fn build_provider() -> Result<OpenAICompatProvider, String> {
    let base_url =
        std::env::var("LA_LITELLM_BASE_URL").unwrap_or_else(|_| "http://localhost:4000".to_owned());
    let api_key = std::env::var("LA_LITELLM_API_KEY").unwrap_or_else(|_| "la-local-dev".to_owned());
    let model = std::env::var("LA_LITELLM_MODEL").unwrap_or_else(|_| model_from_legacy_env());
    OpenAICompatProvider::for_litellm(Some(base_url), api_key, model)
}

/// Map the legacy `LA_LLM` + `LA_MODEL` pair to a LiteLLM model prefix.
fn model_from_legacy_env() -> String {
    let backend = std::env::var("LA_LLM").unwrap_or_default().to_lowercase();
    let model = std::env::var("LA_MODEL").unwrap_or_default();
    match backend.as_str() {
        "ollama" => {
            let m = if model.is_empty() {
                "glm-5.1:cloud"
            } else {
                &model
            };
            format!("ollama/{m}")
        }
        "anthropic" | "claude-api" => {
            let m = if model.is_empty() {
                "claude-sonnet-4-6"
            } else {
                &model
            };
            format!("anthropic/{m}")
        }
        _ => "anthropic/claude-sonnet-4-6".to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn build_provider_succeeds_with_defaults() {
        // Unset LA_LITELLM_* so the hardcoded defaults are used.
        unsafe {
            std::env::remove_var("LA_LITELLM_BASE_URL");
            std::env::remove_var("LA_LITELLM_API_KEY");
            std::env::remove_var("LA_LITELLM_MODEL");
            std::env::remove_var("LA_LLM");
            std::env::remove_var("LA_MODEL");
        }
        let p = build_provider().unwrap();
        assert_eq!(p.name(), "litellm");
    }

    #[test]
    fn model_from_legacy_env_ollama() {
        unsafe {
            std::env::set_var("LA_LLM", "ollama");
            std::env::set_var("LA_MODEL", "llama3");
        }
        assert_eq!(model_from_legacy_env(), "ollama/llama3");
        unsafe {
            std::env::remove_var("LA_LLM");
            std::env::remove_var("LA_MODEL");
        }
    }

    #[test]
    fn model_from_legacy_env_anthropic() {
        unsafe {
            std::env::set_var("LA_LLM", "anthropic");
            std::env::set_var("LA_MODEL", "claude-opus-4-7");
        }
        assert_eq!(model_from_legacy_env(), "anthropic/claude-opus-4-7");
        unsafe {
            std::env::remove_var("LA_LLM");
            std::env::remove_var("LA_MODEL");
        }
    }

    #[test]
    fn model_from_legacy_env_defaults_to_claude_sonnet() {
        unsafe {
            std::env::remove_var("LA_LLM");
            std::env::remove_var("LA_MODEL");
        }
        assert_eq!(model_from_legacy_env(), "anthropic/claude-sonnet-4-6");
    }
}
