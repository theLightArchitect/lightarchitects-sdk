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

#[cfg(feature = "agent-cli")]
mod claude;
#[cfg(feature = "agent-cli")]
pub use claude::ClaudeCliProvider;
