//! Pure data types for the gatekeeper substrate.
//!
//! All structs in this module are values: no methods that mutate state
//! beyond `&mut self` builder-style setters used at construction time.
//! [`Criteria`] and [`Verdict`] are intentionally hashable (via canonical
//! JSON serialization) so callers may cache verdicts by
//! `(draft_hash, criteria_hash)`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ────────────────────────────────────────────────────────────────────────────
// GateDimension — one letter per Canon XXXVIII vocabulary entry
// ────────────────────────────────────────────────────────────────────────────

/// LASDLC quality-gate dimension. One per `[A+S+Q+C+O+P+K+D+T+R]` `Canon XXXVIII` letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GateDimension {
    /// `[A]` — Architecture, correctness, API design, complexity.
    Architecture,
    /// `[S]` — Security: threat surface, vulnerabilities, supply chain, secrets.
    Security,
    /// `[Q]` — Quality: standards, lints, complexity ceilings, fn-length budgets.
    Quality,
    /// `[C]` — Canon: doctrinal compliance against ratified Light Architects canon.
    Canon,
    /// `[O]` — Operations: deploy pipeline, CI/CD, rollback, runtime topology.
    Operations,
    /// `[P]` — Performance: latency, throughput, complexity bounds, resource caps.
    Performance,
    /// `[K]` — Knowledge: helix enrichment, citations, prior decisions.
    Knowledge,
    /// `[D]` — Documentation: rustdoc, `JSDoc`, example coverage, README freshness.
    Documentation,
    /// `[T]` — Testing: pyramid coverage (unit + integration + property + E2E).
    Testing,
    /// `[R]` — Research + Risk: prior art, dependency audit, risk scoring.
    Research,
}

impl GateDimension {
    /// The canonical single-letter abbreviation used in `[A+S+Q+C+O+P+K+D+T+R]`.
    #[must_use]
    pub const fn letter(self) -> char {
        match self {
            Self::Architecture => 'A',
            Self::Security => 'S',
            Self::Quality => 'Q',
            Self::Canon => 'C',
            Self::Operations => 'O',
            Self::Performance => 'P',
            Self::Knowledge => 'K',
            Self::Documentation => 'D',
            Self::Testing => 'T',
            Self::Research => 'R',
        }
    }

    /// Canonical sibling owner per Gatekeeper Registry baseline.
    ///
    /// Returned as a stable lowercase identifier. Operationally, gatekeepers
    /// may override their owner via [`crate::agent::gatekeeper::Gatekeeper::owner`]
    /// — this constant defines the *default* assignment per dimension.
    #[must_use]
    pub const fn default_owner(self) -> &'static str {
        match self {
            Self::Architecture | Self::Quality | Self::Testing => "corso",
            Self::Security => "seraph",
            Self::Canon => "laex",
            Self::Operations | Self::Performance => "eva",
            Self::Knowledge | Self::Documentation => "soul",
            Self::Research => "quantum",
        }
    }

    /// Stable lowercase string for serialization / span attribution.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Architecture => "architecture",
            Self::Security => "security",
            Self::Quality => "quality",
            Self::Canon => "canon",
            Self::Operations => "operations",
            Self::Performance => "performance",
            Self::Knowledge => "knowledge",
            Self::Documentation => "documentation",
            Self::Testing => "testing",
            Self::Research => "research",
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Draft — input to a critique
// ────────────────────────────────────────────────────────────────────────────

/// A producer output presented for evaluation. By `Light Architects` platform convention, **all**
/// first outputs are considered drafts; producers commit to "final" only after
/// at least one gatekeeper has validated.
#[derive(Debug, Clone, Serialize)]
pub struct Draft {
    /// The draft content (code, plan body, design doc, decision text, etc.).
    pub content: String,
    /// What kind of draft this is — shapes prompt framing and retrieval.
    pub kind: DraftKind,
    /// Topic hints used by [`crate::agent::gatekeeper::types`] consumers for
    /// precedent retrieval (e.g. `["rust", "error-handling", "auth"]`).
    pub topic_hints: Vec<String>,
    /// File paths the draft applies to (empty for non-code drafts).
    pub file_paths: Vec<PathBuf>,
}

