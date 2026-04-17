//! AYIN inline handler — in-process observability actions.
//!
//! Wraps AYIN's `dispatch_action` as an in-process function call instead of
//! spawning the `ayin-mcp` binary as a subprocess. The handler reads from
//! the same JSONL trace files that the standalone AYIN viewer uses.

use std::sync::Arc;

use async_trait::async_trait;
use ayin_core::store::TraceStoreQuery;
use ayin_engine::JsonlTraceStore;
use ayin_mcp::{AYIN_ACTIONS, dispatch_action};
use lightarchitects::core::handler::{HandlerConfig, HandlerError, SiblingHandler};
use serde_json::Value;

use crate::config::GatewayConfig;

/// In-process AYIN handler.
///
/// Holds a [`TraceStoreQuery`] backend that reads AYIN's JSONL conversation
/// traces. All 12 AYIN actions are dispatched directly without MCP handshake
/// overhead.
pub struct AyinHandler {
    store: Arc<dyn TraceStoreQuery>,
}

impl AyinHandler {
    /// Create a new AYIN handler from gateway config.
    pub fn new(_config: &GatewayConfig) -> Self {
        let store: Arc<dyn TraceStoreQuery> = Arc::new(JsonlTraceStore::new());
        Self { store }
    }
}

#[async_trait]
impl SiblingHandler for AyinHandler {
    fn name(&self) -> &'static str {
        "ayin"
    }

    fn actions(&self) -> &[&'static str] {
        AYIN_ACTIONS
    }

    async fn call(&self, action: &str, params: Value) -> Result<Value, HandlerError> {
        if !AYIN_ACTIONS.contains(&action) {
            return Err(HandlerError::unknown_action("ayin", action));
        }

        let result = dispatch_action(&self.store, action, &params).await;

        // Wrap the raw action result in MCP `content` format.
        // `dispatch_action` returns {"verdict": ..., "data": ...}
        // The MCP wire format expects {"content": [{"type": "text", "text": "..."}]}
        let text = serde_json::to_string_pretty(&result).unwrap_or_else(|_| {
            serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
        });

        Ok(serde_json::json!({
            "content": [{
                "type": "text",
                "text": text,
            }]
        }))
    }

    async fn initialize(&self, _config: &HandlerConfig) -> Result<(), HandlerError> {
        // AYIN has no external services to connect — JSONL files are read on-demand.
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn handler() -> AyinHandler {
        AyinHandler::new(&GatewayConfig::default())
    }

    #[test]
    fn name_returns_ayin() {
        assert_eq!(handler().name(), "ayin");
    }

    #[test]
    fn actions_match_canonical_list() {
        let binding = handler();
        let actions = binding.actions();
        assert_eq!(actions.len(), AYIN_ACTIONS.len());
        for action in AYIN_ACTIONS {
            assert!(actions.contains(action), "missing action: {action}");
        }
    }

    #[tokio::test]
    async fn call_returns_ok_for_known_action() {
        let handler = handler();
        let result = handler.call("stats", serde_json::json!({})).await;
        assert!(result.is_ok());
        let binding = result.unwrap();
        let text = binding["content"][0]["text"].as_str().unwrap();
        // Real dispatch returns verdict + data, not the old stub text
        assert!(
            !text.contains("inline handler stub"),
            "should not return stub response"
        );
    }

    #[tokio::test]
    async fn call_returns_error_for_unknown_action() {
        let handler = handler();
        let result = handler.call("frobnicate", serde_json::json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, HandlerError::UnknownAction { .. }));
    }
}
