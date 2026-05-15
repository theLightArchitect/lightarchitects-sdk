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
    AgentRequest, AgentResponse, LlmAgentProvider, ProviderCapabilities, ProviderError, SchemaMode,
    TokenUsage,
};

mod dispatch;
pub use dispatch::dispatch_action;

#[cfg(feature = "agent-cli")]
mod claude;
#[cfg(feature = "agent-cli")]
pub use claude::{ClaudeCliProvider, MAX_PARAM_BYTES, sanitize_params};
