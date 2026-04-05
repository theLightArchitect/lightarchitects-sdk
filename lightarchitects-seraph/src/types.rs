//! Response types and operation enums for `penTools` actions.
//!
//! All 18 SERAPH actions produce AI-generated offensive security output.
//! Every response is wrapped in [`ActionOutput`].
//!
//! Operation enums enumerate the valid sub-operations for actions that require
//! them, preventing typos and documenting the complete set of supported values
//! at compile time.
//!
//! # Wing response types
//!
//! Six typed result structs map to the six SERAPH pentest lifecycle phases:
//! [`ScopeResult`], [`ReconResult`], [`SurveyResult`], [`ExamineResult`],
//! [`StrikeResult`], and [`ReportResult`].

use std::time::Duration;

// ── Operation enums ───────────────────────────────────────────────────────────

/// Wing selection for [`crate::SeraphClient::execute`].
///
/// Maps to the 6 SERAPH offensive wings. Each wing represents a distinct
/// attack-surface capability within an authorised engagement.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
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

// ── Typed wing response types ──────────────────────────────────────────────────

/// Scope authorization verdict returned by a scope-check action.
///
/// # Security constraints
///
/// This type is `#[must_use]`: callers that ignore the scope verdict receive a
/// compiler warning, preventing silent authorization bypass.
///
/// This type does **not** implement `Serialize` — serializing scope data risks
/// leaking authorization verdicts, TTL values, or engagement identifiers into
/// logs or network responses.
///
/// TTL is stored as a [`Duration`] computed at parse time — no I/O is performed
/// at verdict-check time.
#[must_use = "scope authorization verdict must be checked — ignoring it bypasses scope gate"]
#[derive(Debug, Clone)]
pub struct ScopeResult {
    /// The full scope authorization response from SERAPH.
    pub output: String,
    /// Whether the scope governor authorized the requested action.
    authorized: bool,
    /// Remaining time-to-live on the engagement scope at the time of this check.
    ttl_remaining: Duration,
}

impl ScopeResult {
    /// Construct a `ScopeResult` from SERAPH's prose response.
    ///
    /// `authorized` should be `true` only when the server confirms the action
    /// is within scope. `ttl_remaining` is the Duration remaining on the
    /// engagement scope at parse time — computed once, never re-fetched.
    pub fn new(output: String, authorized: bool, ttl_remaining: Duration) -> Self {
        Self {
            output,
            authorized,
            ttl_remaining,
        }
    }

    /// Returns `true` if the scope governor authorized the requested action.
    ///
    /// Always check this before dispatching further wing actions. An
    /// unauthorized result means SERAPH rejected the target or tool.
    #[must_use]
    pub fn is_authorized(&self) -> bool {
        self.authorized
    }

    /// Remaining TTL on the engagement scope at the time of this check.
    ///
    /// This is a snapshot — it does not update after the result is constructed.
    #[must_use]
    pub fn ttl_remaining(&self) -> Duration {
        self.ttl_remaining
    }
}

/// Recon phase response from SERAPH.
///
/// Contains AI-generated open-source intelligence and host-discovery output.
///
/// # Security note on IP and hostname fields
///
/// Any IP addresses or hostnames appearing in `output` originate from target
/// systems or external intelligence sources and are **attacker-influenced**.
/// Do not use them to construct outbound connections in SDK code.
#[derive(Debug, Clone)]
pub struct ReconResult {
    /// AI-generated recon output from SERAPH.
    ///
    /// May contain IP addresses, hostnames, open ports, and service banners.
    ///
    /// **Do not use IP addresses or hostnames extracted from this field to
    /// construct outbound connections in SDK code.** These values originate
    /// from target systems and are not validated.
    pub output: String,
}

/// Survey phase response from SERAPH.
///
/// Contains host and service enumeration prose from the scan or capture wings.
#[derive(Debug, Clone)]
pub struct SurveyResult {
    /// AI-generated survey output from SERAPH.
    pub output: String,
}

/// Examine phase response from SERAPH.
///
/// Contains binary, protocol, or artefact analysis prose from the analyze wing.
#[derive(Debug, Clone)]
pub struct ExamineResult {
    /// AI-generated analysis output from SERAPH.
    pub output: String,
}

/// Strike phase response from SERAPH.
///
/// Contains output from the execute or detonate wings.
///
/// # Security constraints
///
/// All fields in this struct originate from target systems or sandboxed
/// detonation environments and are **attacker-controlled**. Treat every
/// field as untrusted input.
#[derive(Debug, Clone)]
pub struct StrikeResult {
    /// AI-generated strike output from SERAPH.
    ///
    /// # UNTRUSTED
    ///
    /// This field originates from the target system or sandbox.
    /// Do not render it unescaped in any UI, log aggregator, or downstream
    /// system without sanitization. Treat as attacker-controlled data.
    pub output: String,
    /// Raw findings from the execution backend, if present.
    ///
    /// # UNTRUSTED
    ///
    /// Originates from the target system. Do not render unescaped.
    /// Do not parse as trusted structured data.
    pub raw_findings: Option<String>,
}

/// Report phase response from SERAPH.
///
/// Contains the final engagement summary generated after investigation close.
/// Prose fields are stored as [`Box<str>`] to avoid re-allocation — report
/// output is written once and read many times.
#[derive(Debug, Clone)]
pub struct ReportResult {
    /// Engagement summary — full prose generated by SERAPH.
    ///
    /// Stored as `Box<str>` to avoid re-allocation of large report text.
    pub summary: Box<str>,
    /// Structured engagement identifier, if present in the response.
    pub engagement_id: Option<Box<str>>,
}
