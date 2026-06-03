//! End-to-end integration tests for the Quality gatekeeper.
//!
//! Verifies the full path: `Draft` → `CriteriaAssembler` → `QualityGatekeeper` →
//! `Verdict` against scripted provider responses + in-memory fake source.
//!
//! Run via `cargo test --features gatekeepers,loops-core,agent-cli --test
//! gatekeeper_q_integration`.

#![cfg(all(feature = "gatekeepers", feature = "loops-core", feature = "agent-cli"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use async_trait::async_trait;
use futures_util::stream::BoxStream;
use lightarchitects::agent::gatekeeper::{
    AssemblerConfig, AssemblyError, BaselineRef, CanonRef, CriteriaAssembler, CriteriaSource,
    Draft, DraftKind, GateDimension, Gatekeeper as _, HelixSnapshotId, PrecedentRef,
    QualityGatekeeper, Severity, VerdictStatus,
};
use lightarchitects::agent::{
    AgentResponse, LlmAgentProvider, ProviderCapabilities, ProviderError, ProviderEvent,
    SanitizedAgentRequest, SchemaMode,
};

// ────────────────────────────────────────────────────────────────────────────
// Test fixtures: scripted provider + in-memory criteria source
// ────────────────────────────────────────────────────────────────────────────

struct ScriptedProvider {
    response: String,
}

#[async_trait]
impl LlmAgentProvider for ScriptedProvider {
    fn name(&self) -> &'static str {
        "scripted"
    }
    async fn spawn(&self, _r: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
        Err(ProviderError::Internal("integration uses streaming".into()))
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
        _r: SanitizedAgentRequest,
    ) -> Result<BoxStream<'static, ProviderEvent>, ProviderError> {
        let events = vec![
            ProviderEvent::MessageStart {
                model: "scripted".to_owned(),
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
                text: self.response.clone(),
            },
            ProviderEvent::ContentBlockStop { index: 0 },
            ProviderEvent::MessageDelta {
                stop_reason: "end_turn".to_owned(),
                output_tokens: 10,
            },
            ProviderEvent::MessageStop,
        ];
        Ok(Box::pin(futures_util::stream::iter(events)))
    }
    fn estimate_cost(&self, _i: u32, _o: u32) -> f64 {
        0.0
    }
}

struct InMemoryQualitySource {
    canon: Vec<CanonRef>,
    baselines: Vec<BaselineRef>,
    precedent: Vec<PrecedentRef>,
}

#[async_trait]
impl CriteriaSource for InMemoryQualitySource {
    async fn fetch_canon(
        &self,
        _dim: GateDimension,
        _topics: &[String],
        limit: usize,
    ) -> Result<Vec<CanonRef>, AssemblyError> {
        Ok(self.canon.iter().take(limit).cloned().collect())
    }
    async fn fetch_baselines(
        &self,
        _dim: GateDimension,
        _topics: &[String],
        limit: usize,
    ) -> Result<Vec<BaselineRef>, AssemblyError> {
        Ok(self.baselines.iter().take(limit).cloned().collect())
    }
    async fn fetch_precedent(
        &self,
        _dim: GateDimension,
        _topics: &[String],
        _lookback_days: u32,
        limit: usize,
    ) -> Result<Vec<PrecedentRef>, AssemblyError> {
        Ok(self.precedent.iter().take(limit).cloned().collect())
    }
    fn snapshot_id(&self) -> HelixSnapshotId {
        HelixSnapshotId::from_timestamp(
            chrono::DateTime::from_timestamp(1_780_000_000, 0).unwrap_or_else(chrono::Utc::now),
        )
    }
}

fn no_unwrap_canon_section() -> CanonRef {
    CanonRef {
        doc: "builders-cookbook".to_owned(),
        section: "§48 — Rust Standards".to_owned(),
        excerpt: "Production code MUST NOT use .unwrap() or .expect(). \
                  Return Result and propagate with ?."
            .to_owned(),
        uri: "canon://builders-cookbook#section-48".to_owned(),
    }
}

