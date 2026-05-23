//! [`AchExecutor`] implementation for the QUANTUM sibling.
//!
//! `QuantumAchExecutor` maps each ACH phase to the QUANTUM action that best
//! covers its domain intent:
//!
//! | ACH phase             | QUANTUM action | Rationale |
//! |-----------------------|---------------|-----------|
//! | `generate_hypotheses` | `theorize`    | Both produce a ranked list of candidate explanations |
//! | `build_predictions`   | `probe`       | Probe answers "what would be observable if H were true?" |
//! | `score_predictions`   | `verify`      | Verify tests hypotheses against collected evidence |
//!
//! The SDK's [`AchScoringEngine`] runs the deterministic scoring pass over
//! the returned predictions — no additional QUANTUM call required.
//!
//! # Gating
//!
//! This module is only compiled when `features = ["loops-core"]`.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    agent::loops::{
        ach::{AchExecutor, Prediction, TestResult, TestType},
        error::LoopError,
        runner::StepContext,
    },
    core::transport::Transport,
};

use super::QuantumClient;

/// [`AchExecutor`] that delegates each ACH phase to the QUANTUM MCP server.
pub struct QuantumAchExecutor<T: Transport> {
    client: Arc<QuantumClient<T>>,
}

impl<T: Transport> QuantumAchExecutor<T> {
    /// Wrap an existing `QuantumClient` for shared use across ACH loop phases.
    #[must_use]
    pub fn new(client: Arc<QuantumClient<T>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> AchExecutor for QuantumAchExecutor<T> {
    /// Phase 1: call QUANTUM `theorize` and return each line as a hypothesis.
    async fn generate_hypotheses(
        &self,
        query: &str,
        _ctx: &StepContext,
    ) -> Result<Vec<String>, LoopError> {
        let r = self
            .client
            .theorize(query, None)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        let hypotheses = r
            .output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_owned)
            .collect();
        Ok(hypotheses)
    }

    /// Phase 2: call QUANTUM `probe` for each hypothesis to build its prediction set.
    async fn build_predictions(
        &self,
        hypotheses: &[String],
        _ctx: &StepContext,
    ) -> Result<Vec<Vec<Prediction>>, LoopError> {
        let mut matrix = Vec::with_capacity(hypotheses.len());
        for hypothesis in hypotheses {
            let r = self
                .client
                .probe(hypothesis)
                .await
                .map_err(|e| LoopError::StepFailed(e.to_string()))?;
            let predictions = r
                .output
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|line| Prediction {
                    claim: line.to_owned(),
                    test_type: TestType::PatternPresence,
                    result: None,
                })
                .collect();
            matrix.push(predictions);
        }
        Ok(matrix)
    }

    /// Phase 3: call QUANTUM `verify` for each hypothesis and annotate predictions.
    async fn score_predictions(
        &self,
        hypotheses: &[String],
        predictions: &[Vec<Prediction>],
        _ctx: &StepContext,
    ) -> Result<Vec<Vec<Prediction>>, LoopError> {
        let mut scored = Vec::with_capacity(hypotheses.len());
        for (hypothesis, preds) in hypotheses.iter().zip(predictions.iter()) {
            let r = self
                .client
                .verify(hypothesis)
                .await
                .map_err(|e| LoopError::StepFailed(e.to_string()))?;
            let verdict = r.output.to_lowercase();
            let test_result = if verdict.contains("confirmed") || verdict.contains("verified") {
                TestResult::Confirmed(r.output.clone())
            } else if verdict.contains("refuted") || verdict.contains("contradict") {
                TestResult::Refuted(r.output.clone())
            } else {
                TestResult::Inconclusive(r.output.clone())
            };
            let annotated = preds
                .iter()
                .map(|p| Prediction {
                    result: Some(test_result.clone()),
                    ..p.clone()
                })
                .collect();
            scored.push(annotated);
        }
        Ok(scored)
    }
}
