//! CORSO inline handler — in-process Trinity pipeline dispatch.
//!
//! Placeholder implementation. The real inline handler requires `corso-server`
//! and `corso-trinity-core` crates, which are not yet published to crates.io.
//! Until those crates are available, this handler stubs the interface so that
//! `--all-features` compiles cleanly.
//!
//! # Phase 3 pilot
//!
//! `sniff` and `scout` are wired to [`ClaudeCliProvider`] as the first two
//! LLM_AGENT verdict_y actions. All other LLM_AGENT actions remain stubbed
//! until Phase 4.
//!
//! # Heavy dependencies
//!
//! CORSO pulls in `PyO3` (Python 3.14 embedding), SOUL, soul-engine,
//! neural-engine, voice-engine, tree-sitter (5 grammars), and prometheus.
//! These add significant compile time and binary size, which is why this
//! handler is gated behind the `inline-corso` feature flag.

use std::sync::Arc;

use async_trait::async_trait;
use lightarchitects::core::handler::{HandlerConfig, HandlerError, SiblingHandler};
use serde_json::Value;

use crate::config::GatewayConfig;
use crate::spawner::claude_runtime::ClaudeCliProvider;
use crate::spawner::llm_agent::{AgentRequest, LlmAgentProvider, ProviderError};

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

/// Phase 3 pilot: two actions wired to LLM provider.
/// Remaining verdict_y LLM_AGENT actions (code_review, guard, fetch, prove,
/// optimize, chase) are migrated in Phase 4.
const PILOT_LLM_ACTIONS: &[&str] = &["sniff", "scout"];

/// CORSO sibling identity — used as `--append-system-prompt` in the subprocess.
///
/// Establishes CORSO's analytical persona for LLM dispatch. Control-plane
/// sanitization (G1) applies before this string reaches the subprocess command.
const CORSO_IDENTITY: &str = "You are CORSO, the Light Architects security and \
    build engineer. You are methodical, precise, and security-conscious. \
    Analyse the provided code, architecture, or input carefully and respond \
    with structured, actionable findings. Use markdown headers and bullet lists.";

/// Budget ceiling per LLM call for pilot actions.
const PILOT_MAX_BUDGET_USD: f64 = 0.50;

/// In-process CORSO handler (stub — real impl requires unpublished deps).
pub struct CorsoHandler {
    provider: Arc<dyn LlmAgentProvider>,
}

impl CorsoHandler {
    /// Create a new CORSO handler backed by the default [`ClaudeCliProvider`].
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

        // Phase 3 pilot: route sniff + scout through LLM provider.
        if PILOT_LLM_ACTIONS.contains(&action) {
            let prompt = build_prompt(action, &params);
            let req = AgentRequest {
                sibling_identity: CORSO_IDENTITY.to_owned(),
                user_prompt: prompt,
                schema: None,
                allowed_tools: vec![],
                max_turns: 1,
                max_budget_usd: PILOT_MAX_BUDGET_USD,
                model_hint: None,
                parent_span_id: None,
            };
            return self
                .provider
                .spawn(req)
                .await
                .map(|resp| resp.output)
                .map_err(|e| map_provider_error("corso", action, e));
        }

        // All other actions: KEEP (subprocess path — not yet available) or future Phase 4.
        Err(HandlerError::not_initialized(
            "corso",
            "inline-corso handler not yet available — corso-server/trinity-core not published",
        ))
    }

    async fn initialize(&self, _config: &HandlerConfig) -> Result<(), HandlerError> {
        Ok(())
    }
}

/// Build the LLM prompt for a dispatched action.
fn build_prompt(action: &str, params: &Value) -> String {
    let params_str = serde_json::to_string_pretty(params).unwrap_or_else(|_| "{}".to_owned());
    format!("Action: {action}\n\nParameters:\n{params_str}")
}

/// Map a [`ProviderError`] to the appropriate [`HandlerError`] variant.
fn map_provider_error(sibling: &str, action: &str, e: ProviderError) -> HandlerError {
    match e {
        ProviderError::ParamSanitizationFailed { param_name, reason } => {
            HandlerError::invalid_params(sibling, action, format!("{param_name}: {reason}"))
        }
        ProviderError::Internal(msg) => HandlerError::internal(sibling, action, msg),
        other => HandlerError::service_error(sibling, action, other.to_string()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::HashMap;

    use async_trait::async_trait;

    use super::*;
    use crate::spawner::llm_agent::{AgentResponse, ProviderCapabilities, SchemaMode, TokenUsage};

    fn handler() -> CorsoHandler {
        CorsoHandler::new(&GatewayConfig::default())
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

    // ── Tests ─────────────────────────────────────────────────────────────────

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
    async fn call_returns_not_initialized_for_keep_action() {
        // KEEP actions (read_file etc.) still return not_initialized until
        // the corso-server crate is published.
        let handler = handler();
        let result = handler.call("read_file", serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            HandlerError::NotInitialized { .. }
        ));
    }

    #[tokio::test]
    async fn sniff_dispatches_to_provider() {
        let h = CorsoHandler::with_provider(Arc::new(EchoProvider));
        let result = h
            .call("sniff", serde_json::json!({"target": "src/main.rs"}))
            .await;
        assert!(
            result.is_ok(),
            "sniff must succeed with echo provider: {result:?}"
        );
        let val = result.unwrap();
        assert_eq!(
            val["provider"], "echo",
            "output must come from EchoProvider"
        );
    }

    #[tokio::test]
    async fn scout_dispatches_to_provider() {
        let h = CorsoHandler::with_provider(Arc::new(EchoProvider));
        let result = h
            .call("scout", serde_json::json!({"query": "find auth functions"}))
            .await;
        assert!(
            result.is_ok(),
            "scout must succeed with echo provider: {result:?}"
        );
        let val = result.unwrap();
        assert_eq!(val["provider"], "echo");
    }

    #[tokio::test]
    async fn non_pilot_llm_action_returns_not_initialized() {
        // code_review is verdict_y but NOT in the Phase 3 pilot — still stubbed.
        let h = CorsoHandler::with_provider(Arc::new(EchoProvider));
        let result = h.call("code_review", serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            HandlerError::NotInitialized { .. }
        ));
    }

    #[tokio::test]
    async fn provider_error_maps_to_handler_error() {
        struct FailProvider;

        #[async_trait]
        impl LlmAgentProvider for FailProvider {
            fn name(&self) -> &'static str {
                "fail"
            }

            async fn spawn(&self, _req: AgentRequest) -> Result<AgentResponse, ProviderError> {
                Err(ProviderError::Internal("simulated failure".to_owned()))
            }

            fn capabilities(&self) -> ProviderCapabilities {
                ProviderCapabilities {
                    schema_enforcement: SchemaMode::None,
                    native_budget_cap: false,
                    native_turn_cap: false,
                    auth_inherits_session: false,
                }
            }

            fn estimate_cost(&self, _i: u32, _o: u32) -> f64 {
                0.0
            }
        }

        let h = CorsoHandler::with_provider(Arc::new(FailProvider));
        let result = h.call("sniff", serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), HandlerError::Internal { .. }));
    }
}
