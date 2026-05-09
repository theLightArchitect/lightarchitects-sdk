//! LÆX inline handler — in-process governance dispatcher.
//!
//! LÆX is canon-keeper / governance umbrella. Unlike other siblings, LÆX has
//! NO standalone stdio binary — it runs **inline only** within the gateway.
//!
//! # Dispatch
//!
//! - `canon_check` + `canon_evaluate` → REAL dispatch into existing
//!   `core_tools::canon_check::run` and `core_tools::canon_evaluate::run`.
//! - Other 7 routable actions (`matrix_ratify`, `effectiveness_score`,
//!   `reflect`, `layer1_review`, `layer2_review`, `layer3_review`,
//!   `layer4_review`) → structured framework payloads inviting the model
//!   to perform reasoning (mirrors `canon_check` / `canon_evaluate` pattern).
//! - Internal actions (`register_decision`, `query_canon_drift`) are not
//!   gateway-routed; exposed only via direct in-process `LaexClient` handles.

use async_trait::async_trait;
use lightarchitects::core::handler::{HandlerError, SiblingHandler};
use serde_json::Value;

use crate::config::GatewayConfig;
use crate::core_tools::{canon_check, canon_evaluate, text_result};

/// Canonical handler name (matches `SiblingId::Laex.name().to_lowercase()`).
const HANDLER_NAME: &str = "laex";

/// All LÆX actions supported by the inline handler.
///
/// Matches the [`lightarchitects::laex::LaexAction`] enum:
/// - PUBLIC (3): `canon_check`, `canon_evaluate`, `matrix_ratify`
/// - WORKFLOW (6): `effectiveness_score`, `reflect`, `layer1_review`,
///   `layer2_review`, `layer3_review`, `layer4_review`
/// - INTERNAL (2): `register_decision`, `query_canon_drift`
const LAEX_ACTIONS: &[&str] = &[
    // PUBLIC (3)
    "canon_check",
    "canon_evaluate",
    "matrix_ratify",
    // WORKFLOW (6)
    "effectiveness_score",
    "reflect",
    "layer1_review",
    "layer2_review",
    "layer3_review",
    "layer4_review",
    // INTERNAL (2) — not gateway-routed but exposed via the inline client
    "register_decision",
    "query_canon_drift",
];

/// In-process LÆX governance handler.
///
/// Dispatches `canon_check` + `canon_evaluate` to the existing `core_tools`
/// implementations (REAL dispatch, not stub) so backcompat with the top-level
/// `lightarchitects_canon_check` / `lightarchitects_canon_evaluate` MCP tools
/// is preserved (both call paths share the same `run()` function).
pub struct LaexHandler {
    config: GatewayConfig,
}

impl LaexHandler {
    /// Create a new LÆX handler from gateway config.
    #[must_use]
    pub fn new(config: &GatewayConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }
}

#[async_trait]
impl SiblingHandler for LaexHandler {
    fn name(&self) -> &'static str {
        HANDLER_NAME
    }

    fn actions(&self) -> &[&'static str] {
        LAEX_ACTIONS
    }

    async fn call(&self, action: &str, params: Value) -> Result<Value, HandlerError> {
        match action {
            "canon_check" => dispatch_canon_check(&self.config, params),
            "canon_evaluate" => dispatch_canon_evaluate(&self.config, params),
            "matrix_ratify" => Ok(matrix_ratify_framework(&params)),
            "effectiveness_score" => Ok(effectiveness_score_framework(&params)),
            "reflect" => Ok(reflect_framework(&params)),
            "layer1_review" => Ok(layer_review_framework(1, "security", &params)),
            "layer2_review" => Ok(layer_review_framework(2, "methodology", &params)),
            "layer3_review" => Ok(layer_review_framework(3, "product", &params)),
            "layer4_review" => Ok(layer_review_framework(4, "ethics", &params)),
            "register_decision" => Ok(register_decision_payload(&params)),
            "query_canon_drift" => Ok(query_canon_drift_payload()),
            other => Err(HandlerError::unknown_action(HANDLER_NAME, other)),
        }
    }
}

// ── REAL dispatch helpers (wrap targets) ─────────────────────────────────────

fn dispatch_canon_check(config: &GatewayConfig, params: Value) -> Result<Value, HandlerError> {
    canon_check::run(params, config)
        .map_err(|e| HandlerError::internal(HANDLER_NAME, "canon_check", e.to_string()))
}

