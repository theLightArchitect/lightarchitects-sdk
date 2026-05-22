//! Analysis of Competing Hypotheses (ACH) strategy — SDK port of QUANTUM agentic/`hypothesis_testing.rs`.
//!
//! Three-phase loop: generate hypotheses → build prediction matrix → score and eliminate.
//! Includes the deterministic scoring engine (pure math, no LLM required).
//!
//! Source: Heuer (1999) "Psychology of Intelligence Analysis"; adapted for agentic loops.

use async_trait::async_trait;

use super::{
    error::LoopError,
    runner::{Outcome, StepContext, Strategy},
};

// ── Types (ported from QUANTUM) ───────────────────────────────────────────────

/// A testable prediction for an ACH hypothesis.
#[derive(Debug, Clone)]
pub struct Prediction {
    /// The testable claim.
    pub claim: String,
    /// Category of test.
    pub test_type: TestType,
    /// Result after testing (`None` = not yet tested).
    pub result: Option<TestResult>,
}

/// Test category — drives evidence routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestType {
    /// Pattern presence in collected logs or artifacts.
    PatternPresence,
    /// Pattern absence (absence of evidence is evidence of absence).
    PatternAbsence,
    /// Timeline correlation check.
    TimelineCorrelation,
    /// Configuration value verification.
    ConfigValue,
    /// Statistical threshold check.
    StatisticalThreshold,
}

/// Outcome of testing a single prediction.
#[derive(Debug, Clone)]
pub enum TestResult {
    /// Evidence confirms the prediction.
    Confirmed(String),
    /// Evidence contradicts the prediction.
    Refuted(String),
    /// Evidence is insufficient to decide.
    Inconclusive(String),
    /// Cannot be tested programmatically.
    NotTestable(String),
}

impl TestResult {
    /// Returns `true` if this result is a confirmation.
    #[must_use]
    pub fn is_confirmed(&self) -> bool {
        matches!(self, Self::Confirmed(_))
    }

    /// Returns `true` if this result is a refutation.
    #[must_use]
    pub fn is_refuted(&self) -> bool {
        matches!(self, Self::Refuted(_))
    }
}

/// Confidence level classification from a convergence score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfidenceLevel {
    /// < 40% — no predictions tested yet.
    Speculative,
    /// 40–69% — some predictions confirmed.
    Supported,
    /// 70–89% — multiple threads confirm, no contradictions.
    Convergent,
    /// ≥ 90% — all confirmed, alternatives eliminated.
    ProbableRootCause,
    /// Human-approved at HITL gate or sandbox-reproduced.
    Confirmed,
}

impl ConfidenceLevel {
    /// Classify a convergence score into a confidence level.
    #[must_use]
    pub fn from_score(score: f64) -> Self {
        if score >= 0.9 {
            Self::ProbableRootCause
        } else if score >= 0.7 {
            Self::Convergent
        } else if score >= 0.4 {
            Self::Supported
        } else {
            Self::Speculative
        }
    }
}

/// Scored hypothesis from the ACH pipeline.
#[derive(Debug, Clone)]
pub struct HypothesisTest {
    /// Identifier for this hypothesis.
    pub hypothesis_id: String,
    /// Hypothesis text.
    pub hypothesis: String,
    /// Tested predictions.
    pub predictions: Vec<Prediction>,
    /// Convergence score (0.0–1.0).
    pub convergence_score: f64,
    /// Confidence level derived from convergence.
    pub confidence_level: ConfidenceLevel,
    /// Eliminated alternative hypothesis IDs.
    pub eliminated_alternatives: Vec<String>,
}

// ── Scoring engine ────────────────────────────────────────────────────────────

/// Pure-math scoring engine for ACH hypothesis testing.
///
/// No LLM required — operates entirely on [`Prediction`] results.
pub struct AchScoringEngine;

impl AchScoringEngine {
    /// Compute convergence score for a set of predictions.
    ///
    /// Formula: `(confirmed / testable) × (1 - refuted / total_checks) × testability × elimination_bonus`
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn compute_convergence(predictions: &[Prediction], eliminated: usize) -> f64 {
        let (testable, not_testable, confirmed, refuted) = Self::count_results(predictions);
        let all = testable + not_testable;
        if testable == 0 || all == 0 {
            return 0.0;
        }
        let testability = testable as f64 / all as f64;
        let confirmation = confirmed as f64 / testable as f64;
        let total_checks = confirmed + refuted + (testable - confirmed - refuted);
        let contradiction_penalty = if total_checks > 0 {
            1.0 - (refuted as f64 / total_checks as f64)
        } else {
            1.0
        };
        let elimination_bonus = (1.0 + 0.1 * eliminated as f64).min(1.3);
        (confirmation * contradiction_penalty * testability * elimination_bonus).min(1.0)
    }

