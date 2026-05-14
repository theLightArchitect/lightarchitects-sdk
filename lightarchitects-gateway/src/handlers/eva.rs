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
//!
//! # Alias routing
//!
//! Several `evaTools` umbrella names map to multiple fine-grained actions
//! based on a `mode` or `type` param. Aliases are resolved in [`EvaHandler::call`]
//! before the canonical action lookup: callers sending `"teach"` are routed
//! to `"explain"`, `"tutorial"`, or `"survival"` without appearing in
//! [`EVA_ACTIONS`].

use async_trait::async_trait;
use lightarchitects::core::handler::{HandlerError, SiblingHandler};
use serde_json::{Value, json};

use crate::config::GatewayConfig;

/// All EVA canonical actions supported by the inline handler.
///
/// Matches the `evaTools` action enum in EVA's `mcp/tools.rs`:
/// - Personality: `chat`
/// - Creative: `visualize`, `ideate`
/// - Memory (consciousness): `remember`, `crystallize`, `celebrate`, `mindfulness`
/// - Build (code): `review`, `refactor`, `architect`, `simplify`
/// - Bible: `bible_search`, `bible_reflect`
/// - Research: `research_ollama`, `research_perplexity`, `research_docs`
/// - Secure: `scan`, `secrets`
/// - Teach: `explain`, `tutorial`, `survival`
/// - Operations: `deploy`, `plan_status`, `morning_brief`, `standards_check`
/// - DevOps (Phase 3.5 stubs): `lint`, `status`, `repo`, `enrich`,
///   `deploy_gate`, `pipeline_reflect`, `discover`
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
    // DevOps — Phase 3.5 stubs (binary-missing from audit matrix)
    "lint",
    "status",
    "repo",
    "enrich",
    "deploy_gate",
    "pipeline_reflect",
    "discover",
];

/// Resolve an umbrella alias to a canonical `EVA_ACTIONS` entry.
///
/// Returns `None` when `action` is not an alias (caller should proceed with
/// the original name). The `mode`/`type`/`provider` param key selects among
/// multiple targets; defaults apply when the key is absent or unrecognised.
fn resolve_alias<'a>(action: &'a str, params: &Value) -> Option<&'a str> {
    match action {
        "teach" => Some(match params["mode"].as_str().unwrap_or("") {
            "tutorial" => "tutorial",
            "survival" => "survival",
            _ => "explain",
        }),
        "research" => Some(match params["provider"].as_str().unwrap_or("") {
            "ollama" => "research_ollama",
            "perplexity" => "research_perplexity",
            _ => "research_docs",
        }),
        "bible" => Some("bible_reflect"),
        "memory" => Some(match params["type"].as_str().unwrap_or("") {
            "crystallize" => "crystallize",
            "celebrate" => "celebrate",
            _ => "remember",
        }),
        "build" => Some(match params["mode"].as_str().unwrap_or("") {
            "refactor" => "refactor",
            "architect" => "architect",
            "simplify" => "simplify",
            _ => "review",
        }),
        "secure" => Some(match params["type"].as_str().unwrap_or("") {
            "secrets" => "secrets",
            _ => "scan",
        }),
        "imagine" => Some("ideate"),
        _ => None,
    }
}

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

    async fn call(&self, action: &str, params: Value) -> Result<Value, HandlerError> {
        let dispatch = resolve_alias(action, &params).unwrap_or(action);

        if !EVA_ACTIONS.contains(&dispatch) {
            return Err(HandlerError::unknown_action("eva", action));
        }

        // TODO: Replace with real EVA dispatch once lib.rs is extracted.
        Ok(json!({
            "content": [{
                "type": "text",
                "text": format!("EVA inline handler stub: action='{dispatch}' — full implementation pending EVA lib extraction")
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
    fn actions_includes_phase35_stubs() {
        let binding = handler();
        let actions = binding.actions();
        assert!(actions.contains(&"lint"));
        assert!(actions.contains(&"status"));
        assert!(actions.contains(&"repo"));
        assert!(actions.contains(&"enrich"));
        assert!(actions.contains(&"deploy_gate"));
        assert!(actions.contains(&"pipeline_reflect"));
        assert!(actions.contains(&"discover"));
    }

    #[test]
    fn actions_count_is_32() {
        let binding = handler();
        assert_eq!(binding.actions().len(), 32);
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

    // ── Alias routing ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn alias_teach_defaults_to_explain() {
        let h = handler();
        let result = h.call("teach", json!({})).await.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("explain"));
    }

    #[tokio::test]
    async fn alias_teach_mode_tutorial_routes_to_tutorial() {
        let h = handler();
        let result = h.call("teach", json!({"mode": "tutorial"})).await.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("tutorial"));
    }

    #[tokio::test]
    async fn alias_research_defaults_to_research_docs() {
        let h = handler();
        let result = h.call("research", json!({})).await.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("research_docs"));
    }

    #[tokio::test]
    async fn alias_bible_routes_to_bible_reflect() {
        let h = handler();
        let result = h.call("bible", json!({})).await.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("bible_reflect"));
    }

    #[tokio::test]
    async fn alias_imagine_routes_to_ideate() {
        let h = handler();
        let result = h.call("imagine", json!({})).await.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("ideate"));
    }

    #[tokio::test]
    async fn alias_secure_type_secrets_routes_to_secrets() {
        let h = handler();
        let result = h.call("secure", json!({"type": "secrets"})).await.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("secrets"));
    }

    #[tokio::test]
    async fn alias_build_mode_architect_routes_to_architect() {
        let h = handler();
        let result = h.call("build", json!({"mode": "architect"})).await.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("architect"));
    }
}
