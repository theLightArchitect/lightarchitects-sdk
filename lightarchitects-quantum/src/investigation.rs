//! [`QuantumInvestigation`] — stateful driver for the QUANTUM investigation lifecycle.
//!
//! QUANTUM's forensic investigation follows a seven-phase cycle:
//!
//! ```text
//! Initial ──triage()──► Triaged ──sweep()──► Swept ──trace()──► Traced
//!   ──probe()──► Probed ──theorize()──► Theorized ──verify()──► Verified
//!   ──close()──► Closed
//! ```
//!
//! [`QuantumInvestigation`] wraps a [`QuantumClient`] reference and tracks phase
//! progression client-side. Each phase method advances the machine and appends
//! the server response to the accumulated step history.
//!
//! Unlike SERAPH's strictly linear engagement, QUANTUM phases after `triage`
//! may be called in flexible order — the state machine records what was done
//! without enforcing strict sequencing beyond `Initial → Triaged` and
//! `Verified → Closed`.
//!
//! # Example
//!
//! ```no_run
//! # async fn example() -> Result<(), lightarchitects_core::SdkError> {
//! use lightarchitects_quantum::{QuantumClient, QuantumInvestigation};
//!
//! let client = QuantumClient::builder()
//!     .timeout(std::time::Duration::from_secs(120))
//!     .build()
//!     .await?;
//!
//! let mut inv = QuantumInvestigation::new(&client, "auth failures in prod");
//!
//! inv.triage().await?;
//! inv.sweep().await?;
//! inv.trace().await?;
//! inv.probe("JWT expiry logic").await?;
//! inv.theorize(None).await?;
//! inv.verify("clock skew root cause").await?;
//!
//! let report = inv.close("NTP drift confirmed on node-3").await?;
//! println!("{}", report.output);
//! # Ok(()) }
//! ```

use lightarchitects_core::error::SdkError;
use lightarchitects_core::transport::Transport;

use crate::client::QuantumClient;
use crate::types::ActionOutput;

// ── InvestigationPhase ─────────────────────────────────────────────────────────

/// Phase of a [`QuantumInvestigation`].
///
/// Progresses from `Initial` through each investigation step to `Closed`.
/// After `Triaged`, intermediate phases may be visited in any order — the state
/// machine records what was executed without enforcing strict sequencing.
/// Only `triage()` and `close()` enforce predecessor constraints.
///
/// **Forensic integrity note**: for evidence-grade investigations, follow the
/// canonical sequence (`triage → sweep → trace → probe → theorize → verify →
/// close`). The state machine accepts flexible ordering as a deliberate
/// design choice for exploratory work, but skipping phases weakens the evidence
/// chain.
///
/// # Matching
///
/// This enum is `#[non_exhaustive]` — exhaustive `match` arms require a `_ => {}` arm
/// even when all current variants are covered, because future QUANTUM versions may add
/// new investigation phases.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvestigationPhase {
    /// No action taken yet — `triage()` has not been called.
    Initial,
    /// Initial evidence gathered (`triage` called).
    Triaged,
    /// Broad evidence sweep completed.
    Swept,
    /// Evidence chain traced.
    Traced,
    /// Deep probe of a target completed.
    Probed,
    /// Hypotheses formed from evidence.
    Theorized,
    /// Hypotheses verified against signals.
    Verified,
    /// Investigation closed with final report.
    Closed,
}

// ── QuantumInvestigation ───────────────────────────────────────────────────────

/// Stateful driver for the QUANTUM forensic investigation lifecycle.
///
/// Tracks phase progression and accumulates step outputs across calls.
/// Construct via [`QuantumInvestigation::new`], then drive through phases
/// with the individual phase methods.
pub struct QuantumInvestigation<'a, T: Transport> {
    client: &'a QuantumClient<T>,
    subject: String,
    phase: InvestigationPhase,
    steps: Vec<ActionOutput>,
}

impl<'a, T: Transport> QuantumInvestigation<'a, T> {
    /// Create a new investigation for `subject`.
    ///
    /// `subject` describes what is being investigated (e.g. a bug, incident,
    /// or question). The investigation starts in [`InvestigationPhase::Initial`].
    #[must_use]
    pub fn new(client: &'a QuantumClient<T>, subject: impl Into<String>) -> Self {
        Self {
            client,
            subject: subject.into(),
            phase: InvestigationPhase::Initial,
            steps: Vec::new(),
        }
    }

    // ── State accessors ────────────────────────────────────────────────────────

    /// Current investigation phase.
    #[must_use]
    pub fn phase(&self) -> &InvestigationPhase {
        &self.phase
    }

    /// Investigation subject as supplied at construction.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// All step outputs accumulated so far, in chronological order.
    #[must_use]
    pub fn steps(&self) -> &[ActionOutput] {
        &self.steps
    }

    // ── Phase methods ──────────────────────────────────────────────────────────

    /// Phase 1 — initial evidence discovery (`triage`).
    ///
    /// Must be called first, while in [`InvestigationPhase::Initial`].
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if not in `Initial` phase.
    /// Returns a transport error if QUANTUM rejects the call.
    pub async fn triage(&mut self) -> Result<&ActionOutput, SdkError> {
        if self.phase != InvestigationPhase::Initial {
            return Err(SdkError::Config(format!(
                "triage() must be called first (current phase: {:?})",
                self.phase
            )));
        }
        let result = self.client.triage(&self.subject).await?;
        self.steps.push(ActionOutput {
            output: result.output,
        });
        self.phase = InvestigationPhase::Triaged;
        Self::last_or_err(&self.steps)
    }

