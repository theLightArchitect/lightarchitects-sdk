//! Conformance tests for `code.trait.runner` and `operator.surface` contracts.
//!
//! These tests prove behavioural contract requirements are met by the Runner
//! implementations without requiring live LLM provider access.  The three
//! named tests map directly to the plan deliverables (Phase 6, line 783):
//!
//! - [`test_copilot_send_message_proof_round_trip`] — provider-agnostic routing
//!   for the copilot send-message path (provider pill → runner capability).
//! - [`test_dispatch_wave_artifacts_persist`] — artifact files written to the
//!   sandbox dir are discoverable via the `dispatch-artifacts` contract path.
//! - [`test_provider_switch_routes_to_new_provider`] — `select_runner` routes
//!   to a different concrete impl when the provider name changes.
//!
//! Live-provider end-to-end tests are gated by `LA_CONFORMANCE_LIVE=1`
//! (Cookbook §50.10 two-envvar opt-in) and are marked `#[ignore]` so they
//! never run in CI without the gate.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt as _;
use lightarchitects::agent::{
    AgentRequest, AgentResponse, LlmAgentProvider, ProviderCapabilities, ProviderError,
    SanitizedAgentRequest, SchemaMode, TokenUsage,
};
use lightarchitects::lightsquad::{
    agent_role::AgentRole,
    runner::{AgentEvent, AgentSpec, RunnerError, select_runner},
};
use tempfile::TempDir;

// ── StubProvider ─────────────────────────────────────────────────────────────
//
// `#[cfg(test)]` items are invisible to integration test crates (separate compilation
// unit), so we reproduce the minimum StubProvider here rather than re-exporting the
// private one from runner.rs.

struct StubProvider {
    name: &'static str,
    text: String,
}

impl StubProvider {
    fn returning(name: &'static str, text: &str) -> Self {
        Self {
            name,
            text: text.to_owned(),
        }
    }
}

