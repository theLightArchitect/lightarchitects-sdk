//! Lightspace replay byte-equivalence tests.
//!
//! Verifies that replaying the same event sequence through the pure reducer
//! twice produces byte-identical snapshots. This is the snapshot/restore
//! round-trip invariant declared in `lightarchitects-lightspace/src/lib.rs`:
//! "Snapshot and restore round-trip to byte-equivalent state."
//!
//! Three assertion points: after 1 event, after 3 events, after 5 events.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use chrono::Utc;
use lightarchitects_lightspace::{
    CanvasEvent, Lightspace,
    snapshot::Snapshot,
    types::{Actor, CardData, CardKind, CardState, CardTransition, Provenance, UpdateMode},
};
use uuid::Uuid;

/// Build a minimal `Provenance` for test cards.
fn test_provenance() -> Provenance {
    Provenance {
        agent: "test-agent".to_owned(),
        source_uri: "helix://test".to_owned(),
        span_id: None,
        ts: Utc::now(),
    }
}

/// Build a `CanvasEvent::Card` event for the given index.
fn card_event(idx: u32) -> CanvasEvent {
    CanvasEvent::Card(CardData {
        id: format!("card-{idx:04}"),
        kind: CardKind::Artifact,
        title: format!("Test card {idx}"),
        content: serde_json::json!({ "index": idx }),
        provenance: test_provenance(),
        state: CardState::Attached,
        attribution: None,
    })
}

/// Build a `CanvasEvent::Update` event for card at `idx`.
fn update_event(idx: u32, seq: u64) -> CanvasEvent {
    CanvasEvent::Update {
        card_id: format!("card-{idx:04}"),
        seq,
        mode: UpdateMode::Replace,
        path: None,
        payload: serde_json::json!({ "index": idx, "updated": true }),
    }
}

/// Build a `CanvasEvent::Lifecycle` event that detaches card at `idx`.
///
/// Cards are born `Attached` via `CanvasEvent::Card`, so `Detach` is the only
/// valid transition from the initial state — `Attach` would be rejected with
/// `IllegalTransition("card is already Attached")`.
fn lifecycle_event(idx: u32) -> CanvasEvent {
    CanvasEvent::Lifecycle {
        card_id: format!("card-{idx:04}"),
        transition: CardTransition::Detach,
        actor: Actor::Operator, // WHY: only Operator is authorised to Detach (Copilot cannot)
        ghost: false,
        attribution: None,
    }
}

/// The canonical 5-event sequence used for all replay tests.
///
/// Event layout:
///   0 — Card(card-0000)
///   1 — Card(card-0001)
///   2 — Update(card-0000, seq=1)
///   3 — Card(card-0002)
///   4 — Lifecycle attach card-0002
fn five_event_sequence() -> Vec<CanvasEvent> {
    vec![
        card_event(0),
        card_event(1),
        update_event(0, 1),
        card_event(2),
        lifecycle_event(2),
    ]
}

/// Apply a prefix of `events` to a fresh `Lightspace` and return the snapshot bytes.
fn apply_and_snapshot(session_id: Uuid, events: &[CanvasEvent], count: usize) -> Vec<u8> {
    let mut ls = Lightspace::new(session_id);
    for event in events.iter().take(count) {
        ls = ls.reduce(event.clone()).expect("reduce failed");
    }
    Snapshot::capture(&ls.state)
        .to_bytes()
        .expect("snapshot serialization failed")
}

/// Replay identical to `apply_and_snapshot` — a second independent traversal.
fn replay_and_snapshot(session_id: Uuid, events: &[CanvasEvent], count: usize) -> Vec<u8> {
    let mut ls = Lightspace::new(session_id);
    for event in events.iter().take(count) {
        ls = ls.reduce(event.clone()).expect("reduce failed on replay");
    }
    Snapshot::capture(&ls.state)
        .to_bytes()
        .expect("snapshot serialization failed on replay")
}

/// After 1 event: two independent traversals produce byte-identical snapshots.
#[test]
fn replay_byte_equivalent_after_1_event() {
    let session_id = Uuid::new_v4();
    let events = five_event_sequence();

    let snap_a = apply_and_snapshot(session_id, &events, 1);
    let snap_b = replay_and_snapshot(session_id, &events, 1);

    assert_eq!(
        snap_a, snap_b,
        "snapshots diverged after 1 event — reducer is non-deterministic"
    );
}

/// After 3 events: two independent traversals produce byte-identical snapshots.
#[test]
fn replay_byte_equivalent_after_3_events() {
    let session_id = Uuid::new_v4();
    let events = five_event_sequence();

    let snap_a = apply_and_snapshot(session_id, &events, 3);
    let snap_b = replay_and_snapshot(session_id, &events, 3);

    assert_eq!(
        snap_a, snap_b,
        "snapshots diverged after 3 events — reducer is non-deterministic"
    );
}

/// After 5 events: two independent traversals produce byte-identical snapshots.
#[test]
fn replay_byte_equivalent_after_5_events() {
    let session_id = Uuid::new_v4();
    let events = five_event_sequence();

    let snap_a = apply_and_snapshot(session_id, &events, 5);
    let snap_b = replay_and_snapshot(session_id, &events, 5);

    assert_eq!(
        snap_a, snap_b,
        "snapshots diverged after 5 events — reducer is non-deterministic"
    );
}

/// Restore from a captured snapshot and verify the re-captured snapshot is byte-identical.
///
/// This exercises the `Lightspace::restore` path: capture → restore → re-capture
/// must be idempotent.
#[test]
fn restore_then_recapture_is_idempotent() {
    let session_id = Uuid::new_v4();
    let events = five_event_sequence();

    // First pass: apply all 5 events and capture.
    let mut ls = Lightspace::new(session_id);
    for event in &events {
        ls = ls.reduce(event.clone()).expect("reduce failed");
    }
    let snap1 = Snapshot::capture(&ls.state);
    let bytes1 = snap1.to_bytes().expect("serialize snap1");

    // Second pass: restore from snapshot and immediately re-capture.
    let snap2 = Snapshot::from_bytes(&bytes1).expect("deserialize snap1");
    let ls2 = Lightspace::restore(snap2);
    let bytes2 = Snapshot::capture(&ls2.state)
        .to_bytes()
        .expect("serialize snap2");

    assert_eq!(
        bytes1, bytes2,
        "restore → re-capture is not idempotent — snapshot round-trip broken"
    );
}
