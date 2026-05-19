//! Smoke — Canon XXVII Suite 6 gateway health checks.
//!
//! Bounded, fast checks (< 30 s each) proving the gateway's critical
//! dispatch surfaces behave correctly without spawning the real `claude`
//! CLI subprocess or requiring external services.
//!
//! Tests are partitioned into two blocks:
//! - Always-on: pure sanitization + chain-depth invariants (no feature flags).
//! - Feature-gated: handler-level dispatch (requires `inline-corso`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

use lightarchitects::agent::{ChainContext, MAX_CHAIN_DEPTH, ProviderError, sanitize_params};

// ── G1 sanitization smoke ──────────────────────────────────────────────────

#[test]
fn g1_rejects_null_byte_in_identity() {
    assert!(
        sanitize_params("bad\x00ident", "safe prompt").is_err(),
        "null byte in identity must be rejected by G1 control-plane"
    );
}

#[test]
fn g1_rejects_system_token_in_identity() {
    assert!(
        sanitize_params("<system>", "prompt").is_err(),
        "<system> token must be rejected by G1 control-plane"
    );
}

#[test]
fn g1_rejects_rtl_override_in_identity() {
    assert!(
        sanitize_params("look \u{202E}legit", "prompt").is_err(),
        "RTL override U+202E must be rejected by G1 control-plane"
    );
}

// ── Chain-depth guard smoke ────────────────────────────────────────────────

#[test]
fn chain_depth_default_is_zero() {
    assert_eq!(ChainContext::default().depth, 0);
}

#[test]
fn chain_child_increments_depth() {
    let child = ChainContext::default()
        .child()
        .expect("depth 0 → 1 must succeed");
    assert_eq!(child.depth, 1);
}

#[test]
fn chain_depth_exceeded_at_max() {
    let at_max = ChainContext {
        depth: MAX_CHAIN_DEPTH,
        ..ChainContext::default()
    };
    let result = at_max.child();
    assert!(
        matches!(result, Err(ProviderError::ChainDepthExceeded { depth }) if depth == MAX_CHAIN_DEPTH),
        "child() at MAX_CHAIN_DEPTH must return ChainDepthExceeded, got: {result:?}"
    );
}

// ── Handler-level action-allowlist smoke ────────────────────────────────────

#[cfg(feature = "inline-corso")]
mod handler_smoke {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use lightarchitects::agent::{
        AgentResponse, LlmAgentProvider, ProviderCapabilities, ProviderError,
        SanitizedAgentRequest, SchemaMode, TokenUsage,
    };
    use lightarchitects::core::handler::{HandlerError, SiblingHandler};
    use lightarchitects_gateway::config::GatewayConfig;
    use lightarchitects_gateway::handlers::CorsoHandler;

    struct NopProvider;

    #[async_trait]
    impl LlmAgentProvider for NopProvider {
        fn name(&self) -> &'static str {
            "nop"
        }

        async fn spawn(&self, _req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
            Ok(AgentResponse {
                output: serde_json::json!({}),
                turns_used: 0,
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

        fn estimate_cost(&self, _i: u32, _o: u32) -> f64 {
            0.0
        }
    }

    #[tokio::test]
    async fn corso_rejects_unknown_action() {
        let h = CorsoHandler::new(&GatewayConfig::default());
        let result = h.call("not_a_real_action", serde_json::json!({})).await;
        assert!(
            matches!(result, Err(HandlerError::UnknownAction { .. })),
            "unknown action must return HandlerError::UnknownAction; got: {result:?}"
        );
    }

    #[tokio::test]
    async fn corso_rejects_action_with_null_byte() {
        // An action name containing a null byte cannot be in the CORSO_ACTIONS allowlist;
        // it must be rejected (not a panic or internal error).
        let h = CorsoHandler::new(&GatewayConfig::default());
        let result = h.call("sniff\x00inject", serde_json::json!({})).await;
        assert!(
            result.is_err(),
            "action name with null byte must always be rejected"
        );
    }

    #[tokio::test]
    async fn corso_rejects_oversized_params() {
        let h = CorsoHandler::with_provider(Arc::new(NopProvider));
        let big = "x".repeat(5_000);
        let result = h.call("sniff", serde_json::json!({"data": big})).await;
        assert!(
            matches!(result, Err(HandlerError::InvalidParams { .. })),
            "oversized params must yield InvalidParams; got: {result:?}"
        );
    }
}
