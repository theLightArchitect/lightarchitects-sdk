//! QUANTUM inline handler — in-process investigation and research actions.
//!
//! Wraps QUANTUM's hook system + provider routing + sandbox isolation as
//! direct function calls. QUANTUM has heavy deps (Ollama/rayon/cloud-storage,
//! SERAPH SDK) that increase compile time and binary size.
//!
//! # Status
//! Stub implementation — real dispatch requires `quantum_q::call_tool` with
//! initialized providers, hook registry, and sandbox. The action list is
//! canonical and matches `qsTools` in the MCP protocol.

use async_trait::async_trait;
use lightarchitects::core::handler::{HandlerError, SiblingHandler};
use serde_json::{Value, json};

use crate::config::GatewayConfig;

/// All QUANTUM actions supported by the inline handler.
///
/// Matches the `qsTools` action enum in `mcp.rs`:
/// - Investigation cycle: triage → sweep → trace → probe → theorize → verify → close
/// - Shortcuts: quick, research, helix
/// - Utility: discover, list, execute, workflow, scan
const QUANTUM_ACTIONS: &[&str] = &[
    // Investigation cycle (7 phases)
    "triage", "sweep", "trace", "probe", "theorize", "verify", "close", // Shortcuts
    "quick", "research", "helix", // Utility
    "discover", "list", "execute", "workflow", "scan",
];

/// In-process QUANTUM handler (stub).
///
/// TODO: Replace stub dispatch with `quantum_q::call_tool()` once
/// provider initialization is wired through `HandlerConfig`.
pub struct QuantumHandler {
    _marker: (),
}

impl QuantumHandler {
    /// Create a new QUANTUM handler from gateway config.
    #[must_use]
    pub fn new(_config: &GatewayConfig) -> Self {
        Self { _marker: () }
    }
}

#[async_trait]
impl SiblingHandler for QuantumHandler {
    fn name(&self) -> &'static str {
        "quantum"
    }

    fn actions(&self) -> &[&'static str] {
        QUANTUM_ACTIONS
    }

    async fn call(&self, action: &str, _params: Value) -> Result<Value, HandlerError> {
        if !QUANTUM_ACTIONS.contains(&action) {
            return Err(HandlerError::unknown_action("quantum", action));
        }

        // TODO: Replace with real dispatch:
        //   use quantum_q::orchestrators::call_tool::{call_tool, CallToolParams, CallToolOperation};
        //   let params = CallToolParams { operation, tool, params, ... };
        //   let result = call_tool(params).await?;
        Ok(json!({
            "content": [{
                "type": "text",
                "text": format!("QUANTUM inline handler stub: action='{action}' — full implementation pending provider initialization")
            }]
        }))
    }

    async fn initialize(
        &self,
        _config: &lightarchitects::core::handler::HandlerConfig,
    ) -> Result<(), HandlerError> {
        // TODO: Initialize QUANTUM providers, hook registry, and sandbox.
        // Provider initialization requires API keys from HandlerConfig.
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn handler() -> QuantumHandler {
        QuantumHandler::new(&GatewayConfig::default())
    }

    #[test]
    fn name_returns_quantum() {
        assert_eq!(handler().name(), "quantum");
    }

    #[test]
    fn actions_includes_investigation_cycle() {
        let binding = handler();
        let actions = binding.actions();
        assert!(actions.contains(&"triage"));
        assert!(actions.contains(&"sweep"));
        assert!(actions.contains(&"probe"));
        assert!(actions.contains(&"verify"));
        assert!(actions.contains(&"close"));
    }

    #[test]
    fn actions_includes_utility() {
        let binding = handler();
        let actions = binding.actions();
        assert!(actions.contains(&"discover"));
        assert!(actions.contains(&"list"));
        assert!(actions.contains(&"execute"));
        assert!(actions.contains(&"helix"));
    }

    #[test]
    fn actions_count_is_15() {
        let binding = handler();
        assert_eq!(binding.actions().len(), 15);
    }

    #[tokio::test]
    async fn call_returns_ok_for_known_action() {
        let handler = handler();
        let result = handler.call("triage", json!({})).await;
        assert!(result.is_ok());
        let binding = result.unwrap();
        let text = binding["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("triage"));
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
