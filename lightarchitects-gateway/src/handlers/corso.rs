//! CORSO inline handler — in-process Trinity pipeline dispatch.
//!
//! Wraps CORSO's `ToolRouter::execute_tool` as direct function calls instead
//! of spawning the `corso` binary as a subprocess. The Trinity pipeline
//! (RUACH → IESOUS → ADONAI) runs entirely in-process.
//!
//! # Heavy dependencies
//!
//! CORSO pulls in `PyO3` (Python 3.14 embedding), SOUL, soul-engine,
//! neural-engine, voice-engine, tree-sitter (5 grammars), and prometheus.
//! These add significant compile time and binary size, which is why this
//! handler is gated behind the `inline-corso` feature flag.

use std::sync::OnceLock;

use async_trait::async_trait;
use corso_server::router::ToolRouter;
use corso_trinity_core::CorsoError;
use lightarchitects::core::handler::{HandlerConfig, HandlerError, SiblingHandler};
use serde_json::{Value, json};

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

/// In-process CORSO handler.
///
/// Holds a [`ToolRouter`] instance that wraps the full Trinity pipeline.
/// `ToolRouter::new()` initializes `RuachAgent`, `IesousAgent`, `AdonaiAgent`,
/// plus helpers (Cherubim, sandbox client, tree-sitter cache). This is done
/// once at gateway startup in [`initialize`](SiblingHandler::initialize).
pub struct CorsoHandler {
    router: OnceLock<ToolRouter>,
}

impl CorsoHandler {
    /// Create a new CORSO handler from gateway config.
    ///
    /// The `ToolRouter` is not initialized here — it's deferred to
    /// [`initialize`](SiblingHandler::initialize) because it can fail
    /// on missing Trinity configuration.
    #[must_use]
    pub fn new(_config: &GatewayConfig) -> Self {
        Self {
            router: OnceLock::new(),
        }
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

    async fn call(&self, action: &str, params: Value) -> Result<Value, HandlerError> {
        if !CORSO_ACTIONS.contains(&action) {
            return Err(HandlerError::unknown_action("corso", action));
        }

        let router = self
            .router
            .get()
            .ok_or_else(|| HandlerError::not_initialized("corso", "ToolRouter not initialized"))?;

        match router.execute_tool(action, params).await {
            Ok(response_text) => Ok(json!({
                "content": [{
                    "type": "text",
                    "text": response_text,
                }]
            })),
            Err(CorsoError::ToolNotFound(name)) => {
                Err(HandlerError::unknown_action("corso", &name))
            }
            Err(CorsoError::SecurityValidation(msg)) => {
                Err(HandlerError::service_error("corso", action, msg))
            }
            Err(CorsoError::InvalidInput(msg)) => {
                Err(HandlerError::invalid_params("corso", action, msg))
            }
            Err(e) => Err(HandlerError::service_error("corso", action, e.to_string())),
        }
    }

    async fn initialize(&self, _config: &HandlerConfig) -> Result<(), HandlerError> {
        // ToolRouter::new() initializes RuachAgent, IesousAgent, AdonaiAgent,
        // CherubimHelper, sandbox client, and tree-sitter cache.
        let router = ToolRouter::new().map_err(|e| {
            HandlerError::not_initialized("corso", format!("ToolRouter init failed: {e}"))
        })?;

        if self.router.set(router).is_err() {
            // initialize() called twice — programming error, not runtime.
            // Log but don't panic (matches registry.rs pattern).
            tracing::error!("CorsoHandler::initialize called more than once — this is a bug");
        }

        Ok(())
    }

    async fn shutdown(&self) -> Result<(), HandlerError> {
        // ToolRouter has no explicit shutdown — its Arc<RuachAgent> etc.
        // drop naturally when the handler is dropped.
        Ok(())
    }
}

#[cfg(test)]
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
        // Core routes from each domain
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
        let result = handler.call("nonexistent_tool", json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, HandlerError::UnknownAction { .. }));
    }

    #[tokio::test]
    async fn call_returns_error_when_not_initialized() {
        let handler = handler();
        let result = handler.call("guard", json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, HandlerError::NotInitialized { .. }));
    }
}
