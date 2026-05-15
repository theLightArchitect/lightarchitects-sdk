//! G16 contract test — `MockProvider` dispatched through `Arc<dyn LlmAgentProvider>`.
//!
//! Validates that the trait object dispatch, budget enforcement, capabilities
//! declaration, and cost estimation all work correctly without requiring the
//! real Claude CLI binary.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use lightarchitects::agent::{
    AgentRequest, AgentResponse, LlmAgentProvider, ProviderCapabilities, ProviderError, SchemaMode,
    TokenUsage,
};

// ── MockProvider ───────────────────────────────────────────────────────────────

struct MockProvider {
    name: &'static str,
}

#[async_trait]
impl LlmAgentProvider for MockProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn spawn(&self, req: AgentRequest) -> Result<AgentResponse, ProviderError> {
        if req.max_budget_usd < 0.001 {
            return Err(ProviderError::BudgetExceeded {
                cap_usd: req.max_budget_usd,
                actual_usd: 0.001,
            });
        }
        Ok(AgentResponse {
            output: serde_json::json!({"mock": true, "prompt": req.user_prompt}),
            turns_used: 1,
            cost_usd: self.estimate_cost(100, 50),
            tokens: TokenUsage {
                input: 100,
                output: 50,
            },
            provider_attrs: HashMap::new(),
            retry_count: 0,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            schema_enforcement: SchemaMode::BestEffort,
            native_budget_cap: true,
            native_turn_cap: true,
            auth_inherits_session: true,
        }
    }

    fn estimate_cost(&self, input: u32, max_output: u32) -> f64 {
        (f64::from(input) / 1_000_000.0 * 3.0) + (f64::from(max_output) / 1_000_000.0 * 15.0)
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn mock_request(budget: f64) -> AgentRequest {
    AgentRequest {
        sibling_identity: "test system".into(),
        user_prompt: "hello".into(),
        schema: None,
        allowed_tools: vec![],
        max_turns: 3,
        max_budget_usd: budget,
        model_hint: Some("claude-sonnet-4-6".into()),
        parent_span_id: None,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn trait_dispatch_through_arc() {
    let provider: Arc<dyn LlmAgentProvider> = Arc::new(MockProvider { name: "mock" });
    assert_eq!(provider.name(), "mock");

    let resp = provider
        .spawn(mock_request(0.10))
        .await
        .expect("mock should succeed");

    assert_eq!(resp.turns_used, 1);
    assert!(resp.cost_usd > 0.0, "cost should be positive");
    assert_eq!(resp.output["mock"], true);
}

#[tokio::test]
async fn budget_cap_enforced() {
    let provider: Arc<dyn LlmAgentProvider> = Arc::new(MockProvider { name: "mock" });
    let result = provider.spawn(mock_request(0.0001)).await;
    assert!(
        matches!(result, Err(ProviderError::BudgetExceeded { .. })),
        "expected BudgetExceeded, got {result:?}"
    );
}

#[test]
fn capabilities_match_declared_mode() {
    let provider = MockProvider { name: "mock" };
    let caps = provider.capabilities();
    assert_eq!(caps.schema_enforcement, SchemaMode::BestEffort);
    assert!(caps.native_budget_cap);
    assert!(caps.native_turn_cap);
    assert!(caps.auth_inherits_session);
}

#[test]
fn estimate_cost_within_5_percent() {
    let provider = MockProvider { name: "mock" };
    let cost = provider.estimate_cost(1_000, 500);
    let expected = (1_000.0_f64 / 1_000_000.0 * 3.0) + (500.0_f64 / 1_000_000.0 * 15.0);
    let tolerance = expected * 0.05;
    assert!(
        (cost - expected).abs() <= tolerance,
        "cost={cost}, expected={expected}, tolerance={tolerance}"
    );
}
