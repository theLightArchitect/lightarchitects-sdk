//! [`CritiqueExecutor`] implementation for the QUANTUM sibling.
//!
//! `QuantumCritiqueExecutor` bridges the SDK's `CritiqueRefineStrategy` loop
//! to QUANTUM's MCP transport. The loop orchestration runs in the SDK/gateway
//! process; QUANTUM handles individual step execution.
//!
//! Each step maps to a single QUANTUM action:
//!
//! | Loop phase | QUANTUM action | Notes |
//! |------------|---------------|-------|
//! | `theorize` | `theorize` | Generates critique notes from the current draft |
//! | `verify` | `verify` | Refines the draft against critique notes |
//! | `close` | — | Pass-through; synthesis complete by this point |
//!
//! # Gating
//!
//! This module is only compiled when `features = ["loops-core"]`. The strategy
//! itself and the `CritiqueExecutor` trait live in `lightarchitects::agent::loops`.
//!
//! # Example
//!
//! ```no_run
//! use std::sync::Arc;
//! use lightarchitects::agent::{ChainContext, loops::{Budget, critique_refine::{CritiqueRefineStrategy, CritiqueState}, runner::{LoopRunner, Outcome}}};
//! use lightarchitects::quantum::{QuantumClient, critique_executor::QuantumCritiqueExecutor};
//! use futures_util::StreamExt as _;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = Arc::new(QuantumClient::builder().build()?);
//! let executor = QuantumCritiqueExecutor::new(Arc::clone(&client));
//! let strategy = CritiqueRefineStrategy::new(executor, 2);
//! let runner = LoopRunner::new(strategy, Budget::unlimited());
//! let mut stream = runner.run(CritiqueState::new("auth token refresh failures"), ChainContext::default(), None);
//!
//! let mut final_output = String::new();
//! while let Some(step) = stream.next().await {
//!     let s = step?;
//!     if let Outcome::Halt(out) = s.outcome { final_output = out; }
//! }
//! println!("{final_output}");
//! # Ok(()) }
//! ```

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    agent::loops::{critique_refine::CritiqueExecutor, error::LoopError, runner::StepContext},
    core::transport::Transport,
};

use super::QuantumClient;

/// [`CritiqueExecutor`] that delegates each loop phase to the QUANTUM MCP server.
///
/// The loop itself runs in the SDK/gateway process. QUANTUM provides the
/// per-step domain intelligence via its `theorize` and `verify` actions.
pub struct QuantumCritiqueExecutor<T: Transport> {
    client: Arc<QuantumClient<T>>,
}

impl<T: Transport> QuantumCritiqueExecutor<T> {
    /// Wrap an existing `QuantumClient` in an `Arc` for shared use across loop steps.
    #[must_use]
    pub fn new(client: Arc<QuantumClient<T>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> CritiqueExecutor for QuantumCritiqueExecutor<T> {
    /// Phase 1 — calls QUANTUM `theorize` with the current draft and returns
    /// the result as a single-element critique note list.
    async fn theorize(
        &self,
        draft: &str,
        _ctx: &StepContext,
    ) -> std::result::Result<Vec<String>, LoopError> {
        let result = self
            .client
            .theorize(draft, None)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(vec![result.output])
    }

    /// Phase 2 — calls QUANTUM `verify` with the critique-annotated draft.
    ///
    /// Critique notes are prepended to the draft so QUANTUM's verifier sees
    /// the full context in a single prompt.
    async fn verify(
        &self,
        draft: &str,
        critiques: &[String],
        _ctx: &StepContext,
    ) -> std::result::Result<String, LoopError> {
        let annotated = if critiques.is_empty() {
            draft.to_owned()
        } else {
            format!("{draft}\n\nCritique notes:\n{}", critiques.join("\n"))
        };
        let result = self
            .client
            .verify(&annotated)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(result.output)
    }

    /// Phase 3 — synthesis is complete; return the draft unchanged.
    async fn close(
        &self,
        draft: &str,
        _ctx: &StepContext,
    ) -> std::result::Result<String, LoopError> {
        Ok(draft.to_owned())
    }
}