    /// Score a complete hypothesis.
    #[must_use]
    pub fn score_hypothesis(
        hypothesis_id: impl Into<String>,
        hypothesis: impl Into<String>,
        predictions: Vec<Prediction>,
        eliminated_alternatives: Vec<String>,
    ) -> HypothesisTest {
        let score = Self::compute_convergence(&predictions, eliminated_alternatives.len());
        HypothesisTest {
            hypothesis_id: hypothesis_id.into(),
            hypothesis: hypothesis.into(),
            convergence_score: score,
            confidence_level: ConfidenceLevel::from_score(score),
            predictions,
            eliminated_alternatives,
        }
    }

    fn count_results(predictions: &[Prediction]) -> (usize, usize, usize, usize) {
        let mut testable = 0usize;
        let mut not_testable = 0usize;
        let mut confirmed = 0usize;
        let mut refuted = 0usize;
        for pred in predictions {
            match &pred.result {
                Some(TestResult::Confirmed(_)) => {
                    testable += 1;
                    confirmed += 1;
                }
                Some(TestResult::Refuted(_)) => {
                    testable += 1;
                    refuted += 1;
                }
                Some(TestResult::Inconclusive(_)) => {
                    testable += 1;
                }
                Some(TestResult::NotTestable(_)) | None => {
                    not_testable += 1;
                }
            }
        }
        (testable, not_testable, confirmed, refuted)
    }
}

// ── Phase + State ─────────────────────────────────────────────────────────────

/// Phase of the ACH loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AchPhase {
    /// Phase 1 — generate competing hypotheses from the query.
    GenerateHypotheses,
    /// Phase 2 — build prediction matrix for each hypothesis.
    BuildPredictions,
    /// Phase 3 — score hypotheses and eliminate alternatives.
    ScoreEliminate,
    /// Phase 4 — close with final ranked hypotheses.
    Close,
}

/// State threaded through each step of the [`AchStrategy`] loop.
#[derive(Debug, Clone)]
pub struct AchState {
    /// Investigation query.
    pub query: String,
    /// Current phase.
    pub phase: AchPhase,
    /// Generated competing hypotheses.
    pub hypotheses: Vec<String>,
    /// Prediction matrix keyed by hypothesis index.
    pub predictions: Vec<Vec<Prediction>>,
    /// Scored hypotheses (populated in [`AchPhase::ScoreEliminate`] phase).
    pub tests: Vec<HypothesisTest>,
    /// Completed round count.
    pub round: u32,
    /// Maximum rounds before forced close.
    pub max_rounds: u32,
}

impl AchState {
    /// Start a new ACH investigation.
    #[must_use]
    pub fn new(query: impl Into<String>, max_rounds: u32) -> Self {
        Self {
            query: query.into(),
            phase: AchPhase::GenerateHypotheses,
            hypotheses: Vec::new(),
            predictions: Vec::new(),
            tests: Vec::new(),
            round: 0,
            max_rounds,
        }
    }
}

// ── Executor ──────────────────────────────────────────────────────────────────

/// Provider-agnostic executor for the three ACH phases.
#[async_trait]
pub trait AchExecutor: Send + Sync + 'static {
    /// Phase 1: generate competing hypotheses for the query.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn generate_hypotheses(
        &self,
        query: &str,
        ctx: &StepContext,
    ) -> Result<Vec<String>, LoopError>;

    /// Phase 2: build prediction matrix — what would be true if each hypothesis were correct?
    ///
    /// Returns one `Vec<Prediction>` per hypothesis (same index as `hypotheses`).
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn build_predictions(
        &self,
        hypotheses: &[String],
        ctx: &StepContext,
    ) -> Result<Vec<Vec<Prediction>>, LoopError>;

    /// Phase 3: score predictions against available evidence.
    ///
    /// Returns tested predictions (same structure as input, with `result` fields filled).
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn score_predictions(
        &self,
        hypotheses: &[String],
        predictions: &[Vec<Prediction>],
        ctx: &StepContext,
    ) -> Result<Vec<Vec<Prediction>>, LoopError>;
}

// ── Strategy ─────────────────────────────────────────────────────────────────

/// ACH investigation loop.
///
/// Drives an [`AchExecutor`] through generate → predict → score phases.
/// After `max_rounds` the loop closes with the ranked [`HypothesisTest`] list.
pub struct AchStrategy<E> {
    executor: E,
    name: &'static str,
}

