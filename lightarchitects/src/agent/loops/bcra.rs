//! `BcraStrategy` — BCRA (Blast-Consequence-Risk-Action) risk analysis loop.
//!
//! A 6-phase L0 strategy following the FAIR/Bowtie compound blast-score model:
//! **Map** → **Pull** → **Score** → **Research** → **Prove** → **Declare**.
//!
//! L0 class: custom [`BcraState`] and [`BcraOutput`]; not registered in
//! [`RegisteredStrategy`]. Requires a [`BcraExecutor`] for LLM-backed phases.
//!
//! Full phase logic implemented in Phase 3.
//!
//! [`RegisteredStrategy`]: super::registry::RegisteredStrategy

use async_trait::async_trait;

use super::{
    error::LoopError,
    runner::{Outcome, StepContext, Strategy},
};

// ── Phase ─────────────────────────────────────────────────────────────────────

/// BCRA loop phases (0-based sequential execution).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BcraPhase {
    /// Phase 0: map the threat landscape and asset inventory.
    Map,
    /// Phase 1: pull threat intelligence data for enumerated assets.
    Pull,
    /// Phase 2: score each threat using FAIR/Bowtie blast-score model.
    Score,
    /// Phase 3: research high-score threats for deeper evidence.
    Research,
    /// Phase 4: prove or disprove each scored threat with evidence.
    Prove,
    /// Phase 5: declare final risk posture and recommended actions.
    Declare,
}

impl BcraPhase {
    /// Convert a 0-based index to the corresponding phase.
    #[must_use]
    pub fn from_index(n: u32) -> Option<Self> {
        match n {
            0 => Some(Self::Map),
            1 => Some(Self::Pull),
            2 => Some(Self::Score),
            3 => Some(Self::Research),
            4 => Some(Self::Prove),
            5 => Some(Self::Declare),
            _ => None,
        }
    }

