//! [`IttExecutor`] implementation for the SERAPH sibling.
//!
//! `SeraphIttExecutor` drives ITT security investigation via SERAPH's typed
//! pentest operations:
//!
//! | ITT operation       | SERAPH operation         | Rationale |
//! |---------------------|--------------------------|-----------|
//! | `expand`            | `start_investigation` / `advance_investigation` | Opens new investigation branches per hypothesis |
//! | `collect_evidence`  | `capture` + `scan`       | Passive evidence collection (capture artifacts, active scan) |
//! | `verify_hypothesis` | `examine`                | Deep structural analysis of a target to confirm/refute |
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

use super::SeraphClient;

/// [`IttExecutor`] that delegates each ITT operation to the SERAPH MCP server.
pub struct SeraphIttExecutor<T: Transport> {
    client: Arc<SeraphClient<T>>,
}

impl<T: Transport> SeraphIttExecutor<T> {
    /// Wrap an existing `SeraphClient` for shared use across ITT loop steps.
    #[must_use]
    pub fn new(client: Arc<SeraphClient<T>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> IttExecutor for SeraphIttExecutor<T> {
    /// Expand a node by starting or advancing a SERAPH investigation to surface child targets.
    async fn expand(
        &self,
        node_id: &str,
        hypothesis: &str,
        _ctx: &StepContext,
    ) -> Result<Vec<(String, f64)>, LoopError> {
        let r = self
            .client
            .advance_investigation(hypothesis)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        let _ = node_id;
        let children = r
            .output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| (line.to_owned(), 0.5_f64))
            .collect();
        Ok(children)
    }

    /// Collect evidence by running SERAPH `capture` and `scan` on the hypothesis target.
    async fn collect_evidence(
        &self,
        node_id: &str,
        hypothesis: &str,
        _ctx: &StepContext,
    ) -> Result<Vec<EvidenceRef>, LoopError> {
        let capture = self
            .client
            .capture(hypothesis)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        let scan = self
            .client
            .scan(hypothesis)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(vec![
            EvidenceRef {
                id: format!("{node_id}-capture"),
                path: format!("seraph://capture/{hypothesis}"),
                description: capture.output,
                collected_by: QPhase::Probe,
            },
            EvidenceRef {
                id: format!("{node_id}-scan"),
                path: format!("seraph://scan/{hypothesis}"),
                description: scan.output,
                collected_by: QPhase::Scan,
            },
        ])
    }

    /// Verify a hypothesis by calling SERAPH `examine` on the target.
    async fn verify_hypothesis(
        &self,
        node_id: &str,
        hypothesis: &str,
        _ctx: &StepContext,
    ) -> Result<VerificationResult, LoopError> {
        let r = self
            .client
            .examine(hypothesis)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        let output_lower = r.output.to_lowercase();
        let confirmed = output_lower.contains("confirmed")
            || output_lower.contains("vulnerable")
            || output_lower.contains("finding");
        let confidence = if confirmed { 0.8 } else { 0.3 };
        Ok(VerificationResult {
            confirmed,
            evidence_ids: vec![format!("{node_id}-capture"), format!("{node_id}-scan")],
            confidence,
            conclusion: r.output,
        })
    }
}