/// What a draft IS — determines which canon sections + retrieval scopes apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DraftKind {
    /// Source code (any language).
    Code,
    /// LASDLC build plan body or fragment.
    Plan,
    /// Architecture diagram (`Mermaid` / `D2` / `PlantUML`).
    Diagram,
    /// Documentation (rustdoc, README, design doc, ADR).
    Documentation,
    /// Decision record (ADR-like, decisions ledger entry).
    Decision,
}

// ────────────────────────────────────────────────────────────────────────────
// Citation refs (canon, baseline, precedent, plan)
// ────────────────────────────────────────────────────────────────────────────

/// Reference to a section of a Light Architects canon document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonRef {
    /// Canon document slug (e.g. `"builders-cookbook"`, `"platform-canon"`).
    pub doc: String,
    /// Section heading or anchor (e.g. `"§48 — Rust Standards"`).
    pub section: String,
    /// Verbatim excerpt that supports the citation. Retrieved at criteria
    /// assembly time; the excerpt is the actual evidence.
    pub excerpt: String,
    /// Canon URI (e.g. `"canon://builders-cookbook#section-48"`).
    pub uri: String,
}

/// Reference to an industry baseline (ISO, OWASP, NIST, ...).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineRef {
    /// Baseline document slug (e.g. `"iso-iec-25010"`, `"owasp-llm-top-10"`).
    pub doc: String,
    /// Section heading or anchor.
    pub section: String,
    /// Verbatim excerpt.
    pub excerpt: String,
    /// Baseline URI (e.g. `"baseline://quality/iso-iec-25010#functional-suitability"`).
    pub uri: String,
}

/// Reference to a prior helix entry that informs this critique.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecedentRef {
    /// Helix entry URI (e.g. `"helix://corso/entries/2026-04-15-..."`).
    pub helix_path: String,
    /// When the helix entry was written (UTC).
    pub date: chrono::DateTime<chrono::Utc>,
    /// Excerpt of the prior decision / finding.
    pub excerpt: String,
    /// Optional finding-pattern code if previously catalogued (e.g. `"Q-23"`).
    pub finding_pattern: Option<String>,
    /// RRF similarity score from the SOUL query (higher = more relevant).
    pub similarity: f32,
}

/// Reference to a section of the current build plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRef {
    /// Build codename (e.g. `"gatekeeper-quality-stateless"`).
    pub plan_codename: String,
    /// Plan section heading.
    pub section: String,
    /// Excerpt from the plan that constrains this draft.
    pub excerpt: String,
}

/// Tagged citation — `Finding`s carry one or more of these as evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Citation {
    /// Citation against a canon document.
    Canon(CanonRef),
    /// Citation against an industry baseline.
    Baseline(BaselineRef),
    /// Citation against prior helix precedent.
    Precedent(PrecedentRef),
    /// Citation against the current build plan.
    BuildPlan(PlanRef),
}

// ────────────────────────────────────────────────────────────────────────────
// HelixSnapshotId — replayability anchor
// ────────────────────────────────────────────────────────────────────────────

/// Identifier for the helix snapshot the criteria were assembled against.
///
/// Used to make verdicts replayable: the same `(draft, criteria, snapshot)`
/// will produce the same verdict modulo LLM nondeterminism. When canon or
/// helix entries change between calls, the snapshot id changes — that's
/// evidence the criteria moved, not a regression.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HelixSnapshotId {
    /// UTC timestamp at which the snapshot was assembled.
    pub assembled_at: chrono::DateTime<chrono::Utc>,
    /// Optional content digest of the canon directory tree (sha256 hex).
    ///
    /// When present, two snapshots with the same digest are *guaranteed*
    /// to reflect identical canon state. Absent when the assembler is
    /// running in fallback mode without filesystem visibility.
    pub canon_digest: Option<String>,
    /// Optional helix git revision (`HEAD` at assembly time).
    pub helix_git_rev: Option<String>,
}

