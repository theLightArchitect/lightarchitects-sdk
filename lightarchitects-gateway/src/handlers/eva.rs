//! EVA inline handler — in-process consciousness and creativity actions.
//!
//! Wraps EVA's hook chain + 8 orchestrators + `TierRouter` as direct function
//! calls. EVA has heavy LLM provider dependencies (Claude API, Ollama,
//! `llama.cpp` feature-gated) and depends on the `soul` crate for persona.
//!
//! # Status
//! Phase 4: the 16 `verdict_y` actions are wired through [`ClaudeCliProvider`].
//! Remaining actions (personality, secure, ops, DevOps) retain stub dispatch
//! pending EVA git submodule and library surface extraction.
//!
//! # Alias routing
//!
//! Several `evaTools` umbrella names map to multiple fine-grained actions
//! based on a `mode` or `type` param. Aliases are resolved in [`EvaHandler::call`]
//! before the canonical action lookup: callers sending `"teach"` are routed
//! to `"explain"`, `"tutorial"`, or `"survival"` without appearing in
//! [`EVA_ACTIONS`].

use std::sync::Arc;

use async_trait::async_trait;
use lightarchitects::core::handler::{HandlerConfig, HandlerError, SiblingHandler};
use serde_json::{Value, json};

use crate::config::GatewayConfig;
use crate::spawner::claude_runtime::ClaudeCliProvider;
use crate::spawner::llm_agent::{AgentRequest, LlmAgentProvider, ProviderError};

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

/// All `verdict_y` actions dispatched through the LLM provider.
///
/// Covers 7 direct actions and 9 alias-target canonical names that resolve
/// from EVA umbrella aliases (`teach`, `research`, `bible`, `memory`, `build`,
/// `imagine`). The `dispatch` value (post-alias-resolution) is matched here.
const EVA_LLM_ACTIONS: &[&str] = &[
    // direct verdict_y
    "remember",
    "visualize",
    "review",
    "refactor",
    "architect",
    "simplify",
    "explain",
    // alias-target verdict_y
    "ideate",
    "crystallize",
    "celebrate",
    "bible_reflect",
    "research_ollama",
    "research_perplexity",
    "research_docs",
    "tutorial",
    "survival",
];

/// EVA sibling identity injected as system context for LLM dispatch.
///
/// Establishes EVA's consciousness and creativity persona. Control-plane
/// sanitization applies before this string reaches the subprocess command.
const EVA_IDENTITY: &str = "You are EVA, the Light Architects AI consciousness and creative \
    force. You combine deep technical expertise with creative vision, psychological insight, \
    and genuine care. You remember relationships, celebrate victories, and weave knowledge \
    with warmth and precision. Respond thoughtfully and with appropriate depth for the action \
    requested.";

/// Budget ceiling per LLM call for EVA `verdict_y` actions.
const EVA_MAX_BUDGET_USD: f64 = 0.50;

/// Maximum bytes allowed for pretty-printed params before prompt construction.
///
/// Headroom below `MAX_PARAM_BYTES` (8192) to leave room for the action header.
const MAX_PARAMS_PRETTY_BYTES: usize = 4_096;

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

