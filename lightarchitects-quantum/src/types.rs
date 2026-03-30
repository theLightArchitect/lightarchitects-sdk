//! Response types for `qsTools` actions.
//!
//! All 13 QUANTUM actions produce AI-generated investigation output. Every
//! response is wrapped in [`ActionOutput`] — a single `output: String` field
//! containing QUANTUM's findings, hypotheses, or analysis as prose.
//!
//! There are no structured-JSON responses in the QUANTUM protocol: the server
//! is an investigative AI that reasons over evidence, not a data-retrieval API.

// ── Response types ─────────────────────────────────────────────────────────────

/// Generic wrapper for all `qsTools` actions.
///
/// QUANTUM returns AI-generated investigation prose for every action. The
/// `output` field contains the full text — hypothesis chains, evidence
/// summaries, workflow status, or helix query results.
#[derive(Debug, Clone)]
pub struct ActionOutput {
    /// The full investigation response from QUANTUM.
    pub output: String,
}
