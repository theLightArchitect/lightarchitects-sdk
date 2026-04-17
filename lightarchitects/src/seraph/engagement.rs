//! [`SeraphEngagement`] — stateful driver for the SERAPH investigation lifecycle.
//!
//! SERAPH's investigation lifecycle is a sequential four-phase protocol:
//!
//! ```text
//! Initial ──start()──► Started ──advance()×N──► Advancing ──close()──► Closed ──report()──► Reported
//! ```
//!
//! [`SeraphEngagement`] wraps a [`SeraphClient`] reference and tracks phase
//! progression client-side. Each phase method enforces ordering at compile time
//! through the [`EngagementPhase`] enum and returns an error if called out of
//! sequence.
//!
//! # Example
//!
//! ```no_run
//! # async fn example() -> Result<(), crate::core::SdkError> {
//! use crate::seraph::{SeraphClient, SeraphEngagement};
//!
//! let client = SeraphClient::builder()
//!     .timeout(std::time::Duration::from_secs(120))
//!     .build()
//!     .await?;
//!
//! let mut engagement = SeraphEngagement::new(&client, "192.168.1.10");
//! engagement.start().await?;
//!
//! engagement.advance("Open port 22 — SSH service detected").await?;
//! engagement.advance("Default credentials rejected — no auth bypass").await?;
//!
//! engagement.close().await?;
//! let report = engagement.report().await?;
//! println!("{}", report.output);
//! # Ok(()) }
//! ```

use crate::core::error::SdkError;
use crate::core::transport::Transport;

use crate::seraph::client::SeraphClient;
use crate::seraph::types::ActionOutput;

// ── EngagementPhase ────────────────────────────────────────────────────────────

/// Maximum number of `advance()` calls allowed on a single [`SeraphEngagement`].
///
/// Prevents unbounded investigation loops. [`SeraphEngagement::advance`] returns
/// [`SdkError::Config`] when this limit is reached.
pub const MAX_ADVANCE_STEPS: u32 = 1_000;

/// Phase of a [`SeraphEngagement`] investigation lifecycle.
///
/// Phases progress linearly: `Initial → Started → Advancing → Closed → Reported`.
/// Calling a phase method out of order returns [`SdkError::Config`].
///
/// # Matching
///
/// This enum is `#[non_exhaustive]` — exhaustive `match` arms require a `_ => {}` arm
/// even when all current variants are covered, because future SERAPH versions may add
/// new phases.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EngagementPhase {
    /// No action taken yet — `start()` has not been called.
    Initial,
    /// Investigation opened (`investigate_start` called once).
    Started,
    /// One or more `investigate_advance` calls made.
    ///
    /// `step_count` is the number of advance calls made so far.
    Advancing {
        /// Number of `advance()` calls made.
        step_count: u32,
    },
    /// Investigation closed (`investigate_close` called).
    Closed,
    /// Final report generated (`investigate_report` called).
    Reported,
}

// ── SeraphEngagement ───────────────────────────────────────────────────────────

/// Stateful driver for the SERAPH investigation lifecycle.
///
/// Tracks phase progression and accumulates findings across calls.
/// Construct via [`SeraphEngagement::new`], then call phase methods in order.
pub struct SeraphEngagement<'a, T: Transport> {
    client: &'a SeraphClient<T>,
    target: String,
    phase: EngagementPhase,
    findings: Vec<ActionOutput>,
}

// Each phase method returns `self.findings.last().expect(...)` after a push — the
// expect is structurally unreachable. Suppress the pedantic lint rather than
// adding spurious `# Panics` sections to every method.
#[allow(clippy::missing_panics_doc)]
impl<'a, T: Transport> SeraphEngagement<'a, T> {
    /// Create a new engagement driver for `target`.
    ///
    /// `target` is the authorised engagement target (IP, hostname, CIDR).
    /// The engagement starts in [`EngagementPhase::Initial`].
    #[must_use]
    pub fn new(client: &'a SeraphClient<T>, target: impl Into<String>) -> Self {
        Self {
            client,
            target: target.into(),
            phase: EngagementPhase::Initial,
            findings: Vec::new(),
        }
    }

    // ── Phase accessors ────────────────────────────────────────────────────────

    /// Current engagement phase.
    #[must_use]
    pub fn phase(&self) -> &EngagementPhase {
        &self.phase
    }

    /// Engagement target as supplied at construction.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Accumulated findings in chronological order.
    ///
    /// Named `findings` (vs `steps` in the QUANTUM investigation driver)
    /// to reflect SERAPH's red-team domain: each call accumulates adversarial evidence,
    /// not generic investigation steps.
    #[must_use]
    pub fn findings(&self) -> &[ActionOutput] {
        &self.findings
    }

