//! `CanonGatekeeper` — doctrinal-compliance critique implementation.
//!
//! Owned by LÆX (per gatekeeper registry default for the `[C]` dimension).
//! Checks drafts against:
//!
//! - `canon://platform-canon` (constitutional principles, Canon I–XL+)
//! - `canon://builders-cookbook` (coding standards, pattern constraints)
//! - `canon://architects-blueprint` (plan rubric, C1–C8 dimensions)
//! - `helix://laex/entries/` (prior canon-compliance rulings — precedent)
//! - the current build plan's doctrinal sections
//!
//! Stateless by construction: no `&mut self`, no interior mutability, no
//! global state. The compile-time invariant test
//! `canon_gatekeeper_has_no_mutable_state` greps this file for forbidden
//! tokens and fails if any appear in the struct definition or impl body.
//!
//! # Shield
//!
//! Unlike [`QualityGatekeeper`], the [`IndirectInjectionShield`] is injected
//! at construction as `Arc<IndirectInjectionShield>`. This enables callers
//! (e.g. `PlanToWaves`) to share a single configured shield instance across
//! multiple gatekeeper calls without re-allocation.
//!
//! [`QualityGatekeeper`]: super::quality::QualityGatekeeper

use std::fmt::Write as _;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt as _;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use super::trait_def::Gatekeeper;
use super::types::{
    Citation, Criteria, Draft, DraftKind, Finding, GateDimension, GateError, Severity, Verdict,
    VerdictStatus,
};
use crate::agent::IndirectInjectionShield;
use crate::agent::provider::{AgentRequest, LlmAgentProvider, ProviderEvent};

const C_VERSION: &str = "canon-v1.0";
const C_OWNER: &str = "laex";
const C_BUDGET_USD: f64 = 0.25;
const C_MIN_CRITERIA: usize = 2;

/// Canon dimension gatekeeper. `LÆX`-owned. Stateless.
///
/// # Stateless contract
///
/// `CanonGatekeeper<P>` holds only:
/// - `provider: P` — the LLM agent provider (used by reference per-call)
/// - `shield: Arc<IndirectInjectionShield>` — itself stateless; shared via `Arc`
///
/// No mutable fields. No interior mutability. No global access. Same
/// `(draft, criteria) → Verdict` always (modulo LLM nondeterminism).
///
/// # Composition
///
/// Construct with [`CanonGatekeeper::new`] passing any
/// [`LlmAgentProvider`] and a shared shield reference. The `Arc` allows
/// sharing one shield instance across many concurrent gatekeeper calls
/// (e.g. across waves in [`crate::agent::PlanToWaves`]).
pub struct CanonGatekeeper<P: LlmAgentProvider> {
    provider: P,
    shield: Arc<IndirectInjectionShield>,
}

impl<P: LlmAgentProvider> CanonGatekeeper<P> {
    /// Construct a new canon gatekeeper backed by `provider` and `shield`.
    ///
    /// The `shield` is shared via `Arc` — clone the `Arc` to give additional
    /// gatekeeper instances the same configured shield.
    #[must_use]
    pub fn new(provider: P, shield: Arc<IndirectInjectionShield>) -> Self {
        Self { provider, shield }
    }
}

