//! [`CoVeExecutor`] implementation for the SERAPH sibling.
//!
//! `SeraphCoVeExecutor` uses SERAPH's recon, scan, and examine operations to
//! drive the Chain-of-Verification loop for security finding validation:
//!
//! | CoVe phase          | SERAPH operation | Rationale |
//! |---------------------|-----------------|-----------|
//! | `extract_claims`    | `scan`          | Scan surfaces security-relevant factual claims |
//! | `plan_verification` | `analyze`       | Analyze structures evidence into verification questions |
//! | `verify`            | `examine`       | Examine provides deep structural confirmation per claim |
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

use super::SeraphClient;

/// [`CoVeExecutor`] that delegates each `CoVe` phase to the SERAPH MCP server.
pub struct SeraphCoVeExecutor<T: Transport> {
    client: Arc<SeraphClient<T>>,
}

impl<T: Transport> SeraphCoVeExecutor<T> {
    /// Wrap an existing `SeraphClient` for shared use across `CoVe` loop phases.
    #[must_use]
    pub fn new(client: Arc<SeraphClient<T>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> CoVeExecutor for SeraphCoVeExecutor<T> {
    /// Phase 1: call SERAPH `scan` on the input and extract security claims from the output.
    async fn extract_claims(
        &self,
        input: &str,
        _ctx: &StepContext,
    ) -> Result<Vec<Claim>, LoopError> {
        let r = self
            .client
            .scan(input)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        let claims = r
            .output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| Claim {
                text: line.to_owned(),
                source: "seraph://scan".to_owned(),
                category: ClaimCategory::Factual,
            })
            .collect();
        Ok(claims)
    }

    /// Phase 2: call SERAPH `analyze` to structure verification questions for each claim.
    async fn plan_verification(
        &self,
        claims: &[Claim],
        _ctx: &StepContext,
    ) -> Result<Vec<VerificationQuestion>, LoopError> {
        let combined = claims
            .iter()
            .enumerate()
            .map(|(i, c)| format!("[{i}] {}", c.text))
            .collect::<Vec<_>>()
            .join("\n");
        let r = self
            .client
            .analyze(&combined)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        let questions = claims
            .iter()
            .enumerate()
            .map(|(i, c)| VerificationQuestion {
                question: format!("Verify: {}", c.text),
                claim_index: i,
                evidence_source: r.output.clone(),
                expected_format: "confirmed/refuted/inconclusive".to_owned(),
            })
            .collect();
        Ok(questions)
    }

    /// Phase 3: call SERAPH `examine` per claim and score verification results.
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
                .examine(&claim.text)
                .await
                .map_err(|e| LoopError::StepFailed(e.to_string()))?;
            let output_lower = r.output.to_lowercase();
            let (status, confidence) =
                if output_lower.contains("confirmed") || output_lower.contains("vulnerable") {
                    (VerificationStatus::Verified, 0.85)
                } else if output_lower.contains("refuted") || output_lower.contains("not found") {
                    (VerificationStatus::Refuted, 0.75)
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
