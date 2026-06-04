//! Per-agent dispatch trait (`code.trait.runner` contract).
//!
//! [`Runner`] is the platform's per-agent dispatch seam that replaces
//! hardcoded `claude --print -p` subprocess invocation in the dispatch
//! executor with a provider-pluggable implementation.
//!
//! # Design
//!
//! The Runner trait sits **above** the existing [`LlmAgentProvider`] seam:
//!
//! ```text
//! Dispatch executor
//!   └── Runner::run(AgentSpec)          ← this module
//!         └── LlmAgentProvider::spawn   ← agent/provider.rs
//!               └── subprocess / HTTP
//! ```
//!
//! The JSON-schema rubric supervisors in `contract_supervisor.rs:570-784`
//! (`run_claude_code`, `run_ollama`, etc.) are a **separate concern** — they
//! POST prompt + schema to an LLM and evaluate the structured response.
//! They are not touched by this module.
//!
//! # Phase status
//!
//! Phase 1 declares all type signatures. Phase 3 adds `ClaudeCliRunner` impl.
//! Phase 4 adds `AnthropicHttpRunner`, `OllamaRunner`, `OpenAICompatRunner`
//! impls + wires `select_runner` into the dispatch executor.
//!
//! [`LlmAgentProvider`]: crate::agent::provider::LlmAgentProvider

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use futures_util::{StreamExt as _, stream::BoxStream};
use tokio::sync::mpsc;

use crate::agent::{LlmAgentProvider, ProviderError, ProviderEvent, SanitizedAgentRequest};
use crate::lightsquad::agent_role::AgentRole;

// ── Public types ─────────────────────────────────────────────────────────────

/// Input specification for a single agent invocation.
///
/// `artifact_dir` MUST be a subdirectory of `$PROJECT_ROOT/.tmp/dispatch-<id>/`
/// (verified by the Runner sandbox check per Cookbook §63.P4 ancestor-walk).
pub struct AgentSpec {
    /// Display name of the agent (e.g. `"engineer"`, `"reviewer"`).
    pub agent_name: String,
    /// Role tag for A2A routing.
    pub agent_role: AgentRole,
    /// Pre-sanitized task description (G1 sanitization proof type).
    pub task: SanitizedAgentRequest,
    /// Directory where the agent writes its output artifact.
    ///
    /// Runner implementations MUST canonicalize this path and verify it
    /// starts with the project root before any write (Cookbook §63.P4).
    pub artifact_dir: PathBuf,
    /// W3C `TraceContext` span ID from the parent dispatch call (for AYIN propagation).
    pub parent_span_id: Option<String>,
    /// Estimated input token count (used for cost estimation and budget enforcement).
    pub input_tokens_estimate: u32,
    /// Maximum output tokens allowed for this invocation.
    pub max_output_tokens: u32,
    /// Files this agent owns during the dispatch wave (Playbook §XVI partitioning).
    pub file_ownership: Vec<PathBuf>,
}

impl fmt::Debug for AgentSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // WHY: SanitizedAgentRequest deliberately omits Debug to prevent prompt
        // data from appearing in logs. Redact the task field explicitly here.
        f.debug_struct("AgentSpec")
            .field("agent_name", &self.agent_name)
            .field("agent_role", &self.agent_role)
            .field("task", &"<redacted>")
            .field("artifact_dir", &self.artifact_dir)
            .field("parent_span_id", &self.parent_span_id)
            .field("input_tokens_estimate", &self.input_tokens_estimate)
            .field("max_output_tokens", &self.max_output_tokens)
            .field("file_ownership", &self.file_ownership)
            .finish()
    }
}

/// Output artifact produced by a successful agent invocation.
#[derive(Debug, Clone)]
pub struct AgentArtifact {
    /// Agent name (matches `AgentSpec::agent_name`).
    pub agent_name: String,
    /// Path to the artifact file on disk (absolute, inside `artifact_dir`).
    pub artifact_path: PathBuf,
    /// Actual input tokens consumed.
    pub tokens_input: u32,
    /// Actual output tokens produced.
    pub tokens_output: u32,
    /// Why the agent stopped producing output.
    pub stop_reason: StopReason,
    /// Wall-clock duration of the invocation.
    pub duration_ms: u64,
}

