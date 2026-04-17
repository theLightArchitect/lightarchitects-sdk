//! Integration tests for the `graphrag_ingest` fluent builder on `SoulClient`.
//!
//! All tests use an in-process mock transport — no SOUL binary is spawned, no
//! network traffic is produced.  Each test drives the builder end-to-end through
//! the `SoulClient` public API and asserts on the decoded `GraphRagIngestResult`.
//!
//! ## Transport design
//!
//! Two mock transports are used:
//!
//! - `RecordingMock` — captures every `JsonRpcRequest` sent by the client so
//!   tests can inspect the action name, tool name, and serialised parameters.
//!   Used for tests that verify what was *sent*, not just what was returned.
//!
//! - `QueueMock` — a simple pre-programmed response queue.  Used for tests
//!   that only care about what the client *returns* given a canned server
//!   response.
//!
//! SOUL uses direct JSON results (no MCP content-block envelope).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lightarchitects_core::jsonrpc::{JsonRpcError, JsonRpcResponse};
use lightarchitects_core::transport::Transport;
use lightarchitects_core::{JsonRpcRequest, RetryConfig, SdkError};
use lightarchitects_soul::{IngestSource, SoulClient};

// ── RetryConfig for tests ─────────────────────────────────────────────────────

/// One attempt, no delay — keeps tests fast and deterministic.
fn test_retry() -> RetryConfig {
    RetryConfig {
        max_attempts: 1,
        base_delay: Duration::ZERO,
        jitter: 0.0,
    }
}

// ── RecordingMock ─────────────────────────────────────────────────────────────
//
// Captures every outgoing `JsonRpcRequest` AND returns pre-queued responses.
// Used when a test needs to inspect what was sent (action name, tool name, …).

#[derive(Clone, Default)]
struct RecordingMock {
    /// Outgoing requests captured in send order.
    captured: Arc<Mutex<Vec<JsonRpcRequest>>>,
    /// Queued responses popped in FIFO order.
    responses: Arc<Mutex<VecDeque<JsonRpcResponse>>>,
}

impl RecordingMock {
    /// Push a successful result payload onto the queue.
    fn push(&self, payload: serde_json::Value) {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: Some(1),
            result: Some(payload),
            error: None,
        };
        self.responses
            .lock()
            .expect("test fixture: recording mock lock")
            .push_back(resp);
    }

    /// Return all requests captured so far.
    fn captured_requests(&self) -> Vec<JsonRpcRequest> {
        self.captured
            .lock()
            .expect("test fixture: captured lock")
            .clone()
    }
}

impl Transport for RecordingMock {
    async fn send(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, SdkError> {
        // Record the request before popping a response so the capture is always
        // consistent regardless of whether the queue is empty.
        self.captured
            .lock()
            .expect("test fixture: captured lock")
            .push(request.clone());

        let response = self
            .responses
            .lock()
            .expect("test fixture: recording mock lock")
            .pop_front()
            .unwrap_or_else(|| {
                // Return a null result when the queue is exhausted; this is the
                // same behaviour as `MockTransport::null()` in core.
                JsonRpcResponse {
                    jsonrpc: "2.0".to_owned(),
                    id: Some(request.id),
                    result: Some(serde_json::Value::Null),
                    error: None,
                }
            });

        // Echo the request id so McpClient correlation checks pass.
        let mut resp = response;
        resp.id = Some(request.id);
        Ok(resp)
    }
}

// ── QueueMock ─────────────────────────────────────────────────────────────────
//
// Pre-programmed response queue without request capture.  Simpler than
// `RecordingMock` for tests that only care about the decoded return value.

#[derive(Clone, Default)]
struct QueueMock {
    responses: Arc<Mutex<VecDeque<Result<JsonRpcResponse, SdkError>>>>,
}

impl QueueMock {
    fn push(&self, payload: serde_json::Value) {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: Some(1),
            result: Some(payload),
            error: None,
        };
        self.responses
            .lock()
            .expect("test fixture: queue mock lock")
            .push_back(Ok(resp));
    }

    fn push_error(&self, code: i64, message: &str) {
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
            .expect("test fixture: queue mock lock")
            .push_back(Ok(resp));
    }
}

