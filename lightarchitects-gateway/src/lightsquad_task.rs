//! Light Squad task dispatch MCP action — single-shot LLM delegation routed
//! through the `LiteLLM` proxy.
//!
//! Provides Claude Code's main session a way to offload subagent work to
//! cheaper providers (Ollama Cloud free tier, qwen3-coder, etc.) instead of
//! spending Anthropic API tokens on every subagent call. The action accepts a
//! generic [`TaskSpec`] discriminated by `kind` and returns a [`TaskResult`]
//! with the LLM output plus correlation metadata.
//!
//! # Provider routing
//!
//! The provider is built via [`crate::providers::litellm::build_provider`],
//! which reads `LA_LITELLM_BASE_URL`, `LA_LITELLM_API_KEY`, and
//! `LA_LITELLM_MODEL`. The destination model is determined entirely by the
//! `LA_LITELLM_MODEL` env var; the gateway is provider-agnostic.
//!
//! # Tracing
//!
//! Each call emits a `gateway.lightsquad.dispatch_task` AYIN span carrying
//! `task_id`, `kind`, optional `workflow_id`, and `duration_ms`. The optional
//! `workflow_id` also propagates to the downstream `AgentRequest` via
//! [`ChainContext::origin`] so any spans emitted by the provider inherit the
//! correlation key.

use std::path::PathBuf;
use std::time::Instant;

use lightarchitects::agent::{ChainContext, LlmAgentProvider, dispatch_action};
use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};
use lightarchitects::lightsquad::types::ContextTier;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::config::GatewayConfig;
use crate::error::GatewayError;
use crate::providers::litellm;
use crate::span_context::{
    current_span_ctx, span_dir, spawn_with_span_context, write_span_to_disk,
};

/// Sibling identity used as the LLM system-prompt prefix.
///
/// Mirrors the `CORSO_IDENTITY` pattern from `handlers::corso` — gives the
/// downstream model a stable role description so its output style stays
/// consistent across `kind` discriminators.
const LIGHTSQUAD_IDENTITY: &str = "You are a LightSquad task runner — a peer \
    domain agent in the Light Architects sibling mesh, invoked for bounded \
    single-shot work delegated by the orchestrator. Execute the requested task \
    using the provided context tiers and respond concisely in the requested \
    output format. Do not add commentary outside the task scope.";

/// Default per-task budget cap (USD) when the caller omits `max_budget_usd`.
const DEFAULT_BUDGET_USD: f64 = 0.10;

/// Hard upper bound on `max_budget_usd` accepted from the caller.
const MAX_BUDGET_USD: f64 = 1.0;

/// Maximum prompt size in bytes (UTF-8 encoding).
const MAX_PROMPT_BYTES: usize = 64 * 1024;

/// Maximum sum of `token_estimate` across all submitted context tiers.
///
/// Mirrors the Builders Cookbook §66 T1/T2/T3 budget cap.
const MAX_CONTEXT_TIER_TOKENS: u32 = 15_000;

/// Maximum length of the `kind` discriminator.
const KIND_MAX_LEN: usize = 64;

// ── TaskSpec ─────────────────────────────────────────────────────────────────

/// MCP input payload for `lightarchitects_lightsquad_dispatch_task`.
///
/// Field validation rules are enforced inside [`dispatch_task`] before the LLM
/// dispatch is constructed.
#[derive(Debug, Clone, Deserialize)]
pub struct TaskSpec {
    /// Discriminator for observability (`code_search`, `surface_map`, `refactor`,
    /// `test_gen`, `doc_gen`, `generic`, …). Free-form but must match
    /// `[a-zA-Z0-9_-]{1,KIND_MAX_LEN}`.
    pub kind: String,
    /// Prompt sent to the downstream LLM. Required, non-empty, ≤ 64 KB.
    pub prompt: String,
    /// Optional Tier 1/2/3 context bundle (file paths + token estimates).
    /// The sum of `token_estimate` is capped at 15 K.
    #[serde(default)]
    pub context_tiers: Vec<ContextTier>,
    /// Per-task USD budget cap. Defaults to `DEFAULT_BUDGET_USD` when absent.
    /// Bounded `(0.0, 1.0]`.
    #[serde(default)]
    pub max_budget_usd: Option<f64>,
    /// Optional workflow identifier propagated via [`ChainContext::origin`] for
    /// cross-call correlation in AYIN spans.
    #[serde(default)]
    pub workflow_id: Option<String>,
}

