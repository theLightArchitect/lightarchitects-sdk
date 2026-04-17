//! Unit tests for [`OracleVerdict`] and [`Consensus`] — verdict synthesis logic.
//!
//! `compute_consensus` is tested directly (it was promoted to `pub`). The
//! consensus heuristic checks for positive/negative keywords in `Finding.content`,
//! so tests construct findings with specific content strings to drive each branch.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use lightarchitects_oracle::{
    Consensus, Finding, FindingStatus, ModelId, ModelRole, OracleVerdict,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn ok_finding(model: ModelId, content: &str) -> Finding {
    Finding {
        model,
        role: ModelRole::Derivation,
        display: model.to_string(),
        status: FindingStatus::Ok,
        content: content.to_string(),
        elapsed: Duration::from_millis(100),
        tokens_in: 10,
        tokens_out: 20,
    }
}

fn error_finding(model: ModelId) -> Finding {
    Finding {
        model,
        role: ModelRole::Reasoning,
        display: model.to_string(),
        status: FindingStatus::Error("connection timeout".to_string()),
        content: String::new(),
        elapsed: Duration::from_millis(5_000),
        tokens_in: 0,
        tokens_out: 0,
    }
}

fn timeout_finding(model: ModelId) -> Finding {
    Finding {
        model,
        role: ModelRole::FormalProof,
        display: model.to_string(),
        status: FindingStatus::Timeout,
        content: String::new(),
        elapsed: Duration::from_secs(180),
        tokens_in: 0,
        tokens_out: 0,
    }
}

fn verdict_from(findings: Vec<Finding>) -> OracleVerdict {
    let ok = findings
        .iter()
        .filter(|f| f.status == FindingStatus::Ok)
        .count();
    let total = findings.len();
    let consensus = OracleVerdict::compute_consensus(&findings);
    OracleVerdict {
        prompt: "test prompt".to_string(),
        findings,
        consensus,
        total_elapsed: Duration::from_secs(1),
        models_ok: ok,
        models_total: total,
    }
}

// ── compute_consensus: Insufficient ──────────────────────────────────────────

#[test]
fn consensus_insufficient_when_no_models_ok() {
    let findings = vec![
        error_finding(ModelId::Deepseek),
        timeout_finding(ModelId::Qwen),
    ];
    assert_eq!(
        OracleVerdict::compute_consensus(&findings),
        Consensus::Insufficient
    );
}

#[test]
fn consensus_insufficient_when_only_one_model_ok() {
    let findings = vec![
        ok_finding(ModelId::Deepseek, "The derivation holds."),
        error_finding(ModelId::Qwen),
    ];
    assert_eq!(
        OracleVerdict::compute_consensus(&findings),
        Consensus::Insufficient,
        "single Ok finding is not enough for consensus"
    );
}

#[test]
fn consensus_insufficient_with_empty_findings() {
    assert_eq!(
        OracleVerdict::compute_consensus(&[]),
        Consensus::Insufficient
    );
}

// ── compute_consensus: Unanimous ─────────────────────────────────────────────

#[test]
fn consensus_unanimous_when_all_models_ok_no_keywords() {
    // No positive or negative keywords → all_ok && !disagreement → Unanimous.
    let findings = vec![
        ok_finding(ModelId::Deepseek, "Analysis complete."),
        ok_finding(ModelId::Qwen, "Numerical check passed."),
        ok_finding(ModelId::Kimi, "Step-by-step done."),
    ];
    assert_eq!(
        OracleVerdict::compute_consensus(&findings),
        Consensus::Unanimous
    );
}

#[test]
fn consensus_unanimous_when_all_models_ok_with_positive_keywords() {
    let findings = vec![
        ok_finding(ModelId::Deepseek, "This is proven via induction."),
        ok_finding(ModelId::Qwen, "Verified with bounds check."),
    ];
    assert_eq!(
        OracleVerdict::compute_consensus(&findings),
        Consensus::Unanimous
    );
}

// ── compute_consensus: Majority ───────────────────────────────────────────────

#[test]
fn consensus_majority_when_some_models_fail() {
    // Two ok, one error, no conflicting keywords → Majority (not all responded).
    let findings = vec![
        ok_finding(ModelId::Deepseek, "Derivation checks out."),
        ok_finding(ModelId::Qwen, "Bound is tight."),
        error_finding(ModelId::Leanstral),
    ];
    assert_eq!(
        OracleVerdict::compute_consensus(&findings),
        Consensus::Majority
    );
}

// ── compute_consensus: Disagreement ──────────────────────────────────────────

