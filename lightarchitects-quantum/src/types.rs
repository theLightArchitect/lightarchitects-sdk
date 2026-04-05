//! Response types and investigation state tracking for `qsTools` actions.
//!
//! # Response types
//!
//! All 13 QUANTUM actions produce AI-generated investigation prose. Each action
//! has its own typed result struct — a single `output: String` field containing
//! QUANTUM's findings, hypotheses, or analysis. There are no structured-JSON
//! responses in the QUANTUM protocol: the server is an investigative AI that
//! reasons over evidence, not a data-retrieval API.
//!
//! # Investigation state tracking
//!
//! [`InvestigationState`] is a lightweight client-side tracker for multi-call
//! QUANTUM investigation sessions. It records phase progression and a bounded
//! history of phase summaries without retaining raw MCP response text.
//!
//! Unlike [`crate::QuantumInvestigation`], which is a stateful *driver* that
//! owns the [`crate::QuantumClient`] and makes MCP calls, `InvestigationState`
//! is purely a *tracker* — it records what happened and is used independently
//! of the transport layer.

use std::fmt;

use lightarchitects_core::error::SdkError;

use crate::investigation::InvestigationPhase;

// ── Per-action response types ──────────────────────────────────────────────────

/// Response from the `triage` action (Phase 1 — initial evidence discovery).
#[derive(Debug, Clone)]
pub struct TriageResult {
    /// AI-generated triage findings from QUANTUM.
    pub output: String,
}

/// Response from the `sweep` action (Phase 2 — broad evidence collection).
#[derive(Debug, Clone)]
pub struct SweepResult {
    /// AI-generated sweep summary from QUANTUM.
    pub output: String,
}

/// Response from the `trace` action (Phase 3 — evidence chain tracing).
#[derive(Debug, Clone)]
pub struct TraceResult {
    /// AI-generated trace output from QUANTUM.
    pub output: String,
}

/// Response from the `probe` action (Phase 4 — deep target analysis).
#[derive(Debug, Clone)]
pub struct ProbeResult {
    /// AI-generated probe findings from QUANTUM.
    pub output: String,
}

/// Response from the `theorize` action (Phase 5 — hypothesis generation).
#[derive(Debug, Clone)]
pub struct TheorizeResult {
    /// AI-generated hypothesis chain from QUANTUM.
    pub output: String,
}

/// Response from the `verify` action (Phase 6 — hypothesis verification).
#[derive(Debug, Clone)]
pub struct VerifyResult {
    /// AI-generated verification verdict from QUANTUM.
    pub output: String,
}

/// Response from the `close` action (Phase 7 — final report).
#[derive(Debug, Clone)]
pub struct CloseResult {
    /// AI-generated final investigation report from QUANTUM.
    pub output: String,
}

/// Response from the `quick` action (abbreviated investigation cycle).
#[derive(Debug, Clone)]
pub struct QuickResult {
    /// AI-generated compressed investigation report from QUANTUM.
    pub output: String,
}

/// Response from the `research` action (multi-source knowledge retrieval).
#[derive(Debug, Clone)]
pub struct ResearchResult {
    /// AI-generated research synthesis from QUANTUM.
    pub output: String,
}

/// Response from the `helix` action (SOUL helix vault query).
#[derive(Debug, Clone)]
pub struct HelixResult {
    /// AI-generated helix query response from QUANTUM.
    pub output: String,
}

/// Response from the `discover` action (pattern recognition).
#[derive(Debug, Clone)]
pub struct DiscoverResult {
    /// AI-generated pattern discovery output from QUANTUM.
    pub output: String,
}

/// Response from the `list` action (investigation history).
#[derive(Debug, Clone)]
pub struct ListResult {
    /// AI-generated investigation listing from QUANTUM.
    pub output: String,
}

/// Response from the `workflow` action (named investigation sequence).
#[derive(Debug, Clone)]
pub struct WorkflowResult {
    /// AI-generated workflow execution summary from QUANTUM.
    pub output: String,
}

