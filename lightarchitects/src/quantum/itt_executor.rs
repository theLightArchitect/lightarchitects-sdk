//! [`IttExecutor`] implementation for the QUANTUM sibling.
//!
//! `QuantumIttExecutor` drives ITT tree exploration via QUANTUM actions:
//!
//! | ITT operation       | QUANTUM action | Rationale |
//! |---------------------|---------------|-----------|
//! | `collect_evidence`  | `sweep`       | Sweep collects cross-signal evidence for a subject |
//! | `verify_hypothesis` | `verify`      | Verify tests a specific hypothesis against evidence |
//! | `expand`            | `probe`       | Probe deep-dives into a target to surface child hypotheses |
//!
//! # Gating
//!
//! This module is only compiled when `features = ["loops-core"]`.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    agent::loops::{
        error::LoopError,
        itt::{EvidenceRef, IttExecutor, QPhase, VerificationResult},
        runner::StepContext,
    },
    core::transport::Transport,
};

use super::QuantumClient;

/// [`IttExecutor`] that delegates each ITT operation to the QUANTUM MCP server.
pub struct QuantumIttExecutor<T: Transport> {
    client: Arc<QuantumClient<T>>,
}

impl<T: Transport> QuantumIttExecutor<T> {
    /// Wrap an existing `QuantumClient` for shared use across ITT loop steps.
    #[must_use]
    pub fn new(client: Arc<QuantumClient<T>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> IttExecutor for QuantumIttExecutor<T> {
    /// Expand a node by calling QUANTUM `probe` and parsing child hypotheses from the output.
    async fn expand(
        &self,
        _node_id: &str,
        hypothesis: &str,
        _ctx: &StepContext,
    ) -> Result<Vec<(String, f64)>, LoopError> {
        let r = self
            .client
            .probe(hypothesis)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        let children = r
            .output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| (line.to_owned(), 0.5_f64))
            .collect();
        Ok(children)
    }

    /// Collect evidence by calling QUANTUM `sweep` for the hypothesis.
    async fn collect_evidence(
        &self,
        node_id: &str,
        hypothesis: &str,
        _ctx: &StepContext,
    ) -> Result<Vec<EvidenceRef>, LoopError> {
        let r = self
            .client
            .sweep(hypothesis)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        let evidence = vec![EvidenceRef {
            id: format!("{node_id}-sweep"),
            path: "quantum://sweep".to_owned(),
            description: r.output,
            collected_by: QPhase::Sweep,
        }];
        Ok(evidence)
    }

    /// Verify a hypothesis by calling QUANTUM `verify`.
    async fn verify_hypothesis(
        &self,
        node_id: &str,
        hypothesis: &str,
        _ctx: &StepContext,
    ) -> Result<VerificationResult, LoopError> {
        let r = self
            .client
            .verify(hypothesis)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        let verdict = r.output.to_lowercase();
        let confirmed = verdict.contains("confirmed") || verdict.contains("verified");
        let confidence = if confirmed { 0.8 } else { 0.3 };
        Ok(VerificationResult {
            confirmed,
            evidence_ids: vec![format!("{node_id}-sweep")],
            confidence,
            conclusion: r.output,
        })
    }
}