impl<E: AchExecutor> AchStrategy<E> {
    /// Create a strategy wrapping the given executor.
    #[must_use]
    pub fn new(executor: E) -> Self {
        Self {
            executor,
            name: "ACH",
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
impl<E: AchExecutor> Strategy for AchStrategy<E> {
    type State = AchState;
    type Output = Vec<HypothesisTest>;

    async fn step(
        &self,
        state: AchState,
        ctx: &StepContext,
    ) -> Result<Outcome<AchState, Vec<HypothesisTest>>, LoopError> {
        match state.phase {
            AchPhase::GenerateHypotheses => {
                let hypotheses = self.executor.generate_hypotheses(&state.query, ctx).await?;
                Ok(Outcome::Continue(AchState {
                    phase: AchPhase::BuildPredictions,
                    hypotheses,
                    ..state
                }))
            }
            AchPhase::BuildPredictions => {
                let predictions = self
                    .executor
                    .build_predictions(&state.hypotheses, ctx)
                    .await?;
                Ok(Outcome::Continue(AchState {
                    phase: AchPhase::ScoreEliminate,
                    predictions,
                    ..state
                }))
            }
            AchPhase::ScoreEliminate => {
                let tested = self
                    .executor
                    .score_predictions(&state.hypotheses, &state.predictions, ctx)
                    .await?;
                let tests: Vec<HypothesisTest> = state
                    .hypotheses
                    .iter()
                    .enumerate()
                    .map(|(i, h)| {
                        AchScoringEngine::score_hypothesis(
                            format!("h-{i}"),
                            h.clone(),
                            tested.get(i).cloned().unwrap_or_default(),
                            Vec::new(),
                        )
                    })
                    .collect();
                let round = state.round + 1;
                let next_phase = if round >= state.max_rounds {
                    AchPhase::Close
                } else {
                    AchPhase::GenerateHypotheses
                };
                Ok(Outcome::Continue(AchState {
                    phase: next_phase,
                    tests,
                    round,
                    predictions: Vec::new(),
                    ..state
                }))
            }
            AchPhase::Close => Ok(Outcome::Halt(state.tests)),
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
    impl AchExecutor for StubExecutor {
        async fn generate_hypotheses(
            &self,
            _query: &str,
            _ctx: &StepContext,
        ) -> Result<Vec<String>, LoopError> {
            Ok(vec!["H1: memory leak".into(), "H2: deadlock".into()])
        }

        async fn build_predictions(
            &self,
            hypotheses: &[String],
            _ctx: &StepContext,
        ) -> Result<Vec<Vec<Prediction>>, LoopError> {
            Ok(hypotheses
                .iter()
                .map(|h| {
                    vec![Prediction {
                        claim: format!("if {h}, then pool grows"),
                        test_type: TestType::PatternPresence,
                        result: None,
                    }]
                })
                .collect())
        }

        async fn score_predictions(
            &self,
            _hypotheses: &[String],
            predictions: &[Vec<Prediction>],
            _ctx: &StepContext,
        ) -> Result<Vec<Vec<Prediction>>, LoopError> {
            Ok(predictions
                .iter()
                .map(|preds| {
                    preds
                        .iter()
                        .map(|p| Prediction {
                            result: Some(TestResult::Confirmed("found".into())),
                            ..p.clone()
                        })
                        .collect()
                })
                .collect())
        }
    }

    #[tokio::test]
    async fn ach_completes_one_round() {
        let runner = LoopRunner::new(AchStrategy::new(StubExecutor), Budget::unlimited());
        let mut stream = runner.run(AchState::new("timeout", 1), ChainContext::default(), None);

        let mut result = None;
        while let Some(step) = stream.next().await {
            let s = step.unwrap();
            if let Outcome::Halt(tests) = s.outcome {
                result = Some(tests);
            }
        }
        let tests = result.unwrap();
        assert_eq!(tests.len(), 2);
        assert!(tests[0].convergence_score > 0.0);
    }

    #[test]
    fn scoring_all_confirmed() {
        let preds = vec![
            Prediction {
                claim: "c1".into(),
                test_type: TestType::PatternPresence,
                result: Some(TestResult::Confirmed("ok".into())),
            },
            Prediction {
                claim: "c2".into(),
                test_type: TestType::PatternPresence,
                result: Some(TestResult::Confirmed("ok".into())),
            },
        ];
        let score = AchScoringEngine::compute_convergence(&preds, 0);
        assert!(
            (score - 1.0).abs() < f64::EPSILON,
            "all confirmed → score 1.0"
        );
    }

    #[test]
    fn scoring_refuted_penalises() {
        let preds = vec![
            Prediction {
                claim: "c1".into(),
                test_type: TestType::PatternPresence,
                result: Some(TestResult::Confirmed("ok".into())),
            },
            Prediction {
                claim: "c2".into(),
                test_type: TestType::PatternPresence,
                result: Some(TestResult::Refuted("no".into())),
            },
        ];
        let score = AchScoringEngine::compute_convergence(&preds, 0);
        assert!(score < 0.5, "refuted prediction should penalise score");
    }

    #[test]
    fn confidence_level_boundaries() {
        assert_eq!(
            ConfidenceLevel::from_score(0.1),
            ConfidenceLevel::Speculative
        );
        assert_eq!(ConfidenceLevel::from_score(0.5), ConfidenceLevel::Supported);
        assert_eq!(
            ConfidenceLevel::from_score(0.75),
            ConfidenceLevel::Convergent
        );
        assert_eq!(
            ConfidenceLevel::from_score(0.95),
            ConfidenceLevel::ProbableRootCause
        );
    }
}
