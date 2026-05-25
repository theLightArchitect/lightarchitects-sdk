//! SSE event-ordering invariants (W8.3 regression anchor).
//!
//! Verifies that a canonical turn sequence — `status_update → text chunks →
//! token_usage → complete` — is preserved in the SSE byte stream and that
//! no events appear after `complete`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects::agent::conversation::{
    ConversationEvent, SseTransport, Transport, event::TerminationReason,
};

// ── helpers ───────────────────────────────────────────────────────────────────

/// Collect all SSE frames emitted by a sequence of events into a `Vec<String>`,
/// where each entry is `"<event_name>:<json_data>"`.
async fn collect_frames(events: &[ConversationEvent]) -> Vec<String> {
    let mut buf = Vec::new();
    {
        let mut transport = SseTransport::new(&mut buf);
        for ev in events {
            transport.emit(ev).await.unwrap();
        }
        // transport drops here, releasing the mutable borrow on buf
    }

    // Parse raw bytes into (event_name, data_json) pairs.
    let raw = String::from_utf8(buf).unwrap();
    let mut frames = Vec::new();
    let mut current_event: Option<String> = None;
    let mut current_data: Option<String> = None;

    for line in raw.lines() {
        if let Some(name) = line.strip_prefix("event: ") {
            current_event = Some(name.to_owned());
        } else if let Some(data) = line.strip_prefix("data: ") {
            current_data = Some(data.to_owned());
        } else if line.is_empty() {
            // Frame boundary — flush the accumulated event+data.
            if let (Some(ev), Some(dat)) = (current_event.take(), current_data.take()) {
                frames.push(format!("{ev}:{dat}"));
            }
        }
    }
    frames
}

/// Extract only the event names from a frame list.
fn event_names(frames: &[String]) -> Vec<&str> {
    frames
        .iter()
        .map(|f| f.split(':').next().unwrap_or(""))
        .collect()
}

// ── Invariant A: status_update precedes first text chunk ──────────────────────

#[tokio::test]
async fn status_update_precedes_first_text_chunk() {
    let events = vec![
        ConversationEvent::StatusUpdate {
            text: "Connecting to Ollama Cloud…".into(),
        },
        ConversationEvent::Text {
            chunk: "Hello".into(),
        },
        ConversationEvent::Text {
            chunk: ", world".into(),
        },
        ConversationEvent::TokenUsage {
            input: 12,
            output: 3,
        },
        ConversationEvent::Complete {
            reason: TerminationReason::Complete,
        },
    ];

    let frames = collect_frames(&events).await;
    let names = event_names(&frames);

    let first_text = names.iter().position(|&n| n == "text").unwrap();
    let last_status = names
        .iter()
        .rposition(|&n| n == "status_update")
        .unwrap_or(0);

    assert!(
        last_status < first_text,
        "status_update must precede first text chunk (status={last_status}, text={first_text})"
    );
}

// ── Invariant B: text chunk ordering is monotonic in the stream ───────────────

#[tokio::test]
async fn text_chunks_are_monotonically_ordered() {
    let chunks = ["one", "two", "three", "four"];
    let events: Vec<ConversationEvent> = chunks
        .iter()
        .map(|&c| ConversationEvent::Text { chunk: c.into() })
        .collect();

    let frames = collect_frames(&events).await;

    // Reconstruct text in emission order and verify it matches emission order.
    let reconstructed: Vec<String> = frames
        .iter()
        .filter(|f| f.starts_with("text:"))
        .map(|f| {
            let json: serde_json::Value = serde_json::from_str(&f["text:".len()..]).unwrap();
            json["chunk"].as_str().unwrap_or("").to_owned()
        })
        .collect();

    let expected: Vec<String> = chunks.iter().map(|&s| s.to_owned()).collect();
    assert_eq!(
        reconstructed, expected,
        "text chunks must arrive in emission order"
    );
}

// ── Invariant C: token_usage precedes complete ────────────────────────────────

#[tokio::test]
async fn token_usage_precedes_complete() {
    let events = vec![
        ConversationEvent::StatusUpdate {
            text: "Running…".into(),
        },
        ConversationEvent::Text {
            chunk: "result".into(),
        },
        ConversationEvent::TokenUsage {
            input: 100,
            output: 20,
        },
        ConversationEvent::Complete {
            reason: TerminationReason::Complete,
        },
    ];

    let frames = collect_frames(&events).await;
    let names = event_names(&frames);

    let usage_pos = names
        .iter()
        .position(|&n| n == "token_usage")
        .expect("token_usage must be present");
    let complete_pos = names
        .iter()
        .position(|&n| n == "complete")
        .expect("complete must be present");

    assert!(
        usage_pos < complete_pos,
        "token_usage must precede complete (usage={usage_pos}, complete={complete_pos})"
    );
}

// ── Invariant D: no events are emitted after complete ────────────────────────

#[tokio::test]
async fn no_events_after_complete() {
    let events = vec![
        ConversationEvent::StatusUpdate {
            text: "Done".into(),
        },
        ConversationEvent::TokenUsage {
            input: 5,
            output: 2,
        },
        ConversationEvent::Complete {
            reason: TerminationReason::Complete,
        },
    ];

    let frames = collect_frames(&events).await;
    let names = event_names(&frames);
    let complete_pos = names
        .iter()
        .position(|&n| n == "complete")
        .expect("complete must be present");

    // Nothing after complete.
    assert_eq!(
        complete_pos,
        names.len() - 1,
        "no events may appear after complete; got: {names:?}"
    );
}