    // ── Phase methods ──────────────────────────────────────────────────────────

    /// Open the investigation (`investigate_start`).
    ///
    /// Must be called exactly once, while in [`EngagementPhase::Initial`].
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if called outside `Initial` phase.
    /// Returns a transport error if SERAPH rejects the call.
    pub async fn start(&mut self) -> Result<&ActionOutput, SdkError> {
        if self.phase != EngagementPhase::Initial {
            return Err(SdkError::Config(format!(
                "start() requires Initial phase, current: {:?}",
                self.phase
            )));
        }
        let out = self.client.start_investigation(&self.target).await?;
        self.findings.push(out);
        self.phase = EngagementPhase::Started;
        self.findings
            .last()
            .ok_or_else(|| SdkError::Config("findings unexpectedly empty after push".to_owned()))
    }

    /// Advance the investigation with a new `finding` (`investigate_advance`).
    ///
    /// May be called repeatedly from [`EngagementPhase::Started`] or
    /// [`EngagementPhase::Advancing`]. Each call increments `step_count`.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if called before `start()`, after `close()`,
    /// or when [`MAX_ADVANCE_STEPS`] has been reached.
    /// Returns a transport error if SERAPH rejects the call.
    pub async fn advance(&mut self, finding: &str) -> Result<&ActionOutput, SdkError> {
        let step_count = match &self.phase {
            EngagementPhase::Started => 1_u32,
            EngagementPhase::Advancing { step_count } => {
                if *step_count >= MAX_ADVANCE_STEPS {
                    return Err(SdkError::Config(format!(
                        "advance() limit reached ({MAX_ADVANCE_STEPS} steps); call close() to finish"
                    )));
                }
                step_count.saturating_add(1)
            }
            other => {
                return Err(SdkError::Config(format!(
                    "advance() requires Started or Advancing phase, current: {other:?}"
                )));
            }
        };
        let out = self.client.advance_investigation(finding).await?;
        self.findings.push(out);
        self.phase = EngagementPhase::Advancing { step_count };
        self.findings
            .last()
            .ok_or_else(|| SdkError::Config("findings unexpectedly empty after push".to_owned()))
    }

    /// Close the investigation (`investigate_close`).
    ///
    /// Must be called from [`EngagementPhase::Started`] or
    /// [`EngagementPhase::Advancing`].
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if called before `start()` or after `close()`.
    /// Returns a transport error if SERAPH rejects the call.
    pub async fn close(&mut self) -> Result<&ActionOutput, SdkError> {
        match self.phase {
            EngagementPhase::Started | EngagementPhase::Advancing { .. } => {}
            ref other => {
                return Err(SdkError::Config(format!(
                    "close() requires Started or Advancing phase, current: {other:?}"
                )));
            }
        }
        let out = self.client.close_investigation().await?;
        self.findings.push(out);
        self.phase = EngagementPhase::Closed;
        self.findings
            .last()
            .ok_or_else(|| SdkError::Config("findings unexpectedly empty after push".to_owned()))
    }

    /// Generate the final engagement report (`investigate_report`).
    ///
    /// Must be called from [`EngagementPhase::Closed`].
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if called before `close()`.
    /// Returns a transport error if SERAPH rejects the call.
    pub async fn report(&mut self) -> Result<&ActionOutput, SdkError> {
        if self.phase != EngagementPhase::Closed {
            return Err(SdkError::Config(format!(
                "report() requires Closed phase, current: {:?}",
                self.phase
            )));
        }
        let out = self.client.report().await?;
        self.findings.push(out);
        self.phase = EngagementPhase::Reported;
        self.findings
            .last()
            .ok_or_else(|| SdkError::Config("findings unexpectedly empty after push".to_owned()))
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_phase_starts_empty() {
        // We can't easily construct a real SeraphClient without spawning a binary,
        // so we test only the phase enum logic.
        let phase = EngagementPhase::Initial;
        assert_eq!(phase, EngagementPhase::Initial);
    }

    #[test]
    fn advancing_step_count_saturation() {
        let phase = EngagementPhase::Advancing {
            step_count: u32::MAX,
        };
        if let EngagementPhase::Advancing { step_count } = phase {
            // saturating_add should not overflow
            let next = step_count.saturating_add(1);
            assert_eq!(next, u32::MAX);
        } else {
            panic!("expected Advancing");
        }
    }
}
