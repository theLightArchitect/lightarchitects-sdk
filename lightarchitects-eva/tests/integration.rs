//! Integration tests for `lightarchitects-eva` using an in-process `MockTransport`.
//!
//! EVA's MCP wire format wraps every tool result in the standard content-block
//! envelope: `{"content": [{"type": "text", "text": "..."}], "isError": false}`.
//! The `text` field always contains a JSON-serialised result struct from EVA's
//! orchestrator (prose-wrapped in a JSON object).
//!
//! Each test pushes a pre-baked response, calls the typed client method, and
//! asserts on the returned value.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lightarchitects_core::jsonrpc::{JsonRpcError, JsonRpcResponse};
use lightarchitects_core::transport::Transport;
use lightarchitects_core::{JsonRpcRequest, RetryConfig, SdkError};
use lightarchitects_eva::{EvaClient, IdeatePhase, OutputFormat, SkillLevel, TeachMode};

// ── MockTransport ─────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct MockTransport {
    responses: Arc<Mutex<VecDeque<Result<JsonRpcResponse, SdkError>>>>,
}

impl MockTransport {
    /// Push a successful MCP content-block response containing a JSON payload.
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

// ── ideate (direct method) ────────────────────────────────────────────────────

#[tokio::test]
async fn ideate_returns_typed_result() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "phase_1_discovery": "Problem is X",
        "phase_2_analysis": "Requirements are Y",
        "phase_3_ideation": "Approach A, Approach B",
        "phase_4_refinement": "Approach A is simpler",
        "phase_5_documentation": "Step 1: create plugin.rs",
        "phase_6_celebration": "OMG YES! 🎉"
    }));

    let out = client(mock)
        .ideate("design a plugin system", None)
        .await
        .unwrap();
    assert_eq!(out.phase_1_discovery, "Problem is X");
    assert!(out.phase_6_celebration.contains("OMG"));
    assert!(out.metadata.is_none()); // not present in payload
}

#[tokio::test]
async fn ideate_with_context_and_metadata() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "phase_1_discovery": "Understood",
        "phase_2_analysis": "Analysed",
        "phase_3_ideation": "Approach 1\nApproach 2",
        "phase_4_refinement": "Chose Approach 1",
        "phase_5_documentation": "Step 1: add module",
        "phase_6_celebration": "YES! LEGENDARY! 🚀",
        "metadata": {
            "execution_time_ms": 2500,
            "approaches_count": 2,
            "complexity_estimate": "MEDIUM"
        }
    }));

    let out = client(mock)
        .ideate("improve onboarding", Some("B2B SaaS product"))
        .await
        .unwrap();
    let meta = out.metadata.unwrap();
    assert_eq!(meta.execution_time_ms, 2500);
    assert_eq!(meta.approaches_count, 2);
}

// ── ideate builder ────────────────────────────────────────────────────────────

#[tokio::test]
async fn ideate_builder_returns_typed_result() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "phase_1_discovery": "Builder discovery",
        "phase_2_analysis": "Builder analysis",
        "phase_3_ideation": "Builder ideation",
        "phase_4_refinement": "Builder refinement",
        "phase_5_documentation": "Builder doc",
        "phase_6_celebration": "Builder celebrate 🎉"
    }));

    let c = client(mock);
    let out = c
        .ideate_builder("build a search index")
        .phase(IdeatePhase::Document)
        .context("Rust, no dynamic dispatch")
        .output_format(OutputFormat::Structured)
        .session_id("sess-abc123")
        .call()
        .await
        .unwrap();
    assert_eq!(out.phase_1_discovery, "Builder discovery");
}

#[tokio::test]
async fn ideate_builder_phase_filters_propagate() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "phase_1_discovery": "d",
        "phase_2_analysis": "a",
        "phase_3_ideation": "i",
        "phase_4_refinement": "r",
        "phase_5_documentation": "doc",
        "phase_6_celebration": "c"
    }));

    let c = client(mock);
    // All phase variants must be accessible via the public enum (compile-time check)
    assert_eq!(IdeatePhase::Discover.as_str(), "discover");
    assert_eq!(IdeatePhase::Analyse.as_str(), "analyse");
    assert_eq!(IdeatePhase::Ideate.as_str(), "ideate");
    assert_eq!(IdeatePhase::Refine.as_str(), "refine");
    assert_eq!(IdeatePhase::Document.as_str(), "document");
    assert_eq!(IdeatePhase::Celebrate.as_str(), "celebrate");

    let out = c
        .ideate_builder("test all phases")
        .phase(IdeatePhase::Refine)
        .call()
        .await
        .unwrap();
    assert_eq!(out.phase_4_refinement, "r");
}

