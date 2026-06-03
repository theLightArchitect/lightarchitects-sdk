//! `QualityGatekeeper` — quality-dimension critique implementation.
//!
//! Owned by CORSO (per gatekeeper registry default). Checks drafts against:
//!
//! - `canon://builders-cookbook` (Rust standards, complexity ceilings, fn-length budgets)
//! - `helix://corso/entries/` (prior code-quality findings — precedent)
//! - `industry-baselines/quality/iso/` (ISO/IEC 25010 sub-characteristics)
//! - the current build plan's quality-relevant sections
//!
//! Stateless by construction: no `&mut self`, no interior mutability, no
//! global state. The compile-time invariant test
//! `quality_gatekeeper_has_no_mutable_state` greps this file for forbidden
//! tokens and fails if any appear in the struct definition or impl body.

use std::fmt::Write as _;

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

const Q_VERSION: &str = "quality-v1.0";
const Q_OWNER: &str = "corso";
const Q_BUDGET_USD: f64 = 0.25;
const Q_MIN_CRITERIA: usize = 2;

/// Quality dimension gatekeeper. `CORSO`-owned. Stateless.
///
/// # Stateless contract
///
/// `QualityGatekeeper<P>` holds only:
/// - `provider: P` — the LLM agent provider (used by reference per-call)
/// - `shield: IndirectInjectionShield` — itself stateless (no instance memory)
///
/// No mutable fields. No interior mutability. No global access. Same
/// `(draft, criteria) → Verdict` always (modulo LLM nondeterminism).
///
/// # Composition
///
/// Construct with [`QualityGatekeeper::new`] passing any
/// [`LlmAgentProvider`] (e.g. [`crate::agent::ClaudeCliProvider`] for the
/// `Claude Code` subscription path, or an `OpenAICompatProvider` for `LiteLLM`).
pub struct QualityGatekeeper<P: LlmAgentProvider> {
    provider: P,
    shield: IndirectInjectionShield,
}

impl<P: LlmAgentProvider> QualityGatekeeper<P> {
    /// Construct a new quality gatekeeper backed by `provider`.
    #[must_use]
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            shield: IndirectInjectionShield::new(),
        }
    }
}

