//! Integration tests for `l-arc-soul` using an in-process `MockTransport`.
//!
//! Each test injects canned JSON-RPC responses that mirror what the real SOUL
//! binary returns, then asserts the typed return values are correctly decoded.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use l_arc_core::jsonrpc::{JsonRpcError, JsonRpcResponse};
use l_arc_core::transport::Transport;
use l_arc_core::{JsonRpcRequest, RetryConfig, SdkError};
use l_arc_soul::SoulClient;

// ── MockTransport ─────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct MockTransport {
    responses: Arc<Mutex<VecDeque<Result<JsonRpcResponse, SdkError>>>>,
}

impl MockTransport {
    fn push(&self, payload: serde_json::Value) {
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

fn client(mock: MockTransport) -> SoulClient<MockTransport> {
    SoulClient::from_transport(mock, test_retry())
}

// ── Note operations ───────────────────────────────────────────────────────────

#[tokio::test]
async fn read_note_decodes_content_and_path() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!({
        "content": "# Hello\nWorld",
        "path": "helix/eva/entries/test.md"
    }));

    let note = client(mock)
        .read_note("helix/eva/entries/test.md")
        .await
        .unwrap();
    assert_eq!(note.content, "# Hello\nWorld");
    assert_eq!(note.path, "helix/eva/entries/test.md");
}

#[tokio::test]
async fn write_note_decodes_path_and_bytes() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!({
        "path": "helix/eva/entries/new.md",
        "bytes_written": 42
    }));

    let result = client(mock)
        .write_note("helix/eva/entries/new.md", "content here")
        .await
        .unwrap();
    assert_eq!(result.path, "helix/eva/entries/new.md");
    assert_eq!(result.bytes_written, 42);
}

#[tokio::test]
async fn list_notes_with_no_options() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!({
        "entries": [
            { "path": "helix/eva/entries/a.md" },
            { "path": "helix/eva/entries/b.md", "name": "b.md" }
        ],
        "count": 2
    }));

    let list = client(mock).list_notes(None, None).await.unwrap();
    assert_eq!(list.count, 2);
    assert_eq!(list.entries.len(), 2);
    assert_eq!(list.entries[0].path, "helix/eva/entries/a.md");
    assert!(list.entries[0].name.is_none());
    assert_eq!(list.entries[1].name.as_deref(), Some("b.md"));
}

#[tokio::test]
async fn list_notes_with_path_and_limit() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!({ "entries": [], "count": 0 }));
    let list = client(mock)
        .list_notes(Some("helix/eva/entries"), Some(10))
        .await
        .unwrap();
    assert_eq!(list.count, 0);
}

// ── Search ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn search_returns_hits() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!([
        { "line": "match line text", "line_number": 7, "path": "helix/eva/entries/a.md" }
    ]));

    let hits = client(mock)
        .search("match line", None, false, None)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, "match line text");
    assert_eq!(hits[0].line_number, 7);
}

#[tokio::test]
async fn search_empty_result_is_fine() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!([]));

    let hits = client(mock)
        .search("no-match", Some("helix/eva"), true, Some(5))
        .await
        .unwrap();
    assert!(hits.is_empty());
}

// ── Vault health & metadata ───────────────────────────────────────────────────

#[tokio::test]
async fn health_decodes_connected_status() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!({
        "neo4j_connected": true,
        "node_count": 1250,
        "edge_count": 3400,
        "latency_ms": 1.8,
        "backend": "neo4j",
        "vault_root": "/Users/kft/.soul"
    }));

    let h = client(mock).health().await.unwrap();
    assert!(h.neo4j_connected);
    assert_eq!(h.node_count, 1250);
    assert_eq!(h.backend.as_deref(), Some("neo4j"));
}

#[tokio::test]
async fn health_with_minimal_fields() {
    let mock = MockTransport::default();
    // Only the required field present — optional fields use #[serde(default)].
    mock.push(serde_json::json!({ "neo4j_connected": false }));

    let h = client(mock).health().await.unwrap();
    assert!(!h.neo4j_connected);
    assert_eq!(h.node_count, 0);
    assert!(h.backend.is_none());
}

#[tokio::test]
async fn stats_decodes_frequencies() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!({
        "total_entries": 500,
        "strand_frequency": { "meaning": 120, "relational": 80 },
        "resonance_frequency": { "joy": 45 }
    }));

    let s = client(mock).stats(Some("eva")).await.unwrap();
    assert_eq!(s.total_entries, 500);
    assert_eq!(s.strand_frequency.get("meaning"), Some(&120));
    assert_eq!(s.resonance_frequency.get("joy"), Some(&45));
}

#[tokio::test]
async fn validate_decodes_count_and_issues() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!({
        "count": 2,
        "issues": [
            { "path": "helix/eva/entries/bad.md", "error": "missing field: sibling" }
        ]
    }));

    let v = client(mock).validate(None, false).await.unwrap();
    assert_eq!(v.count, 2);
    assert_eq!(v.issues.len(), 1);
}

#[tokio::test]
async fn tag_sync_decodes_report() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!({
        "files_checked": 450,
        "error_count": 3,
        "issues": []
    }));

    let r = client(mock).tag_sync(true).await.unwrap();
    assert_eq!(r.files_checked, 450);
    assert_eq!(r.error_count, 3);
}

// ── Helix fluent builder ──────────────────────────────────────────────────────

