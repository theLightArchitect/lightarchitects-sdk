//! LLM provider implementations for the agentic loop (vibe-coding-loop Phase 2).
//!
//! Each provider implements [`lightarchitects::agent::LlmAgentProvider`] so the
//! agentic loop can swap backends without touching call sites.

pub mod anthropic;
pub mod tool_executor;

pub use anthropic::AnthropicHttpProvider;
pub use tool_executor::GatewayToolExecutor;