/// Companion to `no_events_after_complete`: demonstrates the checker is NOT
/// tautological.  `SseTransport` is a pass-through — it emits whatever the
/// caller provides.  A rogue post-complete event DOES appear in the stream,
/// so the `complete_pos == names.len() - 1` invariant check would fire red
/// in a session that violates the ordering contract.
#[tokio::test]
async fn post_complete_rogue_event_is_detectable() {
    let events = vec![
        ConversationEvent::Complete {
            reason: TerminationReason::Complete,
        },
        // Rogue event after complete — simulates a misbehaving session.
        ConversationEvent::Text {
            chunk: "rogue post-complete chunk".into(),
        },
    ];

    let frames = collect_frames(&events).await;
    let names = event_names(&frames);
    let complete_pos = names
        .iter()
        .position(|&n| n == "complete")
        .expect("complete must be present");

    // The transport emits the rogue event — proving the checker above would
    // catch a real violation (complete_pos < names.len() - 1 here).
    assert_ne!(
        complete_pos,
        names.len() - 1,
        "rogue post-complete event must make complete non-terminal: {names:?}"
    );
}

// ── Invariant E: ToolStart + ToolComplete pair ordering ───────────────────────

#[tokio::test]
async fn tool_start_precedes_tool_complete_for_same_id() {
    let events = vec![
        ConversationEvent::StatusUpdate {
            text: "Calling tool…".into(),
        },
        ConversationEvent::ToolStart {
            name: "bash".into(),
            id: "call_abc".into(),
            input: serde_json::json!({ "command": "ls" }),
        },
        ConversationEvent::ToolComplete {
            id: "call_abc".into(),
            success: true,
            duration_ms: 12,
            result: Some("file.txt".into()),
        },
        ConversationEvent::TokenUsage {
            input: 30,
            output: 10,
        },
        ConversationEvent::Complete {
            reason: TerminationReason::Complete,
        },
    ];

    let frames = collect_frames(&events).await;
    let names = event_names(&frames);

    let start_pos = names
        .iter()
        .position(|&n| n == "tool_start")
        .expect("tool_start must be present");
    let complete_pos = names
        .iter()
        .position(|&n| n == "tool_complete")
        .expect("tool_complete must be present");

    assert!(
        start_pos < complete_pos,
        "tool_start must precede tool_complete (start={start_pos}, complete={complete_pos})"
    );
}

// ── Suite 3: Property tests — SseTransport pass-through invariants ────────────

use proptest::prelude::*;

proptest! {
    /// Each emitted event must produce exactly one SSE frame.
    /// `SseTransport` is a pure pass-through; one `emit()` call → one `event:/data:` block.
    #[test]
    fn proptest_each_event_produces_exactly_one_frame(n in 1usize..=20usize) {
        let events: Vec<ConversationEvent> = (0..n)
            .map(|i| ConversationEvent::Text { chunk: format!("chunk-{i}") })
            .collect();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let frames = rt.block_on(collect_frames(&events));
        prop_assert_eq!(frames.len(), n, "one SSE frame must be emitted per event");
    }

    /// Text chunk content must survive the SSE serialisation round-trip verbatim.
    /// Any printable ASCII string must appear unchanged in the `chunk` JSON field.
    #[test]
    fn proptest_text_chunk_survives_sse_round_trip(content in "[ -~]{0,120}") {
        let events = vec![ConversationEvent::Text { chunk: content.clone() }];
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let frames = rt.block_on(collect_frames(&events));
        prop_assert_eq!(frames.len(), 1);
        let json: serde_json::Value =
            serde_json::from_str(&frames[0]["text:".len()..]).unwrap();
        prop_assert_eq!(json["chunk"].as_str().unwrap_or("MISSING"), content);
    }

    /// A `Complete` event followed by an arbitrary number of additional events
    /// must produce `n+1` frames total — the transport does NOT gate on complete.
    /// (The session layer owns that invariant; the transport is unconditional.)
    #[test]
    fn proptest_complete_then_extra_events_all_appear(extra in 1usize..=8usize) {
        let mut events = vec![ConversationEvent::Complete {
            reason: TerminationReason::Complete,
        }];
        events.extend(
            (0..extra).map(|i| ConversationEvent::Text { chunk: format!("rogue-{i}") }),
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let frames = rt.block_on(collect_frames(&events));
        prop_assert_eq!(
            frames.len(),
            extra + 1,
            "transport emits all events including those after complete"
        );
        let names = event_names(&frames);
        prop_assert_eq!(names[0], "complete");
        prop_assert!(names[1..].iter().all(|&n| n == "text"), "rogue events must appear");
    }
}

// ── Invariant F: full canonical turn sequence is well-formed ─────────────────

#[tokio::test]
async fn canonical_turn_sequence_is_well_formed() {
    let events = vec![
        ConversationEvent::StatusUpdate {
            text: "Connecting…".into(),
        },
        ConversationEvent::Text {
            chunk: "I found ".into(),
        },
        ConversationEvent::Text {
            chunk: "the answer.".into(),
        },
        ConversationEvent::TokenUsage {
            input: 50,
            output: 7,
        },
        ConversationEvent::Complete {
            reason: TerminationReason::Complete,
        },
    ];

    let frames = collect_frames(&events).await;
    let names = event_names(&frames);

    // A+C+D combined: status_update < text < token_usage < complete.
    let status_pos = names.iter().position(|&n| n == "status_update").unwrap();
    let first_text = names.iter().position(|&n| n == "text").unwrap();
    let usage_pos = names.iter().position(|&n| n == "token_usage").unwrap();
    let complete_pos = names.iter().position(|&n| n == "complete").unwrap();

    assert!(status_pos < first_text, "A: status before text");
    assert!(first_text < usage_pos, "ordering: text before token_usage");
    assert!(usage_pos < complete_pos, "C: token_usage before complete");
    assert_eq!(complete_pos, names.len() - 1, "D: complete is last");
}
