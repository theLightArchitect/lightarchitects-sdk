//! [`CoVeExecutor`] implementation for the CORSO sibling.
//!
//! `CorsoCoVeExecutor` uses CORSO's architecture analysis and code search to
//! drive the Chain-of-Verification loop for architectural claim validation:
//!
//! | CoVe phase          | CORSO operation          | Rationale |
//! |---------------------|--------------------------|-----------|
//! | `extract_claims`    | `analyze_architecture`   | Architecture analysis surfaces verifiable structural claims |
//! | `plan_verification` | `search_code`            | Code search structures each claim into a searchable verification question |
//! | `verify`            | `guard`                  | Guard checks each claim against security + quality gates |
//!
//! # Gating
//!
//! This module is only compiled when `features = ["loops-core"]`.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    agent::loops::{
        cove::{
            Claim, ClaimCategory, CoVeExecutor, VerificationQuestion, VerificationStatus,
            VerifiedClaim,
        },
        error::LoopError,
        runner::StepContext,
    },
    core::transport::Transport,
};

use super::CorsoClient;

/// [`CoVeExecutor`] that delegates each `CoVe` phase to the CORSO MCP server.
pub struct CorsoCoVeExecutor<T: Transport> {
    client: Arc<CorsoClient<T>>,
}

impl<T: Transport> CorsoCoVeExecutor<T> {
    /// Wrap an existing `CorsoClient` for shared use across `CoVe` loop phases.
    #[must_use]
    pub fn new(client: Arc<CorsoClient<T>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> CoVeExecutor for CorsoCoVeExecutor<T> {
    /// Phase 1: call CORSO `analyze_architecture` to extract verifiable structural claims.
    async fn extract_claims(
        &self,
        input: &str,
        _ctx: &StepContext,
    ) -> Result<Vec<Claim>, LoopError> {
        let r = self
            .client
            .analyze_architecture(input)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        let claims = r
            .output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| Claim {
                text: line.to_owned(),
                source: "corso://analyze_architecture".to_owned(),
                category: ClaimCategory::Configuration,
            })
            .collect();
        Ok(claims)
    }

    /// Phase 2: call CORSO `search_code` per claim to structure verification questions.
    async fn plan_verification(
        &self,
        claims: &[Claim],
        _ctx: &StepContext,
    ) -> Result<Vec<VerificationQuestion>, LoopError> {
        let mut questions = Vec::with_capacity(claims.len());
        for (i, claim) in claims.iter().enumerate() {
            let hits = self
                .client
                .search_code(&claim.text, None)
                .await
                .map_err(|e| LoopError::StepFailed(e.to_string()))?;
            let evidence_source = hits
                .iter()
                .map(|h| format!("{}:{} — {}", h.file, h.line, h.content))
                .collect::<Vec<_>>()
                .join("\n");
            questions.push(VerificationQuestion {
                question: format!("Does the codebase support: {}", claim.text),
                claim_index: i,
                evidence_source,
                expected_format: "confirmed/refuted/inconclusive".to_owned(),
            });
        }
        Ok(questions)
    }

    /// Phase 3: call CORSO `guard` per claim and score the verification result.
    async fn verify(
        &self,
        claims: &[Claim],
        questions: &[VerificationQuestion],
        _ctx: &StepContext,
    ) -> Result<Vec<VerifiedClaim>, LoopError> {
        let mut verified = Vec::with_capacity(claims.len());
        for (claim, question) in claims.iter().zip(questions.iter()) {
            let r = self
                .client
                .guard(&claim.text)
                .await
                .map_err(|e| LoopError::StepFailed(e.to_string()))?;
            let output_lower = r.output.to_lowercase();
            let (status, confidence) =
                if output_lower.contains("pass") || output_lower.contains("clean") {
                    (VerificationStatus::Verified, 0.85)
                } else if output_lower.contains("fail") || output_lower.contains("blocking") {
                    (VerificationStatus::Refuted, 0.8)
                } else {
                    (VerificationStatus::Inconclusive, 0.5)
                };
            verified.push(VerifiedClaim {
                claim: claim.clone(),
                questions: vec![question.clone()],
                status,
                evidence: r.output,
                confidence,
            });
        }
        Ok(verified)
    }
}
