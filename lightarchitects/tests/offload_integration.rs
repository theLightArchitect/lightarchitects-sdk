//! Integration tests for the `agent::offload` module.
//!
//! Wires `OffloadAwareProvider` with real `OffloadCatalog` (loaded from a
//! temp YAML) and mock dispatcher/escalator across module boundaries —
//! no external services required.
//!
//! Canon XXVII Suite 2 (integration) for the offload surface.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use lightarchitects::agent::offload::hitl_bridge::NullEscalator;
use lightarchitects::agent::offload::laex_supervisor::{LaexSupervisor, OffloadDispatcher};
use lightarchitects::agent::offload::provider::OffloadAwareProvider;
use lightarchitects::agent::offload::{OffloadCatalog, Pattern};
use lightarchitects::agent::{
    AgentRequest, AgentResponse, LlmAgentProvider, ProviderCapabilities, ProviderError,
    SanitizedAgentRequest, SchemaMode, TokenUsage,
};
use serde_json::Value;

// ── Test fixtures ────────────────────────────────────────────────────────────

const MINIMAL_CATALOG_YAML: &str = r#"
version: "1.1"
default_model: "glm-test"
patterns:
  - id: P_TEST
    name: "Integration test pattern"
    template: "Explain this: {{code}}"
    eligible:
      siblings: ["corso"]
      tool_use_required: false
      max_input_tokens: 4000
    shape:
      kind: "sentence_no_fences"
      max_words: 50
    refinement:
      anchor: "respond with one sentence"
    verifier:
      enabled: false
    calibration:
      last_dry_run: null
      sample_count: 0
      success_rate: 0.0
"#;

struct EchoDispatcher {
    reply: String,
}

#[async_trait]
impl OffloadDispatcher for EchoDispatcher {
    async fn dispatch(&self, _pattern: &Pattern, _prompt: &str) -> Result<String, String> {
        Ok(self.reply.clone())
    }
}

struct FallbackSentinel;

