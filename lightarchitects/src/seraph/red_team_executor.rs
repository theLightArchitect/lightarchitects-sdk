//! [`RedTeamExecutor`] implementation for the SERAPH sibling.
//!
//! `SeraphRedTeamExecutor` drives the red-team assessment loop using SERAPH's
//! scope, reconnaissance, survey, strike, and reporting operations:
//!
//! | RedTeam phase | SERAPH operation     | Rationale |
//! |---------------|---------------------|-----------|
//! | `hydrate`     | `scope_check`       | Load engagement scope + control anchors |
//! | `surface`     | `recon`             | Enumerate the attack surface |
//! | `probe`       | `survey`            | Actively probe identified surface entries |
//! | `chain`       | `strike`            | Chain probe findings into an exploit narrative |
//! | `verdict`     | `typed_report`      | Produce the final security verdict |
//!
//! # Gating
//!
//! This module is only compiled when `features = ["loops-core"]`.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    agent::loops::{
        error::LoopError,
        red_team::{RedTeamExecutor, RedTeamState},
        runner::StepContext,
    },
    core::transport::Transport,
};

use super::SeraphClient;

/// [`RedTeamExecutor`] that delegates each red-team phase to the SERAPH MCP server.
pub struct SeraphRedTeamExecutor<T: Transport> {
    client: Arc<SeraphClient<T>>,
}

impl<T: Transport> SeraphRedTeamExecutor<T> {
    /// Wrap an existing `SeraphClient` for shared use across red-team loop phases.
    #[must_use]
    pub fn new(client: Arc<SeraphClient<T>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> RedTeamExecutor for SeraphRedTeamExecutor<T> {
    /// Phase 0 (MANDATORY): call SERAPH `scope_check` to load the engagement scope
    /// and extract control anchors from the authorisation response.
    ///
    /// SERAPH SKILL.md §0 — Hydrate MUST NOT be skipped; every engagement begins
    /// with a scope gate.
    async fn hydrate(&self, scope: &str, _ctx: &StepContext) -> Result<Vec<String>, LoopError> {
        let r = self
            .client
            .scope_check(scope)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(non_empty_lines(&r.output))
    }

    /// Phase 1: call SERAPH `recon` on the scope to enumerate the attack surface.
    async fn surface(
        &self,
        scope: &str,
        _anchors: &[String],
        _ctx: &StepContext,
    ) -> Result<Vec<String>, LoopError> {
        let r = self
            .client
            .recon(scope)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(non_empty_lines(&r.output))
    }

    /// Phase 2: call SERAPH `survey` on the attack surface to actively probe it.
    async fn probe(
        &self,
        surface: &[String],
        _ctx: &StepContext,
    ) -> Result<Vec<String>, LoopError> {
        let target = surface.join(",");
        let r = self
            .client
            .survey(&target)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(non_empty_lines(&r.output))
    }

    /// Phase 3: call SERAPH `strike` to chain probe findings into an exploit narrative.
    async fn chain(&self, findings: &[String], _ctx: &StepContext) -> Result<String, LoopError> {
        let combined = findings.join("\n");
        let r = self
            .client
            .strike(&combined)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(r.output)
    }

    /// Phase 4: call SERAPH `typed_report` to produce the final security verdict.
    async fn verdict(
        &self,
        _state: &RedTeamState,
        _ctx: &StepContext,
    ) -> Result<String, LoopError> {
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
