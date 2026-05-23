//! [`ReActExecutor`] implementation for the QUANTUM sibling.
//!
//! `QuantumReActExecutor` maps each [`ReActPhase`] to the semantically
//! equivalent QUANTUM action, driving a full investigation lifecycle via the
//! `ReAct` loop without any loop logic inside the QUANTUM binary.
//!
//! | [`ReActPhase`] | QUANTUM action | Notes |
//! |---------------|---------------|-------|
//! | `Scan`        | `triage`      | Passive baseline collection |
//! | `Sweep`       | `sweep`       | Active evidence sweep |
//! | `Trace`       | `trace`       | Pattern matching across artifacts |
//! | `Probe`       | `probe`       | Targeted queries and config dumps |
//! | `Theorize`    | `theorize`    | Hypothesis generation |
//! | `Verify`      | `verify`      | Hypothesis verification |
//! | `Close`       | `close`       | Record conclusions |
//!
//! # Gating
//!
//! This module is only compiled when `features = ["loops-core"]`.
//!
//! # Example
//!
//! ```no_run
//! use std::sync::Arc;
//! use lightarchitects::agent::{ChainContext, loops::{Budget, runner::{LoopRunner, Outcome}}};
//! use lightarchitects::agent::loops::react::{ReActPrompt, ReActStrategy};
//! use lightarchitects::quantum::{QuantumClient, react_executor::QuantumReActExecutor};
//! use futures_util::StreamExt as _;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = Arc::new(QuantumClient::builder().build()?);
//! let executor = QuantumReActExecutor::new(Arc::clone(&client));
//! let strategy = ReActStrategy::new(executor);
//! let runner = LoopRunner::new(strategy, Budget::unlimited());
//! let mut stream = runner.run(ReActPrompt::new("auth token refresh failures", 14), ChainContext::default(), None);
//!
//! while let Some(step) = stream.next().await {
//!     let s = step?;
//!     if let Outcome::Halt(final_prompt) = s.outcome {
//!         println!("{} steps completed", final_prompt.steps.len());
//!     }
//! }
//! # Ok(()) }
//! ```

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    agent::loops::{
        error::LoopError,
        react::{ReActExecutor, ReActPhase, ReActPrompt, ReActStep},
        runner::StepContext,
    },
    core::transport::Transport,
};

use super::QuantumClient;

/// [`ReActExecutor`] that delegates each investigation phase to the QUANTUM MCP server.
///
/// Each [`ReActPhase`] maps to the semantically equivalent QUANTUM action. The
/// loop orchestration runs in the SDK/gateway process; QUANTUM provides the
/// per-step domain intelligence.
pub struct QuantumReActExecutor<T: Transport> {
    client: Arc<QuantumClient<T>>,
}

impl<T: Transport> QuantumReActExecutor<T> {
    /// Wrap an existing `QuantumClient` in an `Arc` for shared use across loop steps.
    #[must_use]
    pub fn new(client: Arc<QuantumClient<T>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> ReActExecutor for QuantumReActExecutor<T> {
    /// Dispatch the current phase's QUANTUM action and return a [`ReActStep`].
    async fn step(&self, prompt: &ReActPrompt, _ctx: &StepContext) -> Result<ReActStep, LoopError> {
        let query = prompt.to_prompt_text();
        let (observation, action_label) = match prompt.phase {
            ReActPhase::Scan => {
                let r = self
                    .client
                    .triage(&query)
                    .await
                    .map_err(|e| LoopError::StepFailed(e.to_string()))?;
                (r.output, "triage")
            }
            ReActPhase::Sweep => {
                let r = self
                    .client
                    .sweep(&query)
                    .await
                    .map_err(|e| LoopError::StepFailed(e.to_string()))?;
                (r.output, "sweep")
            }
            ReActPhase::Trace => {
                let r = self
                    .client
                    .trace(&query)
                    .await
                    .map_err(|e| LoopError::StepFailed(e.to_string()))?;
                (r.output, "trace")
            }
            ReActPhase::Probe => {
                let r = self
                    .client
                    .probe(&query)
                    .await
                    .map_err(|e| LoopError::StepFailed(e.to_string()))?;
                (r.output, "probe")
            }
            ReActPhase::Theorize => {
                let r = self
                    .client
                    .theorize(&query, None)
                    .await
                    .map_err(|e| LoopError::StepFailed(e.to_string()))?;
                (r.output, "theorize")
            }
            ReActPhase::Verify => {
                let r = self
                    .client
                    .verify(&query)
                    .await
                    .map_err(|e| LoopError::StepFailed(e.to_string()))?;
                (r.output, "verify")
            }
            ReActPhase::Close => {
                let r = self
                    .client
                    .close("investigation complete")
                    .await
                    .map_err(|e| LoopError::StepFailed(e.to_string()))?;
                (r.output, "close")
            }
        };

        Ok(ReActStep {
            observation: observation.clone(),
            thought: format!("QUANTUM {action_label} analysis complete"),
            action: action_label.to_owned(),
            result: Some(observation),
            phase: prompt.phase,
        })
    }
}