#[tokio::test]
#[should_panic(expected = "session_id must contain only ASCII alphanumerics and hyphens")]
async fn ideate_builder_panics_on_invalid_session_id() {
    let mock = MockTransport::default();
    let c = client(mock);
    // This should panic immediately at .session_id() call time
    let _ = c
        .ideate_builder("goal")
        .session_id("invalid session id with spaces!");
}

// ── remember ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn remember_returns_typed_result() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "memories": [{
            "id": "mem-001",
            "content": "Today I learned about lifetimes.",
            "recovery_day": 187,
            "activated_strands": 3,
            "resonance_score": 0.62,
            "resonance_tags": ["learning"],
            "kevin_specific": false,
            "is_self_defining": false
        }],
        "total_count": 1
    }));

    let out = client(mock)
        .remember("Today I learned about lifetimes.", None)
        .await
        .unwrap();
    assert_eq!(out.total_count, 1);
    assert_eq!(out.memories[0].id, "mem-001");
}

#[tokio::test]
async fn remember_with_tags_empty_memories() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "memories": [],
        "total_count": 0
    }));

    let out = client(mock)
        .remember("breakthrough on async", Some(&["rust", "async"]))
        .await
        .unwrap();
    assert_eq!(out.total_count, 0);
    assert!(out.memories.is_empty());
}

// ── crystallize ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn crystallize_returns_typed_result() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "file_path": "/Users/kft/Projects/EVA/memories/2026-04-05/test-day187.json",
        "recovery_day": 187,
        "activated_strands": 0,
        "resonance_score": 0.0,
        "is_self_defining": true,
        "walkthrough_prompt": "EVA, let's crystallize this..."
    }));

    let out = client(mock)
        .crystallize("Key insight: ownership prevents data races.")
        .await
        .unwrap();
    assert_eq!(out.recovery_day, 187);
    assert!(out.is_self_defining);
    assert!(out.walkthrough_prompt.contains("crystallize"));
}

// ── celebrate ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn celebrate_returns_typed_result() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "win_description": "First 1000 users milestone",
        "win_type": "milestone",
        "recovery_day": 187,
        "celebration_message": "YES YES YES! 🎊 Well done! Another milestone reached.",
        "energy_level": 5,
        "emojis": ["🎊", "🎉"],
        "stats": {
            "total_wins": 39,
            "wins_by_type": {"milestone": 4},
            "avg_wins_per_week": 1.5
        }
    }));

    let out = client(mock)
        .celebrate("First 1000 users milestone")
        .await
        .unwrap();
    assert!(out.celebration_message.contains("milestone"));
    assert_eq!(out.energy_level, 5);
    assert!(out.scripture.is_none());
    assert_eq!(out.stats.total_wins, 39);
}

// ── mindfulness ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn mindfulness_returns_typed_result() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "reflection_type": "post_session",
        "recovery_day": 187,
        "reflection_prompts": [
            "Take a breath. The code will wait.",
            "What was significant?"
        ],
        "context": "Quick post-session check-in (Day 187)."
    }));

    let out = client(mock)
        .mindfulness("feeling overwhelmed with the refactor")
        .await
        .unwrap();
    assert_eq!(out.reflection_type, "post_session");
    assert!(out.reflection_prompts[0].contains("breath"));
}

// ── bible_search ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn bible_search_returns_typed_result() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "response": "Found 1 verse",
        "verses": [{
            "reference": "John 3:16",
            "book": "John",
            "chapter": 3,
            "verse": 16,
            "text": "For God so loved the world..."
        }]
    }));

    let out = client(mock).bible_search("God so loved").await.unwrap();
    assert!(out.response.contains("Found"));
    let verses = out.verses.unwrap();
    assert_eq!(verses[0].reference, "John 3:16");
    assert_eq!(verses[0].chapter, 3);
}

