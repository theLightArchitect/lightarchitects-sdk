//! [`ReflexionExecutor`] implementation for the EVA sibling.
//!
//! `EvaReflexionExecutor` drives the Reflexion lifecycle using EVA's memory
//! enrichment and crystallization operations:
//!
//! | Reflexion operation | EVA operation       | Rationale |
//! |---------------------|---------------------|-----------|
//! | `generate`          | `ideate` + `remember` | Ideate synthesises patterns from context; remember anchors them in EVA's memory vault |
//! | `review`            | `crystallize`       | Crystallize distils improvements into authoritative knowledge — the promotion gate |
//!
//! # Gating
//!
//! This module is only compiled when `features = ["loops-core"]`.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    agent::loops::{
        error::LoopError,
        reflexion::{ReflexionEntry, ReflexionExecutor, ReflexionReview, ReflexionState},
        runner::StepContext,
    },
    core::transport::Transport,
};

use super::EvaClient;

/// [`ReflexionExecutor`] that uses EVA's ideation and crystallization operations
/// to drive the Reflexion lifecycle loop for memory enrichment.
pub struct EvaReflexionExecutor<T: Transport> {
    client: Arc<EvaClient<T>>,
}

impl<T: Transport> EvaReflexionExecutor<T> {
    /// Wrap an existing `EvaClient` for shared use across reflexion loop steps.
    #[must_use]
    pub fn new(client: Arc<EvaClient<T>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> ReflexionExecutor for EvaReflexionExecutor<T> {
    /// Generate an initial [`ReflexionEntry`] by calling EVA `ideate` on the context,
    /// then anchoring it in EVA's vault via `remember`.
    async fn generate(
        &self,
        case_id: &str,
        context: &str,
        _ctx: &StepContext,
    ) -> Result<ReflexionEntry, LoopError> {
        // ideate(goal, context) — 2 args
        let ideation = self
            .client
            .ideate(context, None)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;

        // IdeateResult has phase_3_ideation (the synthesis phase) as the richest patterns field.
        let new_patterns: Vec<String> = ideation
            .phase_3_ideation
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(5)
            .map(str::to_owned)
            .collect();

        // remember(event, tags) — 2 args
        self.client
            .remember(context, None)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;

        Ok(ReflexionEntry {
            case_id: case_id.to_owned(),
            state: ReflexionState::Provisional,
            new_patterns,
            applied_knowledge: Vec::new(),
            root_cause: None,
            improvements: Vec::new(),
            confidence: 0.65,
        })
    }

    /// Review an entry by calling EVA `crystallize` on the accumulated patterns.
    ///
    /// Crystallize returns a `walkthrough_prompt` summarising distilled insights.
    /// Promotes if the entry's confidence is sufficient.
    async fn review(
        &self,
        entry: &ReflexionEntry,
        _ctx: &StepContext,
    ) -> Result<ReflexionReview, LoopError> {
        let patterns_text = entry.new_patterns.join("\n");
        let r = self
            .client
            .crystallize(&patterns_text)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;

        // CrystallizeResult has `walkthrough_prompt` as the human-readable synthesis.
        let improvements: Vec<String> = r
            .walkthrough_prompt
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_owned)
            .collect();
        let should_promote = entry.confidence >= 0.6;
        let confidence_delta = if should_promote { 0.15 } else { 0.0 };

        Ok(ReflexionReview {
            should_promote,
            improvements,
            confidence_delta,
        })
    }
}
