//! Error types for the agentic loop runtime.

use thiserror::Error;

use crate::agent::provider::ProviderError;

/// Errors that can occur during a strategy loop execution.
#[derive(Debug, Error)]
pub enum LoopError {
    /// The loop consumed more turns or budget than the configured [`Budget`] permits.
    ///
    /// [`Budget`]: crate::agent::loops::budget::Budget
    #[error("budget exceeded after {used_turns} turns (${used_usd:.4} USD)")]
    BudgetExceeded {
        /// Number of turns consumed before the budget was hit.
        used_turns: u32,
        /// USD cost accumulated before the budget was hit.
        used_usd: f64,
    },

    /// A [`ChainContext::child()`] call would have pushed the chain depth past
    /// [`MAX_CHAIN_DEPTH`] (Canon §2.6).
    ///
    /// [`ChainContext::child()`]: crate::agent::ChainContext::child
    /// [`MAX_CHAIN_DEPTH`]: crate::agent::MAX_CHAIN_DEPTH
    #[error(
        "chain depth exceeded at hop {depth} (max {})",
        crate::agent::MAX_CHAIN_DEPTH
    )]
    ChainDepthExceeded {
        /// Depth value that triggered the violation.
        depth: u8,
    },

    /// The underlying [`LlmAgentProvider`] returned an error.
    ///
    /// [`LlmAgentProvider`]: crate::agent::LlmAgentProvider
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),

    /// A strategy step returned a non-recoverable domain error.
    #[error("strategy step failed: {0}")]
    StepFailed(String),
}