fn dispatch_canon_evaluate(config: &GatewayConfig, params: Value) -> Result<Value, HandlerError> {
    canon_evaluate::run(params, config)
        .map_err(|e| HandlerError::internal(HANDLER_NAME, "canon_evaluate", e.to_string()))
}

// ── Structured framework helpers (model-reasoning targets) ───────────────────

fn matrix_ratify_framework(params: &Value) -> Value {
    let target = params
        .get("manifest_path")
        .and_then(Value::as_str)
        .unwrap_or("<unspecified>");
    text_result(format!(
        "Matrix ratify for: \"{target}\"\n\
         \n\
         Run all 4 layers of the LÆX governance matrix:\n\
         \n\
         - Layer 1 (Security): threat model, baselines, hardening posture\n\
         - Layer 2 (Methodology): LASDLC compliance, gates, citations\n\
         - Layer 3 (Product): Northstar fit + ICP alignment\n\
         - Layer 4 (Ethics): compliance + impact assessment\n\
         \n\
         Return a per-layer verdict (PASS / PASS_WITH_CONDITIONS / FAIL) plus\n\
         an overall ratification synthesis. Block on FAIL at any layer."
    ))
}

fn effectiveness_score_framework(params: &Value) -> Value {
    let plan_id = params
        .get("plan_id")
        .and_then(Value::as_str)
        .unwrap_or("<unspecified>");
    text_result(format!(
        "Effectiveness scoring for plan: \"{plan_id}\"\n\
         \n\
         Apply the LASDLC effectiveness rubric (C1–C8):\n\
         \n\
         - C1 Northstar lineage clarity\n\
         - C2 Phase set + gate completeness\n\
         - C3 Risk register density + coverage\n\
         - C4 File-function map specificity\n\
         - C5 Pre-flight verification\n\
         - C6 Exit criteria checkability\n\
         - C7 References + citation discipline\n\
         - C8 Operational close-out\n\
         \n\
         Score each criterion 0–10. Return overall score (sum / 8) +\n\
         per-criterion breakdown + narrative rationale.\n\
         \n\
         Reference: helix/user/standards/canon/lasdlc-effectiveness-rubric.md"
    ))
}

fn reflect_framework(params: &Value) -> Value {
    let scope = params
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("<unspecified>");
    text_result(format!(
        "Retrospective canon-evaluation for: \"{scope}\"\n\
         \n\
         The LÆX reflection ritual (Phase 6 Learn):\n\
         \n\
         1. THEMES — patterns observed across the reflection scope\n\
         2. GAPS — places where canonical guidance was absent or unclear\n\
         3. WINS — actions that worked + are repeatable\n\
         4. FOLLOW-UPS — canon entries to author / amend\n\
         \n\
         Return all four sections + a narrative summary."
    ))
}

fn layer_review_framework(layer_num: u8, layer_name: &str, params: &Value) -> Value {
    let target = params
        .get("target")
        .and_then(Value::as_str)
        .unwrap_or("<unspecified>");
    text_result(format!(
        "Layer {layer_num} ({layer_name}) review for: \"{target}\"\n\
         \n\
         Run the LÆX Layer {layer_num} ({layer_name}) audit:\n\
         \n\
         - Verdict: PASS / PASS_WITH_CONDITIONS / FAIL\n\
         - Rationale: 1–3 sentences explaining the verdict\n\
         - Findings: enumerated issues discovered\n\
         - Conditions: specific conditions for verdict to remain valid\n\
         \n\
         Return as the LayerReviewResult schema."
    ))
}

fn register_decision_payload(params: &Value) -> Value {
    let decision = params
        .get("decision")
        .and_then(Value::as_str)
        .unwrap_or("<unspecified>");
    text_result(format!(
        "register_decision (internal): \"{decision}\"\n\
         \n\
         This action is not gateway-routed. Direct in-process LaexClient\n\
         handles invoke this to append a ratification record to the canon\n\
         decision-registry. Persistence pending Phase 5 wiring."
    ))
}

