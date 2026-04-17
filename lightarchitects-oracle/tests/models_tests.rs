//! Unit tests for [`ModelId`], [`ModelRole`], and [`OracleMode`] — model registry logic.
//!
//! These tests are purely synchronous and require no network access or I/O.
//! They verify the model-selection contracts that `OracleQuery` depends on.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects_oracle::{ModelId, ModelRole, OracleMode};

// ── ModelId::for_mode ─────────────────────────────────────────────────────────

#[test]
fn for_mode_prove_dispatches_to_three_models() {
    let models = ModelId::for_mode(OracleMode::Prove);
    assert_eq!(models.len(), 3, "Prove requires exactly 3 models");
    assert!(
        models.contains(&ModelId::Leanstral),
        "Prove needs formal proof"
    );
    assert!(
        models.contains(&ModelId::Deepseek),
        "Prove needs derivation"
    );
    assert!(
        models.contains(&ModelId::Qwen),
        "Prove needs numerical check"
    );
}

#[test]
fn for_mode_optimize_dispatches_to_three_models() {
    let models = ModelId::for_mode(OracleMode::Optimize);
    assert_eq!(models.len(), 3);
    assert!(
        !models.contains(&ModelId::Leanstral),
        "Optimize skips formal proof"
    );
    assert!(models.contains(&ModelId::Deepseek));
    assert!(models.contains(&ModelId::Qwen));
    assert!(
        models.contains(&ModelId::Kimi),
        "Optimize adds deep reasoning"
    );
}

#[test]
fn for_mode_full_dispatches_all_five_models() {
    let models = ModelId::for_mode(OracleMode::Full);
    assert_eq!(models.len(), 5, "Full must include all 5 models");
    let expected = [
        ModelId::Leanstral,
        ModelId::Deepseek,
        ModelId::Qwen,
        ModelId::Kimi,
        ModelId::Cogito,
    ];
    for id in expected {
        assert!(models.contains(&id), "Full must include {id}");
    }
}

#[test]
fn for_mode_custom_returns_empty_caller_provides_models() {
    let models = ModelId::for_mode(OracleMode::Custom);
    assert!(
        models.is_empty(),
        "Custom mode returns empty — caller passes models via .models()"
    );
}

// ── Prove and Optimize are disjoint on Leanstral ─────────────────────────────

#[test]
fn prove_and_optimize_share_no_unique_models() {
    let prove = ModelId::for_mode(OracleMode::Prove);
    let optimize = ModelId::for_mode(OracleMode::Optimize);
    let shared: Vec<_> = prove.iter().filter(|m| optimize.contains(m)).collect();
    // Deepseek and Qwen are shared — both appear in Prove and Optimize.
    assert_eq!(
        shared.len(),
        2,
        "Deepseek + Qwen are shared between Prove and Optimize"
    );
}

// ── ModelId Display ───────────────────────────────────────────────────────────

#[test]
fn model_id_display_lowercase_strings() {
    assert_eq!(ModelId::Leanstral.to_string(), "leanstral");
    assert_eq!(ModelId::Deepseek.to_string(), "deepseek");
    assert_eq!(ModelId::Qwen.to_string(), "qwen");
    assert_eq!(ModelId::Kimi.to_string(), "kimi");
    assert_eq!(ModelId::Cogito.to_string(), "cogito");
}

// ── ModelId serde ─────────────────────────────────────────────────────────────

#[test]
fn model_id_serializes_to_snake_case() {
    // serde(rename_all = "snake_case") on a single-word enum → lowercase.
    let json = serde_json::to_string(&ModelId::Leanstral).expect("serialize");
    assert_eq!(json, r#""leanstral""#);
    let json = serde_json::to_string(&ModelId::Deepseek).expect("serialize");
    assert_eq!(json, r#""deepseek""#);
}

#[test]
fn model_id_roundtrip_serde() {
    for id in [
        ModelId::Leanstral,
        ModelId::Deepseek,
        ModelId::Qwen,
        ModelId::Kimi,
        ModelId::Cogito,
    ] {
        let json = serde_json::to_string(&id).expect("serialize");
        let back: ModelId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back, "serde roundtrip failed for {id}");
    }
}

// ── ModelRole serde ───────────────────────────────────────────────────────────

#[test]
fn model_role_roundtrip_serde() {
    for role in [
        ModelRole::FormalProof,
        ModelRole::Derivation,
        ModelRole::Numerical,
        ModelRole::Reasoning,
    ] {
        let json = serde_json::to_string(&role).expect("serialize");
        let back: ModelRole = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(role, back, "serde roundtrip failed for {role:?}");
    }
}

// ── OracleMode properties ─────────────────────────────────────────────────────

#[test]
fn full_mode_is_superset_of_prove_and_optimize() {
    let full = ModelId::for_mode(OracleMode::Full);
    for id in ModelId::for_mode(OracleMode::Prove) {
        assert!(full.contains(&id), "Full must include Prove model {id}");
    }
    for id in ModelId::for_mode(OracleMode::Optimize) {
        assert!(full.contains(&id), "Full must include Optimize model {id}");
    }
}
