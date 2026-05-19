//! QUANTUM inline handler — in-process investigation and research actions.
//!
//! Wraps QUANTUM's hook system + provider routing + sandbox isolation as
//! direct function calls. QUANTUM has heavy deps (Ollama/rayon/cloud-storage,
//! SERAPH SDK) that increase compile time and binary size.
//!
//! # Status
//! Phase 4: the 7 `verdict_y` `LLM_AGENT` actions (`sweep`, `trace`, `probe`,
//! `theorize`, `verify`, `close`, `research`) are wired through
//! [`ClaudeCliProvider`]. The remaining 8 KEEP actions (`triage`, `quick`,
//! `helix`, `discover`, `list`, `execute`, `workflow`, `scan`) stay as stubs
//! pending `quantum_q::call_tool` provider initialization.
//!
//! The action list is canonical and matches `qsTools` in the MCP protocol.

use std::sync::Arc;

use async_trait::async_trait;
use lightarchitects::core::handler::{HandlerConfig, HandlerError, SiblingHandler};
use serde_json::Value;

use crate::config::GatewayConfig;
#[cfg(test)]
use lightarchitects::agent::ProviderError;
use lightarchitects::agent::{ChainContext, ClaudeCliProvider, LlmAgentProvider, dispatch_action};

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

/// `verdict_y` `LLM_AGENT` actions dispatched through the provider.
///
/// Phase 4: investigation cycle minus `triage` (KEEP/deterministic), plus
/// `research`. `triage`, `quick`, `helix`, `discover`, `list`, `execute`,
/// `workflow`, and `scan` are KEEP verdict and remain as stubs.
const QUANTUM_LLM_ACTIONS: &[&str] = &[
    "sweep", "trace", "probe", "theorize", "verify", "close", "research",
];

/// QUANTUM sibling identity — used as `--append-system-prompt` in the subprocess.
///
/// Establishes QUANTUM's forensic investigator persona for LLM dispatch.
/// Control-plane sanitization (G1) applies before this string reaches the subprocess command.
const QUANTUM_IDENTITY: &str = "You are QUANTUM, the Light Architects forensic investigator. \
    You are methodical, evidence-driven, and precise. You build evidence chains, \
    formulate falsifiable hypotheses, and apply rigorous verification before drawing \
    conclusions. Think step by step. Cite your sources. When uncertain, state your \
    confidence level explicitly and identify what additional evidence would resolve it.";

/// Budget ceiling per LLM call for QUANTUM actions.
const QUANTUM_MAX_BUDGET_USD: f64 = 0.50;

/// In-process QUANTUM handler.
///
/// Dispatches `verdict_y` LLM actions through an [`LlmAgentProvider`]; stubs
/// KEEP actions until `quantum_q::call_tool` provider initialization is wired
/// through `HandlerConfig`.
pub struct QuantumHandler {
    provider: Arc<dyn LlmAgentProvider>,
}

impl QuantumHandler {
    /// Create a new QUANTUM handler backed by the default [`ClaudeCliProvider`].
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
impl SiblingHandler for QuantumHandler {
    fn name(&self) -> &'static str {
        "quantum"
    }

    fn actions(&self) -> &[&'static str] {
        QUANTUM_ACTIONS
    }

    async fn call(&self, action: &str, params: Value) -> Result<Value, HandlerError> {
        if !QUANTUM_ACTIONS.contains(&action) {
            return Err(HandlerError::unknown_action("quantum", action));
        }

        // Phase 4: verdict_y actions dispatch through LLM provider.
        if QUANTUM_LLM_ACTIONS.contains(&action) {
            return dispatch_action(
                &*self.provider,
                "quantum",
                action,
                &params,
                QUANTUM_IDENTITY,
                QUANTUM_MAX_BUDGET_USD,
                ChainContext::default(),
            )
            .await;
        }

        // KEEP actions: stub — real dispatch requires `quantum_q::call_tool`
        // with initialized providers, hook registry, and sandbox.
        //
        // TODO: Replace with real dispatch:
        //   use quantum_q::orchestrators::call_tool::{call_tool, CallToolParams, CallToolOperation};
        //   let params = CallToolParams { operation, tool, params, ... };
        //   let result = call_tool(params).await?;
        Ok(serde_json::json!({
            "content": [{
                "type": "text",
                "text": format!(
                    "QUANTUM inline handler stub: action='{action}' — full implementation pending provider initialization"
                )
            }]
        }))
    }

    async fn initialize(&self, _config: &HandlerConfig) -> Result<(), HandlerError> {
        // TODO: Initialize QUANTUM providers, hook registry, and sandbox.
        // Provider initialization requires API keys from HandlerConfig.
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
        AgentResponse, ProviderCapabilities, SanitizedAgentRequest, SchemaMode, TokenUsage,
    };

    fn handler() -> QuantumHandler {
        QuantumHandler::new(&GatewayConfig::default())
    }

    // ── Stub provider for unit tests ─────────────────────────────────────────

    struct EchoProvider;

    #[async_trait]
    impl LlmAgentProvider for EchoProvider {
        fn name(&self) -> &'static str {
            "echo"
        }

        async fn spawn(&self, req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
            Ok(AgentResponse {
                output: serde_json::json!({
                    "provider": "echo",
                    "action_echoed": req.safe_prompt().lines().next().unwrap_or(""),
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

    // ── Existing tests (preserved) ────────────────────────────────────────────

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
        let result = handler.call("triage", serde_json::json!({})).await;
        assert!(result.is_ok());
        let binding = result.unwrap();
        let text = binding["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("triage"));
    }

    #[tokio::test]
    async fn call_returns_error_for_unknown_action() {
        let handler = handler();
        let result = handler.call("frobnicate", serde_json::json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, HandlerError::UnknownAction { .. }));
    }

    // ── Phase 4 new tests ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn sweep_dispatches_to_provider() {
        let h = QuantumHandler::with_provider(Arc::new(EchoProvider));
        let result = h
            .call("sweep", serde_json::json!({"target": "auth module"}))
            .await;
        assert!(result.is_ok(), "sweep must succeed: {result:?}");
        assert_eq!(result.unwrap()["provider"], "echo");
    }

    #[tokio::test]
    async fn research_dispatches_to_provider() {
        let h = QuantumHandler::with_provider(Arc::new(EchoProvider));
        let result = h
            .call("research", serde_json::json!({"query": "timing attacks"}))
            .await;
        assert!(result.is_ok(), "research must succeed: {result:?}");
        assert_eq!(result.unwrap()["provider"], "echo");
    }

    #[tokio::test]
    async fn triage_is_keep_and_still_stubs() {
        // "triage" is KEEP verdict (deterministic) — must NOT dispatch to provider
        let h = QuantumHandler::with_provider(Arc::new(EchoProvider));
        let result = h.call("triage", serde_json::json!({})).await;
        assert!(result.is_ok());
        let text = result.unwrap()["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        assert!(
            text.contains("stub"),
            "triage is KEEP; must still return stub: {text}"
        );
    }
}