#[async_trait]
impl LlmAgentProvider for FallbackSentinel {
    fn name(&self) -> &'static str {
        "fallback-sentinel"
    }
    async fn spawn(&self, _req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
        Ok(AgentResponse {
            output: Value::String("fallback".to_owned()),
            turns_used: 1,
            cost_usd: 0.0,
            tokens: TokenUsage {
                input: 0,
                output: 0,
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

fn make_request(model_hint: Option<&str>, sibling: &str, prompt: &str) -> AgentRequest {
    AgentRequest {
        sibling_identity: sibling.to_owned(),
        user_prompt: prompt.to_owned(),
        model_hint: model_hint.map(str::to_owned),
        chain_origin: None,
        schema: None,
        allowed_tools: vec![],
        max_turns: 1,
        max_budget_usd: 1.0,
        parent_span_id: None,
        chain_depth: 0,
        aud: None,
        conversation_history: vec![],
        tool_definitions: vec![],
    }
}

// ── Integration tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn offload_pipeline_routes_matching_request() {
    let catalog = Arc::new(OffloadCatalog::from_yaml_str(MINIMAL_CATALOG_YAML).unwrap());
    let dispatcher = Arc::new(EchoDispatcher {
        reply: "This code clamps a value.".to_owned(),
    });
    let supervisor = Arc::new(LaexSupervisor::new(catalog.clone(), dispatcher.clone()));
    let provider = OffloadAwareProvider::new(
        Arc::new(FallbackSentinel),
        catalog,
        HashMap::new(),
        dispatcher,
        supervisor,
        Arc::new(NullEscalator),
    );

    let req = make_request(Some("P_TEST"), "corso", "Explain n.clamp(lo, hi)")
        .sanitize()
        .unwrap();
    let resp = provider.spawn(req).await.unwrap();

    // Should be offloaded (not the "fallback" sentinel)
    assert_ne!(resp.output.as_str().unwrap_or(""), "fallback");
    assert!(resp.provider_attrs.contains_key("offload.pattern_id"));
    assert_eq!(
        resp.provider_attrs["offload.pattern_id"].as_str().unwrap(),
        "P_TEST"
    );
}

#[tokio::test]
async fn offload_pipeline_falls_through_on_tool_use() {
    let catalog = Arc::new(OffloadCatalog::from_yaml_str(MINIMAL_CATALOG_YAML).unwrap());
    let dispatcher = Arc::new(EchoDispatcher {
        reply: "This code clamps a value.".to_owned(),
    });
    let supervisor = Arc::new(LaexSupervisor::new(catalog.clone(), dispatcher.clone()));
    let provider = OffloadAwareProvider::new(
        Arc::new(FallbackSentinel),
        catalog,
        HashMap::new(),
        dispatcher,
        supervisor,
        Arc::new(NullEscalator),
    );

    let mut req = make_request(Some("P_TEST"), "corso", "Explain n.clamp(lo, hi)");
    // Tool-using request → must fall through
    req.tool_definitions
        .push(lightarchitects::agent::ToolDefinition {
            name: "bash".to_owned(),
            description: "Run bash".to_owned(),
            input_schema: serde_json::json!({}),
        });
    let req = req.sanitize().unwrap();
    let resp = provider.spawn(req).await.unwrap();

    assert_eq!(resp.output.as_str().unwrap_or(""), "fallback");
}

#[tokio::test]
async fn offload_pipeline_falls_through_on_unknown_sibling() {
    let catalog = Arc::new(OffloadCatalog::from_yaml_str(MINIMAL_CATALOG_YAML).unwrap());
    let dispatcher = Arc::new(EchoDispatcher {
        reply: "This code clamps a value.".to_owned(),
    });
    let supervisor = Arc::new(LaexSupervisor::new(catalog.clone(), dispatcher.clone()));
    let provider = OffloadAwareProvider::new(
        Arc::new(FallbackSentinel),
        catalog,
        HashMap::new(),
        dispatcher,
        supervisor,
        Arc::new(NullEscalator),
    );

    // "unknown-sibling" is not in P_TEST eligible.siblings
    let req = make_request(Some("P_TEST"), "unknown-sibling", "Explain n.clamp(lo, hi)")
        .sanitize()
        .unwrap();
    let resp = provider.spawn(req).await.unwrap();

    assert_eq!(resp.output.as_str().unwrap_or(""), "fallback");
}

#[tokio::test]
async fn offload_pipeline_falls_through_on_shape_violation() {
    // Use a catalog with strict max_words: 3 so a longer reply violates shape.
    const STRICT_CATALOG: &str = r#"
version: "1.1"
default_model: "glm-test"
patterns:
  - id: P_TEST
    name: "Strict word-count test pattern"
    template: "Explain: {{code}}"
    eligible:
      siblings: ["corso"]
      tool_use_required: false
      max_input_tokens: 4000
    shape:
      kind: "sentence_no_fences"
      max_words: 3
    refinement:
      anchor: "respond with one sentence"
    verifier:
      enabled: false
    calibration:
      last_dry_run: null
      sample_count: 0
      success_rate: 0.0
"#;
    let catalog = Arc::new(OffloadCatalog::from_yaml_str(STRICT_CATALOG).unwrap());
    // Reply exceeds max_words: 3 → shape violation → retry → still fails → fallthrough
    let dispatcher = Arc::new(EchoDispatcher {
        reply: "This function clamps a numeric value to a range.".to_owned(),
    });
    let supervisor = Arc::new(LaexSupervisor::new(catalog.clone(), dispatcher.clone()));
    let provider = OffloadAwareProvider::new(
        Arc::new(FallbackSentinel),
        catalog,
        HashMap::new(),
        dispatcher,
        supervisor,
        Arc::new(NullEscalator),
    );

    let req = make_request(Some("P_TEST"), "corso", "Explain n.clamp(lo, hi)")
        .sanitize()
        .unwrap();
    let resp = provider.spawn(req).await.unwrap();

    // Shape violation → falls through → sentinel returns "fallback"
    assert_eq!(resp.output.as_str().unwrap_or(""), "fallback");
}

#[tokio::test]
async fn offload_pipeline_respects_chain_depth_limit() {
    let catalog = Arc::new(OffloadCatalog::from_yaml_str(MINIMAL_CATALOG_YAML).unwrap());
    let dispatcher = Arc::new(EchoDispatcher {
        reply: "This code clamps a value.".to_owned(),
    });
    let supervisor = Arc::new(LaexSupervisor::new(catalog.clone(), dispatcher.clone()));
    let provider = OffloadAwareProvider::new(
        Arc::new(FallbackSentinel),
        catalog,
        HashMap::new(),
        dispatcher,
        supervisor,
        Arc::new(NullEscalator),
    );

    let mut req = make_request(Some("P_TEST"), "corso", "Explain n.clamp(lo, hi)");
    req.chain_depth = 7; // at MAX_CHAIN_DEPTH — should fall through (S7 gate)
    let req = req.sanitize().unwrap();
    let resp = provider.spawn(req).await.unwrap();

    assert_eq!(resp.output.as_str().unwrap_or(""), "fallback");
}

// ── Property tests (Suite 3) ──────────────────────────────────────────────────

use proptest::prelude::*;

proptest! {
    #[test]
    fn valid_baseline_path_rejects_any_traversal(
        prefix in "[a-z]{1,10}",
        suffix in "[a-z]{1,10}",
    ) {
        // Any path containing ".." must be rejected — exhaustive over prefix/suffix combos
        let traversal = format!("{prefix}/../../{suffix}");
        let catalog_yaml = format!(
            r#"
version: "1.1"
default_model: "m"
patterns:
  - id: P1
    name: "n"
    template: "t"
    eligible:
      siblings: ["a"]
      tool_use_required: false
      max_input_tokens: 4000
    context_sources:
      default:
        - kind: "industry-baseline"
          category: "security"
          path: {traversal:?}
          token_budget: 100
    shape:
      kind: "sentence_no_fences"
      max_words: 50
    verifier:
      enabled: false
    calibration:
      last_dry_run: null
      sample_count: 0
      success_rate: 0.0
"#
        );
        // Catalog validation must reject path with ".."
        let result = OffloadCatalog::from_yaml_str(&catalog_yaml);
        prop_assert!(
            result.is_err(),
            "catalog with traversal path {traversal:?} should be rejected"
        );
    }
}
