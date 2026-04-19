//! CORSO inline handler — in-process Trinity pipeline dispatch.
//!
//! Placeholder implementation. The real inline handler requires `corso-server`
//! and `corso-trinity-core` crates, which are not yet published to crates.io.
//! Until those crates are available, this handler stubs the interface so that
//! `--all-features` compiles cleanly.
//!
//! # Heavy dependencies
//!
//! CORSO pulls in `PyO3` (Python 3.14 embedding), SOUL, soul-engine,
//! neural-engine, voice-engine, tree-sitter (5 grammars), and prometheus.
//! These add significant compile time and binary size, which is why this
//! handler is gated behind the `inline-corso` feature flag.
//!
//! Re-enable the real implementation by:
//! 1. Adding `corso-server` and `corso-trinity-core` to `lightarchitects-gateway/Cargo.toml`
//!    under `[dependencies]` with `optional = true` and the `inline-corso` feature.
//! 2. Restoring the full dispatch logic from git history.

use async_trait::async_trait;
use lightarchitects::core::handler::{HandlerConfig, HandlerError, SiblingHandler};
use serde_json::Value;

use crate::config::GatewayConfig;

/// Canonical CORSO action names — matches `tool_routes.rs` ROUTES array.
const CORSO_ACTIONS: &[&str] = &[
    // Ruach — filesystem (direct execution, sub-ms)
    "read_file",
    "write_file",
    "list_directory",
    // Uriel — code & architecture
    "sniff",
    "search_code",
    "generate_code",
    "code_review",
    "find_symbol",
    "get_outline",
    "get_references",
    // Michael — security & deployment
    "guard",
    "deploy",
    "rollback",
    "container_manage",
    "secret_manage",
    "strike",
    "watch",
    // Gabriel — knowledge & strategy
    "fetch",
    "search_documentation",
    "analyze_architecture",
    "scout",
    "prove",
    "optimize",
    // Raphael — infrastructure & ops
    "chase",
    "monitor_health",
    "scale_resources",
    "manage_logs",
];

/// In-process CORSO handler (stub — real impl requires unpublished deps).
pub struct CorsoHandler;

impl CorsoHandler {
    /// Create a new CORSO handler from gateway config.
    #[must_use]
    pub fn new(_config: &GatewayConfig) -> Self {
        Self
    }
}

#[async_trait]
impl SiblingHandler for CorsoHandler {
    fn name(&self) -> &'static str {
        "corso"
    }

    fn actions(&self) -> &[&'static str] {
        CORSO_ACTIONS
    }

    async fn call(&self, action: &str, _params: Value) -> Result<Value, HandlerError> {
        if !CORSO_ACTIONS.contains(&action) {
            return Err(HandlerError::unknown_action("corso", action));
        }
        Err(HandlerError::not_initialized(
            "corso",
            "inline-corso handler not yet available — corso-server/trinity-core not published",
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

    fn handler() -> CorsoHandler {
        CorsoHandler::new(&GatewayConfig::default())
    }

    #[test]
    fn name_returns_corso() {
        assert_eq!(handler().name(), "corso");
    }

    #[test]
    fn actions_includes_canonical_routes() {
        let binding = handler();
        let actions = binding.actions();
        assert!(actions.contains(&"read_file"));
        assert!(actions.contains(&"guard"));
        assert!(actions.contains(&"sniff"));
        assert!(actions.contains(&"fetch"));
        assert!(actions.contains(&"chase"));
        assert!(actions.contains(&"prove"));
    }

    #[test]
    fn actions_count_matches_routes() {
        let binding = handler();
        assert_eq!(binding.actions().len(), 27);
    }

    #[tokio::test]
    async fn call_returns_error_for_unknown_action() {
        let handler = handler();
        let result = handler
            .call("nonexistent_tool", serde_json::json!({}))
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            HandlerError::UnknownAction { .. }
        ));
    }

    #[tokio::test]
    async fn call_returns_not_initialized_for_known_action() {
        let handler = handler();
        let result = handler.call("guard", serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            HandlerError::NotInitialized { .. }
        ));
    }
}