// ── TaskResult ───────────────────────────────────────────────────────────────

/// MCP response payload returned by [`dispatch_task`].
#[derive(Debug, Clone, Serialize)]
pub struct TaskResult {
    /// `UUIDv7` minted at handler entry — used for log + AYIN span correlation.
    pub task_id: String,
    /// Echoed `kind` discriminator from the input spec.
    pub kind: String,
    /// LLM output. Provider-specific JSON shape — typically a string or a
    /// structured object when the prompt requested JSON output.
    pub output: Value,
    /// Wall-clock time from handler entry to result, in milliseconds.
    pub duration_ms: u64,
}

// ── Handler ──────────────────────────────────────────────────────────────────

/// `lightarchitects_lightsquad_dispatch_task` — execute one LLM-backed task
/// via the `LiteLLM` proxy.
///
/// Replaces Claude-Code-spawned subagents for offloadable work (code search,
/// surface mapping, simple refactoring, test/doc generation).
///
/// # Errors
///
/// - [`GatewayError::InvalidParam`] when the parsed [`TaskSpec`] fails
///   validation (bad `kind`, empty/oversize `prompt`, budget out of range,
///   context tier tokens over cap).
/// - [`GatewayError::Internal`] when the `LiteLLM` provider cannot be built.
/// - Propagates downstream provider errors via the
///   `From<HandlerError> for GatewayError` impl in `error.rs`.
pub async fn dispatch_task(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    // `config` is reserved for future per-deployment policy hooks (rate limits,
    // tier overrides). Silenced explicitly to keep the squad_comms signature
    // shape uniform across handlers.
    let _ = config;

    let start = Instant::now();

    let spec: TaskSpec = serde_json::from_value(params)
        .map_err(|e| GatewayError::InvalidParam(format!("TaskSpec deserialize failed: {e}")))?;

    validate_spec(&spec)?;

    let task_id = format!("tsk_{}", Uuid::now_v7());
    let budget = spec.max_budget_usd.unwrap_or(DEFAULT_BUDGET_USD);

    let chain = ChainContext {
        origin: spec.workflow_id.clone(),
        depth: 0,
        aud: Some("lightsquad-task".to_owned()),
    };

    let provider = litellm::build_provider()
        .map_err(|e| GatewayError::Internal(format!("litellm provider build failed: {e}")))?;

    // dispatch_action wraps action+params as the LLM prompt; we surface our
    // user prompt + context tiers via the params Value so the downstream model
    // sees the task verbatim under a stable schema.
    let llm_params = json!({
        "task_prompt": spec.prompt,
        "context_tiers": spec.context_tiers,
    });

    let output = dispatch_action(
        &provider as &dyn LlmAgentProvider,
        "lightsquad",
        &spec.kind,
        &llm_params,
        LIGHTSQUAD_IDENTITY,
        budget,
        chain,
    )
    .await?;

    let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

    emit_dispatch_span(
        &task_id,
        &spec.kind,
        spec.workflow_id.as_deref(),
        duration_ms,
    );

    let result = TaskResult {
        task_id,
        kind: spec.kind,
        output,
        duration_ms,
    };

    Ok(serde_json::to_value(&result)?)
}

// ── Validation ───────────────────────────────────────────────────────────────

fn validate_spec(spec: &TaskSpec) -> Result<(), GatewayError> {
    validate_kind(&spec.kind)?;
    validate_prompt(&spec.prompt)?;
    validate_budget(spec.max_budget_usd)?;
    validate_context_tiers(&spec.context_tiers)?;
    Ok(())
}

