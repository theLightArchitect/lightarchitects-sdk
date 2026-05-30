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
        state: BcraState,
        _ctx: &StepContext,
    ) -> Result<Outcome<BcraState, BcraOutput>, LoopError> {
        // Phase 3 implements the full 6-phase step logic.
        Ok(Outcome::Halt(BcraOutput {
            final_phase: state.phase,
            assets: state.assets,
            threats: state.threats,
            evidence: state.evidence,
            blast_score: state.blast_score,
            declaration: String::new(),
        }))
    }

    fn name(&self) -> &'static str {
        "bcra"
    }
}
