//! [`ReflexionExecutor`] implementation for the SOUL sibling.
//!
//! `SoulReflexionExecutor` drives the Reflexion lifecycle (Provisional →
//! Reviewed → Verified → Archived) using SOUL's helix vault operations:
//!
//! | Reflexion operation | SOUL operation | Rationale |
//! |---------------------|---------------|-----------|
//! | `generate`          | `search`      | Search prior helix entries to build applied-knowledge context |
//! | `review`            | `validate`    | Validate the case path in the helix; promote when no issues found |
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

use super::SoulClient;

/// [`ReflexionExecutor`] that uses SOUL's helix and validation operations to
/// drive the Reflexion lifecycle loop.
pub struct SoulReflexionExecutor<T: Transport> {
    client: Arc<SoulClient<T>>,
}

impl<T: Transport> SoulReflexionExecutor<T> {
    /// Wrap an existing `SoulClient` for shared use across reflexion loop steps.
    #[must_use]
    pub fn new(client: Arc<SoulClient<T>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> ReflexionExecutor for SoulReflexionExecutor<T> {
    /// Generate an initial [`ReflexionEntry`] by searching the helix for prior
    /// context on the case and synthesising a provisional entry.
    async fn generate(
        &self,
        case_id: &str,
        context: &str,
        _ctx: &StepContext,
    ) -> Result<ReflexionEntry, LoopError> {
        // search(pattern, path, frontmatter_only, limit)
        let hits = self
            .client
            .search(case_id, None, false, Some(5))
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;

        let applied_knowledge: Vec<String> = hits.iter().map(|h| h.line.clone()).collect();

        Ok(ReflexionEntry {
            case_id: case_id.to_owned(),
            state: ReflexionState::Provisional,
            new_patterns: vec![format!("context: {context}")],
            applied_knowledge,
            root_cause: None,
            improvements: Vec::new(),
            confidence: 0.5,
        })
    }

    /// Review an entry by calling SOUL `validate` on the associated helix path.
    ///
    /// Promotes when `ValidateReport.count == 0` (no issues) and confidence is sufficient.
    async fn review(
        &self,
        entry: &ReflexionEntry,
        _ctx: &StepContext,
    ) -> Result<ReflexionReview, LoopError> {
        let report = self
            .client
            .validate(Some(&entry.case_id), false)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;

        let no_issues = report.count == 0;
        let should_promote = no_issues && entry.confidence >= 0.5;
        let confidence_delta = if no_issues { 0.1 } else { -0.1 };

        let improvements = report
            .issues
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();

        Ok(ReflexionReview {
            should_promote,
            improvements,
            confidence_delta,
        })
    }
}