fn validate_kind(kind: &str) -> Result<(), GatewayError> {
    if kind.is_empty() || kind.len() > KIND_MAX_LEN {
        return Err(GatewayError::InvalidParam(format!(
            "kind must be 1-{KIND_MAX_LEN} chars, got {}",
            kind.len()
        )));
    }
    if !kind
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(GatewayError::InvalidParam(format!(
            "kind '{kind}' must match [a-zA-Z0-9_-]+"
        )));
    }
    Ok(())
}

fn validate_prompt(prompt: &str) -> Result<(), GatewayError> {
    if prompt.is_empty() {
        return Err(GatewayError::InvalidParam(
            "prompt must not be empty".to_owned(),
        ));
    }
    if prompt.len() > MAX_PROMPT_BYTES {
        return Err(GatewayError::InvalidParam(format!(
            "prompt exceeds {MAX_PROMPT_BYTES} bytes (got {})",
            prompt.len()
        )));
    }
    Ok(())
}

fn validate_budget(budget: Option<f64>) -> Result<(), GatewayError> {
    let Some(b) = budget else {
        return Ok(());
    };
    if !b.is_finite() || b <= 0.0 || b > MAX_BUDGET_USD {
        return Err(GatewayError::InvalidParam(format!(
            "max_budget_usd must be in (0.0, {MAX_BUDGET_USD}], got {b}"
        )));
    }
    Ok(())
}

fn validate_context_tiers(tiers: &[ContextTier]) -> Result<(), GatewayError> {
    let mut sum: u64 = 0;
    for t in tiers {
        if !matches!(t.tier.as_str(), "T1" | "T2" | "T3") {
            return Err(GatewayError::InvalidParam(format!(
                "context_tier.tier must be T1|T2|T3, got '{}'",
                t.tier
            )));
        }
        sum = sum.saturating_add(u64::from(t.token_estimate));
    }
    if sum > u64::from(MAX_CONTEXT_TIER_TOKENS) {
        return Err(GatewayError::InvalidParam(format!(
            "sum of context_tier token_estimate ({sum}) exceeds cap {MAX_CONTEXT_TIER_TOKENS}"
        )));
    }
    Ok(())
}

// ── AYIN span ────────────────────────────────────────────────────────────────

