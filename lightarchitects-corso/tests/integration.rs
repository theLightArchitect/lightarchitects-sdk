//! Integration tests for `lightarchitects-corso` using an in-process `MockTransport`.
//!
//! CORSO wraps every response in an MCP content-block envelope:
//! `{"content": [{"type": "text", "text": "..."}], "isError": false}`.
//! Each test injects that envelope with either a JSON payload (structured
//! actions) or a plain-text payload (analysis actions).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lightarchitects_core::jsonrpc::{JsonRpcError, JsonRpcResponse};
use lightarchitects_core::transport::Transport;
use lightarchitects_core::{JsonRpcRequest, RetryConfig, SdkError};
use lightarchitects_corso::CorsoClient;

// ── MockTransport ─────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct MockTransport {
    responses: Arc<Mutex<VecDeque<Result<JsonRpcResponse, SdkError>>>>,
}

impl MockTransport {
    /// Push a successful MCP content-block response containing JSON payload.
    fn push_json(&self, payload: serde_json::Value) {
        let text = serde_json::to_string(&payload).expect("serialize payload");
        self.push_content_block(&text, false);
    }

    /// Push a successful MCP content-block response containing plain text.
    fn push_text(&self, text: &str) {
        self.push_content_block(text, false);
    }

    /// Push an `isError: true` content-block response.
    fn push_tool_error(&self, message: &str) {
        self.push_content_block(message, true);
    }

    /// Push a JSON-RPC protocol error (not a tool error).
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

    fn push_content_block(&self, text: &str, is_error: bool) {
        let result = serde_json::json!({
            "content": [{ "type": "text", "text": text }],
            "isError": is_error
        });
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: Some(1),
            result: Some(result),
            error: None,
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

fn test_retry() -> RetryConfig {
    RetryConfig {
        max_attempts: 1,
        base_delay: Duration::ZERO,
        jitter: 0.0,
    }
}

fn client(mock: MockTransport) -> CorsoClient<MockTransport> {
    CorsoClient::from_transport(mock, test_retry())
}

// ── Filesystem actions ─────────────────────────────────────────────────────────

#[tokio::test]
async fn read_file_decodes_content_and_path() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "operation": "read",
        "path": "/src/lib.rs",
        "content": "fn main() {}",
        "success": true
    }));

    let file = client(mock).read_file("/src/lib.rs", None).await.unwrap();
    assert_eq!(file.path, "/src/lib.rs");
    assert_eq!(file.content, "fn main() {}");
    assert!(file.success);
}

#[tokio::test]
async fn read_file_with_encoding() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "operation": "read",
        "path": "/data/file.bin",
        "content": "base64-encoded-data",
        "success": true
    }));

    let file = client(mock)
        .read_file("/data/file.bin", Some("binary"))
        .await
        .unwrap();
    assert_eq!(file.content, "base64-encoded-data");
}

#[tokio::test]
async fn write_file_decodes_bytes_written() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "operation": "write",
        "path": "/src/new.rs",
        "bytes_written": 128,
        "success": true
    }));

    let result = client(mock)
        .write_file("/src/new.rs", "fn hello() {}")
        .await
        .unwrap();
    assert_eq!(result.bytes_written, 128);
    assert!(result.success);
}

#[tokio::test]
async fn list_directory_decodes_entries() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "entries": [
            { "name": "lib.rs", "path": "/src/lib.rs", "type": "file", "size": 1024 },
            { "name": "tests", "path": "/src/tests", "type": "directory" }
        ]
    }));

    let entries = client(mock).list_directory("/src", false).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "lib.rs");
    assert_eq!(entries[0].entry_type, "file");
    assert_eq!(entries[0].size, Some(1024));
    assert_eq!(entries[1].entry_type, "directory");
    assert!(entries[1].size.is_none());
}

#[tokio::test]
async fn list_directory_empty() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({ "entries": [] }));

    let entries = client(mock).list_directory("/empty", true).await.unwrap();
    assert!(entries.is_empty());
}

// ── Code intelligence actions ──────────────────────────────────────────────────

#[tokio::test]
async fn search_code_returns_hits() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!([
        { "file": "/src/lib.rs", "line": 42, "content": "fn call_tool(" },
        { "file": "/src/client.rs", "line": 17, "content": "fn call_tool(tool: &str)" }
    ]));

    let hits = client(mock)
        .search_code("fn call_tool", None)
        .await
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].file, "/src/lib.rs");
    assert_eq!(hits[0].line, 42);
}

#[tokio::test]
async fn search_code_empty_result() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!([]));

    let hits = client(mock)
        .search_code("nonexistent_fn", Some("/src"))
        .await
        .unwrap();
    assert!(hits.is_empty());
}

#[tokio::test]
async fn find_symbol_decodes_result() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "query": "McpClient",
        "results": [
            { "file": "/src/client.rs", "line": 27, "kind": "struct" }
        ],
        "total": 1
    }));

    let result = client(mock).find_symbol("McpClient").await.unwrap();
    assert_eq!(result.query, "McpClient");
    assert_eq!(result.total, 1);
    assert_eq!(result.results.len(), 1);
}