/// Generic wrapper used by [`crate::QuantumInvestigation`] step history.
///
/// QUANTUM returns AI-generated investigation prose for every action. The
/// `output` field contains the full text — hypothesis chains, evidence
/// summaries, workflow status, or helix query results.
#[derive(Debug, Clone)]
pub struct ActionOutput {
    /// The full investigation response from QUANTUM.
    pub output: String,
}

// ── InvestigationState ─────────────────────────────────────────────────────────

/// Maximum number of phase advances allowed on a single [`InvestigationState`].
///
/// [`InvestigationState::advance`] returns [`SdkError::Config`] when this
/// limit is reached, preventing unbounded investigation loops.
pub const MAX_ADVANCE_STEPS: u32 = 1_000;

/// One recorded step in an [`InvestigationState`] phase history.
///
/// # Security
///
/// The `summary` field contains caller-supplied text derived from QUANTUM
/// response output. This data is **UNTRUSTED** — callers must not evaluate,
/// execute, or interpolate it into shell commands or SQL queries without
/// sanitisation.
#[derive(Debug, Clone)]
pub struct PhaseRecord {
    /// The investigation phase at the time this record was written.
    pub phase: InvestigationPhase,
    /// Caller-supplied summary of the phase output.
    ///
    /// # Security
    ///
    /// **UNTRUSTED** — derived from AI-generated QUANTUM output. Do not
    /// evaluate, execute, or interpolate without sanitisation.
    pub summary: String,
}

/// Lightweight client-side tracker for a QUANTUM investigation session.
///
/// Tracks phase progression, evidence count, and confidence across multiple
/// MCP calls without retaining raw response text. Used alongside
/// [`crate::QuantumClient`] method calls when you need to persist state across
/// a session without using the higher-level [`crate::QuantumInvestigation`]
/// driver.
///
/// # Example
///
/// ```rust
/// use lightarchitects_quantum::types::{InvestigationState, MAX_ADVANCE_STEPS};
/// use lightarchitects_quantum::investigation::InvestigationPhase;
///
/// let mut state = InvestigationState::new();
/// assert_eq!(*state.phase(), InvestigationPhase::Initial);
///
/// state.advance(InvestigationPhase::Triaged, "Found 3 signals in auth subsystem").unwrap();
/// state.advance(InvestigationPhase::Swept, "Expanded to 12 correlated signals").unwrap();
///
/// assert_eq!(state.evidence_count(), 2);
/// assert_eq!(state.history().len(), 2);
/// println!("{state}");
/// ```
///
/// # Bounds
///
/// `history` is pre-allocated with capacity 8 (typical investigation length).
/// [`InvestigationState::advance`] returns [`SdkError::Config`] after
/// [`MAX_ADVANCE_STEPS`] calls — it never panics.
#[derive(Debug, Clone)]
pub struct InvestigationState {
    /// Current investigation phase.
    phase: InvestigationPhase,
    /// Number of phases advanced so far.
    evidence_count: u32,
    /// Confidence estimate in the current investigation (0.0–1.0).
    ///
    /// Set by the caller — QUANTUM does not emit a structured confidence value.
    pub confidence: f32,
    /// Bounded phase history (private — access via [`InvestigationState::history`]).
    ///
    /// Pre-allocated with capacity 8. Capped at [`MAX_ADVANCE_STEPS`] entries.
    history: Vec<PhaseRecord>,
}

impl Default for InvestigationState {
    fn default() -> Self {
        Self::new()
    }
}

impl InvestigationState {
    /// Create a new `InvestigationState` in [`InvestigationPhase::Initial`].
    ///
    /// `history` is pre-allocated with capacity 8 (typical investigation length).
    #[must_use]
    pub fn new() -> Self {
        Self {
            phase: InvestigationPhase::Initial,
            evidence_count: 0,
            confidence: 0.0,
            history: Vec::with_capacity(8),
        }
    }

