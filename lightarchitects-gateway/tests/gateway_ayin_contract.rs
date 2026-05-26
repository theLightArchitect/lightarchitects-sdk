//! AYIN span contract tests — verify that spans written by the gateway are
//! valid JSON that AYIN can parse, contain required fields, and land atomically.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};
use lightarchitects_gateway::span_context::{
    GatewaySpanContext, current_span_ctx, with_span_context, write_span_to_disk,
};

/// Build a minimal valid span for contract testing.
fn make_span(action: &str) -> lightarchitects::ayin::span::TraceSpan {
    TraceContext::new(Actor::new("gateway"), action)
        .outcome(TraceOutcome::Continue)
        .finish()
        .expect("span build")
}

#[tokio::test]
async fn written_span_is_valid_ayin_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let span = make_span("gateway.tool.dispatch");

    write_span_to_disk(&span, &PathBuf::from(dir.path()))
        .await
        .expect("write");

    let files: Vec<_> = std::fs::read_dir(dir.path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .collect();
    assert_eq!(files.len(), 1, "exactly one span file");

    let raw = std::fs::read(&files[0].path()).expect("read");
    let parsed: serde_json::Value = serde_json::from_slice(&raw).expect("valid json");

    // Required AYIN schema fields.
    // Actor is #[serde(transparent)] — serialises as a plain string, not {"name":"..."}.
    assert!(parsed["id"].is_string(), "id field present");
    assert_eq!(parsed["actor"], "gateway");
    assert_eq!(parsed["action"], "gateway.tool.dispatch");
    assert!(parsed["timestamp"].is_string(), "timestamp present");
}

#[tokio::test]
async fn tmp_file_absent_after_successful_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let span = make_span("gateway.test.cleanup");

    write_span_to_disk(&span, &PathBuf::from(dir.path()))
        .await
        .expect("write");

    let tmp_files: Vec<_> = std::fs::read_dir(dir.path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
        .collect();
    assert!(
        tmp_files.is_empty(),
        "no .tmp files left after atomic rename"
    );
}

#[tokio::test]
async fn with_span_context_visible_inside_scope() {
    let sid = "contract-test-session";
    let ctx = GatewaySpanContext {
        session_id: Some(sid.to_owned()),
        parent_id: None,
    };
    let seen_sid = with_span_context(ctx, async { current_span_ctx().session_id }).await;
    assert_eq!(seen_sid.as_deref(), Some(sid));
}

#[tokio::test]
async fn span_context_absent_outside_scope() {
    // current_span_ctx() must return Default when called outside with_span_context
    let ctx = current_span_ctx();
    assert!(ctx.session_id.is_none());
    assert!(ctx.parent_id.is_none());
}

#[tokio::test]
async fn parent_id_wired_into_span_when_context_set() {
    let parent = uuid::Uuid::new_v4();
    let ctx = GatewaySpanContext {
        session_id: Some("sess-abc".to_owned()),
        parent_id: Some(parent),
    };
    let dir = tempfile::tempdir().expect("tempdir");

    with_span_context(ctx, async {
        // Build span manually with the context values (mirrors emit_llm_span logic)
        let inner_ctx = current_span_ctx();
        let mut builder =
            TraceContext::new(Actor::new("gateway"), "llm.call").outcome(TraceOutcome::Continue);
        if let Some(pid) = inner_ctx.parent_id {
            builder = builder.parent(pid);
        }
        let span = builder.finish().expect("span");
        assert_eq!(span.parent_id, Some(parent));
        write_span_to_disk(&span, &PathBuf::from(dir.path()))
            .await
            .expect("write");
    })
    .await;

    let files: Vec<_> = std::fs::read_dir(dir.path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(files.len(), 1);
    let raw = std::fs::read(&files[0].path()).expect("read");
    let parsed: serde_json::Value = serde_json::from_slice(&raw).expect("json");
    assert_eq!(
        parsed["parent_id"].as_str().unwrap(),
        parent.to_string(),
        "parent_id written correctly"
    );
}
