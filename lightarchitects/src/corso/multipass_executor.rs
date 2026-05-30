//! [`MultiPassExecutor`] implementation for the CORSO sibling.
//!
//! `CorsoMultiPassExecutor` drives N-pass independent verification using
//! CORSO's code review and guard operations:
//!
//! | MultiPass operation | CORSO operation  | Rationale |
//! |---------------------|-----------------|-----------|
//! | `verify_pass`       | `code_review`   | Each independent pass reviews the subject |
//! | `aggregate`         | `guard`         | Guard aggregates notes into a security verdict |
//!
//! # Gating
//!
//! This module is only compiled when `features = ["loops-core"]`.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    agent::loops::{error::LoopError, multipass::MultiPassExecutor, runner::StepContext},
    core::transport::Transport,
};

use super::CorsoClient;

/// [`MultiPassExecutor`] that delegates each verification pass to the CORSO MCP server.
pub struct CorsoMultiPassExecutor<T: Transport> {
    client: Arc<CorsoClient<T>>,
}

impl<T: Transport> CorsoMultiPassExecutor<T> {
    /// Wrap an existing `CorsoClient` for shared use across multi-pass verification.
    #[must_use]
    pub fn new(client: Arc<CorsoClient<T>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> MultiPassExecutor for CorsoMultiPassExecutor<T> {
    /// Run the `n`th (0-based) verification pass via CORSO `code_review`.
    ///
    /// The pass succeeds when the review output contains no blocking-class keywords.
    async fn verify_pass(
        &self,
        n: u32,
        subject: &str,
        _ctx: &StepContext,
    ) -> Result<(bool, String), LoopError> {
        let r = self
            .client
            .code_review(subject, None)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;

        let lower = r.output.to_lowercase();
        let has_blocking = lower.contains("blocking")
            || lower.contains("critical")
            || lower.contains("fail")
            || lower.contains("error");
        let passed = !has_blocking;
        let note = format!("pass {n}: {}", r.output.lines().next().unwrap_or("ok"));
        Ok((passed, note))
    }

    /// Aggregate all pass results into a verdict via CORSO `guard`.
    ///
    /// The guard output becomes the final aggregate verdict.
    async fn aggregate(
        &self,
        results: &[bool],
        notes: &[String],
        _ctx: &StepContext,
    ) -> Result<String, LoopError> {
        let passes_passed = results.iter().filter(|&&p| p).count();
        let summary = format!("{passes_passed}/{} passes", results.len());
        let combined = format!("{summary}\n{}", notes.join("\n"));
        let r = self
            .client
            .guard(&combined)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(r.output)
    }
}
