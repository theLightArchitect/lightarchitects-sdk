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

use async_trait::async_trait;
use lightarchitects::core::handler::{HandlerError, SiblingHandler};
use serde_json::{Value, json};

use crate::config::GatewayConfig;

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

/// In-process SOUL handler (stub).
///
/// TODO: Replace with `soul_mcp::ToolRouter::execute_tool()` once
/// SOUL submodule is wired and `AppState` initialization is handled.
pub struct SoulHandler {
    _marker: (),
}

impl SoulHandler {
    /// Create a new SOUL handler from gateway config.
    #[must_use]
    pub fn new(_config: &GatewayConfig) -> Self {
        Self { _marker: () }
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

    async fn call(&self, action: &str, _params: Value) -> Result<Value, HandlerError> {
        if !SOUL_ACTIONS.contains(&action) {
            return Err(HandlerError::unknown_action("soul", action));
        }

        // TODO: Replace with real SOUL dispatch via soul_mcp::ToolRouter.
        Ok(json!({
            "content": [{
                "type": "text",
                "text": format!("SOUL inline handler stub: action='{action}' — full implementation pending soul_mcp ToolRouter integration")
            }]
        }))
    }

    async fn initialize(
        &self,
        _config: &lightarchitects::core::handler::HandlerConfig,
    ) -> Result<(), HandlerError> {
        // TODO: Initialize SOUL AppState (vault root, optional Neo4j, embed provider).
        // With `helix` feature: also initialize Neo4j connection and fastembed.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handler() -> SoulHandler {
        SoulHandler::new(&GatewayConfig::default())
    }

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
}
