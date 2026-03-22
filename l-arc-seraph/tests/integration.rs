//! Integration tests for `l-arc-seraph` using an in-process `MockTransport`.
//!
//! SERAPH responses are AI-generated pentest prose wrapped in the MCP
//! `ToolCallResult` envelope. Tests inject canned envelopes to verify
//! that every client method correctly decodes the text payload.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use l_arc_core::jsonrpc::{JsonRpcError, JsonRpcResponse};
use l_arc_core::transport::Transport;
use l_arc_core::{JsonRpcRequest, RetryConfig, SdkError};
use l_arc_seraph::{SeraphClient, Wing};

// ── MockTransport ─────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct MockTransport {
    responses: Arc<Mutex<VecDeque<Result<JsonRpcResponse, SdkError>>>>,
}

/// Build the standard `ToolCallResult` envelope that SERAPH returns.
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

fn client(mock: MockTransport) -> SeraphClient<MockTransport> {
    SeraphClient::from_transport(mock, test_retry())
}

// ── Wing actions ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn capture_returns_prose() {
    let mock = MockTransport::default();
    mock.push_text("Capture: 847 packets collected on eth0. 3 SYN floods detected.");
    let out = client(mock).capture("eth0").await.unwrap();
    assert!(out.output.contains("847 packets"));
}

#[tokio::test]
async fn scan_returns_discovery_prose() {
    let mock = MockTransport::default();
    mock.push_text(
        "Scan complete — 7 hosts discovered on 192.168.1.0/24. Open ports: 22, 80, 443.",
    );
    let out = client(mock).scan("192.168.1.0/24").await.unwrap();
    assert!(out.output.contains("7 hosts"));
}

#[tokio::test]
async fn analyze_returns_artefact_prose() {
    let mock = MockTransport::default();
    mock.push_text("Binary analysis: ELF64, 4 imported syscalls, 1 suspicious ROP gadget.");
    let out = client(mock).analyze("suspicious.elf").await.unwrap();
    assert!(out.output.contains("suspicious"));
}

#[tokio::test]
async fn osint_without_depth_returns_prose() {
    let mock = MockTransport::default();
    mock.push_text("OSINT: example.internal → AS12345, 3 subdomains, 1 exposed service.");
    let out = client(mock).osint("example.internal", None).await.unwrap();
    assert!(out.output.contains("OSINT"));
}

#[tokio::test]
async fn osint_with_depth_returns_prose() {
    let mock = MockTransport::default();
    mock.push_text("Deep OSINT: 23 subdomains, 5 certificate records, 2 leaked credentials.");
    let out = client(mock)
        .osint("example.internal", Some("deep"))
        .await
        .unwrap();
    assert!(out.output.contains("23 subdomains"));
}

#[tokio::test]
async fn monitor_returns_initial_report() {
    let mock = MockTransport::default();
    mock.push_text("Monitor started on 10.0.0.1. Baseline established. Alerting on anomalies.");
    let out = client(mock).monitor("10.0.0.1").await.unwrap();
    assert!(out.output.contains("Baseline established"));
}

#[tokio::test]
async fn execute_returns_engagement_prose() {
    let mock = MockTransport::default();
    mock.push_text("Execute: payload delivered to staging-03. Shell obtained. Persistence: none.");
    let out = client(mock).execute("staging-03").await.unwrap();
    assert!(out.output.contains("staging-03"));
}

// ── Service actions ───────────────────────────────────────────────────────────

#[tokio::test]
async fn detonate_returns_sandbox_report() {
    let mock = MockTransport::default();
    mock.push_text("Detonation complete: sample is ransomware. Encrypted 42 files in 2.1s.");
    let out = client(mock).detonate("sample.exe").await.unwrap();
    assert!(out.output.contains("ransomware"));
}

// ── Utility actions ───────────────────────────────────────────────────────────

#[tokio::test]
async fn status_returns_engagement_state() {
    let mock = MockTransport::default();
    mock.push_text("SERAPH status: engagement ENG-DEFAULT active. TTL: 6800h. Scope: home-lab.");
    let out = client(mock).status().await.unwrap();
    assert!(out.output.contains("ENG-DEFAULT"));
}

// ── Generic `wing()` convenience method ──────────────────────────────────────

#[tokio::test]
async fn wing_enum_scan_is_equivalent_to_scan_method() {
    let mock = MockTransport::default();
    mock.push_text("Scan via wing enum: 3 hosts found.");
    let out = client(mock).wing(Wing::Scan, "10.0.0.0/29").await.unwrap();
    assert!(out.output.contains("3 hosts"));
}

#[tokio::test]
async fn wing_enum_analyze() {
    let mock = MockTransport::default();
    mock.push_text("Analyze via wing enum: benign binary.");
    let out = client(mock)
        .wing(Wing::Analyze, "benign.elf")
        .await
        .unwrap();
    assert!(out.output.contains("benign"));
}

#[tokio::test]
async fn wing_enum_osint() {
    let mock = MockTransport::default();
    mock.push_text("OSINT via wing enum: 5 subdomains found.");
    let out = client(mock)
        .wing(Wing::Osint, "internal.example")
        .await
        .unwrap();
    assert!(out.output.contains("5 subdomains"));
}

// ── Error propagation ─────────────────────────────────────────────────────────

#[tokio::test]
async fn tool_error_envelope_surfaces_as_sdk_tool_error() {
    let mock = MockTransport::default();
    mock.push_error_envelope("scope violation: target not in engagement");
    let err = client(mock).scan("10.99.0.1").await.unwrap_err();
    assert!(
        matches!(err, SdkError::Tool(_)),
        "expected Tool error, got: {err:?}"
    );
}

#[tokio::test]
async fn rpc_error_surfaces_as_protocol_error() {
    let mock = MockTransport::default();
    mock.push_rpc_error(-32_600, "invalid request");
    let err = client(mock).status().await.unwrap_err();
    assert!(
        matches!(err, SdkError::Protocol(_)),
        "expected Protocol error, got: {err:?}"
    );
}

#[tokio::test]
async fn exhausted_queue_returns_config_error() {
    let mock = MockTransport::default(); // No responses queued.
    let err = client(mock).status().await.unwrap_err();
    assert!(
        matches!(err, SdkError::Config(_)),
        "expected Config error, got: {err:?}"
    );
}