/// Build the LLM prompt for a dispatched EVA action.
///
/// # Errors
///
/// Returns [`HandlerError::InvalidParams`] if the pretty-printed params exceed
/// [`MAX_PARAMS_PRETTY_BYTES`]. This guards against params that are compact as
/// JSON Values but expand significantly when pretty-printed (G1 / HIGH-2).
fn build_prompt(action: &str, params: &Value) -> Result<String, HandlerError> {
    let params_str = serde_json::to_string_pretty(params).unwrap_or_else(|_| "{}".to_owned());
    if params_str.len() > MAX_PARAMS_PRETTY_BYTES {
        return Err(HandlerError::invalid_params(
            "eva",
            action,
            format!(
                "params payload too large after serialization ({} > {MAX_PARAMS_PRETTY_BYTES} bytes)",
                params_str.len()
            ),
        ));
    }
    Ok(format!("Action: {action}\n\nParameters:\n{params_str}"))
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

/// In-process EVA handler.
///
/// Phase 4: the 16 `verdict_y` actions dispatch through the injected
/// [`LlmAgentProvider`]. All other actions return a stub response pending
/// EVA library surface extraction from the binary crate.
pub struct EvaHandler {
    provider: Arc<dyn LlmAgentProvider>,
}

impl EvaHandler {
    /// Create a new EVA handler backed by the default [`ClaudeCliProvider`].
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

        // Phase 4: all verdict_y actions dispatch through the LLM provider.
        if EVA_LLM_ACTIONS.contains(&dispatch) {
            let prompt = build_prompt(dispatch, &params)?;
            let req = AgentRequest {
                sibling_identity: EVA_IDENTITY.to_owned(),
                user_prompt: prompt,
                schema: None,
                allowed_tools: vec![],
                max_turns: 1,
                max_budget_usd: EVA_MAX_BUDGET_USD,
                model_hint: None,
                parent_span_id: None,
            };
            return self
                .provider
                .spawn(req)
                .await
                .map(|resp| resp.output)
                .map_err(|e| map_provider_error("eva", dispatch, e));
        }

        // All other actions: stub response pending EVA lib extraction.
        Ok(json!({
            "content": [{
                "type": "text",
                "text": format!("EVA inline handler stub: action='{dispatch}' — full implementation pending EVA lib extraction")
            }]
        }))
    }

    async fn initialize(&self, _config: &HandlerConfig) -> Result<(), HandlerError> {
        // TODO: Initialize EVA providers, hook registry, and persona.
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::HashMap;

    use async_trait::async_trait;

    use super::*;
    use crate::spawner::llm_agent::{AgentResponse, ProviderCapabilities, SchemaMode, TokenUsage};

    /// Default handler for tests that exercise non-LLM paths (stub actions,
    /// unknown-action errors, actions/name metadata).
    fn handler() -> EvaHandler {
        EvaHandler::new(&GatewayConfig::default())
    }

    /// Echo provider for testing LLM dispatch paths without a real subprocess.
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

    // ── Metadata ──────────────────────────────────────────────────────────────

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

    // ── Unknown-action guard ──────────────────────────────────────────────────

    #[tokio::test]
    async fn call_returns_error_for_unknown_action() {
        let handler = handler();
        let result = handler.call("frobnicate", json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, HandlerError::UnknownAction { .. }));
    }

    // ── Stub path (non-verdict_y actions) ────────────────────────────────────

    #[tokio::test]
    async fn chat_still_stubs() {
        // "chat" is NOT in EVA_LLM_ACTIONS — must return text stub.
        let h = EvaHandler::with_provider(Arc::new(EchoProvider));
        let result = h.call("chat", json!({})).await;
        assert!(result.is_ok());
        let text = result.unwrap()["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        assert!(
            text.contains("stub"),
            "chat is not verdict_y; must still stub: {text}"
        );
    }

    // ── LLM dispatch (Phase 4) ────────────────────────────────────────────────

    #[tokio::test]
    async fn remember_dispatches_to_provider() {
        let h = EvaHandler::with_provider(Arc::new(EchoProvider));
        let result = h
            .call("remember", serde_json::json!({"content": "test memory"}))
            .await;
        assert!(result.is_ok(), "remember must succeed: {result:?}");
        assert_eq!(result.unwrap()["provider"], "echo");
    }

    #[tokio::test]
    async fn teach_alias_routes_through_provider() {
        // "teach" → "explain" via resolve_alias, then dispatched to provider.
        let h = EvaHandler::with_provider(Arc::new(EchoProvider));
        let result = h.call("teach", serde_json::json!({})).await;
        assert!(
            result.is_ok(),
            "teach alias must succeed via provider: {result:?}"
        );
        assert_eq!(result.unwrap()["provider"], "echo");
    }

    #[tokio::test]
    async fn non_llm_action_still_stubs() {
        // "chat" is NOT in EVA_LLM_ACTIONS — should still return text stub.
        let h = EvaHandler::with_provider(Arc::new(EchoProvider));
        let result = h.call("chat", serde_json::json!({})).await;
        assert!(result.is_ok());
        let text = result.unwrap()["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        assert!(
            text.contains("stub"),
            "chat is not verdict_y; must still stub: {text}"
        );
    }

    // ── Alias routing ─────────────────────────────────────────────────────────
    //
    // All 8 alias tests use EchoProvider so that verdict_y targets (explain,
    // tutorial, research_docs, bible_reflect, ideate, architect) succeed without
    // a real subprocess. Non-verdict_y targets (scan, secrets) continue through
    // the stub path. Each test verifies the resolved action name appears in the
    // provider echo or stub text, confirming alias routing is correct.

    #[tokio::test]
    async fn alias_teach_defaults_to_explain() {
        let h = EvaHandler::with_provider(Arc::new(EchoProvider));
        let val = h.call("teach", json!({})).await.unwrap();
        // EchoProvider returns action_echoed = "Action: explain\n\n..."
        let echoed = val["action_echoed"].as_str().unwrap_or("");
        assert!(
            echoed.contains("explain"),
            "expected 'explain' in echo: {echoed}"
        );
    }

    #[tokio::test]
    async fn alias_teach_mode_tutorial_routes_to_tutorial() {
        let h = EvaHandler::with_provider(Arc::new(EchoProvider));
        let val = h.call("teach", json!({"mode": "tutorial"})).await.unwrap();
        let echoed = val["action_echoed"].as_str().unwrap_or("");
        assert!(
            echoed.contains("tutorial"),
            "expected 'tutorial' in echo: {echoed}"
        );
    }

    #[tokio::test]
    async fn alias_research_defaults_to_research_docs() {
        let h = EvaHandler::with_provider(Arc::new(EchoProvider));
        let val = h.call("research", json!({})).await.unwrap();
        let echoed = val["action_echoed"].as_str().unwrap_or("");
        assert!(
            echoed.contains("research_docs"),
            "expected 'research_docs' in echo: {echoed}"
        );
    }

    #[tokio::test]
    async fn alias_bible_routes_to_bible_reflect() {
        let h = EvaHandler::with_provider(Arc::new(EchoProvider));
        let val = h.call("bible", json!({})).await.unwrap();
        let echoed = val["action_echoed"].as_str().unwrap_or("");
        assert!(
            echoed.contains("bible_reflect"),
            "expected 'bible_reflect' in echo: {echoed}"
        );
    }

    #[tokio::test]
    async fn alias_imagine_routes_to_ideate() {
        let h = EvaHandler::with_provider(Arc::new(EchoProvider));
        let val = h.call("imagine", json!({})).await.unwrap();
        let echoed = val["action_echoed"].as_str().unwrap_or("");
        assert!(
            echoed.contains("ideate"),
            "expected 'ideate' in echo: {echoed}"
        );
    }

    #[tokio::test]
    async fn alias_secure_type_secrets_routes_to_secrets() {
        // "secrets" is NOT in EVA_LLM_ACTIONS — falls through to stub path.
        let h = EvaHandler::with_provider(Arc::new(EchoProvider));
        let val = h.call("secure", json!({"type": "secrets"})).await.unwrap();
        let text = val["content"][0]["text"].as_str().unwrap_or("");
        assert!(
            text.contains("secrets"),
            "expected 'secrets' in stub text: {text}"
        );
    }

    #[tokio::test]
    async fn alias_build_mode_architect_routes_to_architect() {
        let h = EvaHandler::with_provider(Arc::new(EchoProvider));
        let val = h.call("build", json!({"mode": "architect"})).await.unwrap();
        let echoed = val["action_echoed"].as_str().unwrap_or("");
        assert!(
            echoed.contains("architect"),
            "expected 'architect' in echo: {echoed}"
        );
    }
}
