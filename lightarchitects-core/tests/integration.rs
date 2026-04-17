//! Integration tests for [`McpClient`] exercising retry, tool dispatch, and
//! clone semantics through a [`MockTransport`] — no real MCP binary required.
//!
//! [`MockTransport`] implements [`Transport`] by draining a pre-loaded queue of
//! canned responses, recording every request id for post-test inspection. All
//! retry delays are set to zero so tests complete instantly.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lightarchitects_core::error::{ProtocolError, SdkError, TransportError};
use lightarchitects_core::jsonrpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use lightarchitects_core::transport::Transport;
use lightarchitects_core::{McpClient, RetryConfig};

// ── MockTransport ─────────────────────────────────────────────────────────────

/// In-process [`Transport`] double that returns pre-canned responses.
///
/// Cheap to clone — the response queue and request log are `Arc`-shared,
/// so clones of a `MockTransport` observe each other's calls and share state.
#[derive(Clone, Default)]
struct MockTransport {
    /// Pre-programmed responses returned in FIFO order.
    responses: Arc<Mutex<VecDeque<Result<JsonRpcResponse, SdkError>>>>,
    /// `id` field from every [`JsonRpcRequest`] received, in order.
    received_ids: Arc<Mutex<Vec<u64>>>,
}

impl MockTransport {
    fn responses_guard(
        &self,
    ) -> std::sync::MutexGuard<'_, VecDeque<Result<JsonRpcResponse, SdkError>>> {
        match self.responses.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn received_ids_guard(&self) -> std::sync::MutexGuard<'_, Vec<u64>> {
        match self.received_ids.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Enqueue a successful result response.
    fn push_ok(&self, result: serde_json::Value) {
        self.responses_guard()
            .push_back(Ok(make_ok_response(result)));
    }

    /// Enqueue a JSON-RPC error response (transport succeeds, tool fails).
    fn push_rpc_error(&self, code: i64, message: &str) {
        self.responses_guard()
            .push_back(Ok(make_rpc_error_response(code, message)));
    }

    /// Enqueue a retryable [`TransportError::Io`] error.
    fn push_io_error(&self) {
        let err = TransportError::Io(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
        self.responses_guard()
            .push_back(Err(SdkError::Transport(err)));
    }

    /// Enqueue a non-retryable [`TransportError::ProcessSpawn`] error.
    fn push_spawn_error(&self) {
        let err = TransportError::ProcessSpawn {
            binary: "mock".to_owned(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        };
        self.responses_guard()
            .push_back(Err(SdkError::Transport(err)));
    }

    /// How many times `send` was called across all clones of this transport.
    fn call_count(&self) -> usize {
        self.received_ids_guard().len()
    }

    /// All request ids received, in the order they arrived.
    fn received_ids(&self) -> Vec<u64> {
        self.received_ids_guard().clone()
    }
}

impl Transport for MockTransport {
    async fn send(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, SdkError> {
        // Record id — both locks are released before any suspension point.
        self.received_ids_guard().push(request.id);

        self.responses_guard().pop_front().unwrap_or_else(|| {
            Err(SdkError::Config(
                "MockTransport: response queue exhausted".to_owned(),
            ))
        })
    }
}

// ── Response constructors ─────────────────────────────────────────────────────

fn make_ok_response(result: serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_owned(),
        id: Some(0),
        result: Some(result),
        error: None,
    }
}

fn make_rpc_error_response(code: i64, message: &str) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_owned(),
        id: Some(0),
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_owned(),
        }),
    }
}

// ── Retry config helpers ──────────────────────────────────────────────────────

/// Single-attempt config — no retries, no delay.
fn no_retry() -> RetryConfig {
    RetryConfig {
        max_attempts: 1,
        base_delay: Duration::from_millis(0),
        jitter: 0.0,
    }
}