fn complexity_canon_section() -> CanonRef {
    CanonRef {
        doc: "builders-cookbook".to_owned(),
        section: "§49 — Complexity".to_owned(),
        excerpt: "Functions must stay under 60 lines and cyclomatic complexity ≤10.".to_owned(),
        uri: "canon://builders-cookbook#section-49".to_owned(),
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bad_rust_draft_yields_needs_revision() {
    // The scripted LLM response simulates what a CORSO-trained gatekeeper
    // would emit for a draft that uses .unwrap() in production.
    let scripted = r#"{
        "overall_status": "needs_revision",
        "findings": [
            {
                "severity": "high",
                "message": "production code uses .unwrap() — must propagate errors",
                "citation_codes": ["C0"],
                "remediation_hint": "return Result<Value, ParseError> and propagate with ?"
            }
        ]
    }"#;
    let provider = ScriptedProvider {
        response: scripted.to_owned(),
    };
    let gk = QualityGatekeeper::new(provider);

    let source = InMemoryQualitySource {
        canon: vec![no_unwrap_canon_section(), complexity_canon_section()],
        baselines: Vec::new(),
        precedent: Vec::new(),
    };
    let assembler = CriteriaAssembler::new(source);

    let draft = Draft {
        content: "pub fn parse_json(s: &str) -> serde_json::Value { \
                  serde_json::from_str(s).unwrap() }"
            .to_owned(),
        kind: DraftKind::Code,
        topic_hints: vec!["rust".to_owned(), "error-handling".to_owned()],
        file_paths: vec![std::path::PathBuf::from("src/parser.rs")],
    };

    let criteria = assembler
        .assemble(GateDimension::Quality, &draft)
        .await
        .expect("assembly should succeed with non-empty canon");
    assert!(
        criteria.total_evidence_count() >= 2,
        "expected ≥2 evidence entries, got {}",
        criteria.total_evidence_count()
    );

    let verdict = gk.critique(&draft, &criteria).await.expect("critique");
    assert_eq!(verdict.status, VerdictStatus::NeedsRevision);
    assert_eq!(verdict.dimension, GateDimension::Quality);
    assert_eq!(verdict.findings().len(), 1);
    assert_eq!(verdict.findings()[0].severity, Severity::High);
    assert!(
        verdict.findings()[0]
            .message
            .to_lowercase()
            .contains("unwrap"),
        "finding message should mention unwrap"
    );
    assert!(
        !verdict.findings()[0].citations.is_empty(),
        "citation invariant: every finding must cite"
    );
}

#[tokio::test]
async fn valid_rust_draft_yields_validated() {
    let scripted = r#"{
        "overall_status": "validated",
        "findings": []
    }"#;
    let provider = ScriptedProvider {
        response: scripted.to_owned(),
    };
    let gk = QualityGatekeeper::new(provider);
    let source = InMemoryQualitySource {
        canon: vec![no_unwrap_canon_section(), complexity_canon_section()],
        baselines: Vec::new(),
        precedent: Vec::new(),
    };
    let assembler = CriteriaAssembler::new(source);

    let draft = Draft {
        content: "pub fn parse_json(s: &str) -> Result<serde_json::Value, \
                  serde_json::Error> { serde_json::from_str(s) }"
            .to_owned(),
        kind: DraftKind::Code,
        topic_hints: vec!["rust".to_owned()],
        file_paths: vec![std::path::PathBuf::from("src/parser.rs")],
    };

    let criteria = assembler
        .assemble(GateDimension::Quality, &draft)
        .await
        .unwrap();
    let verdict = gk.critique(&draft, &criteria).await.unwrap();

    assert_eq!(verdict.status, VerdictStatus::Validated);
    assert!(verdict.findings().is_empty());
}

#[tokio::test]
async fn insufficient_criteria_refuses_without_llm_call() {
    // Source returns zero canon — total evidence < min_criteria_completeness=2.
    // The gatekeeper must refuse with RetrievalInsufficient WITHOUT consulting
    // the LLM provider (i.e. the scripted response is never reached).
    //
    // We don't have a direct way to assert "provider was not called", but we
    // can supply an obviously-broken response and check it never gets parsed:
    let scripted = "this is not json at all and would fail to parse";
    let provider = ScriptedProvider {
        response: scripted.to_owned(),
    };
    let gk = QualityGatekeeper::new(provider);
    let source = InMemoryQualitySource {
        canon: Vec::new(),
        baselines: Vec::new(),
        precedent: Vec::new(),
    };
    let assembler = CriteriaAssembler::new(source);

    let draft = Draft {
        content: "fn x() {}".to_owned(),
        kind: DraftKind::Code,
        topic_hints: vec!["rust".to_owned()],
        file_paths: vec![],
    };

    let criteria = assembler
        .assemble(GateDimension::Quality, &draft)
        .await
        .unwrap();
    let verdict = gk.critique(&draft, &criteria).await.unwrap();

    assert!(
        matches!(verdict.status, VerdictStatus::RetrievalInsufficient { .. }),
        "expected RetrievalInsufficient, got {:?}",
        verdict.status
    );
    assert!(verdict.findings().is_empty());
}

#[tokio::test]
async fn verdict_determinism_same_inputs_same_hashes() {
    let scripted = r#"{ "overall_status": "validated", "findings": [] }"#;

    let source1 = InMemoryQualitySource {
        canon: vec![no_unwrap_canon_section(), complexity_canon_section()],
        baselines: Vec::new(),
        precedent: Vec::new(),
    };
    let source2 = InMemoryQualitySource {
        canon: vec![no_unwrap_canon_section(), complexity_canon_section()],
        baselines: Vec::new(),
        precedent: Vec::new(),
    };
    let asm1 = CriteriaAssembler::with_config(source1, AssemblerConfig::default());
    let asm2 = CriteriaAssembler::with_config(source2, AssemblerConfig::default());

    let draft = Draft {
        content: "fn x() {}".to_owned(),
        kind: DraftKind::Code,
        topic_hints: vec!["rust".to_owned()],
        file_paths: vec![],
    };

    // Manually set retrieved_at + helix_snapshot to the same value across both
    // assemblies, since `retrieved_at` is Utc::now() and naturally differs.
    let mut c1 = asm1.assemble(GateDimension::Quality, &draft).await.unwrap();
    let mut c2 = asm2.assemble(GateDimension::Quality, &draft).await.unwrap();
    c2.retrieved_at = c1.retrieved_at;
    c2.helix_snapshot = c1.helix_snapshot.clone();
    // Reset warnings to identical (since timestamps drift in formatting):
    c1.assembly_warnings.clear();
    c2.assembly_warnings.clear();

    let gk1 = QualityGatekeeper::new(ScriptedProvider {
        response: scripted.to_owned(),
    });
    let gk2 = QualityGatekeeper::new(ScriptedProvider {
        response: scripted.to_owned(),
    });

    let v1 = gk1.critique(&draft, &c1).await.unwrap();
    let v2 = gk2.critique(&draft, &c2).await.unwrap();

    assert_eq!(v1.draft_hash, v2.draft_hash, "draft hash determinism");
    assert_eq!(
        v1.criteria_hash, v2.criteria_hash,
        "criteria hash determinism"
    );
}