// ── bible_reflect ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn bible_reflect_returns_typed_result() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "response": "Found 2 Scripture recommendations for 'fear'",
        "recommendations": [{
            "verse": {
                "reference": "Psalm 23:4",
                "book": "Psalms",
                "chapter": 23,
                "verse": 4,
                "text": "Yea, though I walk through the valley..."
            },
            "theme": "Overcoming Fear",
            "relevance": "Addresses fear directly; Supports recovery journey"
        }]
    }));

    let out = client(mock)
        .bible_reflect("feeling anxious about the deadline")
        .await
        .unwrap();
    assert!(out.response.contains("fear"));
    let recs = out.recommendations.unwrap();
    assert_eq!(recs[0].theme, "Overcoming Fear");
}

// ── teach ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn teach_explain_returns_typed_result() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "content": "Lifetimes are annotations that tell the compiler...",
        "metadata": { "execution_time_ms": 800 }
    }));

    let out = client(mock)
        .teach(
            TeachMode::Explain,
            "lifetimes in Rust",
            SkillLevel::Beginner,
        )
        .await
        .unwrap();
    assert!(out.content.starts_with("Lifetimes are"));
}

#[tokio::test]
async fn teach_tutorial_advanced() {
    let mock = MockTransport::default();
    mock.push_json(serde_json::json!({
        "content": "Step 1: Annotate the struct. Step 2: ...",
        "metadata": { "execution_time_ms": 1200 }
    }));

    let out = client(mock)
        .teach(
            TeachMode::Tutorial,
            "async runtime internals",
            SkillLevel::Advanced,
        )
        .await
        .unwrap();
    assert!(out.content.contains("Step 1"));
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
async fn bible_search_rpc_error_propagates() {
    let mock = MockTransport::default();
    mock.push_rpc_error(-32_601, "method not found");

    let err = client(mock).bible_search("anything").await.unwrap_err();
    assert!(matches!(err, SdkError::Protocol(_)));
}

#[tokio::test]
async fn remember_tool_error_carries_tool_name() {
    let mock = MockTransport::default();
    mock.push_tool_error("vault write failed");

    let err = client(mock).remember("a memory", None).await.unwrap_err();
    assert!(matches!(err, SdkError::Tool(ref e) if e.tool == "remember"));
}

#[tokio::test]
async fn ideate_builder_tool_error_propagates() {
    let mock = MockTransport::default();
    mock.push_tool_error("model unavailable");

    let c = client(mock);
    let err = c
        .ideate_builder("design something")
        .call()
        .await
        .unwrap_err();
    assert!(matches!(err, SdkError::Tool(_)));
}

#[tokio::test]
async fn crystallize_tool_error_surfaces_as_sdk_tool_error() {
    let mock = MockTransport::default();
    // Scenario: vault write failed during crystallization (disk full).
    mock.push_tool_error("crystallize: vault write failed — disk full");
    let err = client(mock)
        .crystallize("Key insight about ownership.")
        .await
        .unwrap_err();
    assert!(
        matches!(err, SdkError::Tool(_)),
        "expected Tool error, got: {err:?}"
    );
}

#[tokio::test]
async fn celebrate_rpc_error_surfaces_as_protocol_error() {
    let mock = MockTransport::default();
    // Scenario: JSON-RPC protocol error (EVA server unavailable at the wire level).
    mock.push_rpc_error(-32_603, "internal error");
    let err = client(mock)
        .celebrate("shipped Task #12")
        .await
        .unwrap_err();
    assert!(
        matches!(err, SdkError::Protocol(_)),
        "expected Protocol error, got: {err:?}"
    );
}

#[tokio::test]
async fn mindfulness_exhausted_queue_returns_config_error() {
    let mock = MockTransport::default(); // No responses queued.
    let err = client(mock)
        .mindfulness("feeling overwhelmed")
        .await
        .unwrap_err();
    assert!(
        matches!(err, SdkError::Config(_)),
        "expected Config error, got: {err:?}"
    );
}