    /// Phase index (0-based).
    #[must_use]
    pub fn index(self) -> u32 {
        match self {
            Self::Map => 0,
            Self::Pull => 1,
            Self::Score => 2,
            Self::Research => 3,
            Self::Prove => 4,
            Self::Declare => 5,
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

/// Mutable state threaded through each BCRA step.
#[derive(Debug, Clone)]
pub struct BcraState {
    /// Current phase.
    pub phase: BcraPhase,
    /// Asset inventory accumulated during the Map phase.
    pub assets: Vec<String>,
    /// Threat entries enumerated during Pull.
    pub threats: Vec<String>,
    /// Normalised blast score `[0.0, 1.0]` from the Score phase.
    pub blast_score: f64,
    /// Evidence strings gathered in Research and Prove.
    pub evidence: Vec<String>,
}

impl BcraState {
    /// Initialise at the Map phase with an empty inventory.
    #[must_use]
    pub fn new() -> Self {
        Self {
            phase: BcraPhase::Map,
            assets: Vec::new(),
            threats: Vec::new(),
            blast_score: 0.0,
            evidence: Vec::new(),
        }
    }
}

impl Default for BcraState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Output ────────────────────────────────────────────────────────────────────

/// Terminal output produced when `BcraStrategy` halts.
#[derive(Debug)]
pub struct BcraOutput {
    /// Final phase reached before halting.
    pub final_phase: BcraPhase,
    /// Accumulated assets identified during the Map phase.
    pub assets: Vec<String>,
    /// Threats enumerated during the Pull phase.
    pub threats: Vec<String>,
    /// Evidence gathered and validated during the Prove phase.
    pub evidence: Vec<String>,
    /// Final normalised blast score.
    pub blast_score: f64,
    /// Human-readable risk declaration (Declare phase output).
    pub declaration: String,
}

// ── Executor trait ────────────────────────────────────────────────────────────

/// Callback trait that handles LLM-backed phases of the BCRA loop.
///
/// Implementors supply the actual intelligence (threat research, evidence
/// gathering, declaration authoring). The strategy calls these methods at the
/// appropriate phase boundary.
///
/// Phase 3 provides the production implementation.
#[async_trait]
pub trait BcraExecutor: Send + Sync {
    /// Map: enumerate assets in scope.
    async fn map(&self, ctx: &StepContext) -> Result<Vec<String>, LoopError>;
    /// Pull: pull threat intelligence for the given assets.
    async fn pull(&self, assets: &[String], ctx: &StepContext) -> Result<Vec<String>, LoopError>;
    /// Score: compute normalised blast score `[0.0, 1.0]` for enumerated threats.
    async fn score(&self, threats: &[String], ctx: &StepContext) -> Result<f64, LoopError>;
    /// Research: gather deeper evidence for high-score threats.
    async fn research(
        &self,
        threats: &[String],
        score: f64,
        ctx: &StepContext,
    ) -> Result<Vec<String>, LoopError>;
    /// Prove: validate or refute each piece of evidence.
    async fn prove(&self, evidence: &[String], ctx: &StepContext)
    -> Result<Vec<String>, LoopError>;
    /// Declare: produce the final risk declaration.
    async fn declare(&self, state: &BcraState, ctx: &StepContext) -> Result<String, LoopError>;
}

// ── Strategy ──────────────────────────────────────────────────────────────────

/// Six-phase BCRA risk analysis loop.
///
/// Requires a [`BcraExecutor`] for LLM-backed phases.
/// Phase 3 implements the full `step()` logic.
pub struct BcraStrategy<E: BcraExecutor> {
    /// Executor responsible for LLM-backed phase work.
    pub executor: E,
}

impl<E: BcraExecutor> BcraStrategy<E> {
    /// Construct a strategy with the given executor.
    pub fn new(executor: E) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<E: BcraExecutor + 'static> Strategy for BcraStrategy<E> {
    type State = BcraState;
    type Output = BcraOutput;

    async fn step(
        &self,
        mut state: BcraState,
        ctx: &StepContext,
    ) -> Result<Outcome<BcraState, BcraOutput>, LoopError> {
        match state.phase {
            BcraPhase::Map => {
                state.assets = self.executor.map(ctx).await?;
                state.phase = BcraPhase::Pull;
                Ok(Outcome::Continue(state))
            }
            BcraPhase::Pull => {
                state.threats = self.executor.pull(&state.assets, ctx).await?;
                state.phase = BcraPhase::Score;
                Ok(Outcome::Continue(state))
            }
            BcraPhase::Score => {
                state.blast_score = self.executor.score(&state.threats, ctx).await?;
                state.phase = BcraPhase::Research;
                Ok(Outcome::Continue(state))
            }
            BcraPhase::Research => {
                let new_evidence = self
                    .executor
                    .research(&state.threats, state.blast_score, ctx)
                    .await?;
                state.evidence.extend(new_evidence);
                state.phase = BcraPhase::Prove;
                Ok(Outcome::Continue(state))
            }
            BcraPhase::Prove => {
                let proven = self.executor.prove(&state.evidence, ctx).await?;
                state.evidence = proven;
                state.phase = BcraPhase::Declare;
                Ok(Outcome::Continue(state))
            }
            BcraPhase::Declare => {
                let declaration = self.executor.declare(&state, ctx).await?;
                Ok(Outcome::Halt(BcraOutput {
                    final_phase: BcraPhase::Declare,
                    assets: state.assets,
                    threats: state.threats,
                    evidence: state.evidence,
                    blast_score: state.blast_score,
                    declaration,
                }))
            }
        }
    }

    fn name(&self) -> &'static str {
        "bcra"
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::agent::{ChainContext, loops::runner::StepContext};

    fn ctx() -> StepContext {
        StepContext {
            turn: 1,
            chain: ChainContext::default(),
            session_id: None,
        }
    }

    /// Executor that returns fixed deterministic data for each phase.
    struct FixedExecutor;

    #[async_trait]
    impl BcraExecutor for FixedExecutor {
        async fn map(&self, _ctx: &StepContext) -> Result<Vec<String>, LoopError> {
            Ok(vec!["asset-a".into(), "asset-b".into()])
        }

        async fn pull(
            &self,
            _assets: &[String],
            _ctx: &StepContext,
        ) -> Result<Vec<String>, LoopError> {
            Ok(vec!["threat-1".into()])
        }

        async fn score(&self, _threats: &[String], _ctx: &StepContext) -> Result<f64, LoopError> {
            Ok(0.75)
        }

        async fn research(
            &self,
            _threats: &[String],
            _score: f64,
            _ctx: &StepContext,
        ) -> Result<Vec<String>, LoopError> {
            Ok(vec!["evidence-A".into()])
        }

        async fn prove(
            &self,
            evidence: &[String],
            _ctx: &StepContext,
        ) -> Result<Vec<String>, LoopError> {
            Ok(evidence.to_vec())
        }

        async fn declare(
            &self,
            state: &BcraState,
            _ctx: &StepContext,
        ) -> Result<String, LoopError> {
            Ok(format!(
                "RISK HIGH blast={:.2} threats={}",
                state.blast_score,
                state.threats.len()
            ))
        }
    }

    #[tokio::test]
    async fn full_bcra_cycle_reaches_declare() {
        let strategy = BcraStrategy::new(FixedExecutor);
        let mut state = BcraState::new();

        // 5 Continue steps + 1 Halt.
        for _ in 0..=5 {
            match strategy.step(state.clone(), &ctx()).await.unwrap() {
                Outcome::Continue(s) => state = s,
                Outcome::Halt(out) => {
                    assert_eq!(out.final_phase, BcraPhase::Declare);
                    assert_eq!(out.assets.len(), 2);
                    assert_eq!(out.threats.len(), 1);
                    assert!((out.blast_score - 0.75).abs() < f64::EPSILON);
                    assert!(out.declaration.contains("blast=0.75"));
                    return;
                }
                Outcome::Pause(..) => panic!("BcraStrategy should not pause"),
            }
        }
        panic!("should have halted at Declare");
    }

    #[tokio::test]
    async fn map_phase_populates_assets() {
        let strategy = BcraStrategy::new(FixedExecutor);
        let state = BcraState::new();
        let next = match strategy.step(state, &ctx()).await.unwrap() {
            Outcome::Continue(s) => s,
            other => panic!("expected Continue after Map, got {other:?}"),
        };
        assert_eq!(next.phase, BcraPhase::Pull);
        assert_eq!(next.assets, vec!["asset-a", "asset-b"]);
    }

    #[tokio::test]
    async fn score_phase_stores_blast_score() {
        let strategy = BcraStrategy::new(FixedExecutor);
        // Advance to Score phase manually.
        let mut state = BcraState::new();
        state.assets = vec!["x".into()];
        state.threats = vec!["t".into()];
        state.phase = BcraPhase::Score;

        let next = match strategy.step(state, &ctx()).await.unwrap() {
            Outcome::Continue(s) => s,
            other => panic!("expected Continue after Score, got {other:?}"),
        };
        assert_eq!(next.phase, BcraPhase::Research);
        assert!((next.blast_score - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn bcra_phase_from_index_round_trips() {
        for i in 0..6u32 {
            assert!(BcraPhase::from_index(i).is_some());
        }
        assert!(BcraPhase::from_index(6).is_none());
    }
}
