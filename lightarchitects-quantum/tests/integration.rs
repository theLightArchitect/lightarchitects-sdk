//! Integration tests for `lightarchitects-quantum` using an in-process `MockTransport`.
//!
//! All QUANTUM responses are AI-generated prose wrapped in the MCP
//! `ToolCallResult` envelope. Tests push canned envelopes and assert that
//! each typed method correctly extracts the text output.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lightarchitects_core::jsonrpc::{JsonRpcError, JsonRpcResponse};
use lightarchitects_core::transport::Transport;
use lightarchitects_core::{JsonRpcRequest, RetryConfig, SdkError};
use lightarchitects_quantum::QuantumClient;

// ── MockTransport ─────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct MockTransport {
    responses: Arc<Mutex<VecDeque<Result<JsonRpcResponse, SdkError>>>>,
}

/// Build the standard `ToolCallResult` envelope that QUANTUM returns.
fn text_envelope(text: &str) -> serde_json::Value {
    serde_json::json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false
    })
}

/// Build an error `ToolCallResult` envelope (`isError: true`).
fn error_envelope(message: &str) -> serde_json::Value {
    serde_json::json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true
    })
}

impl MockTransport {
    fn push_text(&self, text: &str) {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: Some(1),
            result: Some(text_envelope(text)),
            error: None,
        };
        self.responses
            .lock()
            .expect("mock lock")
            .push_back(Ok(resp));
    }

    fn push_error_envelope(&self, message: &str) {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: Some(1),
            result: Some(error_envelope(message)),
            error: None,
        };
        self.responses
            .lock()
            .expect("mock lock")
            .push_back(Ok(resp));
    }

    fn push_rpc_error(&self, code: i64, message: &str) {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: Some(1),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.to_owned(),
            }),
        };
        self.responses
            .lock()
            .expect("mock lock")
            .push_back(Ok(resp));
    }
}

impl Transport for MockTransport {
    async fn send(&self, _request: JsonRpcRequest) -> Result<JsonRpcResponse, SdkError> {
        self.responses
            .lock()
            .expect("mock lock")
            .pop_front()
            .unwrap_or_else(|| {
                Err(SdkError::Config(
                    "MockTransport: response queue exhausted".to_owned(),
                ))
            })
    }
}

/// Minimal retry config for tests — one attempt, no delays.
fn test_retry() -> RetryConfig {
    RetryConfig {
        max_attempts: 1,
        base_delay: Duration::ZERO,
        jitter: 0.0,
    }
}

fn client(mock: MockTransport) -> QuantumClient<MockTransport> {
    QuantumClient::from_transport(mock, test_retry())
}

// ── Investigation cycle actions ───────────────────────────────────────────────

#[tokio::test]
async fn scan_returns_prose() {
    let mock = MockTransport::default();
    mock.push_text("Initial scan complete — 3 signals detected in auth subsystem.");
    let out = client(mock).triage("auth failures").await.unwrap();
    assert!(out.output.contains("3 signals"));
}

#[tokio::test]
async fn sweep_returns_prose() {
    let mock = MockTransport::default();
    mock.push_text("Sweep expanded evidence pool to 12 correlated signals.");
    let out = client(mock).sweep("auth failures").await.unwrap();
    assert!(out.output.contains("12 correlated"));
}

#[tokio::test]
async fn trace_returns_prose() {
    let mock = MockTransport::default();
    mock.push_text("Evidence chain traced: token refresh → expiry check → clock drift.");
    let out = client(mock).trace("JWT token expiry").await.unwrap();
    assert!(out.output.contains("clock drift"));
}

#[tokio::test]
async fn probe_returns_prose() {
    let mock = MockTransport::default();
    mock.push_text("Deep probe of auth.rs: 2 suspicious patterns found.");
    let out = client(mock).probe("auth.rs").await.unwrap();
    assert!(out.output.contains("2 suspicious"));
}

#[tokio::test]
async fn theorize_without_context_returns_prose() {
    let mock = MockTransport::default();
    mock.push_text(
        "Hypothesis: clock skew on node-3 causes intermittent JWT expiry. Confidence: 80%.",
    );
    let out = client(mock).theorize("JWT expiry", None).await.unwrap();
    assert!(out.output.contains("clock skew"));
}

#[tokio::test]
async fn theorize_with_context_returns_prose() {
    let mock = MockTransport::default();
    mock.push_text("Revised hypothesis: NTP drift confirmed by monitoring logs.");
    let out = client(mock)
        .theorize("JWT expiry", Some("NTP service logs show drift"))
        .await
        .unwrap();
    assert!(out.output.contains("NTP drift"));
}

#[tokio::test]
async fn verify_returns_verdict_prose() {
    let mock = MockTransport::default();
    mock.push_text("Verdict: CONFIRMED — NTP drift of 4.2s explains observed JWT rejections.");
    let out = client(mock)
        .verify("clock skew causes JWT expiry")
        .await
        .unwrap();
    assert!(out.output.contains("CONFIRMED"));
}

