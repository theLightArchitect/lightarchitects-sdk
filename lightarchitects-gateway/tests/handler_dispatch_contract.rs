//! Handler dispatch contract tests — controlled input/output verification.
//!
//! Every test here uses `CapturingProvider` (a spy) or `FailingProvider` instead
//! of the real `ClaudeCliProvider`. This means:
//!
//! - No real `claude` subprocess is spawned.
//! - We control the output and inspect exactly what the handler sent to the provider.
//! - Tests are deterministic, fast, and don't require a Claude subscription.
//!
//! # What these tests prove
//!
//! - **Identity contract**: each sibling injects a persona-appropriate system prompt.
//! - **Prompt format contract**: `dispatch_action` builds `"Action: {a}\n\nParameters:\n{json}"`.
//! - **Budget/turns contract**: handlers pass the declared constants to `AgentRequest`.
//! - **Output passthrough**: the provider's `output` Value is returned verbatim.
//! - **LLM routing**: every `verdict_y` action calls the provider exactly once; KEEP actions call zero times.
//! - **Guard order**: params >4096 bytes (pretty-printed) are rejected *before* `provider.spawn()`.
//! - **Error mapping**: `ProviderError::Internal` → `HandlerError::Internal`.
//! - **EVA alias resolution**: umbrella aliases map to canonical LLM actions before dispatch.

#![cfg(all(
    feature = "inline-corso",
    feature = "inline-eva",
    feature = "inline-soul",
    feature = "inline-quantum"
))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{Value, json};

use lightarchitects::agent::{
    AgentRequest, AgentResponse, LlmAgentProvider, ProviderCapabilities, ProviderError, SchemaMode,
    TokenUsage,
};
use lightarchitects::core::handler::{HandlerError, SiblingHandler};
use lightarchitects_gateway::handlers::{CorsoHandler, EvaHandler, QuantumHandler, SoulHandler};

// ── Test providers ────────────────────────────────────────────────────────────

/// Spy provider — records every request and returns a caller-configured output.
struct CapturingProvider {
    output: Value,
    requests: Arc<Mutex<Vec<AgentRequest>>>,
}

impl CapturingProvider {
    fn new(output: Value) -> (Self, Arc<Mutex<Vec<AgentRequest>>>) {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider = Self {
            output,
            requests: Arc::clone(&requests),
        };
        (provider, requests)
    }
}

#[async_trait]
impl LlmAgentProvider for CapturingProvider {
    fn name(&self) -> &'static str {
        "capturing"
    }

    async fn spawn(&self, req: AgentRequest) -> Result<AgentResponse, ProviderError> {
        self.requests.lock().unwrap().push(req);
        Ok(AgentResponse {
            output: self.output.clone(),
            turns_used: 1,
            cost_usd: 0.001,
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

/// Failure provider — always returns `ProviderError::Internal`.
struct FailingProvider;

#[async_trait]
impl LlmAgentProvider for FailingProvider {
    fn name(&self) -> &'static str {
        "failing"
    }

    async fn spawn(&self, _req: AgentRequest) -> Result<AgentResponse, ProviderError> {
        Err(ProviderError::Internal("injected failure".to_owned()))
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

// ── Identity contract ─────────────────────────────────────────────────────────

#[tokio::test]
async fn quantum_identity_contains_persona_keywords() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = QuantumHandler::with_provider(Arc::new(provider));
    h.call("sweep", json!({})).await.unwrap();

    let req = requests.lock().unwrap();
    let identity = &req[0].sibling_identity;
    assert!(
        identity.contains("QUANTUM"),
        "identity must name the sibling"
    );
    assert!(
        identity.contains("forensic"),
        "identity must state the role"
    );
    assert!(
        identity.contains("evidence"),
        "identity must state the method"
    );
}

#[tokio::test]
async fn soul_identity_contains_persona_keywords() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = SoulHandler::with_provider(Arc::new(provider));
    h.call("converse", json!({})).await.unwrap();

    let req = requests.lock().unwrap();
    let identity = &req[0].sibling_identity;
    assert!(identity.contains("SOUL"), "identity must name the sibling");
    assert!(
        identity.contains("knowledge"),
        "identity must state the role"
    );
}

#[tokio::test]
async fn eva_identity_contains_persona_keywords() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = EvaHandler::with_provider(Arc::new(provider));
    h.call("remember", json!({"content": "test"}))
        .await
        .unwrap();

    let req = requests.lock().unwrap();
    let identity = &req[0].sibling_identity;
    assert!(identity.contains("EVA"), "identity must name the sibling");
    assert!(
        identity.contains("consciousness"),
        "identity must state the role"
    );
}

#[tokio::test]
async fn corso_identity_contains_persona_keywords() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = CorsoHandler::with_provider(Arc::new(provider));
    h.call("sniff", json!({})).await.unwrap();

    let req = requests.lock().unwrap();
    let identity = &req[0].sibling_identity;
    assert!(identity.contains("CORSO"), "identity must name the sibling");
    assert!(
        identity.contains("security"),
        "identity must state the role"
    );
}