/// Why an agent's LLM call terminated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// Natural end-of-response.
    EndTurn,
    /// Hit the configured token budget.
    MaxTokens,
    /// Matched a stop sequence.
    StopSequence,
    /// Stopped to invoke a tool.
    ToolUse,
    /// Stopped due to an error condition.
    Error,
}

/// Capabilities advertised by a [`Runner`] at registration time.
#[derive(Debug, Clone)]
pub struct RunnerCapabilities {
    /// Agent roles this runner can service (empty = all roles).
    pub supported_roles: Vec<AgentRole>,
    /// Maximum number of parallel `run()` invocations allowed.
    pub max_parallelism: u32,
    /// Whether this runner supports structured `tool_use` calls.
    pub tool_use: bool,
    /// Whether `stream()` returns incremental deltas (vs a single terminal event).
    pub streaming: bool,
}

/// Higher-level streaming events emitted by [`Runner::stream`].
///
/// Distinct from [`ProviderEvent`] which is a lower-level LLM message-protocol
/// event. `AgentEvent` models the agent dispatch lifecycle.
///
/// [`ProviderEvent`]: crate::agent::ProviderEvent
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Agent invocation has started.
    AgentStart {
        /// Stable identifier of the runner kind (e.g. `"claude_cli"`).
        runner: String,
        /// Display name of the agent that started (e.g. `"engineer"`).
        agent_name: String,
    },
    /// Incremental text output from the agent.
    TextDelta {
        /// UTF-8 text fragment produced by the LLM.
        text: String,
    },
    /// Agent invocation completed successfully.
    AgentComplete {
        /// The artifact written to disk.
        artifact: AgentArtifact,
    },
    /// Agent invocation failed; the runner encountered an unrecoverable error.
    AgentError {
        /// Human-readable error description (no sensitive data).
        error: String,
    },
}

/// Errors that can occur during a [`Runner`] dispatch.
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    /// The underlying [`LlmAgentProvider`] returned an error.
    #[error("provider failed: {0}")]
    ProviderFailed(#[from] ProviderError),

    /// The artifact path is outside the permitted sandbox boundary.
    ///
    /// WHY: Cookbook §63.P4 — canonicalize-then-starts_with check; a path
    /// that escapes the `.tmp/dispatch-<id>/` subtree must be rejected before
    /// any write, not after.
    #[error("artifact path outside sandbox: {0:?}")]
    SandboxViolation(PathBuf),

    /// Two agents claimed ownership of the same file in the same wave.
    #[error("file ownership conflict on {path:?} between {a} and {b}")]
    FileOwnershipConflict {
        /// The contested file path.
        path: PathBuf,
        /// Name of the first agent claiming ownership.
        a: String,
        /// Name of the second agent claiming ownership.
        b: String,
    },

    /// G1 sanitization was not applied to the task before dispatch.
    #[error("task sanitization required before dispatch")]
    SanitizationFailed,

    /// No runner is registered for the given provider name.
    ///
    /// WHY: Failing closed on unknown providers prevents silent fallback to
    /// hardcoded `claude` subprocess, which was the pre-Runner behaviour.
    #[error("unknown runner provider: {0}")]
    UnknownRunner(String),

    /// An I/O error occurred during artifact persistence.
    #[error("io: {0}")]
    Io(String),
}

// ── Runner trait ─────────────────────────────────────────────────────────────

/// Per-agent dispatch trait (`code.trait.runner` contract).
///
/// Implementations wrap an [`LlmAgentProvider`] and add the dispatch-layer
/// concerns: artifact persistence, sandbox verification, AYIN span emission,
/// and subprocess environment enforcement.
///
/// # Security invariants (zero-exception per `security_compliance` block)
///
/// - `artifact_dir` MUST be verified via §63.P4 ancestor-walk before any write.
/// - Subprocess runners MUST call `.env_clear().envs(SUBPROCESS_ENV_ALLOWLIST)`
///   before spawning (LLM06 subprocess env isolation).
/// - HTTP runners MUST set `https_only(true)` + `min_tls_version(TLS_1_2)`
///   (LLM05 SSRF + TLS guard).
/// - AYIN span metadata MUST NOT contain credential values (fingerprint hash only).
#[async_trait]
pub trait Runner: Send + Sync {
    /// Stable identifier for the runner kind (e.g., `"claude_cli"`, `"anthropic_http"`).
    fn name(&self) -> &'static str;

    /// Capabilities declaration for this runner instance.
    fn capabilities(&self) -> RunnerCapabilities;

