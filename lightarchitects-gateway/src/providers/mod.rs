//! LLM provider implementations for the agentic loop (vibe-coding-loop Phase 2).
//!
//! Each provider implements [`lightarchitects::agent::LlmAgentProvider`] so the
//! agentic loop can swap backends without touching call sites.
//!
//! All sibling handlers and skill dispatch route through [`litellm::build_provider`]
//! so the active backend is controlled entirely by `LA_LITELLM_*` env vars.

pub mod anthropic;
pub mod litellm;
pub mod tool_executor;

pub use anthropic::AnthropicHttpProvider;
pub use litellm::build_provider as build_litellm_provider;
pub use tool_executor::GatewayToolExecutor;
