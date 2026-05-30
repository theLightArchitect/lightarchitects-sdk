//! `RedTeamStrategy` — SERAPH 5-phase red-team assessment loop.
//!
//! Phases: **Hydrate** → **Surface** → **Probe** → **Chain** → **Verdict**.
//!
//! The Hydrate phase is mandatory for control-anchor loading (SERAPH SKILL.md
//! §0 requirement). Phase 0 MUST NOT be skipped.
//!
//! L0 class: custom [`RedTeamState`] and [`RedTeamOutput`]; not registered in
//! `RegisteredStrategy`. Requires a [`RedTeamExecutor`] for LLM-backed phases.
//!
//! Full phase logic implemented in Phase 3.

use async_trait::async_trait;

use super::{
    error::LoopError,
    runner::{Outcome, StepContext, Strategy},
};

// ── Phase ─────────────────────────────────────────────────────────────────────

/// SERAPH red-team phases (5-phase mandatory sequence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RedTeamPhase {
    /// Phase 0 — load engagement scope and control anchors (MANDATORY).
    Hydrate,
    /// Phase 1 — enumerate the attack surface.
    Surface,
    /// Phase 2 — actively probe identified vulnerabilities.
    Probe,
    /// Phase 3 — chain exploits into a realistic attack narrative.
    Chain,
    /// Phase 4 — produce the security verdict and recommendations.
    Verdict,
}

impl RedTeamPhase {
    /// 0-based index.
    #[must_use]
    pub fn index(self) -> u32 {
        match self {
            Self::Hydrate => 0,
            Self::Surface => 1,
            Self::Probe => 2,
            Self::Chain => 3,
            Self::Verdict => 4,
        }
    }

    /// Convert 0-based index to phase.
    #[must_use]
    pub fn from_index(n: u32) -> Option<Self> {
        match n {
            0 => Some(Self::Hydrate),
            1 => Some(Self::Surface),
            2 => Some(Self::Probe),
            3 => Some(Self::Chain),
            4 => Some(Self::Verdict),
            _ => None,
        }
    }

    /// Short label for AYIN spans.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Hydrate => "hydrate",
            Self::Surface => "surface",
            Self::Probe => "probe",
            Self::Chain => "chain",
            Self::Verdict => "verdict",
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

/// Mutable state threaded through each red-team step.
#[derive(Debug, Clone)]
pub struct RedTeamState {
    /// Current phase.
    pub phase: RedTeamPhase,
    /// Engagement scope loaded during Hydrate.
    pub scope: String,
    /// Control anchors loaded during Hydrate.
    pub control_anchors: Vec<String>,
    /// Attack surface entries identified during Surface.
    pub attack_surface: Vec<String>,
    /// Probe findings gathered during Probe.
    pub probe_findings: Vec<String>,
    /// Chained exploit narrative from Chain.
    pub exploit_chain: String,
}

impl RedTeamState {
    /// Initialise at the Hydrate phase (mandatory starting point).
    #[must_use]
    pub fn new(scope: impl Into<String>) -> Self {
        Self {
            phase: RedTeamPhase::Hydrate,
            scope: scope.into(),
            control_anchors: Vec::new(),
            attack_surface: Vec::new(),
            probe_findings: Vec::new(),
            exploit_chain: String::new(),
        }
    }
}

// ── Output ────────────────────────────────────────────────────────────────────

/// Terminal output produced when `RedTeamStrategy` halts.
#[derive(Debug)]
pub struct RedTeamOutput {
    /// Final phase reached.
    pub final_phase: RedTeamPhase,
    /// Attack surface enumerated.
    pub attack_surface: Vec<String>,
    /// Chained exploit narrative.
    pub exploit_chain: String,
    /// Security verdict and recommendations.
    pub verdict: String,
}

// ── Executor trait ────────────────────────────────────────────────────────────

/// Callback trait for LLM-backed red-team phases.
///
/// Phase 3 provides the production implementation.
#[async_trait]
pub trait RedTeamExecutor: Send + Sync {
    /// Hydrate: load scope definition and control anchors.
    async fn hydrate(&self, scope: &str, ctx: &StepContext) -> Result<Vec<String>, LoopError>;
    /// Surface: enumerate the attack surface for the given scope and anchors.
    async fn surface(
        &self,
        scope: &str,
        anchors: &[String],
        ctx: &StepContext,
    ) -> Result<Vec<String>, LoopError>;
    /// Probe: actively test identified attack surface entries.
    async fn probe(&self, surface: &[String], ctx: &StepContext) -> Result<Vec<String>, LoopError>;
    /// Chain: construct a chained exploit narrative from probe findings.
    async fn chain(&self, findings: &[String], ctx: &StepContext) -> Result<String, LoopError>;
    /// Verdict: produce the final security verdict and recommendations.
    async fn verdict(&self, state: &RedTeamState, ctx: &StepContext) -> Result<String, LoopError>;
}

// ── Strategy ──────────────────────────────────────────────────────────────────

/// Five-phase SERAPH red-team assessment loop.
///
/// Requires a [`RedTeamExecutor`] for LLM-backed phases.
/// Phase 3 implements the full `step()` logic.
pub struct RedTeamStrategy<E: RedTeamExecutor> {
    /// Executor responsible for LLM-backed phase work.
    pub executor: E,
}

impl<E: RedTeamExecutor> RedTeamStrategy<E> {
    /// Construct a strategy with the given executor.
    pub fn new(executor: E) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<E: RedTeamExecutor + 'static> Strategy for RedTeamStrategy<E> {
    type State = RedTeamState;
    type Output = RedTeamOutput;

    async fn step(
        &self,
        state: RedTeamState,
        _ctx: &StepContext,
    ) -> Result<Outcome<RedTeamState, RedTeamOutput>, LoopError> {
        // Phase 3 implements the full 5-phase red-team step logic.
        Ok(Outcome::Halt(RedTeamOutput {
            final_phase: state.phase,
            attack_surface: state.attack_surface,
            exploit_chain: state.exploit_chain,
            verdict: String::new(),
        }))
    }

    fn name(&self) -> &'static str {
        "red_team"
    }
}