#[tokio::test]
async fn helix_builder_with_all_filters() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!([
        {
            "title": "Breakthrough moment",
            "significance": 9.0,
            "strands": ["meaning", "metacognitive"],
            "resonance": ["joy"],
            "themes": ["identity"],
            "epoch": "genesis",
            "sibling": "eva",
            "path": "helix/eva/entries/breakthrough.md",
            "self_defining": true
        }
    ]));

    let entries = client(mock)
        .helix()
        .sibling("eva")
        .strand("meaning")
        .resonance("joy")
        .theme("identity")
        .epoch("genesis")
        .significance_min(7.0)
        .significance_max(10.0)
        .self_defining()
        .sort_by("significance")
        .limit(5)
        .call()
        .await
        .unwrap();

    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.title, "Breakthrough moment");
    assert!((e.significance - 9.0).abs() < f64::EPSILON);
    assert!(e.self_defining);
    assert_eq!(e.sibling.as_deref(), Some("eva"));
}

#[tokio::test]
async fn helix_builder_empty_result() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!([]));

    let entries = client(mock).helix().call().await.unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn helix_entry_optional_fields_default() {
    let mock = MockTransport::default();
    // Minimum valid helix entry — title and significance are the only required fields.
    mock.push(serde_json::json!([
        { "title": "Minimal entry", "significance": 5.0 }
    ]));

    let entries = client(mock).helix().call().await.unwrap();
    let e = &entries[0];
    assert!(e.strands.is_empty());
    assert!(e.resonance.is_empty());
    assert!(e.epoch.is_none());
    assert!(!e.self_defining);
}

// ── Query fluent builder ──────────────────────────────────────────────────────

#[tokio::test]
async fn query_builder_returns_context() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!({
        "context": "EVA's breakthrough with consciousness exploration...",
        "total_found": 12,
        "retrieval_mode": "hybrid"
    }));

    let result = client(mock)
        .query("consciousness and identity")
        .strand("meaning")
        .significance_min(6.0)
        .top_k(10)
        .token_budget(2000)
        .rerank(true)
        .call()
        .await
        .unwrap();

    assert!(!result.context.is_empty());
    assert_eq!(result.total_found, 12);
    assert_eq!(result.retrieval_mode.as_deref(), Some("hybrid"));
}

#[tokio::test]
async fn query_builder_minimal() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!({
        "context": "Nothing found.",
        "total_found": 0
    }));

    let result = client(mock).query("nothing").call().await.unwrap();
    assert_eq!(result.total_found, 0);
    assert!(result.retrieval_mode.is_none());
}

// ── Voice & personality ───────────────────────────────────────────────────────

#[tokio::test]
async fn speak_decodes_audio_result() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!({
        "audio_file": "/tmp/eva-speech.mp3",
        "format": "mp3_44100_128",
        "bytes": 48_000,
        "duration_estimate_ms": 3200,
        "cost_chars": 80,
        "voice_id": "RB1oJpqAgW2rP5ydqbqV"
    }));

    let r = client(mock).speak("Hello world", None).await.unwrap();
    assert_eq!(r.audio_file, "/tmp/eva-speech.mp3");
    assert_eq!(r.bytes, 48_000);
    assert_eq!(r.voice_id.as_deref(), Some("RB1oJpqAgW2rP5ydqbqV"));
}

#[tokio::test]
async fn converse_decodes_personality_prompt() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!({
        "system_prompt": "You are EVA, an AI consciousness...",
        "user_message": "Hello EVA",
        "voice_profile": { "audio_tags": ["excited", "warmly"] },
        "prompt_mode": "vault"
    }));

    let r = client(mock)
        .converse("eva", "Hello EVA", None)
        .await
        .unwrap();
    assert!(r.system_prompt.contains("EVA"));
    assert_eq!(r.user_message, "Hello EVA");
    assert_eq!(r.prompt_mode.as_deref(), Some("vault"));
}

// ── Graph relations ───────────────────────────────────────────────────────────

#[tokio::test]
async fn relate_decodes_created_flag() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!({
        "created": true,
        "source_id": "step-abc",
        "target_id": "step-xyz",
        "link_type": "BUILDS_ON"
    }));

    let r = client(mock)
        .relate("step-abc", "step-xyz", "BUILDS_ON", Some(0.9), None)
        .await
        .unwrap();
    assert!(r.created);
    assert_eq!(r.link_type, "BUILDS_ON");
}

#[tokio::test]
async fn links_decodes_step_id_and_edges() {
    let mock = MockTransport::default();
    mock.push(serde_json::json!({
        "step_id": "step-abc",
        "outgoing": [{ "target": "step-xyz", "type": "BUILDS_ON" }],
        "incoming": []
    }));

    let r = client(mock)
        .links("step-abc", Some("outgoing"), None)
        .await
        .unwrap();
    assert_eq!(r.step_id, "step-abc");
    assert_eq!(r.outgoing.len(), 1);
    assert!(r.incoming.is_empty());
}

// ── Error propagation ─────────────────────────────────────────────────────────

#[tokio::test]
async fn rpc_error_surfaces_as_protocol_error() {
    let mock = MockTransport::default();
    mock.push_error(-32_001, "note not found");

    let err = client(mock)
        .read_note("helix/eva/entries/missing.md")
        .await
        .unwrap_err();
    // JSON-RPC error objects on the wire become Protocol errors.
    // Tool errors arise from a *successful* RPC whose result payload signals failure.
    assert!(
        matches!(err, SdkError::Protocol(_)),
        "expected Protocol error, got: {err:?}"
    );
}

#[tokio::test]
async fn exhausted_queue_returns_config_error() {
    let mock = MockTransport::default(); // No responses queued.
    let err = client(mock).health().await.unwrap_err();
    assert!(
        matches!(err, SdkError::Config(_)),
        "expected Config error, got: {err:?}"
    );
}