    /// Run a single agent against the task; persist artifact at
    /// `spec.artifact_dir/<agent_name>.md`.
    ///
    /// # Errors
    ///
    /// Returns [`RunnerError::SandboxViolation`] if `artifact_dir` is outside
    /// the permitted subtree, [`RunnerError::ProviderFailed`] on LLM errors,
    /// [`RunnerError::FileOwnershipConflict`] on write contention, or
    /// [`RunnerError::SanitizationFailed`] if the task was not pre-sanitized.
    async fn run(&self, spec: AgentSpec) -> Result<AgentArtifact, RunnerError>;

    /// Stream agent execution as an [`AgentEvent`] sequence.
    ///
    /// The first event is always [`AgentEvent::AgentStart`]; the last is either
    /// [`AgentEvent::AgentComplete`] or [`AgentEvent::AgentError`].
    ///
    /// # Errors
    ///
    /// Same error variants as [`run`].
    ///
    /// [`run`]: Runner::run
    async fn stream(&self, spec: AgentSpec) -> Result<BoxStream<'static, AgentEvent>, RunnerError>;

    /// Estimate USD cost for this invocation given token estimates.
    ///
    /// Returns `0.0` for runners backed by free-tier or local providers.
    fn estimate_cost(&self, input_tokens: u32, max_output_tokens: u32) -> f64;
}

// ── Factory ───────────────────────────────────────────────────────────────────

/// Subprocess environment variables passed through to Runner-spawned processes.
///
/// All other environment variables are cleared before subprocess spawn
/// (Security Guardrails LLM06 subprocess env isolation).
pub const SUBPROCESS_ENV_ALLOWLIST: &[&str] = &["PATH", "HOME", "TRACEPARENT", "LA_AGENT_ROLE"];

/// Select a [`Runner`] implementation for the given provider name.
///
/// # Errors
///
/// Returns [`RunnerError::UnknownRunner`] for unrecognised provider names.
/// **Never** falls back silently to a default runner — fail-closed is required
/// to prevent silent regression to hardcoded `claude` subprocess dispatch.
///
/// # Implementation note
///
/// Phase 4 wires `AnthropicHttpRunner`, `OllamaRunner`, `OpenAICompatRunner` arms.
pub fn select_runner(
    provider_name: &str,
    provider: Arc<dyn LlmAgentProvider>,
) -> Result<Arc<dyn Runner>, RunnerError> {
    match provider_name {
        "claude-cli" | "claude-code" => {
            let sandbox_root = std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".tmp");
            Ok(Arc::new(ClaudeCliRunner::new(provider, sandbox_root)))
        }
        other => Err(RunnerError::UnknownRunner(other.to_owned())),
    }
}

// ── Sandbox helper ────────────────────────────────────────────────────────────

/// Verify that `artifact_dir` is inside `sandbox_root` using §63.P4 ancestor-walk.
///
/// Walks to the nearest existing ancestor of `artifact_dir`, canonicalizes it,
/// then checks containment via `starts_with`. Never uses `is_symlink` after
/// canonicalize (CWE-22 / TOCTOU per Cookbook §63.P4).
fn sandbox_check(artifact_dir: &Path, sandbox_root: &Path) -> Result<PathBuf, RunnerError> {
    let root_canonical = sandbox_root
        .canonicalize()
        .unwrap_or_else(|_| sandbox_root.to_path_buf());

    // Iterative ancestor-walk: climb until we reach an existing directory,
    // canonicalize it, then rejoin the accumulated non-existent tail.
    // WHY: on macOS `/var` → `/private/var`; a single parent lookup is not
    // sufficient when the target's parent also doesn't exist (Cookbook §63.P4).
    let mut tail = PathBuf::new();
    let mut cursor: &Path = artifact_dir;
    let resolved = loop {
        if cursor.exists() {
            let canonical = cursor
                .canonicalize()
                .unwrap_or_else(|_| cursor.to_path_buf());
            break canonical.join(&tail);
        }
        if let Some(name) = cursor.file_name() {
            tail = PathBuf::from(name).join(&tail);
        }
        match cursor.parent() {
            Some(p) => cursor = p,
            None => {
                // Filesystem root with no existing ancestor — fail closed.
                return Err(RunnerError::SandboxViolation(artifact_dir.to_path_buf()));
            }
        }
    };

    if !resolved.starts_with(&root_canonical) {
        return Err(RunnerError::SandboxViolation(resolved));
    }
    Ok(resolved)
}

