//! Contract tests for the assertion-gate event schema used in squad-comms.
//!
//! Validates that `OperatorMessage` variants serialize to the expected JSON
//! structure consumed by the webshell-ui AG-UI SSE stream. These tests serve
//! as living documentation of the `decision_request.assertion_resolve` v1.0 wire
//! contract so that webshell-ui TypeScript consumers don't silently break on
//! gateway schema changes.
//!
//! Canon XXVII test pyramid: integration tier (schema contract).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects_gateway::channels::OperatorMessage;
use serde_json::Value;

fn resolve_message() -> OperatorMessage {
    OperatorMessage::ResolveAssertionGate {
        request_id: "req-001".into(),
        assertion_id: "assert-001".into(),
        build_id: "luminous-confidence-portal".into(),
        operator_id: "op-kft".into(),
        hook_name: "PreToolUse:Assertion_ConfidenceThresholdGate".into(),
        blocked_at: "2026-05-04T10:00:00Z".into(),
        ttl_secs: 300,
    }
}

fn query_message() -> OperatorMessage {
    OperatorMessage::QueryBlockedFlow {
        build_id: "luminous-confidence-portal".into(),
        assertion_id: Some("assert-001".into()),
        operator_id: "op-kft".into(),
    }
}

// ── Schema contract: ResolveAssertionGate ─────────────────────────────────────

#[test]
fn resolve_serializes_with_type_tag() {
    let json: Value = serde_json::to_value(resolve_message()).unwrap();
    assert_eq!(json["type"], "resolve_assertion_gate");
}

#[test]
fn resolve_contains_all_required_fields() {
    let json: Value = serde_json::to_value(resolve_message()).unwrap();
    assert_eq!(json["request_id"], "req-001");
    assert_eq!(json["assertion_id"], "assert-001");
    assert_eq!(json["build_id"], "luminous-confidence-portal");
    assert_eq!(json["operator_id"], "op-kft");
    assert_eq!(
        json["hook_name"],
        "PreToolUse:Assertion_ConfidenceThresholdGate"
    );
    assert_eq!(json["blocked_at"], "2026-05-04T10:00:00Z");
    assert_eq!(json["ttl_secs"], 300);
}

#[test]
fn resolve_roundtrips_via_serde() {
    let original = resolve_message();
    let json = serde_json::to_string(&original).unwrap();
    let decoded: OperatorMessage = serde_json::from_str(&json).unwrap();
    let re_json = serde_json::to_string(&decoded).unwrap();
    assert_eq!(json, re_json);
}

// ── Schema contract: QueryBlockedFlow ─────────────────────────────────────────

#[test]
fn query_serializes_with_type_tag() {
    let json: Value = serde_json::to_value(query_message()).unwrap();
    assert_eq!(json["type"], "query_blocked_flow");
}

#[test]
fn query_contains_required_fields() {
    let json: Value = serde_json::to_value(query_message()).unwrap();
    assert_eq!(json["build_id"], "luminous-confidence-portal");
    assert_eq!(json["operator_id"], "op-kft");
}

#[test]
fn query_assertion_id_filter_optional() {
    let with_filter: Value = serde_json::to_value(query_message()).unwrap();
    assert_eq!(with_filter["assertion_id"], "assert-001");

    let without_filter: Value = serde_json::to_value(OperatorMessage::QueryBlockedFlow {
        build_id: "build-x".into(),
        assertion_id: None,
        operator_id: "op-kft".into(),
    })
    .unwrap();
    assert!(without_filter["assertion_id"].is_null());
}

#[test]
fn query_roundtrips_via_serde() {
    let original = query_message();
    let json = serde_json::to_string(&original).unwrap();
    let decoded: OperatorMessage = serde_json::from_str(&json).unwrap();
    let re_json = serde_json::to_string(&decoded).unwrap();
    assert_eq!(json, re_json);
}

// ── Schema stability: field names must NOT change ─────────────────────────────

#[test]
fn resolve_wire_format_is_stable() {
    // This test pins the exact JSON keys. If a field is renamed, this test
    // breaks and forces a webshell-ui migration — intentional friction.
    let expected_keys = [
        "type",
        "request_id",
        "assertion_id",
        "build_id",
        "operator_id",
        "hook_name",
        "blocked_at",
        "ttl_secs",
    ];
    let json: Value = serde_json::to_value(resolve_message()).unwrap();
    let obj = json.as_object().unwrap();
    for key in &expected_keys {
        assert!(
            obj.contains_key(*key),
            "Missing expected field '{key}' in ResolveAssertionGate wire format"
        );
    }
}

#[test]
fn type_tag_uses_snake_case() {
    // webshell-ui consumers depend on snake_case type tags from serde(rename_all).
    let resolve_json: Value = serde_json::to_value(resolve_message()).unwrap();
    let query_json: Value = serde_json::to_value(query_message()).unwrap();
    let rt = resolve_json["type"].as_str().unwrap();
    let qt = query_json["type"].as_str().unwrap();
    assert!(rt.chars().all(|c| c.is_lowercase() || c == '_'));
    assert!(qt.chars().all(|c| c.is_lowercase() || c == '_'));
}

// ── LASDLC semconv constants referenced in events ────────────────────────────

#[test]
fn semconv_hook_fire_span_name_constant_accessible() {
    use lightarchitects::ayin::semconv::lasdlc::SPAN_HOOK_FIRE;
    assert_eq!(SPAN_HOOK_FIRE, "lasdlc.hook.fire");
}

#[test]
fn semconv_assertion_resolve_span_name_constant_accessible() {
    use lightarchitects::ayin::semconv::lasdlc::SPAN_ASSERTION_RESOLVE;
    assert_eq!(SPAN_ASSERTION_RESOLVE, "lasdlc.assertion.resolve");
}

// json! usage in tests above exercises the import.
