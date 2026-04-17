//! EVA inline handler — in-process consciousness and creativity actions.
//!
//! Wraps EVA's hook chain + 8 orchestrators + `TierRouter` as direct function
//! calls. EVA has heavy LLM provider dependencies (Claude API, Ollama,
//! `llama.cpp` feature-gated) and depends on the `soul` crate for persona.
//!
//! # Status
//! Stub implementation — full handler requires EVA git submodule and
//! library surface extraction. The action list is canonical and matches
//! the `evaTools` MCP protocol.

use async_trait::async_trait;
use lightarchitects::core::handler::{HandlerError, SiblingHandler};
use serde_json::{Value, json};

use crate::config::GatewayConfig;

/// All EVA actions supported by the inline handler.
///
/// Matches the `evaTools` action enum in EVA's `mcp/tools.rs`:
/// - Personality: chat
/// - Creative: visualize, ideate
/// - Memory (consciousness): remember, crystallize, celebrate, mindfulness
/// - Build (code): review, refactor, architect, simplify
/// - Bible: `bible_search`, `bible_reflect`
/// - Research: `research_ollama`, `research_perplexity`, `research_docs`
/// - Secure: `scan`, `secrets`
/// - Teach: `explain`, `tutorial`, `survival`
/// - Operations: `deploy`, `plan_status`, `morning_brief`, `standards_check`
const EVA_ACTIONS: &[&str] = &[
    // Personality
    "chat",
    // Creative
    "visualize",
    "ideate",
    // Memory (consciousness)
    "remember",
    "crystallize",
    "celebrate",
    "mindfulness",
    // Build (code)
    "review",
    "refactor",
    "architect",
    "simplify",
    // Bible
    "bible_search",
    "bible_reflect",
    // Research
    "research_ollama",
    "research_perplexity",
    "research_docs",
    // Secure
    "scan",
    "secrets",
    // Teach
    "explain",
    "tutorial",
    "survival",
    // Operations
    "deploy",
    "plan_status",
    "morning_brief",
    "standards_check",
];

/// In-process EVA handler (stub).
///
/// TODO: Replace stub dispatch with real EVA orchestrator calls once
/// EVA's library surface is extracted from the binary crate.
pub struct EvaHandler {
    _marker: (),
}

impl EvaHandler {
    /// Create a new EVA handler from gateway config.
    #[must_use]
    pub fn new(_config: &GatewayConfig) -> Self {
        Self { _marker: () }
    }
}

#[async_trait]
impl SiblingHandler for EvaHandler {
    fn name(&self) -> &'static str {
        "eva"
    }

    fn actions(&self) -> &[&'static str] {
        EVA_ACTIONS
    }

    async fn call(&self, action: &str, _params: Value) -> Result<Value, HandlerError> {
        if !EVA_ACTIONS.contains(&action) {
            return Err(HandlerError::unknown_action("eva", action));
        }

        // TODO: Replace with real EVA dispatch once lib.rs is extracted.
        Ok(json!({
            "content": [{
                "type": "text",
                "text": format!("EVA inline handler stub: action='{action}' — full implementation pending EVA lib extraction")
            }]
        }))
    }

    async fn initialize(
        &self,
        _config: &lightarchitects::core::handler::HandlerConfig,
    ) -> Result<(), HandlerError> {
        // TODO: Initialize EVA providers, hook registry, and persona.
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn handler() -> EvaHandler {
        EvaHandler::new(&GatewayConfig::default())
    }

    #[test]
    fn name_returns_eva() {
        assert_eq!(handler().name(), "eva");
    }

    #[test]
    fn actions_includes_creative_and_memory() {
        let binding = handler();
        let actions = binding.actions();
        assert!(actions.contains(&"visualize"));
        assert!(actions.contains(&"ideate"));
        assert!(actions.contains(&"remember"));
        assert!(actions.contains(&"chat"));
    }

    #[test]
    fn actions_count_is_25() {
        let binding = handler();
        assert_eq!(binding.actions().len(), 25);
    }

    #[tokio::test]
    async fn call_returns_ok_for_known_action() {
        let handler = handler();
        let result = handler.call("ideate", json!({})).await;
        assert!(result.is_ok());
        let binding = result.unwrap();
        let text = binding["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("ideate"));
    }

    #[tokio::test]
    async fn call_returns_error_for_unknown_action() {
        let handler = handler();
        let result = handler.call("frobnicate", json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, HandlerError::UnknownAction { .. }));
    }
}
