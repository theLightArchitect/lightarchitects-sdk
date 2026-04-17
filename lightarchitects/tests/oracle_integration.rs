//! Oracle client integration tests — exercise HTTP dispatch via mockito.
//!
//! These tests verify SSE streaming parsing, JSON fallback, error capture,
//! parallel dispatch, and empty-model-set validation. All model endpoints are
//! redirected to an in-process mockito server; no live AI services are needed.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use lightarchitects::oracle::{Consensus, FindingStatus, ModelId, OracleClient, OracleMode};
use std::time::Duration;

// ── Helper ────────────────────────────────────────────────────────────────────

/// Build a client pointing all Ollama models at `endpoint`.
fn client(endpoint: &str) -> OracleClient {
    OracleClient::builder()
        .ollama_endpoint(endpoint)
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build client")
}

// ── JSON response format ──────────────────────────────────────────────────────

/// Oracle returns `FindingStatus::Ok` and captures content from a standard
/// OpenAI-compatible non-streaming JSON response body.
#[tokio::test]
async fn json_fallback_parses_content_correctly() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/v1/chat/completions")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
                "choices":[{"message":{"content":"The bound holds. QED."}}],
                "usage":{"prompt_tokens":15,"completion_tokens":8}
            }"#,
        )
        .create_async()
        .await;

    let verdict = client(&server.url())
        .query("test claim")
        .models(vec![ModelId::Deepseek])
        .call()
        .await
        .expect("call oracle");

    assert_eq!(verdict.models_ok, 1);
    assert_eq!(verdict.models_total, 1);
    assert_eq!(verdict.findings[0].status, FindingStatus::Ok);
    assert!(verdict.findings[0].content.contains("QED"));
}

// ── SSE streaming format ──────────────────────────────────────────────────────

/// Oracle correctly assembles streaming delta chunks into full content.
#[tokio::test]
async fn sse_streaming_assembles_delta_chunks() {
    let sse_body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"The \"}}]}\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"bound \"}}]}\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"holds.\"}}]}\n",
        "data: [DONE]\n",
    );

    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/v1/chat/completions")
        .with_status(200)
        .with_body(sse_body)
        .create_async()
        .await;

    let verdict = client(&server.url())
        .query("streaming test")
        .models(vec![ModelId::Qwen])
        .call()
        .await
        .expect("call oracle");

    assert_eq!(verdict.findings[0].status, FindingStatus::Ok);
    assert_eq!(verdict.findings[0].content, "The bound holds.");
}

// ── Error capture ─────────────────────────────────────────────────────────────

/// Connection refused is captured as `FindingStatus::Error` rather than bubbling
/// as a top-level error — the verdict itself still returns `Ok`.
///
/// Note: HTTP 5xx responses do NOT produce `FindingStatus::Error` because
/// [`OracleClient`] never calls `.error_for_status()`. Only transport-level
/// failures (connection refused, timeout) create error findings.
#[tokio::test]
async fn model_error_captured_in_finding_not_top_level() {
    // Port 1 is always closed → connection refused → reqwest::Error → FindingStatus::Error
    let verdict = OracleClient::builder()
        .ollama_endpoint("http://127.0.0.1:1")
        .timeout(Duration::from_secs(3))
        .build()
        .expect("build client")
        .query("error capture test")
        .models(vec![ModelId::Deepseek])
        .call()
        .await
        .expect("verdict must succeed even when model connection fails");

    assert_eq!(
        verdict.models_ok, 0,
        "connection refused must not count as ok"
    );
    assert!(
        matches!(verdict.findings[0].status, FindingStatus::Error(_)),
        "transport failure must produce FindingStatus::Error"
    );
    assert_eq!(verdict.consensus, Consensus::Insufficient);
}

// ── Parallel dispatch ─────────────────────────────────────────────────────────

/// Two models dispatched in parallel — both findings captured, consensus computed.
#[tokio::test]
async fn parallel_dispatch_collects_all_findings() {
    let body = r#"{"choices":[{"message":{"content":"verified"}}]}"#;

    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/v1/chat/completions")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .expect(2) // must be hit exactly twice (once per model)
        .create_async()
        .await;

    let verdict = client(&server.url())
        .query("parallel test")
        .models(vec![ModelId::Deepseek, ModelId::Qwen])
        .call()
        .await
        .expect("call oracle");

    assert_eq!(verdict.models_total, 2);
    assert_eq!(verdict.models_ok, 2);
    assert_eq!(verdict.findings.len(), 2);
}

// ── Consensus logic ───────────────────────────────────────────────────────────

/// When both models respond with positive signals, consensus is Unanimous.
#[tokio::test]
async fn unanimous_consensus_when_all_models_agree() {
    let body = r#"{"choices":[{"message":{"content":"This is proven and verified. QED."}}]}"#;

    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/v1/chat/completions")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .expect(2)
        .create_async()
        .await;

    let verdict = client(&server.url())
        .query("consensus test")
        .models(vec![ModelId::Deepseek, ModelId::Qwen])
        .call()
        .await
        .expect("call oracle");

    assert_eq!(verdict.consensus, Consensus::Unanimous);
}

// ── Configuration validation ──────────────────────────────────────────────────

/// `.call()` on a Custom mode query with no `.models()` returns a Config error.
#[tokio::test]
async fn empty_model_set_returns_config_error() {
    let result = OracleClient::builder()
        .build()
        .expect("build client")
        .query("will fail")
        .models(vec![]) // explicitly empty
        .call()
        .await;
    assert!(result.is_err());
}

/// Default mode for [`OracleMode::Prove`] selects 3 models.
#[test]
fn prove_mode_selects_three_models() {
    assert_eq!(ModelId::for_mode(OracleMode::Prove).len(), 3);
}