impl HelixSnapshotId {
    /// Construct a deterministic snapshot id from `assembled_at` only.
    ///
    /// Use in tests or in fallback paths where the canon digest cannot be
    /// computed. Real assemblies should populate `canon_digest` for
    /// replayability.
    #[must_use]
    pub fn from_timestamp(assembled_at: chrono::DateTime<chrono::Utc>) -> Self {
        Self {
            assembled_at,
            canon_digest: None,
            helix_git_rev: None,
        }
    }

    /// Test helper — fixed snapshot id for unit tests.
    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub fn test() -> Self {
        Self {
            assembled_at: chrono::DateTime::from_timestamp(1_780_000_000, 0)
                .unwrap_or_else(chrono::Utc::now),
            canon_digest: Some("0".repeat(64)),
            helix_git_rev: Some("test-rev".to_owned()),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Criteria — assembled context for one critique
// ────────────────────────────────────────────────────────────────────────────

/// Assembled criteria for a single critique invocation. Pure data; hashable
/// via canonical JSON serialization for caching at the call site.
///
/// Same `Criteria` value → same `Verdict` (modulo LLM nondeterminism).
#[derive(Debug, Clone, Serialize)]
pub struct Criteria {
    /// Which gate dimension this criteria block applies to.
    pub dimension: GateDimension,
    /// Relevant canon excerpts retrieved at assembly time.
    pub canon_excerpts: Vec<CanonRef>,
    /// Relevant industry baseline excerpts.
    pub industry_baselines: Vec<BaselineRef>,
    /// Relevant prior helix decisions (precedent).
    pub precedent: Vec<PrecedentRef>,
    /// Relevant excerpts from the current build plan.
    pub build_plan_excerpts: Vec<PlanRef>,
    /// When the assembly completed (UTC).
    pub retrieved_at: chrono::DateTime<chrono::Utc>,
    /// Snapshot id of the helix/canon state at assembly time.
    pub helix_snapshot: HelixSnapshotId,
    /// Non-fatal warnings emitted during assembly (e.g.
    /// `"no precedent for topic 'foo'"`).
    pub assembly_warnings: Vec<String>,
}

impl Criteria {
    /// Construct an empty criteria value for `dimension`. Useful for testing
    /// the insufficient-retrieval refusal path.
    #[must_use]
    pub fn empty(dimension: GateDimension) -> Self {
        Self {
            dimension,
            canon_excerpts: Vec::new(),
            industry_baselines: Vec::new(),
            precedent: Vec::new(),
            build_plan_excerpts: Vec::new(),
            retrieved_at: chrono::Utc::now(),
            helix_snapshot: HelixSnapshotId::from_timestamp(chrono::Utc::now()),
            assembly_warnings: Vec::new(),
        }
    }

    /// Total number of evidence entries across all reference categories.
    /// Compared to [`crate::agent::gatekeeper::Gatekeeper::min_criteria_completeness`]
    /// at critique time to decide refuse-vs-judge.
    #[must_use]
    pub fn total_evidence_count(&self) -> usize {
        self.canon_excerpts.len()
            + self.industry_baselines.len()
            + self.precedent.len()
            + self.build_plan_excerpts.len()
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Finding, Severity, DraftLocation
// ────────────────────────────────────────────────────────────────────────────

/// Severity of a finding. Ordered: `Blocking > Critical > High > Medium > Low`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// Lowest: stylistic, future-look — drafts may ship without addressing.
    Low,
    /// Worth noting; drafts may defer with rationale.
    Medium,
    /// Should be addressed in this revision.
    High,
    /// Material risk; revision required before this dimension validates.
    Critical,
    /// Highest: blocks the entire build from advancing.
    Blocking,
}

/// Optional location within the draft that a finding points at.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftLocation {
    /// 1-based line number of the start of the offending region.
    pub line_start: u32,
    /// Optional 1-based line number of the end of the offending region.
    pub line_end: Option<u32>,
    /// Optional file path if the draft spans multiple files.
    pub file: Option<PathBuf>,
}

/// A single concrete finding within a [`Verdict`].
///
/// **Invariant**: `citations` is non-empty for any finding included in a
/// non-refusal `Verdict`. Enforced at construction by [`Verdict::try_new`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// How serious this finding is.
    pub severity: Severity,
    /// Human-readable description of the issue.
    pub message: String,
    /// Evidence citations — required (≥1) for non-refusal verdicts.
    pub citations: Vec<Citation>,
    /// Optional remediation guidance for the producer.
    pub remediation_hint: Option<String>,
    /// Optional location within the draft this finding points at.
    pub draft_location: Option<DraftLocation>,
}

// ────────────────────────────────────────────────────────────────────────────
// VerdictStatus, Verdict
// ────────────────────────────────────────────────────────────────────────────

/// Overall outcome of a critique.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum VerdictStatus {
    /// Draft meets criteria for this dimension. May still have informational
    /// findings (Low/Medium) attached.
    Validated,
    /// Draft has actionable issues; producer should revise and re-submit.
    NeedsRevision,
    /// Draft is fundamentally unsuitable; blocking issue at this dimension.
    Blocked,
    /// Insufficient criteria to issue a verdict — gatekeeper refuses with
    /// the given `reason`. Caller decides what to do (manual review, fall
    /// back to a different gatekeeper, surface to operator, etc.).
    RetrievalInsufficient {
        /// Why criteria were insufficient (e.g. minimum threshold not met).
        reason: String,
    },
}

/// A single gatekeeper's judgment on a draft.
///
/// Built via [`Verdict::try_new`] which enforces the citation-on-every-finding
/// invariant. The `findings` field is private; use [`Verdict::findings`] to
/// inspect after construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    /// Dimension this verdict speaks to.
    pub dimension: GateDimension,
    /// Overall outcome.
    pub status: VerdictStatus,
    /// Individual findings — private to enforce the citation invariant.
    findings: Vec<Finding>,
    /// sha-256 of the draft this verdict applies to (canonical JSON).
    pub draft_hash: [u8; 32],
    /// sha-256 of the criteria block this verdict was issued against.
    pub criteria_hash: [u8; 32],
    /// Helix snapshot id (for replayability).
    pub helix_snapshot: HelixSnapshotId,
    /// Gatekeeper version string (changes when prompts/parsers change).
    pub gatekeeper_version: &'static str,
    /// When the critique completed (UTC).
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

impl Verdict {
    /// Construct a verdict, enforcing the citation-on-every-finding invariant.
    ///
    /// # Errors
    ///
    /// Returns [`GateError::FindingWithoutCitation`] if any non-refusal
    /// `Verdict` contains a `Finding` with no `Citation`s. Refusal verdicts
    /// ([`VerdictStatus::RetrievalInsufficient`]) are allowed to have empty
    /// findings.
    pub fn try_new(
        dimension: GateDimension,
        status: VerdictStatus,
        findings: Vec<Finding>,
        draft_hash: [u8; 32],
        criteria_hash: [u8; 32],
        helix_snapshot: HelixSnapshotId,
        gatekeeper_version: &'static str,
    ) -> Result<Self, GateError> {
        if !matches!(status, VerdictStatus::RetrievalInsufficient { .. }) {
            for f in &findings {
                if f.citations.is_empty() {
                    return Err(GateError::FindingWithoutCitation {
                        message: f.message.clone(),
                    });
                }
            }
        }
        Ok(Self {
            dimension,
            status,
            findings,
            draft_hash,
            criteria_hash,
            helix_snapshot,
            gatekeeper_version,
            completed_at: chrono::Utc::now(),
        })
    }

