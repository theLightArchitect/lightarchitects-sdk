//! Integration tests for the operator resolve-assertion end-to-end flow.
//!
//! Tests the HMAC sign → verify → replay-window chain that underpins the
//! `/v1/admin/operator/resolve-assertion` route (Wave 3.2 cross-repo IPC
//! trust Layer 3).
//!
//! Canon XXVII test pyramid: integration tier.
//!
//! Note: The full webshell round-trip cannot be tested here (no running webshell
//! in CI). The webshell delegation path is covered by contract tests in
//! `assertion_events_schema.rs`. This file tests gateway-side verification only.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects_gateway::security::hmac::{
    HookPayload, SecurityError, replay_window_check, sign_hook_payload, verify_hook_payload,
};

const SECRET: &[u8] = b"test-secret-wave-3-2-integration";

fn fresh_payload() -> HookPayload {
    HookPayload {
        assertion_id: "assert-wv32-001".into(),
        action_type: "provide_citation".into(),
        operator_id: "op-kft".into(),
        timestamp_iso8601: chrono::Utc::now().to_rfc3339(),
    }
}

// ── HMAC sign-verify round-trips ─────────────────────────────────────────────

#[test]
fn sign_and_verify_roundtrip() {
    let payload = fresh_payload();
    let sig = sign_hook_payload(&payload, SECRET).unwrap();
    assert!(verify_hook_payload(&payload, SECRET, &sig).unwrap());
}

#[test]
fn tampered_assertion_id_fails() {
    let payload = fresh_payload();
    let sig = sign_hook_payload(&payload, SECRET).unwrap();
    let mut tampered = payload.clone();
    tampered.assertion_id = "assert-wv32-EVIL".into();
    assert!(!verify_hook_payload(&tampered, SECRET, &sig).unwrap());
}

#[test]
fn tampered_action_type_fails() {
    let payload = fresh_payload();
    let sig = sign_hook_payload(&payload, SECRET).unwrap();
    let mut tampered = payload.clone();
    tampered.action_type = "delete_all".into();
    assert!(!verify_hook_payload(&tampered, SECRET, &sig).unwrap());
}

#[test]
fn tampered_operator_id_fails() {
    let payload = fresh_payload();
    let sig = sign_hook_payload(&payload, SECRET).unwrap();
    let mut tampered = payload.clone();
    tampered.operator_id = "op-attacker".into();
    assert!(!verify_hook_payload(&tampered, SECRET, &sig).unwrap());
}

#[test]
fn wrong_secret_fails() {
    let payload = fresh_payload();
    let sig = sign_hook_payload(&payload, SECRET).unwrap();
    assert!(!verify_hook_payload(&payload, b"wrong-secret", &sig).unwrap());
}

#[test]
fn empty_signature_fails() {
    let payload = fresh_payload();
    assert!(!verify_hook_payload(&payload, SECRET, "").unwrap());
}

#[test]
fn truncated_signature_fails() {
    let payload = fresh_payload();
    let sig = sign_hook_payload(&payload, SECRET).unwrap();
    let truncated = &sig[..32];
    assert!(!verify_hook_payload(&payload, SECRET, truncated).unwrap());
}

#[test]
fn signature_is_64_hex_chars() {
    let payload = fresh_payload();
    let sig = sign_hook_payload(&payload, SECRET).unwrap();
    assert_eq!(sig.len(), 64);
    assert!(
        sig.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
    );
}

// ── Replay window ─────────────────────────────────────────────────────────────

#[test]
fn replay_window_accepts_now() {
    let ts = chrono::Utc::now().to_rfc3339();
    assert!(replay_window_check(&ts).unwrap());
}

#[test]
fn replay_window_accepts_30s_ago() {
    let ts = (chrono::Utc::now() - chrono::Duration::seconds(30)).to_rfc3339();
    assert!(replay_window_check(&ts).unwrap());
}

#[test]
fn replay_window_rejects_65s_ago() {
    let ts = (chrono::Utc::now() - chrono::Duration::seconds(65)).to_rfc3339();
    assert!(!replay_window_check(&ts).unwrap());
}

#[test]
fn replay_window_rejects_far_future() {
    // 120s in the future: well beyond the 5s skew tolerance.
    let ts = (chrono::Utc::now() + chrono::Duration::seconds(120)).to_rfc3339();
    assert!(!replay_window_check(&ts).unwrap());
}

#[test]
fn replay_window_rejects_near_future_beyond_skew() {
    // 10s future exceeds the 5s max clock-skew allowance — must be rejected.
    // This catches pre-signed payloads intended for delivery slightly later.
    let ts = (chrono::Utc::now() + chrono::Duration::seconds(10)).to_rfc3339();
    assert!(!replay_window_check(&ts).unwrap());
}

#[test]
fn replay_window_returns_err_on_invalid_timestamp() {
    let result = replay_window_check("not-a-timestamp");
    assert!(matches!(result, Err(SecurityError::InvalidTimestamp(_))));
}

// ── Adversarial inputs ────────────────────────────────────────────────────────

#[test]
fn sign_rejects_empty_secret() {
    let payload = fresh_payload();
    assert!(sign_hook_payload(&payload, b"").is_err());
}

#[test]
fn verify_with_all_zeros_signature_fails() {
    let payload = fresh_payload();
    let zeros = "0".repeat(64);
    // Should not panic — returns false.
    let result = verify_hook_payload(&payload, SECRET, &zeros).unwrap();
    assert!(!result);
}

#[test]
fn payload_fields_are_all_order_dependent() {
    // Verifies canonical message is deterministic by signing twice and comparing.
    let payload = fresh_payload();
    let sig1 = sign_hook_payload(&payload, SECRET).unwrap();
    let sig2 = sign_hook_payload(&payload, SECRET).unwrap();
    assert_eq!(sig1, sig2);
}
