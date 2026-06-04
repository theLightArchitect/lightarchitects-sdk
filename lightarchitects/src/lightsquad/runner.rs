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
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::stream::BoxStream;

use crate::agent::{LlmAgentProvider, ProviderError, SanitizedAgentRequest};
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
/// Full match arms are wired in Phase 4. Phase 3 adds `ClaudeCliRunner` only.
pub fn select_runner(
    provider_name: &str,
    _provider: Arc<dyn LlmAgentProvider>,
) -> Result<Arc<dyn Runner>, RunnerError> {
    // Phase 3 fills ClaudeCliRunner arm.
    // Phase 4 fills AnthropicHttpRunner, OllamaRunner, OpenAICompatRunner arms.
    let _ = provider_name;
    Err(RunnerError::UnknownRunner(provider_name.to_owned()))
}