// ── ClaudeCliRunner ───────────────────────────────────────────────────────────

/// Runner wrapping `ClaudeCliProvider` (or any CLI-launched provider).
///
/// Adds dispatch-layer concerns above [`LlmAgentProvider`]: sandbox verification,
/// artifact persistence, and AYIN span emission.
///
/// # Security invariants (zero-exception)
///
/// - `artifact_dir` verified via `sandbox_check` (§63.P4) before any write.
/// - AYIN span metadata never includes credential values — fingerprint hash only.
/// - `estimate_cost` returns `0.0` — subscription-funded, no per-token billing.
pub struct ClaudeCliRunner {
    provider: Arc<dyn LlmAgentProvider>,
    /// Absolute path that all artifact directories must descend from.
    sandbox_root: PathBuf,
}

impl ClaudeCliRunner {
    /// Create a new runner wrapping `provider` with the given sandbox boundary.
    #[must_use]
    pub fn new(provider: Arc<dyn LlmAgentProvider>, sandbox_root: PathBuf) -> Self {
        Self {
            provider,
            sandbox_root,
        }
    }
}

#[async_trait]
impl Runner for ClaudeCliRunner {
    fn name(&self) -> &'static str {
        "claude_cli"
    }

    fn capabilities(&self) -> RunnerCapabilities {
        RunnerCapabilities {
            supported_roles: vec![], // empty = all roles
            max_parallelism: 7,
            tool_use: true,
            streaming: true,
        }
    }

    async fn run(&self, spec: AgentSpec) -> Result<AgentArtifact, RunnerError> {
        let artifact_dir = sandbox_check(&spec.artifact_dir, &self.sandbox_root)?;
        let agent_name = spec.agent_name.clone();
        let start = Instant::now();

        let response = self.provider.spawn(spec.task).await;

        // Emit AYIN span — metadata MUST NOT contain credentials (BLAKE3 hash only)
        let span_outcome = match &response {
            Ok(_) => crate::ayin::span::TraceOutcome::Continue,
            Err(e) => crate::ayin::span::TraceOutcome::error(e.to_string()),
        };
        let _ = crate::ayin::span::TraceContext::new(
            crate::ayin::span::Actor::claude(),
            "dispatch.agent",
        )
        .metadata(serde_json::json!({
            "agent_name": agent_name,
            "provider": "claude_cli",
        }))
        .outcome(span_outcome)
        .finish();

        let response = response?;
        let artifact_path = artifact_dir.join(format!("{agent_name}.md"));
        tokio::fs::write(&artifact_path, response.output.to_string().as_bytes())
            .await
            .map_err(|e| RunnerError::Io(e.to_string()))?;

        Ok(AgentArtifact {
            agent_name,
            artifact_path,
            tokens_input: response.tokens.input,
            tokens_output: response.tokens.output,
            stop_reason: StopReason::EndTurn,
            duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }

    async fn stream(&self, spec: AgentSpec) -> Result<BoxStream<'static, AgentEvent>, RunnerError> {
        let artifact_dir = sandbox_check(&spec.artifact_dir, &self.sandbox_root)?;
        let agent_name = spec.agent_name.clone();
        let provider = Arc::clone(&self.provider);

        // Propagate provider errors before spawning the background task
        let mut pstream = provider.spawn_streaming(spec.task).await?;

        let (tx, mut rx) = mpsc::channel::<AgentEvent>(64);
        tokio::spawn(async move {
            let start = Instant::now();
            let _ = tx
                .send(AgentEvent::AgentStart {
                    runner: "claude_cli".to_owned(),
                    agent_name: agent_name.clone(),
                })
                .await;

            let mut accumulated_text = String::new();
            let mut input_tokens: u32 = 0;
            let mut output_tokens: u32 = 0;

            loop {
                match pstream.next().await {
                    None | Some(ProviderEvent::MessageStop) => {
                        let artifact_path = artifact_dir.join(format!("{agent_name}.md"));
                        match tokio::fs::write(&artifact_path, accumulated_text.as_bytes()).await {
                            Ok(()) => {
                                let _ = tx
                                    .send(AgentEvent::AgentComplete {
                                        artifact: AgentArtifact {
                                            agent_name: agent_name.clone(),
                                            artifact_path,
                                            tokens_input: input_tokens,
                                            tokens_output: output_tokens,
                                            stop_reason: StopReason::EndTurn,
                                            duration_ms: u64::try_from(start.elapsed().as_millis())
                                                .unwrap_or(u64::MAX),
                                        },
                                    })
                                    .await;
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(AgentEvent::AgentError {
                                        error: format!("artifact write: {e}"),
                                    })
                                    .await;
                            }
                        }
                        break;
                    }
                    Some(ProviderEvent::MessageStart {
                        input_tokens: toks, ..
                    }) => {
                        input_tokens = toks;
                    }
                    Some(ProviderEvent::TextDelta { text, .. }) => {
                        accumulated_text.push_str(&text);
                        let _ = tx.send(AgentEvent::TextDelta { text }).await;
                    }
                    Some(ProviderEvent::MessageDelta {
                        output_tokens: toks,
                        ..
                    }) => {
                        output_tokens = toks;
                    }
                    Some(_) => {} // ContentBlockStart, ContentBlockStop, ToolResult, etc.
                }
            }
        });

        let stream = futures_util::stream::poll_fn(move |cx| rx.poll_recv(cx));
        Ok(Box::pin(stream))
    }

    fn estimate_cost(&self, _input_tokens: u32, _max_output_tokens: u32) -> f64 {
        // Subscription-funded — no per-token USD cost
        0.0
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::large_stack_arrays
)]
mod tests {
    use super::*;
    use crate::agent::{
        AgentRequest, AgentResponse, ProviderCapabilities, ProviderEvent, SchemaMode, TokenUsage,
    };
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::HashMap;

