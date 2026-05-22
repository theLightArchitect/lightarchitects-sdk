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
    ProviderCapabilities, ProviderError, SanitizedAgentRequest, SchemaMode, TokenUsage,
    sanitize_params,
};

mod dispatch;
pub use dispatch::{ChainContext, dispatch_action};

pub mod cloud_models;
pub use cloud_models::{CLOUD_MODEL_REGISTRY, CloudModel, CostTier};

pub mod error;
pub use error::OllamaError;

/// L1 agentic loop substrate — [`Strategy`], [`LoopRunner`], combinators,
/// `CritiqueRefine`. Provider-agnostic; enabled by the `loops-core` feature.
///
/// [`Strategy`]: loops::Strategy
/// [`LoopRunner`]: loops::LoopRunner
#[cfg(feature = "loops-core")]
pub mod loops;

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
