//! W9.5 — 1M-context stress test (regression anchor).
//!
//! Simulates a 250 000-token-equivalent conversation by emitting 500 synthetic
//! text events of ~500 characters each, with a unique needle at turn 250.
//!
//! Invariants verified:
//!   (A) `SseTransport` emits all events without error.
//!   (B) The needle survives SSE + JSON round-trip verbatim.
//!   (C) `complete` is always the last event in the stream.
//!   (D) No panic or stack overflow under large-volume emission.
//!
//! Requires `--features loops-core` (conversation module is feature-gated).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects::agent::conversation::{
    ConversationEvent, SseTransport, Transport, event::TerminationReason,
};

const TURNS: usize = 500;
const CHARS_PER_TURN: usize = 500;
const NEEDLE_TURN: usize = 250;
const NEEDLE: &str = "NEEDLE_VALUE_42_UNIQUE_MARKER";

/// Emit a synthetic 500-turn conversation and return the raw SSE bytes.
async fn emit_large_conversation() -> Vec<u8> {
    let mut buf = Vec::with_capacity(TURNS * (CHARS_PER_TURN + 64));
    {
        let mut transport = SseTransport::new(&mut buf);

        transport
            .emit(&ConversationEvent::StatusUpdate {
                text: "Starting large-context turn…".into(),
            })
            .await
            .unwrap();

        for i in 0..TURNS {
            let chunk = if i == NEEDLE_TURN {
                format!("Turn {i}: {NEEDLE} embedded here.")
            } else {
                format!("Turn {i}: {}", "x".repeat(CHARS_PER_TURN))
            };
            transport
                .emit(&ConversationEvent::Text { chunk })
                .await
                .unwrap();
        }

        transport
            .emit(&ConversationEvent::TokenUsage {
                input: 250_000,
                output: 500,
            })
            .await
            .unwrap();

        transport
            .emit(&ConversationEvent::Complete {
                reason: TerminationReason::Complete,
            })
            .await
            .unwrap();
        // transport drops here, releasing the mutable borrow on buf
    }
    buf
}

// ── Invariant A: all events emitted without error ────────────────────────────

#[tokio::test]
async fn large_context_emits_all_events_without_error() {
    let buf = emit_large_conversation().await;
    assert!(!buf.is_empty(), "output buffer must be non-empty");

    let raw = std::str::from_utf8(&buf).expect("SSE output must be valid UTF-8");
    // Expected: status_update + TURNS text events + token_usage + complete
    let expected = 1 + TURNS + 1 + 1;
    let actual = raw.matches("event: ").count();
    assert_eq!(
        actual, expected,
        "expected {expected} SSE events, got {actual}"
    );
}

// ── Invariant B: needle survives JSON round-trip ─────────────────────────────

#[tokio::test]
async fn needle_survives_large_context_round_trip() {
    let buf = emit_large_conversation().await;
    let raw = std::str::from_utf8(&buf).unwrap();

    let needle_data_line = raw
        .lines()
        .find(|l| l.starts_with("data: ") && l.contains(NEEDLE))
        .expect("needle must appear in a data: line");

    let json_str = &needle_data_line["data: ".len()..];
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("needle data line must be valid JSON");
    let chunk = parsed["chunk"]
        .as_str()
        .expect("chunk field must be present");
    assert!(
        chunk.contains(NEEDLE),
        "needle must survive JSON round-trip verbatim: {chunk}"
    );
}

// ── Invariant C: complete is the last event ──────────────────────────────────

#[tokio::test]
async fn large_context_complete_is_last_event() {
    let buf = emit_large_conversation().await;
    let raw = std::str::from_utf8(&buf).unwrap();

    let names: Vec<&str> = raw
        .lines()
        .filter_map(|l| l.strip_prefix("event: "))
        .collect();

    assert_eq!(
        names.last().copied(),
        Some("complete"),
        "complete must be the last SSE event; got: {names:?}"
    );
}

// ── Invariant D: token_usage precedes complete ───────────────────────────────

#[tokio::test]
async fn large_context_token_usage_precedes_complete() {
    let buf = emit_large_conversation().await;
    let raw = std::str::from_utf8(&buf).unwrap();

    let names: Vec<&str> = raw
        .lines()
        .filter_map(|l| l.strip_prefix("event: "))
        .collect();

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
        "token_usage ({usage_pos}) must precede complete ({complete_pos})"
    );
}
