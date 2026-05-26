//! Wiring confirmation — `call_subprocess_public` emits AYIN spans that form a
//! session-root → turn-start parent chain.
//!
//! Strategy (Canon XXVII §50.3): call the public entry point and verify the
//! observable outcome (span parent_id chain) proves the span-hierarchy code path
//! was exercised.  The subprocess call itself will fail in CI (no real
//! `claude --print` binary), but `emit_session_start_span` and
//! `emit_turn_start_span` fire *before* the spawn, so the parent chain is
//! observable regardless of whether the subprocess succeeds.
//!
//! Two tests:
//! 1. `span_hierarchy_session_root_then_turn` — first call emits session-root
//!    (parent_id None) then turn-start (parent_id = session-root id).
//! 2. `span_hierarchy_second_turn_reuses_session_root` — second call reuses the
//!    session_span_id stored in `CopilotProcess`; no new session-root span.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::doc_markdown,
    unsafe_code
)]

use std::{path::PathBuf, sync::Arc};

use lightarchitects_webshell::{
    config::{AgentSession, ClaudeBackend},
    copilot::{CopilotProcess, TurnSpanContext, call_subprocess_public},
    events::{WebEvent, WebEventV2},
    session::BuildSession,
};
use tokio::sync::Mutex;

fn la_session() -> Arc<BuildSession> {
    Arc::new(BuildSession::new(
        PathBuf::from("/tmp"),
        AgentSession::Lightarchitects(ClaudeBackend::default()),
    ))
}

fn drain_ayin_spans(
    rx: &mut tokio::sync::broadcast::Receiver<WebEventV2>,
) -> Vec<lightarchitects_webshell::events::types::TraceSpanSummary> {
    let mut spans = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        if let WebEvent::AyinSpan(span) = ev.inner {
            spans.push(span);
        }
    }
    spans
}

/// First call emits a session-root span (parent_id None), then a turn-start span
/// whose parent_id equals the session-root id — proving the lineage chain works.
#[tokio::test]
async fn span_hierarchy_session_root_then_turn() {
    let session = la_session();
    let mut rx = session.event_tx.subscribe();
    let proc_lock = Mutex::new(None::<CopilotProcess>);

    // Call will fail (no real claude binary in CI), but spans fire before spawn.
    let _ = call_subprocess_public("hello", &proc_lock, &session).await;

    let spans = drain_ayin_spans(&mut rx);

    let session_root = spans
        .iter()
        .find(|s| s.action == "copilot.session.started")
        .expect("session-root span must be emitted on first call");

    let turn_start = spans
        .iter()
        .find(|s| s.action == "copilot.turn.started")
        .expect("turn-start span must be emitted");

    assert!(
        session_root.parent_id.is_none(),
        "session-root must have no parent — it IS the lineage root, got {:?}",
        session_root.parent_id
    );
    assert_eq!(
        turn_start.parent_id.as_deref(),
        Some(session_root.id.as_str()),
        "turn-start parent_id must equal the session-root id"
    );
}

/// Second call must reuse the session_span_id stored in `CopilotProcess` — no new
/// session-root span, and the turn-start parent matches the first call's root id.
#[tokio::test]
async fn span_hierarchy_second_turn_reuses_session_root() {
    let session = la_session();
    let proc_lock = Mutex::new(None::<CopilotProcess>);

    // Turn 1 — populates session_span_id in proc_lock.
    let _ = call_subprocess_public("turn 1", &proc_lock, &session).await;

    let session_span_id_after_turn1 = proc_lock
        .lock()
        .await
        .as_ref()
        .and_then(|p| p.session_span_id.clone())
        .expect("session_span_id must be stored in CopilotProcess after first call");

    // Turn 2 — subscribe AFTER turn 1 completes so we only see turn-2 spans.
    let mut rx = session.event_tx.subscribe();
    let _ = call_subprocess_public("turn 2", &proc_lock, &session).await;
    let spans = drain_ayin_spans(&mut rx);

    let new_roots: Vec<_> = spans
        .iter()
        .filter(|s| s.action == "copilot.session.started")
        .collect();
    assert!(
        new_roots.is_empty(),
        "turn 2 must NOT emit a new session-root span — session_span_id must be reused, found: {:?}",
        new_roots.iter().map(|s| &s.id).collect::<Vec<_>>()
    );

    let turn_start = spans
        .iter()
        .find(|s| s.action == "copilot.turn.started")
        .expect("turn-2 turn-start span must be emitted");
    assert_eq!(
        turn_start.parent_id.as_deref(),
        Some(session_span_id_after_turn1.as_str()),
        "turn-2 parent_id must equal the session-root id from turn 1"
    );
}

/// `TurnSpanContext` fields are accessible — compilation + type-shape guard.
#[test]
fn turn_span_context_fields_accessible() {
    let ctx = TurnSpanContext {
        session_span_id: "sess-abc123".to_owned(),
        turn_span_id: "turn-xyz789".to_owned(),
    };
    assert_eq!(ctx.session_span_id, "sess-abc123");
    assert_eq!(ctx.turn_span_id, "turn-xyz789");
}

/// `seed_from_session_id` initialises `session_span_id: None` so the first real
/// turn emits a fresh session-root span.
#[test]
fn copilot_process_seed_session_span_id_starts_none() {
    let p = CopilotProcess::seed_from_session_id("my-session".to_owned());
    assert!(
        p.session_span_id.is_none(),
        "seeded CopilotProcess must have session_span_id: None — first call will emit the root"
    );
}