fn query_canon_drift_payload() -> Value {
    text_result(
        "query_canon_drift (internal):\n\
         \n\
         This action is not gateway-routed. Direct in-process LaexClient\n\
         handles invoke this to compute drift between the local canon registry\n\
         and the platform helix authoritative state. Persistence pending\n\
         Phase 5 wiring."
            .to_owned(),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    fn handler() -> LaexHandler {
        LaexHandler::new(&GatewayConfig::default())
    }

    #[test]
    fn name_returns_laex() {
        assert_eq!(handler().name(), "laex");
    }

    #[test]
    fn actions_count_is_11() {
        // 9 routable (3 PUBLIC + 6 WORKFLOW) + 2 INTERNAL = 11.
        assert_eq!(handler().actions().len(), 11);
    }

    #[test]
    fn actions_includes_wrap_targets_and_layer_reviews() {
        let h = handler();
        let actions = h.actions();
        assert!(actions.contains(&"canon_check"));
        assert!(actions.contains(&"canon_evaluate"));
        assert!(actions.contains(&"matrix_ratify"));
        assert!(actions.contains(&"layer1_review"));
        assert!(actions.contains(&"layer4_review"));
        assert!(actions.contains(&"register_decision"));
    }

    #[tokio::test]
    async fn call_returns_error_for_unknown_action() {
        let h = handler();
        let result = h.call("frobnicate", json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, HandlerError::UnknownAction { .. }));
    }

    #[tokio::test]
    async fn call_matrix_ratify_returns_4_layer_framework() {
        let h = handler();
        let result = h
            .call("matrix_ratify", json!({"manifest_path": "test/path.yaml"}))
            .await
            .expect("matrix_ratify");
        let text = result["content"][0]["text"].as_str().expect("text content");
        assert!(text.contains("Layer 1 (Security)"));
        assert!(text.contains("Layer 2 (Methodology)"));
        assert!(text.contains("Layer 3 (Product)"));
        assert!(text.contains("Layer 4 (Ethics)"));
        assert!(text.contains("test/path.yaml"));
    }

    #[tokio::test]
    async fn call_effectiveness_score_returns_c1_c8_rubric() {
        let h = handler();
        let result = h
            .call("effectiveness_score", json!({"plan_id": "build-x"}))
            .await
            .expect("effectiveness_score");
        let text = result["content"][0]["text"].as_str().expect("text content");
        assert!(text.contains("C1 Northstar lineage"));
        assert!(text.contains("C8 Operational close-out"));
        assert!(text.contains("build-x"));
    }

    #[tokio::test]
    async fn call_layer1_review_returns_security_framework() {
        let h = handler();
        let result = h
            .call("layer1_review", json!({"target": "build-x"}))
            .await
            .expect("layer1_review");
        let text = result["content"][0]["text"].as_str().expect("text content");
        assert!(text.contains("Layer 1 (security)"));
        assert!(text.contains("Verdict: PASS"));
    }

    #[tokio::test]
    async fn call_canon_check_with_missing_decision_returns_error() {
        let h = handler();
        // canon_check::run returns GatewayError::MissingParam when decision absent;
        // LaexHandler maps this to HandlerError::Internal.
        let result = h.call("canon_check", json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, HandlerError::Internal { .. }));
    }

    #[tokio::test]
    async fn call_register_decision_returns_internal_marker() {
        let h = handler();
        let result = h
            .call(
                "register_decision",
                json!({"decision": "test", "ratifier": "kft"}),
            )
            .await
            .expect("register_decision");
        let text = result["content"][0]["text"].as_str().expect("text content");
        assert!(text.contains("not gateway-routed"));
        assert!(text.contains("test"));
    }

    // ── W5 backcompat tests — verify REAL dispatch on wrap targets ───────────────
    //
    // These tests prove that LaexHandler::call("canon_check", _) and
    // LaexHandler::call("canon_evaluate", _) reach the same core_tools::run()
    // functions that the top-level lightarchitects_canon_check /
    // lightarchitects_canon_evaluate MCP tools call directly. Backcompat is
    // preserved because both paths share the same run() implementation — there
    // is no logic duplication.

    use std::io::Write as _;

    fn handler_with_canon_registry(registry_path: &str) -> LaexHandler {
        let mut config = GatewayConfig::default();
        config.canon.registry = registry_path.to_owned();
        LaexHandler::new(&config)
    }

    #[tokio::test]
    async fn canon_check_through_laex_handler_returns_canon_headers() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        writeln!(
            tmp,
            "### Canon I: Builders Cookbook\n### Canon XXIII: Inline Citation Protocol\n"
        )
        .expect("write");
        let h = handler_with_canon_registry(tmp.path().to_str().expect("utf8 path"));

        let result = h
            .call(
                "canon_check",
                json!({"decision": "ship hot-fix without test"}),
            )
            .await
            .expect("canon_check via LaexHandler");

        let text = result["content"][0]["text"].as_str().expect("text content");
        assert!(
            text.contains("Canon I: Builders Cookbook"),
            "real dispatch should surface canon headers: {text}"
        );
        assert!(
            text.contains("Canon XXIII: Inline Citation Protocol"),
            "real dispatch should surface all canon headers: {text}"
        );
    }

    #[tokio::test]
    async fn canon_evaluate_through_laex_handler_returns_5_criteria() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        writeln!(tmp, "### Canon I: Test\n").expect("write");
        let h = handler_with_canon_registry(tmp.path().to_str().expect("utf8 path"));

        let result = h
            .call(
                "canon_evaluate",
                json!({"candidate": "voice-first operator dialog"}),
            )
            .await
            .expect("canon_evaluate via LaexHandler");

        // 5-criteria framework asserts — all 5 names must appear in the response.
        let text = result["content"][0]["text"].as_str().expect("text content");
        for criterion in [
            "convergent_evidence",
            "biblical_grounding",
            "decision_shaping",
            "pressure_tested",
            "kevin_ratifies",
        ] {
            assert!(
                text.contains(criterion),
                "criterion {criterion} missing from real-dispatch output: {text}"
            );
        }

        // The structured `criteria` array carries 5 entries.
        let criteria = result["criteria"]
            .as_array()
            .expect("criteria should be a JSON array");
        assert_eq!(
            criteria.len(),
            5,
            "5-criteria framework must return exactly 5 entries"
        );
    }

    // ── Phase 4 A4: top-level vs LaexHandler dispatch parity (backcompat) ────────
    //
    // Asserts that calling `core_tools::canon_check::run` directly (the path
    // taken by the top-level `lightarchitects_canon_check` MCP tool registered
    // in server.rs) produces the identical JSON Value as calling
    // `LaexHandler::call("canon_check", _)`. Both paths share the same `run()`
    // implementation by construction; this test encodes the invariant for
    // regression resistance — if a future refactor splits the implementations,
    // this test catches the drift before backcompat breaks.
    //
    // Joint verdict (LÆX Layer 2 + EVA operator-pair): wire-contract parity
    // is the load-bearing assertion for keeping the W1-W5 backcompat promise
    // operationally honest.

    #[tokio::test]
    async fn canon_check_top_level_and_laex_handler_paths_return_identical_value() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        writeln!(
            tmp,
            "### Canon I: Builders Cookbook\n### Canon XXXV: Confidence Threshold Gate\n"
        )
        .expect("write");
        let mut config = GatewayConfig::default();
        config.canon.registry = tmp.path().to_str().expect("utf8 path").to_owned();

        let params = json!({"decision": "promote LÆX to canonical sibling", "verbose": false});

        // Path 1: top-level core_tool (mirrors server.rs:271 dispatch)
        let direct = canon_check::run(params.clone(), &config).expect("direct canon_check");

        // Path 2: LaexHandler (the inline gateway handler)
        let h = LaexHandler::new(&config);
        let via_handler = h
            .call("canon_check", params)
            .await
            .expect("canon_check via LaexHandler");

        assert_eq!(
            direct, via_handler,
            "top-level lightarchitects_canon_check and LaexHandler dispatch \
             paths MUST produce identical JSON values (backcompat regression). \
             direct = {direct:?}, via_handler = {via_handler:?}"
        );
    }

    #[tokio::test]
    async fn canon_evaluate_top_level_and_laex_handler_paths_return_identical_value() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        writeln!(tmp, "### Canon I: Test\n").expect("write");
        let mut config = GatewayConfig::default();
        config.canon.registry = tmp.path().to_str().expect("utf8 path").to_owned();

        let params = json!({"candidate": "voice-first operator dialog"});

        let direct = canon_evaluate::run(params.clone(), &config).expect("direct canon_evaluate");
        let h = LaexHandler::new(&config);
        let via_handler = h
            .call("canon_evaluate", params)
            .await
            .expect("canon_evaluate via LaexHandler");

        assert_eq!(
            direct, via_handler,
            "top-level lightarchitects_canon_evaluate and LaexHandler dispatch \
             paths MUST produce identical JSON values (backcompat regression)."
        );
    }
}