    fn test_req() -> SanitizedAgentRequest {
        AgentRequest {
            sibling_identity: "test".to_owned(),
            user_prompt: "write a stub artifact".to_owned(),
            schema: None,
            allowed_tools: vec![],
            max_turns: 1,
            max_budget_usd: 1.0,
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

    // ── StubProvider ──────────────────────────────────────────────────────────

    struct StubProvider {
        events: Vec<ProviderEvent>,
    }

    impl StubProvider {
        fn returning(text: &str) -> Self {
            Self {
                events: vec![
                    ProviderEvent::MessageStart {
                        model: "stub".to_owned(),
                        input_tokens: 4,
                    },
                    ProviderEvent::ContentBlockStart {
                        index: 0,
                        block_type: "text".to_owned(),
                        tool_use_id: None,
                        tool_name: None,
                    },
                    ProviderEvent::TextDelta {
                        index: 0,
                        text: text.to_owned(),
                    },
                    ProviderEvent::ContentBlockStop { index: 0 },
                    ProviderEvent::MessageDelta {
                        stop_reason: "end_turn".to_owned(),
                        output_tokens: 8,
                    },
                    ProviderEvent::MessageStop,
                ],
            }
        }
    }

    #[async_trait]
    impl LlmAgentProvider for StubProvider {
        fn name(&self) -> &'static str {
            "stub"
        }

        async fn spawn(&self, _req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
            let text = self
                .events
                .iter()
                .find_map(|e| {
                    if let ProviderEvent::TextDelta { text, .. } = e {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            Ok(AgentResponse {
                output: json!(text),
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
                auth_inherits_session: false,
            }
        }

        async fn spawn_streaming(
            &self,
            _req: SanitizedAgentRequest,
        ) -> Result<BoxStream<'static, ProviderEvent>, ProviderError> {
            let events = self.events.clone();
            Ok(Box::pin(futures_util::stream::iter(events)))
        }

        fn estimate_cost(&self, _i: u32, _o: u32) -> f64 {
            0.0
        }
    }

    // ── sandbox_check ─────────────────────────────────────────────────────────

    #[test]
    fn sandbox_check_passes_for_child_dir() {
        let tmp = std::env::temp_dir();
        let child = tmp.join("dispatch-abc").join("agent.md");
        // Parent of child doesn't exist yet — ancestor-walk falls back to tmp
        let result = sandbox_check(&child, &tmp);
        assert!(result.is_ok(), "child of sandbox root must pass");
    }

    #[test]
    fn sandbox_check_rejects_path_traversal() {
        let tmp = std::env::temp_dir().join("sandbox-root");
        let escape = tmp.join("..").join("etc").join("passwd");
        let result = sandbox_check(&escape, &tmp);
        assert!(
            matches!(result, Err(RunnerError::SandboxViolation(_))),
            "path traversal must be rejected"
        );
    }

    // ── ClaudeCliRunner ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn run_writes_artifact_and_returns_end_turn() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let sandbox = tmp.path().to_path_buf();
        let artifact_dir = sandbox.join("dispatch-test");
        tokio::fs::create_dir_all(&artifact_dir)
            .await
            .expect("mkdir");

        let provider: Arc<dyn LlmAgentProvider> =
            Arc::new(StubProvider::returning("## Analysis\nLooks good."));
        let runner = ClaudeCliRunner::new(provider, sandbox.clone());

        let spec = AgentSpec {
            agent_name: "engineer".to_owned(),
            agent_role: AgentRole::Engineer,
            task: test_req(),
            artifact_dir: artifact_dir.clone(),
            parent_span_id: None,
            input_tokens_estimate: 100,
            max_output_tokens: 2048,
            file_ownership: vec![],
        };

        let artifact = runner.run(spec).await.expect("run succeeds");
        assert_eq!(artifact.stop_reason, StopReason::EndTurn);
        assert_eq!(artifact.tokens_input, 4);
        assert_eq!(artifact.tokens_output, 8);
        assert!(
            artifact.artifact_path.exists(),
            "artifact file written to disk"
        );
    }

    #[tokio::test]
    async fn run_rejects_sandbox_escape() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let sandbox = tmp.path().join("inner");
        let escape_dir = tmp.path().join("..").join("escape");

        let provider: Arc<dyn LlmAgentProvider> = Arc::new(StubProvider::returning("x"));
        let runner = ClaudeCliRunner::new(provider, sandbox);

        let spec = AgentSpec {
            agent_name: "engineer".to_owned(),
            agent_role: AgentRole::Engineer,
            task: test_req(),
            artifact_dir: escape_dir,
            parent_span_id: None,
            input_tokens_estimate: 10,
            max_output_tokens: 64,
            file_ownership: vec![],
        };

        let err = runner.run(spec).await.expect_err("must reject escape");
        assert!(
            matches!(err, RunnerError::SandboxViolation(_)),
            "unexpected: {err:?}"
        );
    }

    #[tokio::test]
    async fn stream_emits_start_delta_complete_sequence() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let sandbox = tmp.path().to_path_buf();
        let artifact_dir = sandbox.join("dispatch-stream");
        tokio::fs::create_dir_all(&artifact_dir)
            .await
            .expect("mkdir");

        let provider: Arc<dyn LlmAgentProvider> =
            Arc::new(StubProvider::returning("streaming output"));
        let runner = ClaudeCliRunner::new(provider, sandbox);

        let spec = AgentSpec {
            agent_name: "reviewer".to_owned(),
            agent_role: AgentRole::Quality,
            task: test_req(),
            artifact_dir,
            parent_span_id: None,
            input_tokens_estimate: 50,
            max_output_tokens: 512,
            file_ownership: vec![],
        };

        let mut stream = runner.stream(spec).await.expect("stream starts");
        let mut events: Vec<AgentEvent> = Vec::new();
        while let Some(e) = stream.next().await {
            events.push(e);
        }

        assert!(
            matches!(events.first(), Some(AgentEvent::AgentStart { runner, .. }) if runner == "claude_cli"),
            "first event must be AgentStart"
        );
        assert!(
            matches!(events.last(), Some(AgentEvent::AgentComplete { .. })),
            "last event must be AgentComplete"
        );
    }

    // ── select_runner ─────────────────────────────────────────────────────────

    #[test]
    fn select_runner_returns_claude_cli_runner() {
        let provider: Arc<dyn LlmAgentProvider> = Arc::new(StubProvider::returning(""));
        let runner = select_runner("claude-cli", provider.clone()).expect("claude-cli known");
        assert_eq!(runner.name(), "claude_cli");

        let runner2 = select_runner("claude-code", provider).expect("claude-code known");
        assert_eq!(runner2.name(), "claude_cli");
    }

    #[test]
    fn select_runner_fails_closed_on_unknown() {
        let provider: Arc<dyn LlmAgentProvider> = Arc::new(StubProvider::returning(""));
        let result = select_runner("ollama", provider);
        assert!(
            matches!(result, Err(RunnerError::UnknownRunner(ref s)) if s == "ollama"),
            "expected UnknownRunner(ollama)"
        );
    }
}