#[test]
fn consensus_disagreement_when_models_have_conflicting_keywords() {
    // One model says "proven", another says "false" → Disagreement.
    let findings = vec![
        ok_finding(ModelId::Deepseek, "This is proven by construction."),
        ok_finding(ModelId::Qwen, "This claim is false — counterexample: x=0."),
    ];
    assert_eq!(
        OracleVerdict::compute_consensus(&findings),
        Consensus::Disagreement
    );
}

#[test]
fn consensus_disagreement_on_does_not_hold_keyword() {
    let findings = vec![
        ok_finding(
            ModelId::Deepseek,
            "The bound holds for all n > 0, therefore true.",
        ),
        ok_finding(ModelId::Qwen, "This does not hold in general."),
    ];
    assert_eq!(
        OracleVerdict::compute_consensus(&findings),
        Consensus::Disagreement
    );
}

#[test]
fn consensus_disagreement_on_qed_vs_disprove() {
    let findings = vec![
        ok_finding(ModelId::Leanstral, "Proof complete. QED."),
        ok_finding(
            ModelId::Deepseek,
            "I can disprove this with a simple example.",
        ),
    ];
    assert_eq!(
        OracleVerdict::compute_consensus(&findings),
        Consensus::Disagreement
    );
}

// ── compute_consensus: keyword case-insensitivity ─────────────────────────────

#[test]
fn consensus_keywords_are_case_insensitive() {
    // "FALSE" should still trigger the negative signal.
    let findings = vec![
        ok_finding(ModelId::Deepseek, "PROVEN by strong induction."),
        ok_finding(ModelId::Qwen, "This is FALSE for n=0."),
    ];
    assert_eq!(
        OracleVerdict::compute_consensus(&findings),
        Consensus::Disagreement
    );
}

// ── Consensus serde ───────────────────────────────────────────────────────────

#[test]
fn consensus_roundtrip_serde() {
    for c in [
        Consensus::Unanimous,
        Consensus::Majority,
        Consensus::Disagreement,
        Consensus::Insufficient,
    ] {
        let json = serde_json::to_string(&c).expect("serialize");
        let back: Consensus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(c, back);
    }
}

// ── FindingStatus serde ───────────────────────────────────────────────────────

#[test]
fn finding_status_ok_roundtrip() {
    let json = serde_json::to_string(&FindingStatus::Ok).expect("serialize");
    let back: FindingStatus = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, FindingStatus::Ok);
}

#[test]
fn finding_status_error_roundtrip() {
    let status = FindingStatus::Error("something went wrong".to_string());
    let json = serde_json::to_string(&status).expect("serialize");
    let back: FindingStatus = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        back,
        FindingStatus::Error("something went wrong".to_string())
    );
}

#[test]
fn finding_status_timeout_roundtrip() {
    let json = serde_json::to_string(&FindingStatus::Timeout).expect("serialize");
    let back: FindingStatus = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, FindingStatus::Timeout);
}

// ── OracleVerdict Display ─────────────────────────────────────────────────────

#[test]
fn verdict_display_includes_model_counts_and_consensus() {
    let v = verdict_from(vec![
        ok_finding(ModelId::Deepseek, "proven by induction"),
        ok_finding(ModelId::Qwen, "verified numerically"),
        error_finding(ModelId::Leanstral),
    ]);
    let display = v.to_string();
    assert!(display.contains("2/3"), "should show 2 ok out of 3 total");
    assert!(
        display.contains("Majority"),
        "should name the consensus variant"
    );
}

#[test]
fn verdict_display_includes_each_finding_model_name() {
    let v = verdict_from(vec![
        ok_finding(ModelId::Deepseek, "derivation done"),
        ok_finding(ModelId::Qwen, "numerical check done"),
    ]);
    let display = v.to_string();
    assert!(
        display.contains("deepseek"),
        "display should contain model name"
    );
    assert!(
        display.contains("qwen"),
        "display should contain model name"
    );
}

#[test]
fn verdict_display_shows_error_content_for_failed_findings() {
    let v = verdict_from(vec![
        ok_finding(ModelId::Deepseek, "proven"),
        ok_finding(ModelId::Qwen, "verified"),
        error_finding(ModelId::Leanstral),
    ]);
    let display = v.to_string();
    assert!(
        display.contains("ERROR"),
        "error findings should show ERROR label"
    );
}

#[test]
fn verdict_models_ok_matches_ok_findings_count() {
    let findings = vec![
        ok_finding(ModelId::Deepseek, "ok"),
        ok_finding(ModelId::Qwen, "ok"),
        error_finding(ModelId::Leanstral),
        timeout_finding(ModelId::Kimi),
    ];
    let v = verdict_from(findings);
    assert_eq!(v.models_ok, 2);
    assert_eq!(v.models_total, 4);
}
