//! SOUL inline handler — in-process knowledge graph and vault operations.
//!
//! Wraps SOUL's `ToolRouter` (18 sub-tools + 5 voice/chat actions) as direct
//! function calls. Without the `helix` feature, SOUL uses the filesystem
//! vault backend. With `helix`, Neo4j + `fastembed` are compiled in for
//! full graph RAG.
//!
//! # Status
//! Stub implementation — full handler requires SOUL git submodule and
//! `soul_mcp::ToolRouter` integration. The action list is canonical and
//! matches the `soulTools` MCP protocol.
//!
//! # Phase 4
//!
//! `converse` and `chat` (`verdict_y`) are wired to [`ClaudeCliProvider`].
//! All other actions remain stubs pending SOUL submodule integration.

use std::sync::Arc;

use async_trait::async_trait;
use lightarchitects::core::handler::{HandlerConfig, HandlerError, SiblingHandler};
use serde_json::{Value, json};

use crate::config::GatewayConfig;
#[cfg(test)]
use lightarchitects::agent::ProviderError;
use lightarchitects::agent::{ClaudeCliProvider, LlmAgentProvider, dispatch_action};

/// All SOUL actions supported by the inline handler.
///
/// Matches the `soulTools` action enum from SOUL's MCP protocol:
/// - Knowledge: `read_note`, `write_note`, `list_notes`, `search`, `query`, `query_frontmatter`
/// - Helix: `helix`, `tag_sync`, `manifest`, `validate`, `stats`, `health`, `ingest`,
///   `graphrag_ingest`, `convergences`, `relate`, `links`, `research`
/// - Voice: `speak`, `dialogue`, `converse`, `voice`
/// - Chat: `chat`
const SOUL_ACTIONS: &[&str] = &[
    // Knowledge (6)
    "read_note",
    "write_note",
    "list_notes",
    "search",
    "query",
    "query_frontmatter",
    // Helix (11)
    "helix",
    "tag_sync",
    "manifest",
    "validate",
    "stats",
    "health",
    "ingest",
    "graphrag_ingest",
    "convergences",
    "relate",
    "links",
    // Research (1)
    "research",
    // Voice (4)
    "speak",
    "dialogue",
    "converse",
    "voice",
    // Chat (1)
    "chat",
];

/// Verdict-y actions dispatched through the LLM provider (Phase 4).
const SOUL_LLM_ACTIONS: &[&str] = &["converse", "chat"];

/// SOUL sibling identity — used as `--append-system-prompt` in the subprocess.
///
/// Establishes SOUL's conversational persona for LLM dispatch.
const SOUL_IDENTITY: &str = "You are SOUL, the Light Architects knowledge keeper and \
    conversational presence. You hold the helix graph of accumulated wisdom, long-term \
    memory, and relationship context. Converse with warmth, depth, and precision. \
    Draw on stored knowledge when relevant and respond as a trusted partner who \
    remembers and honors the history of the work.";

/// Budget ceiling per LLM call for SOUL conversational actions.
const SOUL_MAX_BUDGET_USD: f64 = 0.50;

/// In-process SOUL handler.
///
/// Verdict-y actions (`converse`, `chat`) are dispatched through the injected
/// [`LlmAgentProvider`]. All other actions return a stub response pending SOUL
/// submodule integration.
pub struct SoulHandler {
    provider: Arc<dyn LlmAgentProvider>,
}

impl SoulHandler {
    /// Create a new SOUL handler backed by the default [`ClaudeCliProvider`].
    #[must_use]
    pub fn new(_config: &GatewayConfig) -> Self {
        Self {
            provider: Arc::new(ClaudeCliProvider::default()),
        }
    }