#[tokio::test]
async fn get_outline_decodes_entries() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "file": "/src/client.rs",
        "entries": [
            { "name": "McpClient", "kind": "struct", "line": 27 },
            { "name": "send_raw", "kind": "fn", "line": 62 },
            { "name": "call_tool", "kind": "fn", "line": 102 }
        ],
        "total": 3
    }));

    let outline = client(mock).get_outline("/src/client.rs").await.unwrap();
    assert_eq!(outline.file, "/src/client.rs");
    assert_eq!(outline.total, 3);
    assert_eq!(outline.entries.len(), 3);
}

#[tokio::test]
async fn get_references_decodes_results() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "query": "Transport",
        "results": [
            { "file": "/src/transport.rs", "line": 32 },
            { "file": "/src/client.rs", "line": 27 }
        ],
        "total": 2
    }));

    let refs = client(mock).get_references("Transport").await.unwrap();
    assert_eq!(refs.query, "Transport");
    assert_eq!(refs.total, 2);
}

// ── AI analysis actions ────────────────────────────────────────────────────────

#[tokio::test]
async fn sniff_returns_analysis_text() {
    let mock = MockTransport::default();
    mock.push_text("Code analysis: cyclomatic complexity 3, well structured.");

    let out = client(mock).sniff("/src/client.rs").await.unwrap();
    assert!(out.output.contains("cyclomatic complexity"));
}

#[tokio::test]
async fn guard_returns_security_report() {
    let mock = MockTransport::default();
    mock.push_text("Security scan: 0 HIGH, 0 CRITICAL. No issues found.");

    let out = client(mock).guard("/src").await.unwrap();
    assert!(out.output.contains("0 HIGH"));
}

#[tokio::test]
async fn fetch_returns_research() {
    let mock = MockTransport::default();
    mock.push_text("Research findings: the tower of babel was the first compile error.");

    let out = client(mock)
        .fetch("Rust error handling patterns")
        .await
        .unwrap();
    assert!(!out.output.is_empty());
}

#[tokio::test]
async fn chase_returns_performance_analysis() {
    let mock = MockTransport::default();
    mock.push_text("Performance: p99 latency 2.1ms, throughput 4200 req/s.");

    let out = client(mock).chase("/benches").await.unwrap();
    assert!(out.output.contains("p99"));
}

#[tokio::test]
async fn code_review_with_context() {
    let mock = MockTransport::default();
    mock.push_text("Review: excellent use of Result propagation, no unwrap() calls.");

    let out = client(mock)
        .code_review("/src/client.rs", Some("focus on error handling"))
        .await
        .unwrap();
    assert!(out.output.contains("Result"));
}

#[tokio::test]
async fn generate_code_returns_output() {
    let mock = MockTransport::default();
    mock.push_text("```rust\npub fn add(a: u32, b: u32) -> u32 { a + b }\n```");

    let out = client(mock)
        .generate_code("implement an add function")
        .await
        .unwrap();
    assert!(out.output.contains("fn add"));
}

// ── Operational actions ────────────────────────────────────────────────────────

#[tokio::test]
async fn deploy_returns_status() {
    let mock = MockTransport::default();
    mock.push_text("Deploy succeeded: corso v2.0.0 → ~/.corso/bin/corso");

    let out = client(mock).deploy("corso").await.unwrap();
    assert!(out.output.contains("Deploy succeeded"));
}

#[tokio::test]
async fn manage_logs_with_line_count() {
    let mock = MockTransport::default();
    mock.push_text("[2026-03-21 12:00:00] INFO  server started");

    let out = client(mock).manage_logs("soul", Some(100)).await.unwrap();
    assert!(out.output.contains("INFO"));
}

#[tokio::test]
async fn scale_resources_returns_confirmation() {
    let mock = MockTransport::default();
    mock.push_text("Scaled soul to 3 replicas.");

    let out = client(mock).scale_resources("soul", 3).await.unwrap();
    assert!(out.output.contains("3 replicas"));
}

// ── Error propagation ─────────────────────────────────────────────────────────

#[tokio::test]
async fn tool_error_surfaces_as_sdk_tool_error() {
    let mock = MockTransport::default();
    // isError: true in the content block → SdkError::Tool
    mock.push_tool_error("file not found: /missing.rs");

    let err = client(mock)
        .read_file("/missing.rs", None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, SdkError::Tool(_)),
        "expected Tool error, got: {err:?}"
    );
}

#[tokio::test]
async fn rpc_error_surfaces_as_protocol_error() {
    let mock = MockTransport::default();
    mock.push_rpc_error(-32_001, "internal server error");

    let err = client(mock)
        .read_file("/src/lib.rs", None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, SdkError::Protocol(_)),
        "expected Protocol error, got: {err:?}"
    );
}

#[tokio::test]
async fn exhausted_queue_returns_config_error() {
    let mock = MockTransport::default();
    let err = client(mock).guard("/src").await.unwrap_err();
    assert!(
        matches!(err, SdkError::Config(_)),
        "expected Config error, got: {err:?}"
    );
}