    /// Phase 2 — broad evidence collection (`sweep`).
    ///
    /// Requires the investigation to have been triaged first.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if in `Initial` phase (triage not called).
    /// Returns a transport error if QUANTUM rejects the call.
    pub async fn sweep(&mut self) -> Result<&ActionOutput, SdkError> {
        self.require_past_initial("sweep")?;
        let result = self.client.sweep(&self.subject).await?;
        self.steps.push(ActionOutput {
            output: result.output,
        });
        self.phase = InvestigationPhase::Swept;
        Self::last_or_err(&self.steps)
    }

    /// Phase 3 — evidence chain tracing (`trace`).
    ///
    /// Requires the investigation to have been triaged first.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if in `Initial` phase.
    /// Returns a transport error if QUANTUM rejects the call.
    pub async fn trace(&mut self) -> Result<&ActionOutput, SdkError> {
        self.require_past_initial("trace")?;
        let result = self.client.trace(&self.subject).await?;
        self.steps.push(ActionOutput {
            output: result.output,
        });
        self.phase = InvestigationPhase::Traced;
        Self::last_or_err(&self.steps)
    }

    /// Phase 4 — deep probe of a specific `target`.
    ///
    /// `target` can be a file, symbol, process, or hypothesis to focus on.
    /// Requires the investigation to have been triaged first.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if in `Initial` phase.
    /// Returns a transport error if QUANTUM rejects the call.
    pub async fn probe(&mut self, target: &str) -> Result<&ActionOutput, SdkError> {
        self.require_past_initial("probe")?;
        let result = self.client.probe(target).await?;
        self.steps.push(ActionOutput {
            output: result.output,
        });
        self.phase = InvestigationPhase::Probed;
        Self::last_or_err(&self.steps)
    }

    /// Phase 5 — hypothesis generation from accumulated evidence.
    ///
    /// Optional `context` provides additional framing.
    /// Requires the investigation to have been triaged first.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if in `Initial` phase.
    /// Returns a transport error if QUANTUM rejects the call.
    pub async fn theorize(&mut self, context: Option<&str>) -> Result<&ActionOutput, SdkError> {
        self.require_past_initial("theorize")?;
        let result = self.client.theorize(&self.subject, context).await?;
        self.steps.push(ActionOutput {
            output: result.output,
        });
        self.phase = InvestigationPhase::Theorized;
        Self::last_or_err(&self.steps)
    }

    /// Phase 6 — verify `hypothesis` against available evidence.
    ///
    /// Requires the investigation to have been triaged first.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if in `Initial` phase.
    /// Returns a transport error if QUANTUM rejects the call.
    pub async fn verify(&mut self, hypothesis: &str) -> Result<&ActionOutput, SdkError> {
        self.require_past_initial("verify")?;
        let result = self.client.verify(hypothesis).await?;
        self.steps.push(ActionOutput {
            output: result.output,
        });
        self.phase = InvestigationPhase::Verified;
        Self::last_or_err(&self.steps)
    }

    /// Phase 7 — close investigation with `summary` and produce final report.
    ///
    /// Must be called after at least `triage()`. Cannot be called when still
    /// in `Initial` phase.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if in `Initial` or already `Closed` phase.
    /// Returns a transport error if QUANTUM rejects the call.
    pub async fn close(&mut self, summary: &str) -> Result<&ActionOutput, SdkError> {
        if self.phase == InvestigationPhase::Initial {
            return Err(SdkError::Config(
                "close() requires at least one prior phase (call triage() first)".to_owned(),
            ));
        }
        if self.phase == InvestigationPhase::Closed {
            return Err(SdkError::Config(
                "investigation is already closed".to_owned(),
            ));
        }
        let result = self.client.close(summary).await?;
        self.steps.push(ActionOutput {
            output: result.output,
        });
        self.phase = InvestigationPhase::Closed;
        Self::last_or_err(&self.steps)
    }

    // ── Internal helpers ───────────────────────────────────────────────────────

    /// Returns a reference to the last element, or an error if the collection
    /// is unexpectedly empty. Used after a `push()` call where the collection
    /// cannot logically be empty — avoids `.expect()` in production code.
    fn last_or_err(slice: &[ActionOutput]) -> Result<&ActionOutput, SdkError> {
        slice.last().ok_or_else(|| {
            SdkError::Config("investigation step collection was unexpectedly empty".to_owned())
        })
    }

    fn require_past_initial(&self, method: &str) -> Result<(), SdkError> {
        if self.phase == InvestigationPhase::Initial {
            Err(SdkError::Config(format!(
                "{method}() requires triage() to be called first"
            )))
        } else {
            Ok(())
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_enum_initial_not_eq_triaged() {
        assert_ne!(InvestigationPhase::Initial, InvestigationPhase::Triaged);
    }

    #[test]
    fn phase_enum_closed_eq_closed() {
        assert_eq!(InvestigationPhase::Closed, InvestigationPhase::Closed);
    }
}