/// Multi-attempt config with zero delay between retries.
fn fast_retry(max_attempts: u32) -> RetryConfig {
    RetryConfig {
        max_attempts,
        base_delay: Duration::from_millis(0),
        jitter: 0.0,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Happy path: tool call returns a successful result.
#[tokio::test]
async fn call_tool_success() {
    let mock = MockTransport::default();
    mock.push_ok(serde_json::json!({"status": "ok"}));
    let client = McpClient::new(mock.clone(), no_retry());

    let result = client
        .call_tool("soulTools", serde_json::json!({"action": "helix"}))
        .await;

    assert_eq!(
        result.as_ref().ok(),
        Some(&serde_json::json!({"status": "ok"}))
    );
    assert_eq!(mock.call_count(), 1);
}

/// A JSON-RPC error object in the response propagates as [`ProtocolError::RpcError`]
/// and does **not** trigger a retry — the transport layer succeeded.
#[tokio::test]
async fn rpc_error_propagates_without_retry() {
    let mock = MockTransport::default();
    mock.push_rpc_error(-32_600, "invalid request");
    let client = McpClient::new(mock.clone(), fast_retry(3));

    let result = client.call_tool("soulTools", serde_json::json!({})).await;

    assert!(result.is_err(), "expected Err");
    assert_eq!(
        mock.call_count(),
        1,
        "rpc error must not trigger retry (only 1 transport call)"
    );
    assert!(
        matches!(
            result,
            Err(SdkError::Protocol(ProtocolError::RpcError {
                code: -32_600,
                ..
            }))
        ),
        "unexpected error variant: {result:?}"
    );
}

/// A non-retryable transport error (e.g. `ProcessSpawn`) breaks the retry loop
/// immediately — only one attempt is made.
#[tokio::test]
async fn non_retryable_transport_error_does_not_retry() {
    let mock = MockTransport::default();
    mock.push_spawn_error();
    let client = McpClient::new(mock.clone(), fast_retry(3));

    let result = client.call_tool("soulTools", serde_json::json!({})).await;

    assert!(result.is_err());
    assert_eq!(
        mock.call_count(),
        1,
        "non-retryable error must break the loop immediately"
    );
    assert!(matches!(
        result,
        Err(SdkError::Transport(TransportError::ProcessSpawn { .. }))
    ));
}

/// A retryable [`TransportError::Io`] on the first attempt triggers a retry;
/// the second attempt succeeds.
#[tokio::test]
async fn io_error_triggers_retry_then_succeeds() {
    let mock = MockTransport::default();
    mock.push_io_error();
    mock.push_ok(serde_json::json!("recovered"));
    let client = McpClient::new(mock.clone(), fast_retry(2));

    let result = client.call_tool("soulTools", serde_json::json!({})).await;

    assert!(
        result.is_ok(),
        "expected success after retry, got {result:?}"
    );
    assert_eq!(
        mock.call_count(),
        2,
        "exactly one retry should have occurred"
    );
}

/// When all retry attempts are exhausted the last error is returned.
#[tokio::test]
async fn retry_exhaustion_returns_last_error() {
    let mock = MockTransport::default();
    mock.push_io_error();
    mock.push_io_error();
    mock.push_io_error();
    let client = McpClient::new(mock.clone(), fast_retry(3));

    let result = client.call_tool("soulTools", serde_json::json!({})).await;

    assert!(result.is_err());
    assert_eq!(mock.call_count(), 3, "all three attempts must be exhausted");
    assert!(
        matches!(result, Err(SdkError::Transport(TransportError::Io(_)))),
        "last io error must propagate"
    );
}

/// `list_tools` deserializes the `tools/list` response payload into a typed
/// [`Vec<ToolInfo>`].
#[tokio::test]
async fn list_tools_deserializes_correctly() {
    let mock = MockTransport::default();
    mock.push_ok(serde_json::json!({
        "tools": [
            {"name": "soulTools",  "inputSchema": {"type": "object"}},
            {"name": "corsoTools", "inputSchema": {"type": "object"}},
        ]
    }));
    let client = McpClient::new(mock, no_retry());

    let tools_result = client.list_tools().await;

    assert!(tools_result.is_ok(), "list_tools failed: {tools_result:?}");
    let tools = tools_result.ok().unwrap_or_default();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name, "soulTools");
    assert_eq!(tools[1].name, "corsoTools");
}

/// Clones of an [`McpClient`] share the same monotonic id counter via
/// `Arc<AtomicU64>`, so request ids are globally unique across all clones.
#[tokio::test]
async fn clone_shares_id_counter() {
    let mock = MockTransport::default();
    mock.push_ok(serde_json::json!(null));
    mock.push_ok(serde_json::json!(null));
    mock.push_ok(serde_json::json!(null));

    let client_a = McpClient::new(mock.clone(), no_retry());
    let client_b = client_a.clone(); // shares Arc<AtomicU64> with client_a

    let primary_call_result = client_a.call_tool("soulTools", serde_json::json!({})).await;
    let clone_call_result = client_b.call_tool("soulTools", serde_json::json!({})).await;
    let followup_call_result = client_a.call_tool("soulTools", serde_json::json!({})).await;

    assert!(
        primary_call_result.is_ok(),
        "a — call 1 failed: {primary_call_result:?}"
    );
    assert!(
        clone_call_result.is_ok(),
        "b — call 1 failed: {clone_call_result:?}"
    );
    assert!(
        followup_call_result.is_ok(),
        "a — call 2 failed: {followup_call_result:?}"
    );

    let ids = mock.received_ids();
    assert_eq!(ids.len(), 3, "all three calls must reach the mock");
    // Strictly ascending ids prove the counter was never reset between clones.
    assert!(
        ids[0] < ids[1] && ids[1] < ids[2],
        "ids must be strictly ascending (shared counter): {ids:?}"
    );
}
