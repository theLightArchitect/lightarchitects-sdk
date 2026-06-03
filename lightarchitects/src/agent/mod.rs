//! LLM agent provider infrastructure.
//!
//! Provides the [`LlmAgentProvider`] trait, request/response types, and the
//! consolidated [`dispatch_action`] helper for inline sibling handlers.
//!
//! With the `agent-cli` feature (included in `default`), also provides
//! [`ClaudeCliProvider`] (spawns `claude -p` subprocesses) and the G1
//! sanitization function [`sanitize_params`].

mod provider;
pub use provider::{
    AgentRequest, AgentResponse, LlmAgentProvider, MAX_CHAIN_DEPTH, MAX_PARAM_BYTES,
    ProviderCapabilities, ProviderError, ProviderEvent, SanitizedAgentRequest, SchemaMode,
    TokenUsage, sanitize_params,
};

/// Canonical [`OpenAIFlavor`] enum — shared by the agentic-loop HTTP provider
/// ([`openai_compat`]) and the helix generation completers.  Not feature-gated
/// so it is available to all crate consumers regardless of feature set.
///
/// [`openai_compat`]: crate::agent::openai_compat
pub mod openai_flavor;
pub use openai_flavor::OpenAIFlavor;

mod dispatch;
pub use dispatch::{ChainContext, dispatch_action};

pub mod cloud_models;
pub use cloud_models::{CLOUD_MODEL_REGISTRY, CloudModel, CostTier};

pub mod error;
pub use error::OllamaError;

/// Tool execution surface — [`ToolExecutor`] trait + [`NullToolExecutor`] fail-closed default (TS-2 §6.1.2).
///
/// [`ToolExecutor`]: tool_executor::ToolExecutor
/// [`NullToolExecutor`]: tool_executor::NullToolExecutor
pub mod tool_executor;
pub use tool_executor::{NullToolExecutor, ToolDefinition, ToolError, ToolExecutor, ToolOutput};

/// Shared LLM stream parsers — framing, SSE (Anthropic/Ollama), NDJSON (Claude CLI).
///
/// All three sub-modules emit [`ProviderEvent`] so provider implementations
/// share a single parsing path (TS-3 §21.3).
pub mod messages_stream_parser;

/// L1 agentic loop substrate — [`Strategy`], [`LoopRunner`], combinators,
/// `CritiqueRefine`. Provider-agnostic; enabled by the `loops-core` feature.
///
/// [`Strategy`]: loops::Strategy
/// [`LoopRunner`]: loops::LoopRunner
#[cfg(feature = "loops-core")]
pub mod loops;

/// Indirect prompt injection defence — sentinel wrapping and pattern detection for
/// tool results (B2 security fold, OWASP-LLM01-1.3, MITRE-ATLAS AML.T0051).
pub mod indirect_injection_shield;
pub use indirect_injection_shield::{
    DetectedPattern, HitlReason, IndirectInjectionShield, InjectionSeverity,
};

/// Bash command-pattern policy — allowlist + denylist for LLM-driven bash tool calls
/// (B3 security fold, Cookbook §63, OWASP-LLM02).
pub mod bash_policy;
pub use bash_policy::{BashPolicy, BashPolicyDecision};

/// Stateless gate-evaluator substrate (`[A+S+Q+C+O+P+K+D+T+R]` LASDLC dimensions).
/// Gatekeepers are pure-function evaluators: `(draft, criteria) → Verdict`.
/// Memory lives in canon + helix; criteria are assembled at call time; the
/// gatekeeper instance carries no mutable state. Canon XXXIII independent
/// verification by construction. See module docs for the full contract.
#[cfg(feature = "gatekeepers")]
pub mod gatekeeper;
#[cfg(feature = "gatekeepers")]
pub use gatekeeper::{
    BaselineRef, CanonRef, Citation, Criteria, Draft, DraftKind, GateDimension, GateError,
    Gatekeeper, HelixSnapshotId, PlanRef, PrecedentRef, QualityGatekeeper, Severity, Verdict,
    VerdictStatus,
};