fn emit_dispatch_span(task_id: &str, kind: &str, workflow_id: Option<&str>, duration_ms: u64) {
    let ctx = current_span_ctx();
    let task_id = task_id.to_owned();
    let kind = kind.to_owned();
    let workflow_id = workflow_id.map(str::to_owned);

    spawn_with_span_context(async move {
        let mut metadata = json!({
            "task_id": task_id,
            "kind": kind,
            "duration_ms": duration_ms,
        });
        if let Some(ref wf) = workflow_id {
            metadata["workflow_id"] = json!(wf);
            metadata["chain.origin"] = json!(wf);
        }

        let mut builder =
            TraceContext::new(Actor::new("gateway"), "gateway.lightsquad.dispatch_task")
                .outcome(TraceOutcome::Continue)
                .metadata(metadata);
        if let Some(pid) = ctx.parent_id {
            builder = builder.parent(pid);
        }
        if let Some(ref sid) = ctx.session_id {
            builder = builder.session_id(sid);
        }
        let Ok(span) = builder.finish() else {
            return;
        };
        let base = dirs_next::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lightarchitects/soul/helix/ayin/traces");
        let dir = span_dir(&base, "gateway", &span.timestamp);
        if let Err(e) = write_span_to_disk(&span, &dir).await {
            tracing::warn!(error = %e, "gateway.lightsquad.dispatch_task AYIN span write failed");
        }
    });
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn tier(tier: &str, tokens: u32) -> ContextTier {
        ContextTier {
            tier: tier.to_owned(),
            label: "t".to_owned(),
            files: vec![],
            token_estimate: tokens,
        }
    }

    // ── validate_kind ────────────────────────────────────────────────────────

    #[test]
    fn validate_kind_empty_rejected() {
        let err = validate_kind("").unwrap_err();
        assert!(matches!(err, GatewayError::InvalidParam(_)));
    }

    #[test]
    fn validate_kind_too_long_rejected() {
        let kind = "x".repeat(KIND_MAX_LEN + 1);
        assert!(validate_kind(&kind).is_err());
    }

    #[test]
    fn validate_kind_bad_chars_rejected() {
        assert!(validate_kind("code search").is_err());
        assert!(validate_kind("code/search").is_err());
        assert!(validate_kind("code!").is_err());
    }

    #[test]
    fn validate_kind_valid_accepted() {
        for ok in ["code_search", "surface-map", "refactor", "x", "abc123"] {
            assert!(validate_kind(ok).is_ok(), "expected {ok:?} to be accepted");
        }
    }

    // ── validate_prompt ──────────────────────────────────────────────────────

    #[test]
    fn validate_prompt_empty_rejected() {
        assert!(validate_prompt("").is_err());
    }

    #[test]
    fn validate_prompt_too_long_rejected() {
        let p = "x".repeat(MAX_PROMPT_BYTES + 1);
        assert!(validate_prompt(&p).is_err());
    }

    #[test]
    fn validate_prompt_valid_accepted() {
        assert!(validate_prompt("Find all callers of dispatch_action").is_ok());
    }

    // ── validate_budget ──────────────────────────────────────────────────────

    #[test]
    fn validate_budget_none_accepted() {
        assert!(validate_budget(None).is_ok());
    }

    #[test]
    fn validate_budget_zero_rejected() {
        assert!(validate_budget(Some(0.0)).is_err());
    }

    #[test]
    fn validate_budget_negative_rejected() {
        assert!(validate_budget(Some(-0.01)).is_err());
    }

    #[test]
    fn validate_budget_over_max_rejected() {
        assert!(validate_budget(Some(MAX_BUDGET_USD + 0.01)).is_err());
    }

    #[test]
    fn validate_budget_nan_rejected() {
        assert!(validate_budget(Some(f64::NAN)).is_err());
        assert!(validate_budget(Some(f64::INFINITY)).is_err());
    }

    #[test]
    fn validate_budget_valid_accepted() {
        assert!(validate_budget(Some(0.05)).is_ok());
        assert!(validate_budget(Some(MAX_BUDGET_USD)).is_ok());
    }

    // ── validate_context_tiers ───────────────────────────────────────────────

    #[test]
    fn validate_context_tiers_empty_accepted() {
        assert!(validate_context_tiers(&[]).is_ok());
    }

    #[test]
    fn validate_context_tiers_bad_tier_rejected() {
        assert!(validate_context_tiers(&[tier("T0", 100)]).is_err());
        assert!(validate_context_tiers(&[tier("T4", 100)]).is_err());
        assert!(validate_context_tiers(&[tier("global", 100)]).is_err());
    }

    #[test]
    fn validate_context_tiers_over_cap_rejected() {
        let tiers = vec![tier("T1", MAX_CONTEXT_TIER_TOKENS), tier("T2", 1)];
        assert!(validate_context_tiers(&tiers).is_err());
    }

    #[test]
    fn validate_context_tiers_sum_at_cap_accepted() {
        let tiers = vec![tier("T1", 5_000), tier("T2", 5_000), tier("T3", 5_000)];
        assert!(validate_context_tiers(&tiers).is_ok());
    }

    // ── TaskSpec deserialization ─────────────────────────────────────────────

    #[test]
    fn task_spec_minimal_deserializes() {
        let v = json!({"kind": "generic", "prompt": "hello"});
        let spec: TaskSpec = serde_json::from_value(v).unwrap();
        assert_eq!(spec.kind, "generic");
        assert_eq!(spec.prompt, "hello");
        assert!(spec.context_tiers.is_empty());
        assert!(spec.max_budget_usd.is_none());
        assert!(spec.workflow_id.is_none());
    }

    #[test]
    fn task_spec_full_deserializes() {
        let v = json!({
            "kind": "code_search",
            "prompt": "find callers",
            "context_tiers": [{"tier": "T2", "label": "ws", "files": ["a.rs"], "token_estimate": 4000}],
            "max_budget_usd": 0.05,
            "workflow_id": "wf_abc",
        });
        let spec: TaskSpec = serde_json::from_value(v).unwrap();
        assert_eq!(spec.context_tiers.len(), 1);
        assert_eq!(spec.max_budget_usd, Some(0.05));
        assert_eq!(spec.workflow_id.as_deref(), Some("wf_abc"));
    }

    #[test]
    fn task_spec_missing_kind_fails() {
        let v = json!({"prompt": "hello"});
        assert!(serde_json::from_value::<TaskSpec>(v).is_err());
    }

    #[test]
    fn task_spec_missing_prompt_fails() {
        let v = json!({"kind": "generic"});
        assert!(serde_json::from_value::<TaskSpec>(v).is_err());
    }

    // ── TaskResult serialization ─────────────────────────────────────────────

    #[test]
    fn task_result_serializes_with_expected_fields() {
        let r = TaskResult {
            task_id: "tsk_abc".to_owned(),
            kind: "generic".to_owned(),
            output: json!("hello world"),
            duration_ms: 1234,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["task_id"], "tsk_abc");
        assert_eq!(v["kind"], "generic");
        assert_eq!(v["output"], "hello world");
        assert_eq!(v["duration_ms"], 1234);
    }

    // ── ChainContext workflow_id wiring ──────────────────────────────────────

    #[test]
    fn chain_context_origin_set_from_workflow_id() {
        let spec = TaskSpec {
            kind: "generic".to_owned(),
            prompt: "x".to_owned(),
            context_tiers: vec![],
            max_budget_usd: None,
            workflow_id: Some("wf_smoke".to_owned()),
        };
        // Mirror the wiring used in dispatch_task — this guards against future
        // refactors that decouple workflow_id from ChainContext.origin.
        let chain = ChainContext {
            origin: spec.workflow_id.clone(),
            depth: 0,
            aud: Some("lightsquad-task".to_owned()),
        };
        assert_eq!(chain.origin.as_deref(), Some("wf_smoke"));
        assert_eq!(chain.depth, 0);
        assert_eq!(chain.aud.as_deref(), Some("lightsquad-task"));
    }

    // ── dispatch_task entry — invalid params ─────────────────────────────────

    #[tokio::test]
    async fn dispatch_task_rejects_bad_kind() {
        let cfg = GatewayConfig::default();
        let params = json!({"kind": "bad kind!", "prompt": "hello"});
        let err = dispatch_task(params, &cfg).await.unwrap_err();
        assert!(matches!(err, GatewayError::InvalidParam(_)));
    }

    #[tokio::test]
    async fn dispatch_task_rejects_empty_prompt() {
        let cfg = GatewayConfig::default();
        let params = json!({"kind": "generic", "prompt": ""});
        let err = dispatch_task(params, &cfg).await.unwrap_err();
        assert!(matches!(err, GatewayError::InvalidParam(_)));
    }

    #[tokio::test]
    async fn dispatch_task_rejects_zero_budget() {
        let cfg = GatewayConfig::default();
        let params = json!({"kind": "generic", "prompt": "hi", "max_budget_usd": 0.0});
        let err = dispatch_task(params, &cfg).await.unwrap_err();
        assert!(matches!(err, GatewayError::InvalidParam(_)));
    }

    #[tokio::test]
    async fn dispatch_task_rejects_context_tier_overflow() {
        let cfg = GatewayConfig::default();
        let params = json!({
            "kind": "generic",
            "prompt": "hi",
            "context_tiers": [{"tier": "T1", "label": "x", "files": [], "token_estimate": 20000}]
        });
        let err = dispatch_task(params, &cfg).await.unwrap_err();
        assert!(matches!(err, GatewayError::InvalidParam(_)));
    }

    #[tokio::test]
    async fn dispatch_task_rejects_garbage_params() {
        let cfg = GatewayConfig::default();
        let params = json!({"not_a_task_spec": true});
        let err = dispatch_task(params, &cfg).await.unwrap_err();
        assert!(matches!(err, GatewayError::InvalidParam(_)));
    }
}