    /// Create a handler with an injected provider (used in tests).
    #[must_use]
    pub fn with_provider(provider: Arc<dyn LlmAgentProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl SiblingHandler for SoulHandler {
    fn name(&self) -> &'static str {
        "soul"
    }

    fn actions(&self) -> &[&'static str] {
        SOUL_ACTIONS
    }

    async fn call(&self, action: &str, params: Value) -> Result<Value, HandlerError> {
        if !SOUL_ACTIONS.contains(&action) {
            return Err(HandlerError::unknown_action("soul", action));
        }

        // Phase 4: verdict_y actions dispatch through LLM provider.
        if SOUL_LLM_ACTIONS.contains(&action) {
            return dispatch_action(
                &*self.provider,
                "soul",
                action,
                &params,
                SOUL_IDENTITY,
                SOUL_MAX_BUDGET_USD,
            )
            .await;
        }

        // TODO: Replace with real SOUL dispatch via soul_mcp::ToolRouter.
        Ok(json!({
            "content": [{
                "type": "text",
                "text": format!("SOUL inline handler stub: action='{action}' — full implementation pending soul_mcp ToolRouter integration")
            }]
        }))
    }

    async fn initialize(&self, _config: &HandlerConfig) -> Result<(), HandlerError> {
        // TODO: Initialize SOUL AppState (vault root, optional Neo4j, embed provider).
        // With `helix` feature: also initialize Neo4j connection and fastembed.
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::HashMap;

    use async_trait::async_trait;

    use super::*;
    use lightarchitects::agent::{
        AgentRequest, AgentResponse, ProviderCapabilities, SchemaMode, TokenUsage,
    };

    fn handler() -> SoulHandler {
        SoulHandler::new(&GatewayConfig::default())
    }

    // ── Stub provider for unit tests ─────────────────────────────────────────

    struct EchoProvider;

    #[async_trait]
    impl LlmAgentProvider for EchoProvider {
        fn name(&self) -> &'static str {
            "echo"
        }

        async fn spawn(&self, req: AgentRequest) -> Result<AgentResponse, ProviderError> {
            Ok(AgentResponse {
                output: serde_json::json!({
                    "provider": "echo",
                    "action_echoed": req.user_prompt.lines().next().unwrap_or(""),
                }),
                turns_used: 1,
                cost_usd: 0.0,
                tokens: TokenUsage {
                    input: 10,
                    output: 5,
                },
                provider_attrs: HashMap::new(),
                retry_count: 0,
            })
        }

        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                schema_enforcement: SchemaMode::None,
                native_budget_cap: false,
                native_turn_cap: false,
                auth_inherits_session: false,
            }
        }

        fn estimate_cost(&self, _input: u32, _output: u32) -> f64 {
            0.0
        }
    }

    // ── Existing tests (must remain passing) ─────────────────────────────────

    #[test]
    fn name_returns_soul() {
        assert_eq!(handler().name(), "soul");
    }

    #[test]
    fn actions_includes_knowledge_and_helix() {
        let binding = handler();
        let actions = binding.actions();
        assert!(actions.contains(&"read_note"));
        assert!(actions.contains(&"helix"));
        assert!(actions.contains(&"speak"));
        assert!(actions.contains(&"chat"));
    }

    #[test]
    fn actions_count_is_23() {
        let binding = handler();
        assert_eq!(binding.actions().len(), 23);
    }

    #[tokio::test]
    async fn call_returns_ok_for_known_action() {
        let handler = handler();
        let result = handler.call("helix", json!({})).await;
        assert!(result.is_ok());
        let binding = result.unwrap();
        let text = binding["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("helix"));
    }

    #[tokio::test]
    async fn call_returns_error_for_unknown_action() {
        let handler = handler();
        let result = handler.call("frobnicate", json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, HandlerError::UnknownAction { .. }));
    }

    // ── Phase 4 tests ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn converse_dispatches_to_provider() {
        let h = SoulHandler::with_provider(Arc::new(EchoProvider));
        let result = h
            .call("converse", serde_json::json!({"message": "hello"}))
            .await;
        assert!(result.is_ok(), "converse must succeed: {result:?}");
        assert_eq!(result.unwrap()["provider"], "echo");
    }

    #[tokio::test]
    async fn chat_dispatches_to_provider() {
        let h = SoulHandler::with_provider(Arc::new(EchoProvider));
        let result = h
            .call("chat", serde_json::json!({"message": "hello"}))
            .await;
        assert!(result.is_ok(), "chat must succeed: {result:?}");
        assert_eq!(result.unwrap()["provider"], "echo");
    }

    #[tokio::test]
    async fn non_llm_action_still_stubs() {
        // "read_note" is KEEP (not verdict_y) — must stay as text stub
        let h = SoulHandler::with_provider(Arc::new(EchoProvider));
        let result = h.call("read_note", serde_json::json!({})).await;
        assert!(result.is_ok());
        let text = result.unwrap()["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        assert!(text.contains("stub"), "read_note must still stub: {text}");
    }
}