#[tokio::test]
async fn close_returns_final_report() {
    let mock = MockTransport::default();
    mock.push_text(
        "Investigation CLOSED. Root cause: NTP drift on node-3. Remediation: resync NTP.",
    );
    let out = client(mock)
        .close("NTP drift confirmed as root cause")
        .await
        .unwrap();
    assert!(out.output.contains("CLOSED"));
}

// ── Utility actions ───────────────────────────────────────────────────────────

#[tokio::test]
async fn quick_returns_compressed_report() {
    let mock = MockTransport::default();
    mock.push_text("Quick investigation: probable cause is misconfigured retry logic.");
    let out = client(mock)
        .quick("retry storms in background worker")
        .await
        .unwrap();
    assert!(!out.output.is_empty());
}

#[tokio::test]
async fn research_returns_synthesised_summary() {
    let mock = MockTransport::default();
    mock.push_text("Research on Rust async cancellation: 3 papers, 7 docs, 2 CVEs found.");
    let out = client(mock)
        .research("Rust async cancellation safety")
        .await
        .unwrap();
    assert!(out.output.contains("3 papers"));
}

#[tokio::test]
async fn helix_without_sibling_filter() {
    let mock = MockTransport::default();
    mock.push_text("Helix query returned 5 entries related to auth failures.");
    let out = client(mock).helix("auth failures", None).await.unwrap();
    assert!(out.output.contains("5 entries"));
}

#[tokio::test]
async fn helix_with_sibling_filter() {
    let mock = MockTransport::default();
    mock.push_text("CORSO helix: 2 entries about security gate failures.");
    let out = client(mock)
        .helix("security gate", Some("corso"))
        .await
        .unwrap();
    assert!(out.output.contains("2 entries"));
}

#[tokio::test]
async fn discover_surfaces_patterns() {
    let mock = MockTransport::default();
    mock.push_text("Pattern discovery: exponential backoff missing in 3 retry paths.");
    let out = client(mock).discover("src/retry/").await.unwrap();
    assert!(out.output.contains("backoff missing"));
}

#[tokio::test]
async fn list_returns_investigation_summary() {
    let mock = MockTransport::default();
    mock.push_text("Active investigations: 0. Closed: 4. Last: JWT expiry (CONFIRMED).");
    let out = client(mock).list().await.unwrap();
    assert!(out.output.contains("Closed: 4"));
}

#[tokio::test]
async fn workflow_executes_named_sequence() {
    let mock = MockTransport::default();
    mock.push_text("Workflow 'auth-audit' complete — 2 findings, 0 critical.");
    let out = client(mock).workflow("auth-audit").await.unwrap();
    assert!(out.output.contains("auth-audit"));
}

// ── Error propagation ─────────────────────────────────────────────────────────

#[tokio::test]
async fn tool_error_envelope_surfaces_as_sdk_tool_error() {
    let mock = MockTransport::default();
    mock.push_error_envelope("scope violation: target not authorised");
    let err = client(mock).triage("192.168.99.1").await.unwrap_err();
    assert!(
        matches!(err, SdkError::Tool(_)),
        "expected Tool error, got: {err:?}"
    );
}

#[tokio::test]
async fn rpc_error_surfaces_as_protocol_error() {
    let mock = MockTransport::default();
    mock.push_rpc_error(-32_600, "invalid request");
    let err = client(mock).triage("test").await.unwrap_err();
    assert!(
        matches!(err, SdkError::Protocol(_)),
        "expected Protocol error, got: {err:?}"
    );
}

#[tokio::test]
async fn exhausted_queue_returns_config_error() {
    let mock = MockTransport::default(); // No responses queued.
    let err = client(mock).list().await.unwrap_err();
    assert!(
        matches!(err, SdkError::Config(_)),
        "expected Config error, got: {err:?}"
    );
}

#[tokio::test]
async fn sweep_tool_error_surfaces_as_sdk_tool_error() {
    let mock = MockTransport::default();
    // Scenario: QUANTUM sweep rejected due to scope constraint.
    mock.push_error_envelope("sweep: evidence pool locked — active investigation required");
    let err = client(mock).sweep("auth failures").await.unwrap_err();
    assert!(
        matches!(err, SdkError::Tool(_)),
        "expected Tool error, got: {err:?}"
    );
}

#[tokio::test]
async fn theorize_rpc_error_surfaces_as_protocol_error() {
    let mock = MockTransport::default();
    // Scenario: JSON-RPC protocol error (QUANTUM server process crashed mid-response).
    mock.push_rpc_error(-32_603, "internal error");
    let err = client(mock).theorize("JWT expiry", None).await.unwrap_err();
    assert!(
        matches!(err, SdkError::Protocol(_)),
        "expected Protocol error, got: {err:?}"
    );
}

#[tokio::test]
async fn research_exhausted_queue_returns_config_error() {
    let mock = MockTransport::default(); // No responses queued.
    let err = client(mock)
        .research("Rust async cancellation")
        .await
        .unwrap_err();
    assert!(
        matches!(err, SdkError::Config(_)),
        "expected Config error, got: {err:?}"
    );
}
