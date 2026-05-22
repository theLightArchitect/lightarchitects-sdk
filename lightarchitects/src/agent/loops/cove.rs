//! Chain-of-Verification (`CoVe`) strategy — SDK port of QUANTUM agentic/`cove.rs`.
//!
//! Three-phase loop: extract claims → plan verification questions → execute verification.
//! Reduces hallucinations by grounding each claim against primary-source evidence.
//!
//! Source: Dhuliawala et al. 2023 (Meta AI), "Chain-of-Verification Reduces Hallucination"

use async_trait::async_trait;

use super::{
    error::LoopError,
    runner::{Outcome, StepContext, Strategy},
};

// ── Types (ported from QUANTUM) ───────────────────────────────────────────────

/// Categories of claims — determines verification routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimCategory {
    /// Factual claim verifiable against collected evidence.
    Factual,
    /// Causal claim verifiable via timeline or correlation.
    Causal,
    /// Configuration value claim verifiable via config dump.
    Configuration,
    /// Temporal claim verifiable via timestamps.
    Temporal,
    /// Statistical claim verifiable via metrics.
    Statistical,
}

/// A claim extracted from a hypothesis or draft output.
#[derive(Debug, Clone)]
pub struct Claim {
    /// The claim text.
    pub text: String,
    /// Source hypothesis or synthesis step that produced it.
    pub source: String,
    /// Category for verification routing.
    pub category: ClaimCategory,
}

/// A verification question generated for a claim.
#[derive(Debug, Clone)]
pub struct VerificationQuestion {
    /// The question to answer.
    pub question: String,
    /// Index of the claim this question verifies.
    pub claim_index: usize,
    /// Evidence source to consult.
    pub evidence_source: String,
    /// Expected answer format.
    pub expected_format: String,
}

/// Verification outcome for a single claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    /// Claim confirmed by primary-source evidence.
    Verified,
    /// Claim contradicted by evidence.
    Refuted,
    /// Evidence insufficient to confirm or refute.
    Inconclusive,
    /// No suitable evidence source available.
    Unverifiable,
}

/// Result of verifying a single claim.
#[derive(Debug, Clone)]
pub struct VerifiedClaim {
    /// Original claim.
    pub claim: Claim,
    /// Questions asked during verification.
    pub questions: Vec<VerificationQuestion>,
    /// Verification outcome.
    pub status: VerificationStatus,
    /// Evidence supporting or refuting the claim.
    pub evidence: String,
    /// Confidence in the verification (0.0–1.0).
    pub confidence: f64,
}

/// Complete [`CoVeResult`] verification result.
#[derive(Debug, Clone)]
pub struct CoVeResult {
    /// All verified claims.
    pub claims: Vec<VerifiedClaim>,
    /// Number of confirmed claims.
    pub verified_count: usize,
    /// Number of refuted claims.
    pub refuted_count: usize,
    /// Number of inconclusive claims.
    pub inconclusive_count: usize,
    /// Number of unverifiable claims.
    pub unverifiable_count: usize,
    /// Verification score (verified / verifiable).
    pub verification_score: f64,
}

impl CoVeResult {
    fn from_claims(claims: Vec<VerifiedClaim>) -> Self {
        let verified_count = claims
            .iter()
            .filter(|c| c.status == VerificationStatus::Verified)
            .count();
        let refuted_count = claims
            .iter()
            .filter(|c| c.status == VerificationStatus::Refuted)
            .count();
        let inconclusive_count = claims
            .iter()
            .filter(|c| c.status == VerificationStatus::Inconclusive)
            .count();
        let unverifiable_count = claims
            .iter()
            .filter(|c| c.status == VerificationStatus::Unverifiable)
            .count();
        let verifiable = verified_count + refuted_count + inconclusive_count;
        #[allow(clippy::cast_precision_loss)]
        let verification_score = if verifiable > 0 {
            verified_count as f64 / verifiable as f64
        } else {
            0.0
        };
        Self {
            claims,
            verified_count,
            refuted_count,
            inconclusive_count,
            unverifiable_count,
            verification_score,
        }
    }
}

// ── Phase + State ─────────────────────────────────────────────────────────────

/// Phase of the [`CoVeStrategy`] loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoVePhase {
    /// Phase 1 — extract claims from the input text.
    ExtractClaims,
    /// Phase 2 — plan verification questions for each claim.
    PlanVerification,
    /// Phase 3 — execute verification and score claims.
    Verify,
    /// Phase 4 — close with `CoVeResult`.
    Close,
}

/// State threaded through each step of the [`CoVeStrategy`] loop.
#[derive(Debug, Clone)]
pub struct CoVeState {
    /// Input text to verify.
    pub input: String,
    /// Current phase.
    pub phase: CoVePhase,
    /// Extracted claims (populated in phase 1).
    pub claims: Vec<Claim>,
    /// Verification questions (populated in phase 2).
    pub questions: Vec<VerificationQuestion>,
    /// Verified claims (populated in phase 3).
    pub verified: Vec<VerifiedClaim>,
}

impl CoVeState {
    /// Start a new [`CoVeStrategy`] verification from an input string.
    #[must_use]
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            phase: CoVePhase::ExtractClaims,
            claims: Vec::new(),
            questions: Vec::new(),
            verified: Vec::new(),
        }
    }
}

// ── Executor ──────────────────────────────────────────────────────────────────