// ── Prompt format contract ────────────────────────────────────────────────────

#[tokio::test]
async fn prompt_starts_with_action_header() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = QuantumHandler::with_provider(Arc::new(provider));
    h.call("sweep", json!({"target": "src/"})).await.unwrap();

    let req = requests.lock().unwrap();
    let prompt = &req[0].user_prompt;
    assert!(
        prompt.starts_with("Action: sweep\n\nParameters:\n"),
        "prompt must open with action header; got: {prompt}"
    );
}

#[tokio::test]
async fn prompt_embeds_params_as_pretty_json() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = QuantumHandler::with_provider(Arc::new(provider));
    h.call("sweep", json!({"key": "value"})).await.unwrap();

    let req = requests.lock().unwrap();
    let prompt = &req[0].user_prompt;
    // Pretty-printed JSON has newlines; the key must appear verbatim
    assert!(
        prompt.contains("\"key\""),
        "params key must appear in prompt"
    );
    assert!(
        prompt.contains("\"value\""),
        "params value must appear in prompt"
    );
}

// ── Budget and turns contract ─────────────────────────────────────────────────

#[tokio::test]
async fn quantum_budget_is_fifty_cents() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = QuantumHandler::with_provider(Arc::new(provider));
    h.call("sweep", json!({})).await.unwrap();

    let req = requests.lock().unwrap();
    assert!(
        (req[0].max_budget_usd - 0.50).abs() < f64::EPSILON,
        "QUANTUM budget must be $0.50; got {}",
        req[0].max_budget_usd
    );
}

#[tokio::test]
async fn soul_budget_is_fifty_cents() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = SoulHandler::with_provider(Arc::new(provider));
    h.call("converse", json!({})).await.unwrap();

    let req = requests.lock().unwrap();
    assert!(
        (req[0].max_budget_usd - 0.50).abs() < f64::EPSILON,
        "SOUL budget must be $0.50; got {}",
        req[0].max_budget_usd
    );
}

#[tokio::test]
async fn corso_budget_is_fifty_cents() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = CorsoHandler::with_provider(Arc::new(provider));
    h.call("sniff", json!({})).await.unwrap();

    let req = requests.lock().unwrap();
    assert!(
        (req[0].max_budget_usd - 0.50).abs() < f64::EPSILON,
        "CORSO budget must be $0.50; got {}",
        req[0].max_budget_usd
    );
}

#[tokio::test]
async fn dispatch_max_turns_is_five() {
    // Every handler uses dispatch_action which sets max_turns=5.
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = QuantumHandler::with_provider(Arc::new(provider));
    h.call("sweep", json!({})).await.unwrap();

    let req = requests.lock().unwrap();
    assert_eq!(req[0].max_turns, 5, "dispatch_action must use max_turns=5");
}

// ── Output passthrough ────────────────────────────────────────────────────────

#[tokio::test]
async fn quantum_returns_provider_output_verbatim() {
    let sentinel = json!({"quantum_sentinel": true, "data": [1, 2, 3]});
    let (provider, _) = CapturingProvider::new(sentinel.clone());
    let h = QuantumHandler::with_provider(Arc::new(provider));

    let result = h.call("sweep", json!({})).await.unwrap();
    assert_eq!(
        result, sentinel,
        "handler must return provider output verbatim"
    );
}

#[tokio::test]
async fn soul_returns_provider_output_verbatim() {
    let sentinel = json!({"soul_sentinel": "hello from SOUL"});
    let (provider, _) = CapturingProvider::new(sentinel.clone());
    let h = SoulHandler::with_provider(Arc::new(provider));

    let result = h.call("converse", json!({})).await.unwrap();
    assert_eq!(result, sentinel);
}

#[tokio::test]
async fn eva_returns_provider_output_verbatim() {
    let sentinel = json!({"eva_sentinel": 42});
    let (provider, _) = CapturingProvider::new(sentinel.clone());
    let h = EvaHandler::with_provider(Arc::new(provider));

    let result = h.call("remember", json!({"content": "x"})).await.unwrap();
    assert_eq!(result, sentinel);
}