#[async_trait]
impl<P: LlmAgentProvider + 'static> Gatekeeper for CanonGatekeeper<P> {
    fn dimension(&self) -> GateDimension {
        GateDimension::Canon
    }

    fn version(&self) -> &'static str {
        C_VERSION
    }

    fn owner(&self) -> &'static str {
        C_OWNER
    }

    fn min_criteria_completeness(&self) -> usize {
        C_MIN_CRITERIA
    }

    async fn critique(&self, draft: &Draft, criteria: &Criteria) -> Result<Verdict, GateError> {
        // 1. Compute hashes BEFORE any work — for verdict identity.
        let draft_hash = canonical_sha256(draft)?;
        let criteria_hash = canonical_sha256(criteria)?;

        // 2. Refuse on insufficient criteria (refusal invariant).
        let total = criteria.total_evidence_count();
        if total < self.min_criteria_completeness() {
            return Verdict::try_new(
                GateDimension::Canon,
                VerdictStatus::RetrievalInsufficient {
                    reason: format!(
                        "criteria evidence count {total} below minimum {min} \
                         (canon={canon}, baselines={base}, precedent={prec}, plan={plan})",
                        min = self.min_criteria_completeness(),
                        canon = criteria.canon_excerpts.len(),
                        base = criteria.industry_baselines.len(),
                        prec = criteria.precedent.len(),
                        plan = criteria.build_plan_excerpts.len(),
                    ),
                },
                Vec::new(),
                draft_hash,
                criteria_hash,
                criteria.helix_snapshot.clone(),
                C_VERSION,
            );
        }

        // 3. Build the prompts.
        let draft_hash_hex = hex::encode(draft_hash);
        let wrapped_draft = self
            .shield
            .wrap_tool_result(&draft_hash_hex[..16], &draft.content);
        let system_prompt = build_canon_system_prompt(criteria);
        let user_prompt = build_canon_user_prompt(&wrapped_draft, draft, criteria);

        // 4. Issue the request through the LLM provider.
        let req = AgentRequest {
            sibling_identity: system_prompt,
            user_prompt,
            schema: None,
            allowed_tools: Vec::new(),
            max_turns: 1,
            max_budget_usd: C_BUDGET_USD,
            model_hint: None,
            parent_span_id: None,
            chain_origin: Some("gatekeeper.canon".to_owned()),
            chain_depth: 0,
            aud: None,
            conversation_history: Vec::new(),
            tool_definitions: Vec::new(),
        };
        let sanitized = req.sanitize().map_err(GateError::Provider)?;

        // Drive the streaming provider and accumulate the response text.
        let mut stream = self
            .provider
            .spawn_streaming(sanitized)
            .await
            .map_err(GateError::Provider)?;
        let mut response_text = String::new();
        let mut saw_message_delta = false;
        while let Some(event) = stream.next().await {
            match event {
                ProviderEvent::TextDelta { text, .. } => response_text.push_str(&text),
                ProviderEvent::MessageDelta { .. } => saw_message_delta = true,
                ProviderEvent::MessageStop => break,
                _ => {}
            }
        }

        // 5. Detect silent failures.
        if !saw_message_delta && response_text.trim().is_empty() {
            return Err(GateError::ParseError(format!(
                "LLM stream ended without MessageDelta or useful output \
                 (likely upstream timeout or empty body; provider={})",
                self.provider.name()
            )));
        }

        // 6. Parse the structured critique.
        let parsed = parse_critique_response(&response_text)?;

        // 7. Build the verdict — Verdict::try_new enforces the citation invariant.
        Verdict::try_new(
            GateDimension::Canon,
            parsed.overall_status,
            parsed.findings,
            draft_hash,
            criteria_hash,
            criteria.helix_snapshot.clone(),
            C_VERSION,
        )
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Prompt construction
// ────────────────────────────────────────────────────────────────────────────

fn build_canon_system_prompt(criteria: &Criteria) -> String {
    let shield_addendum = IndirectInjectionShield::system_prompt_addendum();
    let mut s = String::with_capacity(1024);
    s.push_str(
        "You are the Light Architects Canon gatekeeper (LÆX-owned). \
         Your job is to evaluate a LASDLC plan section against the supplied \
         canon citations and return a structured verdict.\n\n\
         Rules of engagement:\n\
         1. You are stateless. You have no memory of prior drafts.\n\
         2. Every finding you emit MUST cite at least one Criterion from the \
         supplied canon, baseline, precedent, or plan excerpts. Findings \
         without citation will be rejected by the host.\n\
         3. Evaluate ONLY against the supplied criteria. Do not invent rules \
         from general knowledge.\n\
         4. Severity must be one of: blocking, critical, high, medium, low.\n\
         5. Status must be one of: validated, needs_revision, blocked.\n\
         6. A plan section is `validated` when it satisfies all cited canon \
         obligations without blocking findings.\n\n",
    );

    let _ = writeln!(s, "Criteria for dimension {}:", criteria.dimension.as_str());
    if !criteria.canon_excerpts.is_empty() {
        s.push_str("\nCanon excerpts:\n");
        for (i, c) in criteria.canon_excerpts.iter().enumerate() {
            let _ = writeln!(
                s,
                "  C{i}. [{doc} {section}] {excerpt}",
                doc = c.doc,
                section = c.section,
                excerpt = truncate(&c.excerpt, 500),
            );
        }
    }
    if !criteria.industry_baselines.is_empty() {
        s.push_str("\nIndustry baselines:\n");
        for (i, b) in criteria.industry_baselines.iter().enumerate() {
            let _ = writeln!(
                s,
                "  B{i}. [{doc} {section}] {excerpt}",
                doc = b.doc,
                section = b.section,
                excerpt = truncate(&b.excerpt, 500),
            );
        }
    }
    if !criteria.precedent.is_empty() {
        s.push_str("\nPrior canon rulings (precedent):\n");
        for (i, p) in criteria.precedent.iter().enumerate() {
            let pattern = p.finding_pattern.as_deref().unwrap_or("?");
            let _ = writeln!(
                s,
                "  P{i}. [{pattern} @ {date}] {excerpt}",
                date = p.date.format("%Y-%m-%d"),
                excerpt = truncate(&p.excerpt, 500),
            );
        }
    }
    if !criteria.build_plan_excerpts.is_empty() {
        s.push_str("\nBuild plan excerpts:\n");
        for (i, p) in criteria.build_plan_excerpts.iter().enumerate() {
            let _ = writeln!(
                s,
                "  L{i}. [{section}] {excerpt}",
                section = p.section,
                excerpt = truncate(&p.excerpt, 500),
            );
        }
    }

    s.push_str(
        "\n\nWhen citing a criterion in your response, use its short code \
         (C0, B0, P0, L0, ...) in the finding's `citation_codes` field.\n\n",
    );
    s.push_str(shield_addendum);
    s.push_str(
        "\n\nRespond in JSON with the schema:\n\
         {\n\
           \"overall_status\": \"validated|needs_revision|blocked\",\n\
           \"findings\": [\n\
             {\n\
               \"severity\": \"blocking|critical|high|medium|low\",\n\
               \"message\": \"<one sentence describing the canon violation>\",\n\
               \"citation_codes\": [\"C0\", \"B1\", ...],\n\
               \"remediation_hint\": \"<optional one sentence pointing to the canon remedy>\",\n\
               \"line_start\": <optional 1-based line number>\n\
             }\n\
           ]\n\
         }\n",
    );
    s
}

fn build_canon_user_prompt(wrapped_draft: &str, draft: &Draft, criteria: &Criteria) -> String {
    let mut s = String::with_capacity(2048);
    let _ = write!(
        s,
        "Evaluate this LASDLC plan section against the canon citations provided \
         in the system prompt. Apply ONLY the supplied criteria. Return findings \
         with severity, citations, and remediation_hint.\n\n\
         Draft type: {kind}\n\n",
        kind = draft_kind_label(draft.kind),
    );
    if !draft.file_paths.is_empty() {
        s.push_str("Files in scope:\n");
        for p in &draft.file_paths {
            let _ = writeln!(s, "  - {}", p.display());
        }
        s.push('\n');
    }
    if !draft.topic_hints.is_empty() {
        let _ = write!(
            s,
            "Topic hints: {hints}\n\nDimension context: {dim}\n\n",
            hints = draft.topic_hints.join(", "),
            dim = criteria.dimension.as_str(),
        );
    }
    s.push_str("Draft content:\n");
    s.push_str(wrapped_draft);
    s.push_str(
        "\n\nReturn the JSON verdict described in the system prompt and nothing else. \
         Every finding must include a citation_codes entry.",
    );
    s
}

const fn draft_kind_label(k: DraftKind) -> &'static str {
    match k {
        DraftKind::Code => "code",
        DraftKind::Plan => "plan",
        DraftKind::Diagram => "diagram",
        DraftKind::Documentation => "documentation",
        DraftKind::Decision => "decision",
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Response parsing
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RawCritiqueResponse {
    overall_status: String,
    #[serde(default)]
    findings: Vec<RawFinding>,
}

#[derive(Debug, Deserialize)]
struct RawFinding {
    severity: String,
    message: String,
    #[serde(default)]
    citation_codes: Vec<String>,
    #[serde(default)]
    remediation_hint: Option<String>,
    #[serde(default)]
    line_start: Option<u32>,
}

struct ParsedCritique {
    overall_status: VerdictStatus,
    findings: Vec<Finding>,
}

fn parse_critique_response(text: &str) -> Result<ParsedCritique, GateError> {
    let trimmed = extract_json_object(text)
        .ok_or_else(|| GateError::ParseError("no JSON object found in response".to_owned()))?;
    let raw: RawCritiqueResponse = serde_json::from_str(trimmed)
        .map_err(|e| GateError::ParseError(format!("JSON deserialize failed: {e}")))?;

    let overall_status = match raw.overall_status.as_str() {
        "validated" => VerdictStatus::Validated,
        "needs_revision" => VerdictStatus::NeedsRevision,
        "blocked" => VerdictStatus::Blocked,
        other => {
            return Err(GateError::ParseError(format!(
                "unknown overall_status: {other:?}"
            )));
        }
    };

    let mut findings = Vec::with_capacity(raw.findings.len());
    for raw_f in raw.findings {
        let severity = match raw_f.severity.as_str() {
            "blocking" => Severity::Blocking,
            "critical" => Severity::Critical,
            "high" => Severity::High,
            "medium" => Severity::Medium,
            "low" => Severity::Low,
            other => {
                return Err(GateError::ParseError(format!(
                    "unknown severity: {other:?}"
                )));
            }
        };
        let citations: Vec<Citation> = if raw_f.citation_codes.is_empty() {
            Vec::new()
        } else {
            raw_f
                .citation_codes
                .iter()
                .map(|code| {
                    Citation::Canon(super::types::CanonRef {
                        doc: "criterion-code".to_owned(),
                        section: code.clone(),
                        excerpt: format!("see system-prompt criterion {code}"),
                        uri: format!("criterion://{code}"),
                    })
                })
                .collect()
        };
        findings.push(Finding {
            severity,
            message: raw_f.message,
            citations,
            remediation_hint: raw_f.remediation_hint,
            draft_location: raw_f.line_start.map(|l| super::types::DraftLocation {
                line_start: l,
                line_end: None,
                file: None,
            }),
        });
    }

    Ok(ParsedCritique {
        overall_status,
        findings,
    })
}

/// Extract the first top-level `{...}` JSON object substring from a free-form
/// LLM response.
fn extract_json_object(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut start: Option<usize> = None;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        if b == b'"' {
            in_string = true;
            continue;
        }
        if b == b'{' {
            if depth == 0 {
                start = Some(i);
            }
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0
                && let Some(s) = start
            {
                return Some(&text[s..=i]);
            }
        }
    }
    None
}

// ────────────────────────────────────────────────────────────────────────────
// Canonical hashing
// ────────────────────────────────────────────────────────────────────────────

fn canonical_sha256<T: serde::Serialize>(value: &T) -> Result<[u8; 32], GateError> {
    let json = serde_json::to_vec(value)
        .map_err(|e| GateError::ParseError(format!("serialize for hash: {e}")))?;
    let mut hasher = Sha256::new();
    hasher.update(&json);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Ok(out)
}

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

fn truncate(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::agent::gatekeeper::types::{CanonRef, HelixSnapshotId};
    use crate::agent::provider::{
        ProviderCapabilities, ProviderError, SanitizedAgentRequest, SchemaMode,
    };
    use async_trait::async_trait;
    use futures_util::stream::BoxStream;

    // ── Stub provider ─────────────────────────────────────────────────────

    struct StubProvider {
        events: Vec<ProviderEvent>,
    }

    impl StubProvider {
        fn returning(json_text: &str) -> Self {
            Self {
                events: vec![
                    ProviderEvent::MessageStart {
                        model: "stub".to_owned(),
                        input_tokens: 1,
                    },
                    ProviderEvent::ContentBlockStart {
                        index: 0,
                        block_type: "text".to_owned(),
                        tool_use_id: None,
                        tool_name: None,
                    },
                    ProviderEvent::TextDelta {
                        index: 0,
                        text: json_text.to_owned(),
                    },
                    ProviderEvent::ContentBlockStop { index: 0 },
                    ProviderEvent::MessageDelta {
                        stop_reason: "end_turn".to_owned(),
                        output_tokens: 10,
                    },
                    ProviderEvent::MessageStop,
                ],
            }
        }

        fn silent_timeout() -> Self {
            Self {
                events: vec![ProviderEvent::MessageStop],
            }
        }
    }

    #[async_trait]
    impl LlmAgentProvider for StubProvider {
        fn name(&self) -> &'static str {
            "stub"
        }
        async fn spawn(
            &self,
            _req: SanitizedAgentRequest,
        ) -> Result<crate::agent::provider::AgentResponse, ProviderError> {
            Err(ProviderError::Internal("stub spawn unused".into()))
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                schema_enforcement: SchemaMode::None,
                native_budget_cap: false,
                native_turn_cap: false,
                auth_inherits_session: false,
            }
        }
        async fn spawn_streaming(
            &self,
            _req: SanitizedAgentRequest,
        ) -> Result<BoxStream<'static, ProviderEvent>, ProviderError> {
            let events = self.events.clone();
            Ok(Box::pin(futures_util::stream::iter(events)))
        }
        fn estimate_cost(&self, _i: u32, _o: u32) -> f64 {
            0.0
        }
    }

    // ── Fixtures ──────────────────────────────────────────────────────────

    fn plan_draft(content: &str) -> Draft {
        Draft {
            content: content.to_owned(),
            kind: DraftKind::Plan,
            topic_hints: vec!["lasdlc".to_owned(), "canon-compliance".to_owned()],
            file_paths: vec![std::path::PathBuf::from("plans/test-build.md")],
        }
    }

    fn criteria_with_n_canon(n: usize) -> Criteria {
        let mut c = Criteria::empty(GateDimension::Canon);
        for i in 0..n {
            c.canon_excerpts.push(CanonRef {
                doc: "platform-canon".to_owned(),
                section: format!("Canon {}", 30 + i),
                excerpt: format!(
                    "Canon {}: every plan must declare a northstar_lineage block.",
                    30 + i
                ),
                uri: format!("canon://platform-canon#canon-{}", 30 + i),
            });
        }
        c.helix_snapshot = HelixSnapshotId::test();
        c
    }

    fn shared_shield() -> Arc<IndirectInjectionShield> {
        Arc::new(IndirectInjectionShield::new())
    }

    // ── Tests ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn insufficient_criteria_refuses() {
        let gk = CanonGatekeeper::new(StubProvider::silent_timeout(), shared_shield());
        let v = gk
            .critique(
                &plan_draft("## Phase 1\n\n- Wave 1.1: design"),
                &criteria_with_n_canon(0),
            )
            .await
            .unwrap();
        assert!(matches!(
            v.status,
            VerdictStatus::RetrievalInsufficient { .. }
        ));
        assert!(v.findings().is_empty());
    }

    #[tokio::test]
    async fn rejects_finding_without_citation() {
        let bad_response = r#"{
            "overall_status": "needs_revision",
            "findings": [
                {"severity": "high", "message": "missing northstar", "citation_codes": []}
            ]
        }"#;
        let gk = CanonGatekeeper::new(StubProvider::returning(bad_response), shared_shield());
        let r = gk
            .critique(&plan_draft("section body"), &criteria_with_n_canon(2))
            .await;
        assert!(
            matches!(r, Err(GateError::FindingWithoutCitation { .. })),
            "expected FindingWithoutCitation, got {r:?}"
        );
    }

    #[tokio::test]
    async fn silent_timeout_returns_parse_error() {
        let gk = CanonGatekeeper::new(StubProvider::silent_timeout(), shared_shield());
        let r = gk
            .critique(&plan_draft("section body"), &criteria_with_n_canon(2))
            .await;
        assert!(
            matches!(r, Err(GateError::ParseError(_))),
            "expected ParseError on silent stream, got {r:?}"
        );
    }

    #[tokio::test]
    async fn valid_response_yields_canon_verdict() {
        let good_response = r#"{
            "overall_status": "needs_revision",
            "findings": [
                {
                    "severity": "blocking",
                    "message": "northstar_lineage block missing from plan",
                    "citation_codes": ["C0"],
                    "remediation_hint": "Add northstar_lineage with northstar_text per Canon 30"
                }
            ]
        }"#;
        let gk = CanonGatekeeper::new(StubProvider::returning(good_response), shared_shield());
        let v = gk
            .critique(
                &plan_draft("---\ncodename: test\n---\n## Phase 1\n"),
                &criteria_with_n_canon(2),
            )
            .await
            .unwrap();
        assert_eq!(v.status, VerdictStatus::NeedsRevision);
        assert_eq!(v.findings().len(), 1);
        assert_eq!(v.findings()[0].severity, Severity::Blocking);
        assert!(!v.findings()[0].citations.is_empty());
        assert_eq!(v.gatekeeper_version, C_VERSION);
        assert_eq!(v.dimension, GateDimension::Canon);
    }

    #[tokio::test]
    async fn validated_plan_section_yields_no_findings() {
        let good_response = r#"{ "overall_status": "validated", "findings": [] }"#;
        let gk = CanonGatekeeper::new(StubProvider::returning(good_response), shared_shield());
        let v = gk
            .critique(
                &plan_draft("## Phase 1 — Architecture\n\nnorthstar_lineage: present"),
                &criteria_with_n_canon(2),
            )
            .await
            .unwrap();
        assert_eq!(v.status, VerdictStatus::Validated);
        assert!(v.findings().is_empty());
        assert_eq!(v.dimension, GateDimension::Canon);
    }

    #[tokio::test]
    async fn arc_shield_shared_across_instances() {
        // Validates that two CanonGatekeepers can share one shield Arc.
        let shield = shared_shield();
        let response = r#"{ "overall_status": "validated", "findings": [] }"#;
        let gk1 = CanonGatekeeper::new(StubProvider::returning(response), Arc::clone(&shield));
        let gk2 = CanonGatekeeper::new(StubProvider::returning(response), Arc::clone(&shield));
        let draft = plan_draft("content");
        let crit = criteria_with_n_canon(2);
        let v1 = gk1.critique(&draft, &crit).await.unwrap();
        let v2 = gk2.critique(&draft, &crit).await.unwrap();
        assert_eq!(v1.draft_hash, v2.draft_hash);
        assert_eq!(v1.criteria_hash, v2.criteria_hash);
    }

    // ── Compile-time stateless invariants ─────────────────────────────────

    #[test]
    fn canon_gatekeeper_is_send_sync() {
        static_assertions::assert_impl_all!(CanonGatekeeper<StubProvider>: Send, Sync);
    }

    // Reads the source of THIS file and asserts no mutable-state tokens appear
    // in the CanonGatekeeper struct or critique impl body.
    //
    // Forbidden inside the struct/impl:
    //   Mutex, RwLock, RefCell, Cell<, AtomicU, UnsafeCell, &mut self
    #[test]
    fn canon_gatekeeper_has_no_mutable_state() {
        let src = include_str!("canon.rs");
        let start = src
            .find("pub struct CanonGatekeeper<P: LlmAgentProvider>")
            .expect("struct marker");
        let tests_anchor = src.find("// Tests\n").expect("tests marker");
        let region = &src[start..tests_anchor];
        for forbidden in &[
            "Mutex<",
            "RwLock<",
            "RefCell<",
            "Cell<",
            "AtomicU",
            "UnsafeCell",
            "&mut self",
        ] {
            assert!(
                !region.contains(forbidden),
                "CanonGatekeeper region contains forbidden mutable construct: {forbidden:?}"
            );
        }
    }
}