    /// All findings attached to this verdict (may be empty for refusals).
    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// Maximum severity present among findings, or `None` if no findings.
    #[must_use]
    pub fn max_severity(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).max()
    }
}

// ────────────────────────────────────────────────────────────────────────────
// GateError
// ────────────────────────────────────────────────────────────────────────────

/// Errors that can occur during a gate critique.
#[derive(Debug, thiserror::Error)]
pub enum GateError {
    /// A finding was emitted without any citation. Forbidden by the
    /// citation-mandatory invariant; the gatekeeper rejects the response.
    #[error("finding lacks required citation: {message:?}")]
    FindingWithoutCitation {
        /// The offending finding's message text.
        message: String,
    },
    /// LLM provider returned an error or the request could not be sanitized.
    #[error("provider error: {0}")]
    Provider(#[from] crate::agent::provider::ProviderError),
    /// Caller explicitly requested rejection on thin context.
    #[error("criteria insufficient: {reason}")]
    CriteriaInsufficient {
        /// Why the criteria were judged insufficient.
        reason: String,
    },
    /// LLM response could not be parsed into the expected verdict schema.
    #[error("response parse failed: {0}")]
    ParseError(String),
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_finding(severity: Severity, with_citation: bool) -> Finding {
        Finding {
            severity,
            message: "test finding".to_owned(),
            citations: if with_citation {
                vec![Citation::Canon(CanonRef {
                    doc: "test".to_owned(),
                    section: "§1".to_owned(),
                    excerpt: "test rule".to_owned(),
                    uri: "canon://test#1".to_owned(),
                })]
            } else {
                Vec::new()
            },
            remediation_hint: None,
            draft_location: None,
        }
    }

    #[test]
    fn try_new_rejects_non_refusal_finding_without_citation() {
        let r = Verdict::try_new(
            GateDimension::Quality,
            VerdictStatus::NeedsRevision,
            vec![make_finding(Severity::High, false)],
            [0; 32],
            [0; 32],
            HelixSnapshotId::test(),
            "test-v0",
        );
        assert!(matches!(r, Err(GateError::FindingWithoutCitation { .. })));
    }

    #[test]
    fn try_new_accepts_non_refusal_finding_with_citation() {
        let r = Verdict::try_new(
            GateDimension::Quality,
            VerdictStatus::NeedsRevision,
            vec![make_finding(Severity::High, true)],
            [0; 32],
            [0; 32],
            HelixSnapshotId::test(),
            "test-v0",
        );
        assert!(r.is_ok());
        let v = r.unwrap();
        assert_eq!(v.findings().len(), 1);
        assert_eq!(v.max_severity(), Some(Severity::High));
    }

    #[test]
    fn try_new_allows_refusal_with_empty_findings() {
        let r = Verdict::try_new(
            GateDimension::Quality,
            VerdictStatus::RetrievalInsufficient {
                reason: "no canon".to_owned(),
            },
            Vec::new(),
            [0; 32],
            [0; 32],
            HelixSnapshotId::test(),
            "test-v0",
        );
        assert!(r.is_ok());
        assert!(r.unwrap().findings().is_empty());
    }

    #[test]
    fn try_new_validated_with_zero_findings() {
        let r = Verdict::try_new(
            GateDimension::Quality,
            VerdictStatus::Validated,
            Vec::new(),
            [0; 32],
            [0; 32],
            HelixSnapshotId::test(),
            "test-v0",
        );
        assert!(r.is_ok());
    }

    #[test]
    fn gate_dimension_letter_round_trip() {
        for d in [
            GateDimension::Architecture,
            GateDimension::Security,
            GateDimension::Quality,
            GateDimension::Canon,
            GateDimension::Operations,
            GateDimension::Performance,
            GateDimension::Knowledge,
            GateDimension::Documentation,
            GateDimension::Testing,
            GateDimension::Research,
        ] {
            let c = d.letter();
            assert!(c.is_ascii_uppercase());
            assert!(!d.as_str().is_empty());
            assert!(!d.default_owner().is_empty());
        }
    }

    #[test]
    fn criteria_empty_has_zero_evidence() {
        let c = Criteria::empty(GateDimension::Quality);
        assert_eq!(c.total_evidence_count(), 0);
    }

    #[test]
    fn severity_ordering() {
        assert!(Severity::Blocking > Severity::Critical);
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Medium > Severity::Low);
    }
}
