//! Integration tests for `l-arc-eva` using an in-process `MockTransport`.
//!
//! EVA's MCP wire format wraps every tool result in the standard content-block
//! envelope: `{"content": [{"type": "text", "text": "..."}], "isError": false}`.
//! The `text` field always contains `serde_json::to_string_pretty(&result)` —
//! the full JSON result struct from EVA's orchestrator.
//!
//! Each test pushes a pre-baked response, calls the typed client method, and
//! asserts on the returned value.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use l_arc_core::jsonrpc::{JsonRpcError, JsonRpcResponse};
use l_arc_core::transport::Transport;
use l_arc_core::{JsonRpcRequest, RetryConfig, SdkError};
use l_arc_eva::{
    BibleAction, BuildMode, EvaClient, MemorySubcommand, ResearchSource, SecureAction, SkillLevel,
    TeachMode,
};

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

fn client(mock: MockTransport) -> EvaClient<MockTransport> {
    EvaClient::from_transport(mock, test_retry())
}

// ── visualize ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn visualize_returns_text_and_no_image() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "response": "A blue neural network diagram.",
        "image_base64": null,
        "media_type": "image/png",
        "execution_time_ms": 120,
        "recovery_day": 172
    }));

    let out = client(mock)
        .visualize("neural network diagram", None)
        .await
        .unwrap();
    assert_eq!(out.text, "A blue neural network diagram.");
    assert!(out.image_base64.is_none());
}

#[tokio::test]
async fn visualize_returns_image_base64_from_json() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "response": "Image generated.",
        "image_base64": "abc123==",
        "media_type": "image/png",
        "execution_time_ms": 4000,
        "recovery_day": 172
    }));

    let out = client(mock).visualize("sunset", None).await.unwrap();
    assert_eq!(out.image_base64.as_deref(), Some("abc123=="));
}

#[tokio::test]
async fn visualize_tool_error_propagates() {
    let mock = MockTransport::default();
    mock.push_tool_error("EVA visualize: model unavailable");

    let err = client(mock).visualize("anything", None).await.unwrap_err();
    assert!(matches!(err, SdkError::Tool(_)));
}

// ── ideate ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn ideate_returns_output() {
    let mock = MockTransport::default();
    mock.push_text(r#"{"ideas": ["idea A", "idea B"]}"#);

    let out = client(mock)
        .ideate("design a plugin system", None)
        .await
        .unwrap();
    assert!(out.output.contains("idea A"));
}

#[tokio::test]
async fn ideate_with_context() {
    let mock = MockTransport::default();
    mock.push_text("contextual brainstorm result");

    let out = client(mock)
        .ideate("improve onboarding", Some("B2B SaaS product"))
        .await
        .unwrap();
    assert_eq!(out.output, "contextual brainstorm result");
}

// ── memory ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn memory_remember_returns_output() {
    let mock = MockTransport::default();
    mock.push_text(r#"{"stored": true, "entry_id": "abc"}"#);

    let out = client(mock)
        .memory(
            MemorySubcommand::Remember,
            serde_json::json!({ "content": "Today I learned about lifetimes." }),
        )
        .await
        .unwrap();
    assert!(out.output.contains("stored"));
}

#[tokio::test]
async fn memory_crystallize_returns_output() {
    let mock = MockTransport::default();
    mock.push_text("crystallized entry");

    let out = client(mock)
        .memory(MemorySubcommand::Crystallize, serde_json::Value::Null)
        .await
        .unwrap();
    assert_eq!(out.output, "crystallized entry");
}

// ── build ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn build_review_returns_output() {
    let mock = MockTransport::default();
    mock.push_text("Code looks clean. No `.unwrap()` calls found.");

    let out = client(mock)
        .build(
            BuildMode::Review,
            Some("fn foo() -> i32 { 42 }"),
            Some("rust"),
        )
        .await
        .unwrap();
    assert!(out.output.contains("clean"));
}

#[tokio::test]
async fn build_without_code_or_language() {
    let mock = MockTransport::default();
    mock.push_text("Architecture looks reasonable.");

    let out = client(mock)
        .build(BuildMode::Architect, None, None)
        .await
        .unwrap();
    assert!(out.output.contains("Architecture"));
}