/// L2 conversation session — structured turn management, memory, transport.
///
/// Promotes the gateway `AgentRunner` pattern into the SDK. Enabled by the
/// `loops-core` feature (same gate as [`loops`]).
///
/// [`loops`]: crate::agent::loops
#[cfg(feature = "loops-core")]
pub mod conversation;

/// Session lifecycle hooks — pre/post turn and pre/post tool callbacks.
///
/// Enabled alongside [`conversation`] by the `loops-core` feature.
///
/// [`conversation`]: crate::agent::conversation
#[cfg(feature = "loops-core")]
pub mod hooks;

/// L3 orchestration — [`WorkerPool`] (bounded concurrency) and
/// [`Supervisor`] (circuit-breaker). Lifted from `lightsquad::wave_dispatcher`.
///
/// [`WorkerPool`]: orchestration::WorkerPool
/// [`Supervisor`]: orchestration::Supervisor
#[cfg(feature = "loops-core")]
pub mod orchestration;

/// L0 HTTP providers — [`AnthropicHttpProvider`] and [`VertexHttpProvider`].
///
/// Direct API callers without subprocess delegation. Keychain-only key
/// resolution in release builds (SERAPH OA-12).
///
/// [`AnthropicHttpProvider`]: http::AnthropicHttpProvider
/// [`VertexHttpProvider`]: http::VertexHttpProvider
#[cfg(feature = "loops-core")]
pub mod http;

#[cfg(feature = "agent-cli")]
mod claude;
#[cfg(feature = "agent-cli")]
pub use claude::ClaudeCliProvider;

#[cfg(feature = "agent-cli")]
pub mod permissions;
#[cfg(feature = "agent-cli")]
pub use permissions::{CostGate, PermissionMatrix};

#[cfg(feature = "agent-cli")]
mod ollama;
#[cfg(feature = "agent-cli")]
pub use ollama::OllamaCliProvider;

#[cfg(feature = "agent-cli")]
pub mod translator;
#[cfg(feature = "agent-cli")]
pub use translator::sanitize_prompt;

#[cfg(feature = "agent-cli")]
pub mod adk_supervisor;
#[cfg(feature = "agent-cli")]
pub use adk_supervisor::{
    AdkVersion, RestartTracker, SupervisorError, allocate_ephemeral_port, probe,
};

/// OpenAI-compatible HTTP streaming provider — targets any endpoint speaking
/// the OpenAI `/chat/completions` SSE wire format (RunPod vLLM, Together AI,
/// Fireworks, self-hosted vLLM).  Reads credentials from env vars:
/// `LA_OPENAI_COMPAT_BASE_URL`, `LA_OPENAI_COMPAT_API_KEY`,
/// `LA_OPENAI_COMPAT_MODEL`.
///
/// [`OpenAICompatProvider`]: openai_compat::OpenAICompatProvider
#[cfg(feature = "loops-core")]
pub mod openai_compat;
#[cfg(feature = "loops-core")]
pub use openai_compat::OpenAICompatProvider;

/// Ollama Cloud coding provider — executes autonomous ironclaw tasks via
/// structured LLM output + 4-gate security validation before worktree writes.
///
/// Gated on `lightsquad` (not `agent-cli`) because it imports
/// [`crate::lightsquad::ollama_response_validator`].
#[cfg(feature = "lightsquad")]
pub mod ollama_cloud_provider;
#[cfg(feature = "lightsquad")]
pub use ollama_cloud_provider::{
    CodingProviderError, DEFAULT_CODING_MODEL, OLLAMA_TASK_TIMEOUT_DEFAULT_S,
    OllamaCloudCodingProvider, TaskOutcome,
};

/// Skill utilities — SKILL.md parsing, agentskills.io export, registry helpers.
pub mod skills;
