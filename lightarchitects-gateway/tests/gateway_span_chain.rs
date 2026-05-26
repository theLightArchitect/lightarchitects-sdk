//! Span chain contract tests — verifies that the actor/parent_id/session_id
//! wiring the gateway emits matches the AYIN schema contract.
//!
//! These tests exercise the span_context primitives that `llm.rs` and
//! `server.rs` use when emitting spans, without requiring a live LLM backend.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};
use lightarchitects_gateway::span_context::{
    GatewaySpanContext, current_span_ctx, with_span_context, write_span_to_disk,
};

fn make_span_with_actor(actor: &str, action: &str) -> lightarchitects::ayin::span::TraceSpan {
    TraceContext::new(Actor::new(actor), action)
        .outcome(TraceOutcome::Continue)
        .finish()
        .expect("span build")
}

// ── Actor name wiring ─────────────────────────────────────────────────────────

/// Anthropic backend emits spans with actor = "anthropic" (as LlmBackend maps it).
#[tokio::test]
async fn llm_call_span_actor_matches_anthropic_backend() {
    let dir = tempfile::tempdir().expect("tempdir");
    let span = make_span_with_actor("anthropic", "llm.call");

    write_span_to_disk(&span, &PathBuf::from(dir.path()))
        .await
        .expect("write");

    let files: Vec<_> = std::fs::read_dir(dir.path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .collect();
    assert_eq!(files.len(), 1);

    let raw = std::fs::read(&files[0].path()).expect("read");
    let v: serde_json::Value = serde_json::from_slice(&raw).expect("valid json");
    // Actor is #[serde(transparent)] — serialises as plain string.
    assert_eq!(v["actor"], "anthropic");
    assert_eq!(v["action"], "llm.call");
}

/// Ollama backend emits spans with actor = "ollama".
#[tokio::test]
async fn llm_call_span_actor_matches_ollama_backend() {
    let dir = tempfile::tempdir().expect("tempdir");
    let span = make_span_with_actor("ollama", "llm.call");

    write_span_to_disk(&span, &PathBuf::from(dir.path()))
        .await
        .expect("write");

    let files: Vec<_> = std::fs::read_dir(dir.path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .collect();
    assert_eq!(files.len(), 1);

    let raw = std::fs::read(&files[0].path()).expect("read");
    let v: serde_json::Value = serde_json::from_slice(&raw).expect("valid json");
    assert_eq!(v["actor"], "ollama");
}

// ── Parent-child chain ────────────────────────────────────────────────────────

/// Simulates the llm.call → tool.dispatch parent_id chain:
/// the tool dispatch span must carry the LLM span's id as parent_id.
#[tokio::test]
async fn tool_dispatch_span_wires_parent_id_from_llm_call() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dir_path = PathBuf::from(dir.path());

    // Step 1: emit the "llm.call" span and capture its id.
    let llm_span = TraceContext::new(Actor::new("anthropic"), "llm.call")
        .outcome(TraceOutcome::Continue)
        .finish()
        .expect("llm span");
    let llm_id = llm_span.id;
    write_span_to_disk(&llm_span, &dir_path)
        .await
        .expect("write llm span");

    // Step 2: emit the tool dispatch span with the LLM span as parent.
    let tool_span = TraceContext::new(Actor::new("gateway"), "tool.dispatch")
        .parent(llm_id)
        .outcome(TraceOutcome::Continue)
        .finish()
        .expect("tool span");
    let tool_id = tool_span.id;
    write_span_to_disk(&tool_span, &dir_path)
        .await
        .expect("write tool span");

    // Verify the chain in the tool span JSON.
    let files: Vec<_> = std::fs::read_dir(dir.path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .collect();
    assert_eq!(files.len(), 2, "exactly 2 span files");

    let tool_file = files
        .iter()
        .find(|e| e.file_name().to_string_lossy().contains("tool.dispatch"))
        .expect("tool.dispatch span file");

    let raw = std::fs::read(tool_file.path()).expect("read");
    let v: serde_json::Value = serde_json::from_slice(&raw).expect("json");
    assert_eq!(v["id"].as_str().unwrap(), tool_id.to_string());
    assert_eq!(
        v["parent_id"].as_str().unwrap(),
        llm_id.to_string(),
        "tool.dispatch parent_id must equal llm.call id"
    );
}

// ── Session-id propagation ────────────────────────────────────────────────────

/// session_id set in the span context flows into every span written within that scope.
#[tokio::test]
async fn session_id_propagated_to_all_spans_in_context() {
    let expected_sid = "e2e-session-abc123";
    let dir = tempfile::tempdir().expect("tempdir");
    let dir_path = PathBuf::from(dir.path());

    let ctx = GatewaySpanContext {
        session_id: Some(expected_sid.to_owned()),
        parent_id: None,
    };

    with_span_context(ctx, async {
        for action in &["llm.call", "tool.dispatch", "llm.call"] {
            let inner = current_span_ctx();
            let mut builder =
                TraceContext::new(Actor::new("gateway"), *action).outcome(TraceOutcome::Continue);
            if let Some(ref sid) = inner.session_id {
                builder = builder.session_id(sid);
            }
            let span = builder.finish().expect("span");
            write_span_to_disk(&span, &dir_path).await.expect("write");
        }
    })
    .await;

    let json_files: Vec<_> = std::fs::read_dir(dir.path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .collect();
    assert_eq!(json_files.len(), 3, "all 3 spans written");

    for f in &json_files {
        let raw = std::fs::read(f.path()).expect("read");
        let v: serde_json::Value = serde_json::from_slice(&raw).expect("json");
        assert_eq!(
            v["session_id"].as_str().unwrap_or(""),
            expected_sid,
            "session_id missing in {:?}",
            f.path()
        );
    }
}