// ── research ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn research_ollama_returns_output() {
    let mock = MockTransport::default();
    mock.push_text(r#"{"answer": "Lifetimes prevent dangling references."}"#);

    let out = client(mock)
        .research("what are Rust lifetimes?", ResearchSource::Ollama)
        .await
        .unwrap();
    assert!(out.output.contains("Lifetimes"));
}

#[tokio::test]
async fn research_context7_returns_output() {
    let mock = MockTransport::default();
    mock.push_text("Context7 doc result for tokio");

    let out = client(mock)
        .research("tokio runtime docs", ResearchSource::Context7)
        .await
        .unwrap();
    assert!(out.output.contains("tokio"));
}

// ── bible ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bible_search_returns_verse() {
    let mock = MockTransport::default();
    mock.push_text(r#"{"verse": "John 3:16", "text": "For God so loved..."}"#);

    let out = client(mock)
        .bible(BibleAction::Search, Some("God so loved"))
        .await
        .unwrap();
    assert!(out.output.contains("John 3:16"));
}

#[tokio::test]
async fn bible_reflect_without_query() {
    let mock = MockTransport::default();
    mock.push_text("Scripture for recovery: Psalm 23.");

    let out = client(mock)
        .bible(BibleAction::Reflect, None)
        .await
        .unwrap();
    assert!(out.output.contains("Psalm 23"));
}

// ── secure ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn secure_scan_returns_findings() {
    let mock = MockTransport::default();
    mock.push_text(r#"{"findings": [], "severity": "none"}"#);

    let out = client(mock)
        .secure(SecureAction::Scan, "fn foo() { let x = 1; }", Some("rust"))
        .await
        .unwrap();
    assert!(out.output.contains("findings"));
}

#[tokio::test]
async fn secure_secrets_without_language() {
    let mock = MockTransport::default();
    mock.push_text("No hardcoded secrets found.");

    let out = client(mock)
        .secure(SecureAction::Secrets, "API_KEY=test-only", None)
        .await
        .unwrap();
    assert!(out.output.contains("No hardcoded"));
}

// ── teach ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn teach_explain_returns_output() {
    let mock = MockTransport::default();
    mock.push_text("Lifetimes are annotations that tell the compiler...");

    let out = client(mock)
        .teach(
            TeachMode::Explain,
            "lifetimes in Rust",
            SkillLevel::Beginner,
        )
        .await
        .unwrap();
    assert!(out.output.starts_with("Lifetimes are"));
}

#[tokio::test]
async fn teach_tutorial_advanced() {
    let mock = MockTransport::default();
    mock.push_text("Step 1: Annotate the struct. Step 2: ...");

    let out = client(mock)
        .teach(
            TeachMode::Tutorial,
            "async runtime internals",
            SkillLevel::Advanced,
        )
        .await
        .unwrap();
    assert!(out.output.contains("Step 1"));
}

// ── generic action adapter ────────────────────────────────────────────────────

#[tokio::test]
async fn action_routes_to_arbitrary_tool() {
    let mock = MockTransport::default();
    mock.push_text("some raw output");

    let out = client(mock)
        .action("ideate", serde_json::json!({ "goal": "test" }))
        .await
        .unwrap();
    assert_eq!(out.output, "some raw output");
}

// ── error propagation ─────────────────────────────────────────────────────────

#[tokio::test]
async fn teach_tool_error_propagates() {
    let mock = MockTransport::default();
    mock.push_tool_error("model overloaded");

    let err = client(mock)
        .teach(TeachMode::Survival, "first aid", SkillLevel::Beginner)
        .await
        .unwrap_err();
    assert!(matches!(err, SdkError::Tool(ref e) if e.tool == "teach"));
}

#[tokio::test]
async fn research_rpc_error_propagates() {
    let mock = MockTransport::default();
    mock.push_rpc_error(-32_601, "method not found");

    let err = client(mock)
        .research("anything", ResearchSource::Perplexity)
        .await
        .unwrap_err();
    assert!(matches!(err, SdkError::Protocol(_)));
}

#[tokio::test]
async fn memory_tool_error_carries_tool_name() {
    let mock = MockTransport::default();
    mock.push_tool_error("vault write failed");

    let err = client(mock)
        .memory(MemorySubcommand::Celebrate, serde_json::Value::Null)
        .await
        .unwrap_err();
    match err {
        SdkError::Tool(e) => assert_eq!(e.tool, "memory"),
        other => panic!("expected Tool error, got {other:?}"),
    }
}