/// Provider-agnostic executor for the three [`CoVeStrategy`] phases.
#[async_trait]
pub trait CoVeExecutor: Send + Sync + 'static {
    /// Phase 1: extract verifiable claims from the input text.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn extract_claims(&self, input: &str, ctx: &StepContext)
    -> Result<Vec<Claim>, LoopError>;

    /// Phase 2: plan verification questions for each claim.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn plan_verification(
        &self,
        claims: &[Claim],
        ctx: &StepContext,
    ) -> Result<Vec<VerificationQuestion>, LoopError>;

    /// Phase 3: execute verification and score each claim.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn verify(
        &self,
        claims: &[Claim],
        questions: &[VerificationQuestion],
        ctx: &StepContext,
    ) -> Result<Vec<VerifiedClaim>, LoopError>;
}

// ── Strategy ─────────────────────────────────────────────────────────────────

/// Chain-of-Verification loop.
///
/// Drives a [`CoVeExecutor`] through extract → plan → verify, then halts
/// with a [`CoVeResult`] containing verification scores for all claims.
pub struct CoVeStrategy<E> {
    executor: E,
    name: &'static str,
}

impl<E: CoVeExecutor> CoVeStrategy<E> {
    /// Create a strategy wrapping the given executor.
    #[must_use]
    pub fn new(executor: E) -> Self {
        Self {
            executor,
            name: "CoVe",
        }
    }

    /// Override the strategy name.
    #[must_use]
    pub fn with_name(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }
}

#[async_trait]
impl<E: CoVeExecutor> Strategy for CoVeStrategy<E> {
    type State = CoVeState;
    type Output = CoVeResult;

    async fn step(
        &self,
        state: CoVeState,
        ctx: &StepContext,
    ) -> Result<Outcome<CoVeState, CoVeResult>, LoopError> {
        match state.phase {
            CoVePhase::ExtractClaims => {
                let claims = self.executor.extract_claims(&state.input, ctx).await?;
                Ok(Outcome::Continue(CoVeState {
                    phase: CoVePhase::PlanVerification,
                    claims,
                    ..state
                }))
            }
            CoVePhase::PlanVerification => {
                let questions = self.executor.plan_verification(&state.claims, ctx).await?;
                Ok(Outcome::Continue(CoVeState {
                    phase: CoVePhase::Verify,
                    questions,
                    ..state
                }))
            }
            CoVePhase::Verify => {
                let verified = self
                    .executor
                    .verify(&state.claims, &state.questions, ctx)
                    .await?;
                Ok(Outcome::Continue(CoVeState {
                    phase: CoVePhase::Close,
                    verified,
                    ..state
                }))
            }
            CoVePhase::Close => Ok(Outcome::Halt(CoVeResult::from_claims(state.verified))),
        }
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use futures_util::StreamExt as _;

    use crate::agent::{
        ChainContext,
        loops::{Budget, LoopRunner, Outcome},
    };

    use super::*;

    struct StubExecutor;

    #[async_trait::async_trait]
    impl CoVeExecutor for StubExecutor {
        async fn extract_claims(
            &self,
            input: &str,
            _ctx: &StepContext,
        ) -> Result<Vec<Claim>, LoopError> {
            Ok(vec![Claim {
                text: format!("claim from: {input}"),
                source: "input".into(),
                category: ClaimCategory::Factual,
            }])
        }

        async fn plan_verification(
            &self,
            claims: &[Claim],
            _ctx: &StepContext,
        ) -> Result<Vec<VerificationQuestion>, LoopError> {
            Ok(claims
                .iter()
                .enumerate()
                .map(|(i, c)| VerificationQuestion {
                    question: format!("Is '{}' supported?", c.text),
                    claim_index: i,
                    evidence_source: "logs".into(),
                    expected_format: "yes/no".into(),
                })
                .collect())
        }

        async fn verify(
            &self,
            claims: &[Claim],
            questions: &[VerificationQuestion],
            _ctx: &StepContext,
        ) -> Result<Vec<VerifiedClaim>, LoopError> {
            Ok(claims
                .iter()
                .enumerate()
                .map(|(i, c)| VerifiedClaim {
                    claim: c.clone(),
                    questions: questions
                        .iter()
                        .filter(|q| q.claim_index == i)
                        .cloned()
                        .collect(),
                    status: VerificationStatus::Verified,
                    evidence: "log entry confirmed".into(),
                    confidence: 0.9,
                })
                .collect())
        }
    }

    #[tokio::test]
    async fn cove_produces_verified_result() {
        let runner = LoopRunner::new(CoVeStrategy::new(StubExecutor), Budget::unlimited());
        let mut stream = runner.run(CoVeState::new("test input"), ChainContext::default(), None);

        let mut result = None;
        while let Some(step) = stream.next().await {
            if let Outcome::Halt(r) = step.unwrap().outcome {
                result = Some(r);
            }
        }
        let r = result.unwrap();
        assert_eq!(r.verified_count, 1);
        assert_eq!(r.refuted_count, 0);
        assert!((r.verification_score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn cove_result_score_is_correct() {
        let make = |status: VerificationStatus| VerifiedClaim {
            claim: Claim {
                text: "c".into(),
                source: "s".into(),
                category: ClaimCategory::Factual,
            },
            questions: vec![],
            status,
            evidence: String::new(),
            confidence: 0.5,
        };
        let result = CoVeResult::from_claims(vec![
            make(VerificationStatus::Verified),
            make(VerificationStatus::Refuted),
            make(VerificationStatus::Unverifiable),
        ]);
        // verifiable = verified + refuted + inconclusive = 2, verified = 1
        assert!((result.verification_score - 0.5).abs() < f64::EPSILON);
        assert_eq!(result.unverifiable_count, 1);
    }
}