#[async_trait]
impl LlmAgentProvider for StubProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn spawn(&self, _req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
        Ok(AgentResponse {
            output: serde_json::Value::String(self.text.clone()),
            turns_used: 1,
            cost_usd: 0.0,
            tokens: TokenUsage {
                input: 4,
                output: 8,
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
            auth_inherits_session: true,
        }
    }

    fn estimate_cost(&self, _input_tokens: u32, _max_output_tokens: u32) -> f64 {
        0.0
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn test_req() -> SanitizedAgentRequest {
    AgentRequest {
        sibling_identity: "conformance-test".to_owned(),
        user_prompt: "write a stub conformance artifact".to_owned(),
        schema: None,
        allowed_tools: vec![],
        max_turns: 1,
        max_budget_usd: 0.01,
        model_hint: None,
        parent_span_id: None,
        chain_origin: None,
        chain_depth: 0,
        aud: None,
        conversation_history: vec![],
        tool_definitions: vec![],
    }
    .sanitize()
    .expect("test request is valid")
}

fn make_spec(agent_name: &str, artifact_dir: PathBuf) -> AgentSpec {
    AgentSpec {
        agent_name: agent_name.to_owned(),
        agent_role: AgentRole::default(),
        task: test_req(),
        artifact_dir,
        parent_span_id: None,
        input_tokens_estimate: 100,
        max_output_tokens: 512,
        file_ownership: vec![],
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Proves the copilot send-message routing invariant: `select_runner` returns a
/// runner whose `capabilities()` advertises streaming, matching the
/// `copilot.send-message` contract requirement for incremental delta delivery.
///
/// Contract: `standards/canon/contracts/operator.surface/copilot.send-message.yaml`
/// Contract field: streaming delivery required for copilot chat drawer.
#[tokio::test]
async fn test_copilot_send_message_proof_round_trip() {
    let dir = TempDir::new().expect("tempdir");
    let sandbox = dir.path().to_path_buf();

    // "claude-cli" is the canonical provider for the copilot send-message path.
    let provider = Arc::new(StubProvider::returning(
        "claude-cli",
        "# conformance artifact\n",
    ));
    let runner = select_runner("claude-cli", provider, sandbox.clone())
        .expect("claude-cli runner must be constructable");

    let caps = runner.capabilities();
    // Contract requirement: streaming must be advertised so the copilot drawer
    // can display incremental text deltas (TextDelta events).
    assert!(
        caps.streaming,
        "claude-cli runner must advertise streaming (copilot.send-message contract)"
    );

    // Prove a full stream round-trip produces AgentComplete with an artifact.
    // The artifact_dir must exist before the runner writes to it.
    let artifact_dir = sandbox.join("dispatch-conformance-001");
    std::fs::create_dir_all(&artifact_dir).expect("create artifact dir");
    let canon_sandbox = sandbox.canonicalize().expect("canonicalize sandbox");

    let spec = make_spec("copilot", artifact_dir);
    let mut stream = runner.stream(spec).await.expect("stream opened");
    let mut saw_complete = false;

    while let Some(ev) = stream.next().await {
        match ev {
            AgentEvent::AgentComplete { artifact } => {
                saw_complete = true;
                // Use canonicalized sandbox for starts_with — on macOS /var → /private/var.
                let ap = &artifact.artifact_path;
                assert!(
                    ap.starts_with(&canon_sandbox),
                    "artifact must be inside sandbox: {ap:?}",
                );
            }
            #[allow(clippy::panic)]
            AgentEvent::AgentError { error } => {
                panic!("unexpected AgentError: {error}");
            }
            _ => {}
        }
    }

    assert!(saw_complete, "stream must emit AgentComplete");
}

/// Proves that artifacts written to `.tmp/dispatch-<id>/` are visible in the
/// expected path structure, matching the `dispatch-artifacts` contract.
///
/// Contract: `standards/canon/contracts/operator.surface/dispatch-artifacts.yaml`
/// Contract fields: `test_id: dispatch-artifacts-tab`, artifact dir pattern.
#[tokio::test]
async fn test_dispatch_wave_artifacts_persist() {
    let dir = TempDir::new().expect("tempdir");
    let dispatch_id = "dispatch-conformance-002";

    // Mirror the artifact directory pattern from dispatch/routes.rs:
    // `state.config.cwd.join(".tmp").join(format!("dispatch-{id}"))`
    let artifact_dir = dir.path().join(".tmp").join(dispatch_id);
    std::fs::create_dir_all(&artifact_dir).expect("create artifact dir");

    let provider = Arc::new(StubProvider::returning(
        "claude-cli",
        "# wave result\narchitecture: verified\n",
    ));
    let runner = select_runner("claude-cli", provider, dir.path().to_path_buf()).expect("runner");

    let spec = make_spec("engineer", artifact_dir.clone());
    let mut stream = runner.stream(spec).await.expect("stream opened");
    let mut artifact_path: Option<PathBuf> = None;

    while let Some(ev) = stream.next().await {
        if let AgentEvent::AgentComplete { artifact } = ev {
            artifact_path = Some(artifact.artifact_path.clone());
        }
    }

    let path = artifact_path.expect("AgentComplete was emitted");
    // Canonicalize for macOS /var → /private/var comparison.
    let canon_artifact_dir = artifact_dir
        .canonicalize()
        .expect("canonicalize artifact dir");

    // Artifact must exist on disk — the Results tab GET /api/dispatch/{id}/artifacts
    // lists files in this directory, so persistence is a contract requirement.
    assert!(path.exists(), "artifact file must be on disk: {path:?}");

    // Artifact must be inside the dispatch-specific subdirectory.
    assert!(
        path.starts_with(&canon_artifact_dir),
        "artifact {path:?} must be under dispatch dir {canon_artifact_dir:?}",
    );

    // At least one file must be visible in the artifact dir (listing invariant).
    let entries: Vec<_> = std::fs::read_dir(&artifact_dir)
        .expect("read artifact dir")
        .filter_map(Result::ok)
        .collect();
    assert!(
        !entries.is_empty(),
        "artifact dir must be non-empty for Results tab listing"
    );
}

/// Proves that `select_runner` routes to a different concrete implementation
/// when the provider name changes, satisfying the `provider-select` contract
/// requirement that provider switching affects execution behaviour.
///
/// Contract: `standards/canon/contracts/operator.surface/provider-select.yaml`
/// Contract field: provider routing must select a per-provider runner impl.
#[test]
fn test_provider_switch_routes_to_new_provider() {
    let dir = TempDir::new().expect("tempdir");
    let sandbox = dir.path().to_path_buf();

    let provider_a = Arc::new(StubProvider::returning("claude-cli", ""));
    let provider_b = Arc::new(StubProvider::returning("anthropic-http", ""));

    let runner_a =
        select_runner("claude-cli", provider_a, sandbox.clone()).expect("claude-cli runner");
    let runner_b = select_runner("anthropic-http", provider_b, sandbox.clone())
        .expect("anthropic-http runner");

    let caps_a = runner_a.capabilities();
    let caps_b = runner_b.capabilities();

    // AnthropicHttpRunner has higher max_parallelism than ClaudeCliRunner because
    // HTTP calls are not constrained by subprocess spawn limits (8 vs 7).
    assert!(
        caps_b.max_parallelism > caps_a.max_parallelism,
        "anthropic-http runner ({}) must allow more parallelism than claude-cli ({})",
        caps_b.max_parallelism,
        caps_a.max_parallelism
    );

    // Unknown provider names must return an explicit error, not panic.
    let bad_result = select_runner(
        "unknown-provider-xyz",
        Arc::new(StubProvider::returning("x", "")),
        sandbox,
    );
    assert!(
        matches!(bad_result, Err(RunnerError::UnknownRunner(_))),
        "unknown provider must return UnknownRunner error"
    );
}

// ── Live provider tests (opt-in only) ────────────────────────────────────────

/// Live round-trip via `anthropic-http` provider.
///
/// Requires `LA_CONFORMANCE_LIVE=1` AND `ANTHROPIC_API_KEY` to be set.
/// Marked `#[ignore]` so it never runs in CI without explicit opt-in
/// (Cookbook §50.10 two-envvar opt-in pattern).
#[tokio::test]
#[ignore = "requires LA_CONFORMANCE_LIVE=1 and ANTHROPIC_API_KEY"]
async fn test_anthropic_http_live_conformance() {
    if std::env::var("LA_CONFORMANCE_LIVE").as_deref() != Ok("1") {
        return;
    }
    let _api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY required for live conformance test");

    // NOTE: live provider integration is not yet wired in this build. Phase 6
    // scope covers unit conformance; provider-agnostic bridge is platform-provider-abstraction.
    eprintln!("live conformance: placeholder — not yet wired");
}