impl Transport for QueueMock {
    async fn send(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, SdkError> {
        let entry = self
            .responses
            .lock()
            .expect("test fixture: queue mock lock")
            .pop_front()
            .unwrap_or_else(|| {
                Err(SdkError::Config(
                    "QueueMock: response queue exhausted".to_owned(),
                ))
            });

        entry.map(|mut resp| {
            resp.id = Some(request.id);
            resp
        })
    }
}

// ── Client helpers ────────────────────────────────────────────────────────────

fn recording_client(mock: RecordingMock) -> SoulClient<RecordingMock> {
    SoulClient::from_transport(mock, test_retry())
}

fn queue_client(mock: QueueMock) -> SoulClient<QueueMock> {
    SoulClient::from_transport(mock, test_retry())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// The outgoing `tools/call` request must name `"soulTools"` as the tool and
/// include `"action": "graphrag_ingest"` in its arguments.
#[tokio::test]
async fn graphrag_ingest_call_sends_correct_action() {
    let mock = RecordingMock::default();
    mock.push(serde_json::json!({
        "source_id": "sentinel",
        "nodes_created": 0,
        "edges_created": 0,
        "errors": [],
        "dry_run": false
    }));

    recording_client(mock.clone())
        .graphrag_ingest()
        .source(IngestSource::File("/tmp/sentinel.md".into()))
        .call()
        .await
        .expect("test fixture: recording client must succeed");

    let requests = mock.captured_requests();
    assert_eq!(
        requests.len(),
        1,
        "expected exactly one outgoing request; got {}",
        requests.len()
    );

    let req = &requests[0];
    assert_eq!(
        req.method, "tools/call",
        "method must be tools/call; got: {}",
        req.method
    );

    let params = req
        .params
        .as_ref()
        .expect("test fixture: tools/call must carry params");

    assert_eq!(
        params["name"], "soulTools",
        "tool name must be soulTools; got: {}",
        params["name"]
    );

    let action = &params["arguments"]["action"];
    assert_eq!(
        action, "graphrag_ingest",
        "arguments.action must be graphrag_ingest; got: {action}"
    );
}

/// End-to-end: file source → mock returns a valid JSON response → client
/// deserialises `GraphRagIngestResult` correctly.
#[tokio::test]
async fn graphrag_ingest_file_source_roundtrip() {
    let mock = QueueMock::default();
    mock.push(serde_json::json!({
        "source_id": "arxiv-2501-00001",
        "nodes_created": 12,
        "edges_created": 8,
        "errors": [],
        "dry_run": false
    }));

    let result = queue_client(mock)
        .graphrag_ingest()
        .source(IngestSource::File(
            "/data/papers/arxiv-2501-00001.md".into(),
        ))
        .domain("research")
        .call()
        .await
        .expect("test fixture: file roundtrip must succeed");

    assert_eq!(result.source_id, "arxiv-2501-00001");
    assert_eq!(result.nodes_created, 12);
    assert_eq!(result.edges_created, 8);
    assert!(result.errors.is_empty());
    assert!(!result.dry_run);
}

/// End-to-end: inline text source → mock returns a valid JSON response →
/// client deserialises `GraphRagIngestResult` correctly.
#[tokio::test]
async fn graphrag_ingest_text_source_roundtrip() {
    let mock = QueueMock::default();
    mock.push(serde_json::json!({
        "source_id": "meeting-notes-2026-04-06",
        "nodes_created": 7,
        "edges_created": 4,
        "errors": [],
        "dry_run": false
    }));

    let result = queue_client(mock)
        .graphrag_ingest()
        .source(IngestSource::Inline {
            source_id: "meeting-notes-2026-04-06".into(),
            text: "Kevin and the squad discussed the HELIX platform architecture.".into(),
            format: None,
        })
        .sibling("corso")
        .call()
        .await
        .expect("test fixture: inline text roundtrip must succeed");

    assert_eq!(result.source_id, "meeting-notes-2026-04-06");
    assert_eq!(result.nodes_created, 7);
    assert_eq!(result.edges_created, 4);
    assert!(!result.dry_run);
}

/// `dry_run=true` in the builder → mock response carries `dry_run: true` and
/// zero counts → decoded result reflects both.
#[tokio::test]
async fn graphrag_ingest_dry_run_roundtrip() {
    let mock = QueueMock::default();
    mock.push(serde_json::json!({
        "source_id": "draft-spec",
        "nodes_created": 0,
        "edges_created": 0,
        "errors": [],
        "dry_run": true
    }));

    let result = queue_client(mock)
        .graphrag_ingest()
        .source(IngestSource::File("/tmp/draft-spec.md".into()))
        .dry_run()
        .call()
        .await
        .expect("test fixture: dry-run roundtrip must succeed");

    assert!(
        result.dry_run,
        "decoded result must reflect dry_run == true"
    );
    assert_eq!(
        result.nodes_created, 0,
        "dry-run must have zero nodes_created"
    );
    assert_eq!(
        result.edges_created, 0,
        "dry-run must have zero edges_created"
    );
}

/// A JSON-RPC error response from SOUL surfaces as `SdkError::Protocol` —
/// not a panic, not a `SdkError::Config`.
#[tokio::test]
async fn graphrag_ingest_error_response_surfaces_as_sdk_error() {
    let mock = QueueMock::default();
    mock.push_error(-32_001, "graphrag_ingest: Neo4j unavailable");

    let err = queue_client(mock)
        .graphrag_ingest()
        .source(IngestSource::File("/tmp/doc.md".into()))
        .call()
        .await
        .expect_err("test fixture: error response must yield an Err");

    assert!(
        matches!(err, SdkError::Protocol(_)),
        "JSON-RPC errors must surface as SdkError::Protocol; got: {err:?}"
    );
}

/// Regression for the C-1 fix: `nodes_created` and `edges_created` are
/// decoded into separate, distinct fields.  A response where they differ
/// (`nodes_created: 10, edges_created: 5`) must not cross-assign the values.
#[tokio::test]
async fn graphrag_ingest_nodes_and_edges_counts_are_distinct() {
    let mock = QueueMock::default();
    mock.push(serde_json::json!({
        "source_id": "regression-c1",
        "nodes_created": 10,
        "edges_created": 5,
        "errors": [],
        "dry_run": false
    }));

    let result = queue_client(mock)
        .graphrag_ingest()
        .source(IngestSource::File("/tmp/regression-c1.md".into()))
        .call()
        .await
        .expect("test fixture: regression roundtrip must succeed");

    assert_eq!(
        result.nodes_created, 10,
        "nodes_created must be 10; got {}",
        result.nodes_created
    );
    assert_eq!(
        result.edges_created, 5,
        "edges_created must be 5; got {}",
        result.edges_created
    );
    assert_ne!(
        result.nodes_created, result.edges_created,
        "nodes_created and edges_created must not be equal for this fixture"
    );
}
