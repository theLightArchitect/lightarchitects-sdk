//! AYIN inline handler — in-process observability actions.
//!
//! Placeholder implementation. The real inline handler requires `ayin-core`,
//! `ayin-engine`, and `ayin-mcp` crates, which are not yet published to
//! crates.io. Until those crates are available, this handler stubs the
//! interface so that `--all-features` compiles cleanly.
//!
//! Re-enable the real implementation by:
//! 1. Adding `ayin-core`, `ayin-engine`, `ayin-mcp` to `lightarchitects-gateway/Cargo.toml`
//!    under `[dependencies]` with `optional = true` and the `inline-ayin` feature.
//! 2. Restoring the full dispatch logic from git history.

use async_trait::async_trait;
use lightarchitects::core::handler::{HandlerConfig, HandlerError, SiblingHandler};
use serde_json::Value;

use crate::config::GatewayConfig;

/// Canonical AYIN action names — 12 observability actions.
const AYIN_ACTIONS: &[&str] = &[
    "stats",
    "list_conversations",
    "get_conversation",
    "search_conversations",
    "get_turn",
    "list_turns",
    "export_conversation",
    "get_metrics",
    "list_sessions",
    "get_session",
    "get_timeline",
    "get_trace",
];

/// In-process AYIN handler (stub — real impl requires unpublished deps).
pub struct AyinHandler;

impl AyinHandler {
    /// Create a new AYIN handler from gateway config.
    #[must_use]
    pub fn new(_config: &GatewayConfig) -> Self {
        Self
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

    async fn call(&self, action: &str, _params: Value) -> Result<Value, HandlerError> {
        if !AYIN_ACTIONS.contains(&action) {
            return Err(HandlerError::unknown_action("ayin", action));
        }
        Err(HandlerError::not_initialized(
            "ayin",
            "inline-ayin handler not yet available — ayin-core/engine/mcp not published",
        ))
    }

    async fn initialize(&self, _config: &HandlerConfig) -> Result<(), HandlerError> {
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
    fn actions_count() {
        assert_eq!(handler().actions().len(), AYIN_ACTIONS.len());
    }

    #[tokio::test]
    async fn call_unknown_action_errors() {
        let result = handler().call("frobnicate", serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            HandlerError::UnknownAction { .. }
        ));
    }
}
