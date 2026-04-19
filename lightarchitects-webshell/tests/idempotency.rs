//! Idempotency test suite — Canon XXVII §50.
//!
//! Verifies that pure functions return identical results for identical inputs
//! across multiple calls.  Guards against accidental state contamination or
//! non-deterministic output from serialisation / hashing paths.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects_webshell::events::types::{
    AyinStatus, HelixEntrySummary, HelixEventKind, TraceSpanSummary, WebEvent,
};

// --- Serialisation is deterministic ------------------------------------------

#[test]
fn web_event_span_serialises_identically_twice() {
    let event = WebEvent::AyinSpan(TraceSpanSummary {
        id: "span-abc".to_owned(),
        parent_id: None,
        actor: "corso".to_owned(),
        action: "guard.scan".to_owned(),
        timestamp: "2026-04-13T00:00:00Z".to_owned(),
        duration_ms: 42,
        outcome: serde_json::Value::Null,
        metadata: serde_json::Value::Null,
    });
    let first = serde_json::to_string(&event).unwrap();
    let second = serde_json::to_string(&event).unwrap();
    assert_eq!(first, second, "serialisation must be deterministic");
}

#[test]
fn web_event_helix_entry_serialises_identically_twice() {
    let event = WebEvent::HelixEntry(HelixEntrySummary::minimal(
        "eva/entries/2026-04-13-identity.md".to_owned(),
        HelixEventKind::Created,
    ));
    let first = serde_json::to_string(&event).unwrap();
    let second = serde_json::to_string(&event).unwrap();
    assert_eq!(first, second);
}

// --- Status serialisation is deterministic -----------------------------------

#[test]
fn ayin_status_connected_serialises_identically_twice() {
    let event = WebEvent::AyinStatus(AyinStatus::Connected);
    let first = serde_json::to_string(&event).unwrap();
    let second = serde_json::to_string(&event).unwrap();
    assert_eq!(first, second);
}

#[test]
fn ayin_status_reconnecting_serialises_identically_twice() {
    let event = WebEvent::AyinStatus(AyinStatus::Reconnecting { attempt: 3 });
    let first = serde_json::to_string(&event).unwrap();
    let second = serde_json::to_string(&event).unwrap();
    assert_eq!(first, second);
}

// --- backoff_secs is a pure function -----------------------------------------

#[test]
fn backoff_secs_same_input_same_output() {
    use lightarchitects_webshell::events::ayin_client::backoff_secs;
    // Called twice — must return identical values (no hidden state).
    for attempt in 0u32..=8 {
        assert_eq!(
            backoff_secs(attempt),
            backoff_secs(attempt),
            "backoff_secs({attempt}) must be idempotent",
        );
    }
}