#[tokio::test]
async fn corso_returns_provider_output_verbatim() {
    let sentinel = json!({"corso_sentinel": {"findings": []}});
    let (provider, _) = CapturingProvider::new(sentinel.clone());
    let h = CorsoHandler::with_provider(Arc::new(provider));

    let result = h.call("sniff", json!({})).await.unwrap();
    assert_eq!(result, sentinel);
}

// ── LLM routing: verdict_y actions call provider exactly once ─────────────────

#[tokio::test]
async fn quantum_all_llm_actions_call_provider_once() {
    const LLM: &[&str] = &[
        "sweep", "trace", "probe", "theorize", "verify", "close", "research",
    ];
    for action in LLM {
        let (provider, requests) = CapturingProvider::new(json!({}));
        let h = QuantumHandler::with_provider(Arc::new(provider));
        h.call(action, json!({})).await.unwrap_or_default();
        assert_eq!(
            requests.lock().unwrap().len(),
            1,
            "quantum/{action} must call provider exactly once"
        );
    }
}

#[tokio::test]
async fn quantum_keep_actions_call_provider_zero_times() {
    // KEEP actions: deterministic stubs that never touch the provider.
    const KEEP: &[&str] = &[
        "triage", "quick", "helix", "discover", "list", "execute", "workflow", "scan",
    ];
    for action in KEEP {
        let (provider, requests) = CapturingProvider::new(json!({}));
        let h = QuantumHandler::with_provider(Arc::new(provider));
        let _ = h.call(action, json!({})).await; // may return stub or error — we don't assert Ok
        assert_eq!(
            requests.lock().unwrap().len(),
            0,
            "quantum/{action} is KEEP and must NOT call provider"
        );
    }
}

#[tokio::test]
async fn soul_llm_actions_call_provider_once() {
    // Only "converse" and "chat" are verdict_y in SOUL.
    for action in &["converse", "chat"] {
        let (provider, requests) = CapturingProvider::new(json!({}));
        let h = SoulHandler::with_provider(Arc::new(provider));
        h.call(action, json!({})).await.unwrap_or_default();
        assert_eq!(
            requests.lock().unwrap().len(),
            1,
            "soul/{action} must call provider exactly once"
        );
    }
}

#[tokio::test]
async fn soul_keep_action_read_note_calls_provider_zero_times() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = SoulHandler::with_provider(Arc::new(provider));
    let _ = h.call("read_note", json!({})).await;
    assert_eq!(
        requests.lock().unwrap().len(),
        0,
        "soul/read_note is KEEP and must NOT call provider"
    );
}

#[tokio::test]
async fn corso_llm_actions_call_provider_once() {
    const LLM: &[&str] = &[
        "sniff",
        "scout",
        "code_review",
        "guard",
        "fetch",
        "prove",
        "optimize",
        "chase",
        "chow",
    ];
    for action in LLM {
        let (provider, requests) = CapturingProvider::new(json!({}));
        let h = CorsoHandler::with_provider(Arc::new(provider));
        h.call(action, json!({})).await.unwrap_or_default();
        assert_eq!(
            requests.lock().unwrap().len(),
            1,
            "corso/{action} must call provider exactly once"
        );
    }
}

#[tokio::test]
async fn corso_keep_action_read_file_calls_provider_zero_times() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = CorsoHandler::with_provider(Arc::new(provider));
    let _ = h.call("read_file", json!({})).await;
    assert_eq!(
        requests.lock().unwrap().len(),
        0,
        "corso/read_file is KEEP and must NOT call provider"
    );
}

// ── Guard order: oversized params rejected before provider.spawn() ────────────

#[tokio::test]
async fn oversized_params_rejected_before_provider_spawn() {
    // Pretty-printed >4096 bytes triggers the guard in dispatch_action.
    let big = "x".repeat(5_000);
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = QuantumHandler::with_provider(Arc::new(provider));

    let result = h.call("sweep", json!({"data": big})).await;
    assert!(
        matches!(result, Err(HandlerError::InvalidParams { .. })),
        "oversized params must yield InvalidParams before provider is called"
    );
    assert_eq!(
        requests.lock().unwrap().len(),
        0,
        "provider.spawn must NOT be called when params are oversized"
    );
}

#[tokio::test]
async fn oversized_params_rejected_in_corso() {
    let big = "x".repeat(5_000);
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = CorsoHandler::with_provider(Arc::new(provider));

    let result = h.call("sniff", json!({"data": big})).await;
    assert!(matches!(result, Err(HandlerError::InvalidParams { .. })));
    assert_eq!(requests.lock().unwrap().len(), 0);
}

