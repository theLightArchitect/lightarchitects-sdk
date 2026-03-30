//! Response types and operation enums for `penTools` actions.
//!
//! All 18 SERAPH actions produce AI-generated offensive security output.
//! Every response is wrapped in [`ActionOutput`].
//!
//! Operation enums enumerate the valid sub-operations for actions that require
//! them, preventing typos and documenting the complete set of supported values
//! at compile time.

// ── Operation enums ───────────────────────────────────────────────────────────

/// Wing selection for [`crate::SeraphClient::execute`].
///
/// Maps to the 6 SERAPH offensive wings. Each wing represents a distinct
/// attack-surface capability within an authorised engagement.
#[derive(Debug, Clone, Copy)]
pub enum Wing {
    /// Packet capture and traffic interception.
    Capture,
    /// Host and service discovery.
    Scan,
    /// Artefact and binary analysis.
    Analyze,
    /// Open-source intelligence gathering.
    Osint,
    /// Continuous network monitoring.
    Monitor,
    /// Exploitation and payload delivery.
    Execute,
}

impl Wing {
    /// Serialize to the string SERAPH expects in the `wing` field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Scan => "scan",
            Self::Analyze => "analyze",
            Self::Osint => "osint",
            Self::Monitor => "monitor",
            Self::Execute => "execute",
        }
    }
}

// ── Response types ─────────────────────────────────────────────────────────────

/// Generic wrapper for all `penTools` actions.
///
/// SERAPH returns AI-generated offensive-security prose for every action.
/// The `output` field contains the full text — recon reports, vulnerability
/// findings, investigation notes, or operational status.
#[derive(Debug, Clone)]
pub struct ActionOutput {
    /// The full pentest response from SERAPH.
    pub output: String,
}