    /// Advance to `phase` and record `summary` in the phase history.
    ///
    /// Increments `evidence_count` on each call.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] when [`MAX_ADVANCE_STEPS`] has been
    /// reached, preventing unbounded investigation loops. Never panics.
    pub fn advance(
        &mut self,
        phase: InvestigationPhase,
        summary: impl Into<String>,
    ) -> Result<(), SdkError> {
        if self.evidence_count >= MAX_ADVANCE_STEPS {
            return Err(SdkError::Config(format!(
                "advance() limit reached ({MAX_ADVANCE_STEPS} steps); \
                 create a new InvestigationState to continue"
            )));
        }
        self.phase = phase.clone();
        self.history.push(PhaseRecord {
            phase,
            summary: summary.into(),
        });
        self.evidence_count += 1;
        Ok(())
    }

    /// Immutable view of the phase history in chronological order.
    #[must_use]
    pub fn history(&self) -> &[PhaseRecord] {
        &self.history
    }

    /// Current investigation phase (borrow).
    #[must_use]
    pub fn phase(&self) -> &InvestigationPhase {
        &self.phase
    }

    /// Number of phases advanced so far.
    #[must_use]
    pub fn evidence_count(&self) -> u32 {
        self.evidence_count
    }
}

impl fmt::Display for InvestigationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "InvestigationState {{ phase: {:?}, steps: {}, confidence: {:.0}% }}",
            self.phase,
            self.evidence_count,
            self.confidence * 100.0,
        )
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::float_cmp)]
    fn new_starts_in_initial_phase() {
        let state = InvestigationState::new();
        assert_eq!(*state.phase(), InvestigationPhase::Initial);
        assert_eq!(state.evidence_count(), 0);
        assert_eq!(state.confidence, 0.0);
        assert!(state.history().is_empty());
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn default_matches_new() {
        let a = InvestigationState::new();
        let b = InvestigationState::default();
        assert_eq!(a.evidence_count(), b.evidence_count());
        assert_eq!(a.confidence, b.confidence);
    }

    #[test]
    fn advance_records_phase_and_summary() {
        let mut state = InvestigationState::new();
        state
            .advance(InvestigationPhase::Triaged, "3 signals found")
            .unwrap();
        assert_eq!(*state.phase(), InvestigationPhase::Triaged);
        assert_eq!(state.evidence_count(), 1);
        assert_eq!(state.history().len(), 1);
        assert_eq!(state.history()[0].summary, "3 signals found");
    }

    #[test]
    fn advance_multiple_phases_accumulates_history() {
        let mut state = InvestigationState::new();
        state
            .advance(InvestigationPhase::Triaged, "triage done")
            .unwrap();
        state
            .advance(InvestigationPhase::Swept, "sweep done")
            .unwrap();
        state
            .advance(InvestigationPhase::Theorized, "theory formed")
            .unwrap();
        assert_eq!(state.evidence_count(), 3);
        assert_eq!(state.history().len(), 3);
        assert_eq!(*state.phase(), InvestigationPhase::Theorized);
    }

    #[test]
    fn advance_returns_error_at_max_steps() {
        let mut state = InvestigationState::new();
        // Advance to the limit.
        for i in 0..MAX_ADVANCE_STEPS {
            state
                .advance(InvestigationPhase::Triaged, format!("step {i}"))
                .unwrap();
        }
        // One more must return Err, not panic.
        let err = state
            .advance(InvestigationPhase::Swept, "over limit")
            .unwrap_err();
        assert!(
            matches!(err, SdkError::Config(_)),
            "expected Config error, got: {err:?}"
        );
    }

    #[test]
    fn display_shows_phase_steps_confidence() {
        let mut state = InvestigationState::new();
        state.confidence = 0.75;
        state.advance(InvestigationPhase::Triaged, "done").unwrap();
        let text = state.to_string();
        assert!(text.contains("Triaged"), "got: {text}");
        assert!(text.contains("75%"), "got: {text}");
        assert!(text.contains("steps: 1"), "got: {text}");
    }

    #[test]
    fn history_capacity_preallocated() {
        let state = InvestigationState::new();
        // Vec::with_capacity(8) guarantees capacity >= 8.
        assert!(
            state.history.capacity() >= 8,
            "expected capacity >= 8, got {}",
            state.history.capacity()
        );
    }
}