// ── Error mapping ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn provider_internal_error_maps_to_handler_internal() {
    let h = QuantumHandler::with_provider(Arc::new(FailingProvider));
    let result = h.call("sweep", json!({})).await;
    assert!(
        matches!(result, Err(HandlerError::Internal { .. })),
        "ProviderError::Internal must map to HandlerError::Internal; got {result:?}"
    );
}

#[tokio::test]
async fn corso_provider_internal_error_maps_to_handler_internal() {
    let h = CorsoHandler::with_provider(Arc::new(FailingProvider));
    let result = h.call("sniff", json!({})).await;
    assert!(matches!(result, Err(HandlerError::Internal { .. })));
}

// ── EVA alias resolution ──────────────────────────────────────────────────────

#[tokio::test]
async fn eva_teach_default_dispatches_explain() {
    // "teach" with no mode → "explain" action in the prompt.
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = EvaHandler::with_provider(Arc::new(provider));
    h.call("teach", json!({})).await.unwrap();

    let req = requests.lock().unwrap();
    let prompt = &req[0].user_prompt;
    assert!(
        prompt.contains("Action: explain"),
        "teach with no mode must dispatch as 'explain'; prompt: {prompt}"
    );
}

#[tokio::test]
async fn eva_teach_mode_tutorial_dispatches_tutorial() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = EvaHandler::with_provider(Arc::new(provider));
    h.call("teach", json!({"mode": "tutorial"})).await.unwrap();

    let req = requests.lock().unwrap();
    let prompt = &req[0].user_prompt;
    assert!(
        prompt.contains("Action: tutorial"),
        "teach+mode:tutorial must dispatch as 'tutorial'"
    );
}

#[tokio::test]
async fn eva_research_default_dispatches_research_docs() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = EvaHandler::with_provider(Arc::new(provider));
    h.call("research", json!({})).await.unwrap();

    let req = requests.lock().unwrap();
    let prompt = &req[0].user_prompt;
    assert!(
        prompt.contains("Action: research_docs"),
        "research with no provider must dispatch as 'research_docs'"
    );
}

#[tokio::test]
async fn eva_research_provider_ollama_dispatches_research_ollama() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = EvaHandler::with_provider(Arc::new(provider));
    h.call("research", json!({"provider": "ollama"}))
        .await
        .unwrap();

    let req = requests.lock().unwrap();
    let prompt = &req[0].user_prompt;
    assert!(
        prompt.contains("Action: research_ollama"),
        "research+provider:ollama must dispatch as 'research_ollama'"
    );
}

#[tokio::test]
async fn eva_imagine_dispatches_ideate() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = EvaHandler::with_provider(Arc::new(provider));
    h.call("imagine", json!({})).await.unwrap();

    let req = requests.lock().unwrap();
    let prompt = &req[0].user_prompt;
    assert!(
        prompt.contains("Action: ideate"),
        "imagine must dispatch as 'ideate'"
    );
}

#[tokio::test]
async fn eva_memory_crystallize_dispatches_crystallize() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = EvaHandler::with_provider(Arc::new(provider));
    h.call("memory", json!({"type": "crystallize"}))
        .await
        .unwrap();

    let req = requests.lock().unwrap();
    let prompt = &req[0].user_prompt;
    assert!(
        prompt.contains("Action: crystallize"),
        "memory+type:crystallize must dispatch as 'crystallize'"
    );
}

#[tokio::test]
async fn eva_build_mode_architect_dispatches_architect() {
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = EvaHandler::with_provider(Arc::new(provider));
    h.call("build", json!({"mode": "architect"})).await.unwrap();

    let req = requests.lock().unwrap();
    let prompt = &req[0].user_prompt;
    assert!(
        prompt.contains("Action: architect"),
        "build+mode:architect must dispatch as 'architect'"
    );
}

#[tokio::test]
async fn eva_alias_still_passes_original_params_through() {
    // Alias resolution changes the action in the prompt, but params are preserved verbatim.
    let (provider, requests) = CapturingProvider::new(json!({}));
    let h = EvaHandler::with_provider(Arc::new(provider));
    h.call("teach", json!({"topic": "ownership", "depth": 3}))
        .await
        .unwrap();

    let req = requests.lock().unwrap();
    let prompt = &req[0].user_prompt;
    assert!(
        prompt.contains("\"topic\""),
        "alias dispatch must preserve original params"
    );
    assert!(
        prompt.contains("\"ownership\""),
        "alias dispatch must preserve param values"
    );
}
