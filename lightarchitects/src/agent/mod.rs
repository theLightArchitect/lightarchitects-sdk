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
