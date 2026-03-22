//! Integration tests for `l-arc-ayin`.
//!
//! Tests the zero-cost noop path (feature `observe` disabled, which is the
//! default in the test profile). The noop `ObservableTransport<T>` is a
//! transparent newtype — every `send()` delegates directly to the inner
//! transport with no additional overhead.
//!
//! The `observe`-feature path is tested separately via unit tests in
//! `l-arc-ayin/src/lib.rs` (not possible to integration-test here without the
//! AYIN binary running, which is an E2E concern).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use l_arc_ayin::ObservableTransport;
use l_arc_core::jsonrpc::{JsonRpcError, JsonRpcResponse};
use l_arc_core::transport::Transport;
use l_arc_core::{JsonRpcRequest, SdkError};

// ── MockTransport ─────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct MockTransport {
    responses: Arc<Mutex<VecDeque<Result<JsonRpcResponse, SdkError>>>>,
}

impl MockTransport {
    fn push_ok(&self, payload: serde_json::Value) {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: Some(1),
            result: Some(payload),
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

fn dummy_request() -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "tools/list".to_owned(),
        params: None,
    }
}

// ── ObservableTransport noop path ─────────────────────────────────────────────

#[tokio::test]
async fn noop_send_delegates_to_inner() {
    let mock = MockTransport::default();
    mock.push_ok(serde_json::json!({ "result": "ok" }));

    let observable = ObservableTransport::new(mock);
    let resp = observable.send(dummy_request()).await.unwrap();
    assert!(resp.result.is_some());
}

#[tokio::test]
async fn noop_propagates_inner_rpc_error() {
    let mock = MockTransport::default();
    mock.push_rpc_error(-32_600, "invalid request");

    let observable = ObservableTransport::new(mock);
    let resp = observable.send(dummy_request()).await.unwrap();
    // The response reaches us (no transport error) — the RPC error is in the body.
    assert!(resp.error.is_some());
    let err = resp.error.unwrap();
    assert_eq!(err.code, -32_600);
}

#[tokio::test]
async fn noop_with_actor_ignores_actor_name() {
    let mock = MockTransport::default();
    mock.push_ok(serde_json::json!({ "tools": [] }));

    // `with_actor` is a no-op when `observe` feature is disabled.
    let observable = ObservableTransport::with_actor(mock, "test-actor");
    let resp = observable.send(dummy_request()).await.unwrap();
    assert!(resp.result.is_some());
}

#[tokio::test]
async fn noop_exhausted_queue_propagates_config_error() {
    let mock = MockTransport::default(); // No responses queued.
    let observable = ObservableTransport::new(mock);
    let err = observable.send(dummy_request()).await.unwrap_err();
    assert!(
        matches!(err, SdkError::Config(_)),
        "expected Config error, got: {err:?}"
    );
}

#[tokio::test]
async fn noop_multiple_sequential_sends() {
    let mock = MockTransport::default();
    mock.push_ok(serde_json::json!({ "seq": 1 }));
    mock.push_ok(serde_json::json!({ "seq": 2 }));

    let observable = ObservableTransport::new(mock);

    let r1 = observable.send(dummy_request()).await.unwrap();
    let r2 = observable.send(dummy_request()).await.unwrap();

    assert_eq!(r1.result.unwrap()["seq"], 1);
    assert_eq!(r2.result.unwrap()["seq"], 2);
}

#[tokio::test]
async fn noop_is_usable_as_transport_impl() {
    // Verify ObservableTransport<MockTransport> satisfies the Transport bound.
    // This is a compile-time check: if it compiles, the impl is correct.
    fn assert_transport<T: Transport>(_: T) {}
    assert_transport(ObservableTransport::new(MockTransport::default()));
}