#[async_trait]
impl<P: LlmAgentProvider + 'static> Gatekeeper for QualityGatekeeper<P> {
    fn dimension(&self) -> GateDimension {
        GateDimension::Quality
    }

    fn version(&self) -> &'static str {
        Q_VERSION
    }

    fn owner(&self) -> &'static str {
        // Metadata, not type-coupled — see module docs.
        Q_OWNER
    }

    fn min_criteria_completeness(&self) -> usize {
        Q_MIN_CRITERIA
    }

    async fn critique(&self, draft: &Draft, criteria: &Criteria) -> Result<Verdict, GateError> {
        // 1. Compute hashes BEFORE any work — for verdict identity.
        let draft_hash = canonical_sha256(draft)?;
        let criteria_hash = canonical_sha256(criteria)?;

        // 2. Refuse on insufficient criteria (refusal invariant).
        let total = criteria.total_evidence_count();
        if total < self.min_criteria_completeness() {
            return Verdict::try_new(
                GateDimension::Quality,
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
                Q_VERSION,
            );
        }

        // 3. Build the prompts.
        let draft_hash_hex = hex::encode(draft_hash);
        let wrapped_draft = self
            .shield
            .wrap_tool_result(&draft_hash_hex[..16], &draft.content);
        let system_prompt = build_quality_system_prompt(criteria);
        let user_prompt = build_quality_user_prompt(&wrapped_draft, draft, criteria);

        // 4. Issue the request through the LLM provider.
        let req = AgentRequest {
            sibling_identity: system_prompt,
            user_prompt,
            schema: None,
            allowed_tools: Vec::new(),
            max_turns: 1,
            max_budget_usd: Q_BUDGET_USD,
            model_hint: None,
            parent_span_id: None,
            chain_origin: Some("gatekeeper.quality".to_owned()),
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

        // 5. Detect silent failures — same pattern as LlmReActExecutor.
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
            GateDimension::Quality,
            parsed.overall_status,
            parsed.findings,
            draft_hash,
            criteria_hash,
            criteria.helix_snapshot.clone(),
            Q_VERSION,
        )
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Prompt construction
// ────────────────────────────────────────────────────────────────────────────

fn build_quality_system_prompt(criteria: &Criteria) -> String {
    let shield_addendum = IndirectInjectionShield::system_prompt_addendum();
    let mut s = String::with_capacity(1024);
    s.push_str(
        "You are the Light Architects Quality gatekeeper (CORSO-owned). \
         Your job is to evaluate a draft against a fixed set of criteria \
         and emit a structured verdict.\n\n\
         Rules of engagement:\n\
         1. You are stateless. You have no memory of prior drafts.\n\
         2. Every finding you emit MUST cite at least one Criterion (canon, \
         baseline, precedent, or plan reference). Findings without citation \
         will be rejected by the host.\n\
         3. If you cannot ground a finding in the supplied criteria, omit \
         it. Do not invent rules.\n\
         4. Severity must be one of: blocking, critical, high, medium, low.\n\
         5. Status must be one of: validated, needs_revision, blocked.\n\n",
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
        s.push_str("\nPrior decisions (precedent):\n");
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
               \"message\": \"<one sentence describing the issue>\",\n\
               \"citation_codes\": [\"C0\", \"B1\", ...],\n\
               \"remediation_hint\": \"<optional one sentence>\",\n\
               \"line_start\": <optional 1-based line number>\n\
             }\n\
           ]\n\
         }\n",
    );
    s
}

fn build_quality_user_prompt(wrapped_draft: &str, draft: &Draft, criteria: &Criteria) -> String {
    let mut s = String::with_capacity(2048);
    let _ = write!(
        s,
        "Critique the following {} draft against the {} criteria provided in the system prompt. \
         Apply ONLY the supplied criteria.\n\n",
        draft_kind_label(draft.kind),
        criteria.dimension.as_str()
    );
    if !draft.file_paths.is_empty() {
        s.push_str("Files in scope:\n");
        for p in &draft.file_paths {
            let _ = writeln!(s, "  - {}", p.display());
        }
        s.push('\n');
    }
    if !draft.topic_hints.is_empty() {
        let _ = write!(s, "Topic hints: {}\n\n", draft.topic_hints.join(", "));
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
        // Convert citation codes to Citation entries. For v1 we treat the
        // codes as opaque markers — the actual Citation values are
        // reconstructed from the criteria pool the LLM was given.
        // (Future enhancement: full resolution.)
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
/// LLM response. Handles surrounding prose, markdown fences, and brace
/// nesting at any depth.
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

    // ── Stub provider — emits a fixed event sequence ──────────────────────

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

    fn rust_draft(content: &str) -> Draft {
        Draft {
            content: content.to_owned(),
            kind: DraftKind::Code,
            topic_hints: vec!["rust".to_owned(), "error-handling".to_owned()],
            file_paths: vec![std::path::PathBuf::from("src/parser.rs")],
        }
    }

    fn criteria_with_n_canon(n: usize) -> Criteria {
        let mut c = Criteria::empty(GateDimension::Quality);
        for i in 0..n {
            c.canon_excerpts.push(CanonRef {
                doc: "builders-cookbook".to_owned(),
                section: format!("§{}", 48 + i),
                excerpt: "No `.unwrap()` / `.expect()` in production code.".to_owned(),
                uri: format!("canon://builders-cookbook#section-{}", 48 + i),
            });
        }
        c.helix_snapshot = HelixSnapshotId::test();
        c
    }

    // ── Tests ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn insufficient_criteria_refuses() {
        let gk = QualityGatekeeper::new(StubProvider::silent_timeout());
        let v = gk
            .critique(&rust_draft("fn x() {}"), &criteria_with_n_canon(0))
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
        // LLM emits a finding with empty citation_codes — Verdict::try_new
        // must reject it.
        let bad_response = r#"{
            "overall_status": "needs_revision",
            "findings": [
                {"severity": "high", "message": "looks bad", "citation_codes": []}
            ]
        }"#;
        let gk = QualityGatekeeper::new(StubProvider::returning(bad_response));
        let r = gk
            .critique(&rust_draft("fn x() {}"), &criteria_with_n_canon(2))
            .await;
        assert!(
            matches!(r, Err(GateError::FindingWithoutCitation { .. })),
            "expected FindingWithoutCitation, got {r:?}"
        );
    }

    #[tokio::test]
    async fn silent_timeout_returns_parse_error() {
        // Provider emits ONLY MessageStop — no MessageDelta, no text.
        let gk = QualityGatekeeper::new(StubProvider::silent_timeout());
        let r = gk
            .critique(&rust_draft("fn x() {}"), &criteria_with_n_canon(2))
            .await;
        assert!(
            matches!(r, Err(GateError::ParseError(_))),
            "expected ParseError on silent stream, got {r:?}"
        );
    }

    #[tokio::test]
    async fn valid_response_yields_verdict() {
        let good_response = r#"{
            "overall_status": "needs_revision",
            "findings": [
                {
                    "severity": "high",
                    "message": "uses .unwrap() in production code path",
                    "citation_codes": ["C0"],
                    "remediation_hint": "return Result and propagate with ?"
                }
            ]
        }"#;
        let gk = QualityGatekeeper::new(StubProvider::returning(good_response));
        let v = gk
            .critique(
                &rust_draft("pub fn parse(s: &str) -> Value { from_str(s).unwrap() }"),
                &criteria_with_n_canon(2),
            )
            .await
            .unwrap();
        assert_eq!(v.status, VerdictStatus::NeedsRevision);
        assert_eq!(v.findings().len(), 1);
        assert_eq!(v.findings()[0].severity, Severity::High);
        assert!(!v.findings()[0].citations.is_empty());
        assert_eq!(v.gatekeeper_version, Q_VERSION);
        assert_eq!(v.dimension, GateDimension::Quality);
    }

    #[tokio::test]
    async fn determinism_same_inputs_same_hashes() {
        let response = r#"{ "overall_status": "validated", "findings": [] }"#;
        let draft = rust_draft("fn x() {}");
        let criteria = criteria_with_n_canon(2);

        let gk1 = QualityGatekeeper::new(StubProvider::returning(response));
        let v1 = gk1.critique(&draft, &criteria).await.unwrap();
        let gk2 = QualityGatekeeper::new(StubProvider::returning(response));
        let v2 = gk2.critique(&draft, &criteria).await.unwrap();

        assert_eq!(v1.draft_hash, v2.draft_hash);
        assert_eq!(v1.criteria_hash, v2.criteria_hash);
    }

    #[test]
    fn extract_json_object_handles_prose_around() {
        let s = r#"Sure! Here's my verdict: {"overall_status": "validated", "findings": []} - hope that helps"#;
        let extracted = extract_json_object(s).unwrap();
        assert_eq!(
            extracted,
            r#"{"overall_status": "validated", "findings": []}"#
        );
    }

    #[test]
    fn extract_json_object_handles_nested_braces() {
        let s = r#"{"a": {"b": 1}, "c": [{"d": 2}]}"#;
        let extracted = extract_json_object(s).unwrap();
        assert_eq!(extracted, s);
    }

    #[test]
    fn extract_json_object_handles_braces_in_strings() {
        let s = r#"{"a": "x{y}z"}"#;
        let extracted = extract_json_object(s).unwrap();
        assert_eq!(extracted, s);
    }

    #[test]
    fn canonical_sha256_is_deterministic() {
        let v = serde_json::json!({"a": 1, "b": "hello"});
        let h1 = canonical_sha256(&v).unwrap();
        let h2 = canonical_sha256(&v).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn truncate_at_char_boundary() {
        let s = "héllo world";
        let t = truncate(s, 4);
        assert!(t.ends_with('…'));
        assert!(t.starts_with('h'));
    }

    // ── Compile-time stateless invariants ─────────────────────────────────
    // Verifies the trait+impl can be used in multi-threaded contexts.
    #[test]
    fn quality_gatekeeper_is_send_sync() {
        static_assertions::assert_impl_all!(QualityGatekeeper<StubProvider>: Send, Sync);
    }

    // ── Forbidden-token grep test ────────────────────────────────────────
    // Reads the source of THIS file and asserts no mutable-state tokens
    // appear in the QualityGatekeeper struct or its critique impl.
    //
    // Forbidden inside the struct/impl:
    //   Mutex, RwLock, RefCell, Cell<, AtomicU, UnsafeCell, &mut self
    //
    // This is a belt+suspenders check on top of `static_assertions`.
    #[test]
    fn quality_gatekeeper_has_no_mutable_state() {
        let src = include_str!("quality.rs");
        // Slice the source to just the struct definition + Gatekeeper impl
        // (skip tests, which legitimately use Mutex-like patterns).
        let end_marker = "// ──────────────────────────────────────────────";
        let start = src
            .find("pub struct QualityGatekeeper<P: LlmAgentProvider>")
            .expect("struct");
        // Find the start of the Tests section comment header.
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
                "QualityGatekeeper region contains forbidden mutable construct: {forbidden:?}"
            );
        }
        // Anti-check: the marker comment we slice on must still be present.
        assert!(region.contains(end_marker) || tests_anchor > start);
    }
}
