//! [`BcraExecutor`] implementation for the SERAPH sibling.
//!
//! `SeraphBcraExecutor` drives the BCRA risk-analysis loop using SERAPH's
//! reconnaissance, intelligence, and reporting operations:
//!
//! | BCRA phase   | SERAPH operation          | Rationale |
//! |--------------|--------------------------|-----------|
//! | `map`        | `survey("*")`            | Survey surfaces the full asset inventory |
//! | `pull`       | `osint(assets, None)`    | OSINT enriches assets with threat intelligence |
//! | `score`      | `analyze(threats)`       | Analyse derives a normalised blast score |
//! | `research`   | `advance_investigation`  | Deepen evidence for high-score threats |
//! | `prove`      | `examine(evidence)`      | Examine validates or refutes each evidence entry |
//! | `declare`    | `typed_report()`         | Report produces the final risk declaration |
//!
//! # Gating
//!
//! This module is only compiled when `features = ["loops-core"]`.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    agent::loops::{
        bcra::{BcraExecutor, BcraState},
        error::LoopError,
        runner::StepContext,
    },
    core::transport::Transport,
};

use super::SeraphClient;

/// [`BcraExecutor`] that delegates each BCRA phase to the SERAPH MCP server.
pub struct SeraphBcraExecutor<T: Transport> {
    client: Arc<SeraphClient<T>>,
}

impl<T: Transport> SeraphBcraExecutor<T> {
    /// Wrap an existing `SeraphClient` for shared use across BCRA loop phases.
    #[must_use]
    pub fn new(client: Arc<SeraphClient<T>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> BcraExecutor for SeraphBcraExecutor<T> {
    /// Phase 0: call SERAPH `survey("*")` to enumerate the asset inventory.
    async fn map(&self, _ctx: &StepContext) -> Result<Vec<String>, LoopError> {
        let r = self
            .client
            .survey("*")
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(non_empty_lines(&r.output))
    }

    /// Phase 1: call SERAPH `osint` on the joined asset list to pull threat intel.
    async fn pull(&self, assets: &[String], _ctx: &StepContext) -> Result<Vec<String>, LoopError> {
        let target = assets.join(", ");
        let r = self
            .client
            .osint(&target, None)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(non_empty_lines(&r.output))
    }

    /// Phase 2: call SERAPH `analyze` and derive a normalised blast score `[0.0, 1.0]`.
    ///
    /// Scoring uses a keyword-weight table derived from FAIR/Bowtie severity labels.
    async fn score(&self, threats: &[String], _ctx: &StepContext) -> Result<f64, LoopError> {
        let combined = threats.join("\n");
        let r = self
            .client
            .analyze(&combined)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(blast_score_from_output(&r.output))
    }

    /// Phase 3: call SERAPH `advance_investigation` with scored threat context.
    async fn research(
        &self,
        threats: &[String],
        score: f64,
        _ctx: &StepContext,
    ) -> Result<Vec<String>, LoopError> {
        let finding = format!("score:{score:.2} threats:{}", threats.join("; "));
        let r = self
            .client
            .advance_investigation(&finding)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(non_empty_lines(&r.output))
    }

    /// Phase 4: call SERAPH `examine` on the joined evidence strings.
    async fn prove(
        &self,
        evidence: &[String],
        _ctx: &StepContext,
    ) -> Result<Vec<String>, LoopError> {
        let combined = evidence.join("\n");
        let r = self
            .client
            .examine(&combined)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(non_empty_lines(&r.output))
    }

    /// Phase 5: call SERAPH `typed_report` and return the risk declaration.
    async fn declare(&self, _state: &BcraState, _ctx: &StepContext) -> Result<String, LoopError> {
        let r = self
            .client
            .typed_report()
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(r.summary.to_string())
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Split prose output into non-empty trimmed lines.
fn non_empty_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

/// Derive a normalised blast score `[0.0, 1.0]` from SERAPH analyse output.
///
/// Uses a keyword-weight table drawn from FAIR/Bowtie severity vocabulary.
/// Multiple hits accumulate; the sum is clamped to `[0.0, 1.0]`.
fn blast_score_from_output(output: &str) -> f64 {
    let lower = output.to_lowercase();
    let weights: &[(&str, f64)] = &[
        ("critical", 0.35),
        ("exploit", 0.30),
        ("vulnerable", 0.20),
        ("high", 0.20),
        ("attack", 0.15),
        ("risk", 0.10),
        ("medium", 0.10),
        ("exposure", 0.10),
        ("low", 0.05),
    ];
    let raw: f64 = weights
        .iter()
        .filter(|(kw, _)| lower.contains(kw))
        .map(|(_, w)| w)
        .sum();
    raw.clamp(0.0_f64, 1.0_f64)
}
