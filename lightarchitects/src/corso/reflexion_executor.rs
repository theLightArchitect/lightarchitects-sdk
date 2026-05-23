//! [`ReflexionExecutor`] implementation for the CORSO sibling.
//!
//! `CorsoReflexionExecutor` drives the Reflexion lifecycle using CORSO's code
//! review and guard operations:
//!
//! | Reflexion operation | CORSO operation       | Rationale |
//! |---------------------|-----------------------|-----------|
//! | `generate`          | `code_review` / `sniff` | Code review surfaces patterns, quality gaps, and improvement opportunities |
//! | `review`            | `guard`               | Security + quality guard makes the promotion decision |
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

use super::CorsoClient;

/// [`ReflexionExecutor`] that uses CORSO's code review and guard operations to
/// drive the Reflexion lifecycle loop for code quality improvement.
pub struct CorsoReflexionExecutor<T: Transport> {
    client: Arc<CorsoClient<T>>,
}

impl<T: Transport> CorsoReflexionExecutor<T> {
    /// Wrap an existing `CorsoClient` for shared use across reflexion loop steps.
    #[must_use]
    pub fn new(client: Arc<CorsoClient<T>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> ReflexionExecutor for CorsoReflexionExecutor<T> {
    /// Generate an initial [`ReflexionEntry`] by running CORSO `code_review` on the context.
    async fn generate(
        &self,
        case_id: &str,
        context: &str,
        _ctx: &StepContext,
    ) -> Result<ReflexionEntry, LoopError> {
        let review = self
            .client
            .code_review(context, None)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;

        let new_patterns: Vec<String> = review
            .output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(5)
            .map(str::to_owned)
            .collect();

        Ok(ReflexionEntry {
            case_id: case_id.to_owned(),
            state: ReflexionState::Provisional,
            new_patterns,
            applied_knowledge: Vec::new(),
            root_cause: None,
            improvements: Vec::new(),
            confidence: 0.6,
        })
    }

    /// Review an entry by calling CORSO `guard` on the accumulated patterns.
    ///
    /// Promotes if the guard returns no blocking findings.
    async fn review(
        &self,
        entry: &ReflexionEntry,
        _ctx: &StepContext,
    ) -> Result<ReflexionReview, LoopError> {
        let patterns_text = entry.new_patterns.join("\n");
        let r = self
            .client
            .guard(&patterns_text)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;

        let output_lower = r.output.to_lowercase();
        let has_blocking = output_lower.contains("blocking")
            || output_lower.contains("critical")
            || output_lower.contains("security");
        let should_promote = !has_blocking && entry.confidence >= 0.5;
        let confidence_delta = if should_promote { 0.1 } else { -0.05 };

        Ok(ReflexionReview {
            should_promote,
            improvements: r
                .output
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_owned)
                .collect(),
            confidence_delta,
        })
    }
}
