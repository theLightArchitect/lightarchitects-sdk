//! [`Gatekeeper`] trait — the pure-function evaluator contract.

use async_trait::async_trait;

use super::types::{Criteria, Draft, GateDimension, GateError, Verdict};

/// Stateless gate evaluator. **No `&mut self`. No interior mutability.**
///
/// Implementations are pure functions of `(draft, criteria) → Verdict`:
///
/// - Same inputs → same outputs (modulo LLM nondeterminism — pin
///   temperature to 0 in providers that support it for full determinism)
/// - No global state access
/// - No disk I/O except through `criteria` (already assembled by
///   `CriteriaAssembler`)
/// - Network calls only through the LLM provider passed at construction
///
/// Cacheability: callers may memoize verdicts by
/// `(verdict.draft_hash, verdict.criteria_hash)`. The trait contract
/// guarantees identical inputs yield identical hashes — that's the whole
/// point of being stateless.
///
/// # Citation invariant
///
/// Implementations MUST refuse to return a non-refusal [`Verdict`] containing
/// any [`Finding`] without at least one [`Citation`]. [`Verdict::try_new`]
/// enforces this at construction; callers should propagate
/// [`GateError::FindingWithoutCitation`] from the response parser without
/// degrading or retrying silently.
///
/// # Refusal invariant
///
/// When [`Criteria::total_evidence_count`] falls below
/// [`Self::min_criteria_completeness`], the implementation MUST return a
/// [`VerdictStatus::RetrievalInsufficient`] verdict rather than issuing a
/// thin-context judgment. This makes retrieval failures *visible* instead of
/// *silent*.
///
/// [`Finding`]: super::types::Finding
/// [`Citation`]: super::types::Citation
/// [`Verdict`]: super::types::Verdict
/// [`Verdict::try_new`]: super::types::Verdict::try_new
/// [`Criteria::total_evidence_count`]: super::types::Criteria::total_evidence_count
/// [`VerdictStatus::RetrievalInsufficient`]: super::types::VerdictStatus::RetrievalInsufficient
#[async_trait]
pub trait Gatekeeper: Send + Sync {
    /// LASDLC gate dimension this gatekeeper covers.
    fn dimension(&self) -> GateDimension;

    /// Implementation version, used for cache invalidation and audit.
    ///
    /// Change this string whenever the prompt template or response parser
    /// changes — previously cached verdicts become invalid.
    fn version(&self) -> &'static str;

    /// Canonical sibling that operationally owns this gatekeeper.
    ///
    /// Returns a stable lowercase identifier (`"corso"`, `"seraph"`,
    /// `"laex"`, `"eva"`, `"soul"`, `"quantum"`, `"ayin"`). Used for:
    ///
    /// - AYIN span attribution (`actor=<owner>.gatekeeper.<dimension>`)
    /// - Canon attribution in verdict narrative
    /// - Operator-facing UI labels + voice synthesis
    ///
    /// Ownership is **metadata, not type structure**. Changing this string
    /// at runtime is supported by the trait; renaming the implementing type
    /// is not required. Default delegates to
    /// [`GateDimension::default_owner`].
    fn owner(&self) -> &'static str {
        self.dimension().default_owner()
    }

    /// Minimum criteria-evidence count required to issue a non-refusal verdict.
    ///
    /// When `criteria.total_evidence_count() < self.min_criteria_completeness()`
    /// the gatekeeper MUST return [`VerdictStatus::RetrievalInsufficient`]
    /// without invoking the LLM. Default is `1` (at least one reference of
    /// any kind); concrete impls may raise this (e.g.
    /// [`super::quality::QualityGatekeeper`] requires `2`).
    ///
    /// [`VerdictStatus::RetrievalInsufficient`]: super::types::VerdictStatus::RetrievalInsufficient
    fn min_criteria_completeness(&self) -> usize {
        1
    }

    /// Pure critique — same `(draft, criteria)` → same `Verdict`.
    ///
    /// # Errors
    ///
    /// - [`GateError::FindingWithoutCitation`] if the LLM emitted a
    ///   non-refusal finding without citation (citation invariant)
    /// - [`GateError::Provider`] if the underlying LLM call failed
    /// - [`GateError::ParseError`] if the LLM response was malformed
    /// - [`GateError::CriteriaInsufficient`] if an implementation chooses
    ///   to escalate insufficient criteria to a hard error rather than a
    ///   [`VerdictStatus::RetrievalInsufficient`] verdict (rare; the
    ///   refusal-verdict path is preferred)
    ///
    /// [`VerdictStatus::RetrievalInsufficient`]: super::types::VerdictStatus::RetrievalInsufficient
    async fn critique(&self, draft: &Draft, criteria: &Criteria) -> Result<Verdict, GateError>;
}
