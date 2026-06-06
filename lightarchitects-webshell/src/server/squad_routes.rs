//! Squad A2A (agent-to-agent) link SSE stream (cockpit d0 — Constellation).
//!
//! Per-build fleet endpoints (`§2.23` + `§2.24`) deliver per-build agent
//! activity. The platform-d0 Squad Constellation card needs **A2A
//! link edges** across the whole platform — i.e. *which sibling pair
//! exchanged a message in the last N seconds*.
//!
//! This SSE stream synthesises A2A edges from the existing global event
//! bus (`state.event_tx`): every `WebEventV2` carries a `topic` field
//! whose `v1.<domain>.<verb>` prefix identifies the originating sibling.
//! Two events within a short window from different siblings on a
//! correlated `build_id` form a link.
//!
//! ## Route
//!
//! `GET (SSE) /api/squad/a2a`
//!
//! ## Auth
//!
//! `AuthGuard` (cookie or bearer); `EventSource` clients use the cookie.
//!
//! ## Stream shape
//!
//! - First event: `event: snapshot` — synthetic empty initial state.
//!   (Real correlation begins as subsequent events flow.)
//! - Per-event: `event: link` — `{ from, to, kind, build_id?, ts }`.
//! - `event: lag` — `{ skipped: N }` on receiver overflow.
//! - Keepalive: `data: keep-alive` every 30 s.
//!
//! ## Correlation window
//!
//! [`A2A_CORRELATION_MS`] (default 6 000 ms) — events within this window
//! that share a `build_id` are correlated into a link edge. The window
//! is short enough that a casual operator sees fresh links pulse and
//! long enough to bridge SSE jitter.

use std::convert::Infallible;

use axum::{
    extract::State,
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use chrono::Utc;
use futures_util::stream;
use serde::Serialize;
use tokio::{sync::broadcast, time::Duration};

use crate::{auth, events::WebEventV2, server::AppState};

/// Correlation window in milliseconds — events from different siblings
/// on the same `build_id` within this window form an A2A link edge.
pub const A2A_CORRELATION_MS: u64 = 6_000;

/// Canonical sibling identifiers used in link `from` / `to` fields.
const SIBLINGS: [&str; 7] = ["CORSO", "EVA", "SOUL", "QUANTUM", "SERAPH", "AYIN", "LÆX"];

/// A2A link edge — emitted as `event: link`.
#[derive(Debug, Serialize)]
pub struct A2aLink {
    /// Source sibling (canonical codename).
    pub from: &'static str,
    /// Destination sibling (canonical codename).
    pub to: &'static str,
    /// Edge kind (`enrich`, `context`, `canon`, `span`, `dispatch`, `unknown`).
    pub kind: &'static str,
    /// Build UUID correlating the two events, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
    /// ISO-8601 UTC timestamp of the second event in the correlation.
    pub ts: String,
}

/// Initial empty snapshot — emitted as `event: snapshot`.
#[derive(Debug, Serialize)]
struct InitialSnapshot {
    /// Canonical sibling roster.
    siblings: &'static [&'static str; 7],
    /// ISO-8601 capture timestamp.
    captured_at: String,
}

/// `GET /api/squad/a2a` — SSE stream of A2A link edges.
pub async fn squad_a2a_sse_handler(_: auth::AuthGuard, State(state): State<AppState>) -> Response {
    let rx = state.event_tx.subscribe();

    let event_stream = stream::unfold(StreamState::initial(rx), drive_squad_stream);

    Sse::new(event_stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(30))
                .text("keep-alive"),
        )
        .into_response()
}

/// Per-stream state: receiver + last-event memory + pending snapshot.
struct StreamState {
    rx: broadcast::Receiver<WebEventV2>,
    last: Option<EventMemo>,
    pending_snap: bool,
}

impl StreamState {
    fn initial(rx: broadcast::Receiver<WebEventV2>) -> Self {
        Self {
            rx,
            last: None,
            pending_snap: true,
        }
    }
}

/// Small memo of a recent event — used for the correlation window.
struct EventMemo {
    sibling: &'static str,
    build_id: Option<String>,
    kind: &'static str,
    ts_ms: u64,
}

/// SSE state machine — emits snapshot first, then correlated link edges.
async fn drive_squad_stream(
    mut state: StreamState,
) -> Option<(Result<Event, Infallible>, StreamState)> {
    // 1. First call: emit the empty snapshot.
    if state.pending_snap {
        state.pending_snap = false;
        let snap = InitialSnapshot {
            siblings: &SIBLINGS,
            captured_at: Utc::now().to_rfc3339(),
        };
        let json = serde_json::to_string(&snap).unwrap_or_else(|_| "{}".to_owned());
        return Some((Ok(Event::default().event("snapshot").data(json)), state));
    }

    // 2. Pull next event from the global bus.
    loop {
        match state.rx.recv().await {
            Ok(ev) => {
                if let Some(memo) = classify(&ev) {
                    if let Some(link) = correlate(state.last.as_ref(), &memo) {
                        let json = serde_json::to_string(&link).unwrap_or_else(|_| "{}".to_owned());
                        state.last = Some(memo);
                        return Some((Ok(Event::default().event("link").data(json)), state));
                    }
                    state.last = Some(memo);
                }
                // No link → keep draining; do NOT block on a single event.
            }
            Err(broadcast::error::RecvError::Closed) => return None,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                let lag = Event::default()
                    .event("lag")
                    .data(format!("{{\"type\":\"lag\",\"skipped\":{n}}}"));
                return Some((Ok(lag), state));
            }
        }
    }
}

/// Map a `WebEventV2` to a small memo if it has a recognisable sibling.
/// Returns `None` for events that can't be attributed.
fn classify(ev: &WebEventV2) -> Option<EventMemo> {
    let topic = topic_of(ev)?;
    let (sibling, kind) = sibling_and_kind(topic)?;
    let build_id = build_id_of(ev);
    let ts_ms = u64::try_from(Utc::now().timestamp_millis()).unwrap_or(0);
    Some(EventMemo {
        sibling,
        build_id,
        kind,
        ts_ms,
    })
}

/// Build an A2A link edge if two events on the same build but different
/// siblings fall within `A2A_CORRELATION_MS`.
fn correlate(last: Option<&EventMemo>, curr: &EventMemo) -> Option<A2aLink> {
    let prev = last?;
    if prev.sibling == curr.sibling {
        return None;
    }
    if curr.ts_ms.saturating_sub(prev.ts_ms) > A2A_CORRELATION_MS {
        return None;
    }
    if prev.build_id != curr.build_id {
        return None;
    }
    Some(A2aLink {
        from: prev.sibling,
        to: curr.sibling,
        kind: curr.kind,
        build_id: curr.build_id.clone(),
        ts: Utc::now().to_rfc3339(),
    })
}

/// Extract the `topic` field from a `WebEventV2` via JSON round-trip.
/// (Avoids leaking the `WebEventV2` variant surface into this module.)
fn topic_of(ev: &WebEventV2) -> Option<&'static str> {
    let raw = serde_json::to_value(ev).ok()?;
    let topic = raw.get("topic")?.as_str()?;
    static_topic(topic)
}

/// Map a runtime topic string to a `&'static str` from the known set.
fn static_topic(topic: &str) -> Option<&'static str> {
    const KNOWN: &[&str] = &[
        "v1.build.update",
        "v1.build.supervisor.update",
        "v1.conductor.escalation",
        "v1.conductor.tick",
        "v1.helix.entry.changed",
        "v1.helix.entry.promoted",
        "v1.worktree.update",
        "v1.agent.ayin.connected",
        "v1.agent.ayin.disconnected",
        "v1.agent.ayin.reconnecting",
        "v1.agent.claude.activity",
    ];
    KNOWN.iter().copied().find(|k| *k == topic)
}

/// Map a topic to `(sibling_codename, link_kind)` per the
/// canonical event-bus → sibling table.
fn sibling_and_kind(topic: &'static str) -> Option<(&'static str, &'static str)> {
    Some(match topic {
        "v1.helix.entry.changed" | "v1.helix.entry.promoted" => ("SOUL", "enrich"),
        "v1.build.update" | "v1.build.supervisor.update" | "v1.worktree.update" => {
            ("CORSO", "context")
        }
        "v1.conductor.escalation" | "v1.conductor.tick" => ("CORSO", "dispatch"),
        "v1.agent.ayin.connected" | "v1.agent.ayin.disconnected" | "v1.agent.ayin.reconnecting" => {
            ("AYIN", "span")
        }
        "v1.agent.claude.activity" => ("EVA", "context"),
        _ => return None,
    })
}

/// Extract `build_id` field from a `WebEventV2` via JSON round-trip.
fn build_id_of(ev: &WebEventV2) -> Option<String> {
    let raw = serde_json::to_value(ev).ok()?;
    raw.get("build_id")?
        .as_str()
        .map(std::borrow::ToOwned::to_owned)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn memo(
        sibling: &'static str,
        build: Option<&str>,
        kind: &'static str,
        ts_ms: u64,
    ) -> EventMemo {
        EventMemo {
            sibling,
            build_id: build.map(std::borrow::ToOwned::to_owned),
            kind,
            ts_ms,
        }
    }

    #[test]
    fn correlation_links_two_siblings_in_window() {
        let prev = memo("SOUL", Some("b1"), "enrich", 1_000);
        let curr = memo("CORSO", Some("b1"), "context", 3_000);
        let link = correlate(Some(&prev), &curr).expect("link expected");
        assert_eq!(link.from, "SOUL");
        assert_eq!(link.to, "CORSO");
        assert_eq!(link.kind, "context");
    }

    #[test]
    fn correlation_rejects_same_sibling() {
        let prev = memo("SOUL", Some("b1"), "enrich", 1_000);
        let curr = memo("SOUL", Some("b1"), "enrich", 2_000);
        assert!(correlate(Some(&prev), &curr).is_none());
    }

    #[test]
    fn correlation_rejects_window_breach() {
        let prev = memo("SOUL", Some("b1"), "enrich", 1_000);
        let curr = memo(
            "CORSO",
            Some("b1"),
            "context",
            1_000 + A2A_CORRELATION_MS + 1,
        );
        assert!(correlate(Some(&prev), &curr).is_none());
    }

    #[test]
    fn correlation_rejects_mismatched_builds() {
        let prev = memo("SOUL", Some("b1"), "enrich", 1_000);
        let curr = memo("CORSO", Some("b2"), "context", 2_000);
        assert!(correlate(Some(&prev), &curr).is_none());
    }

    #[test]
    fn static_topic_only_returns_known() {
        assert!(static_topic("v1.build.update").is_some());
        assert!(static_topic("v1.nonexistent.xx").is_none());
    }

    #[test]
    fn sibling_and_kind_handles_all_known_topics() {
        let topics = [
            "v1.build.update",
            "v1.conductor.escalation",
            "v1.helix.entry.changed",
            "v1.agent.ayin.connected",
            "v1.agent.claude.activity",
        ];
        for t in topics {
            let st = static_topic(t).expect("static topic");
            assert!(sibling_and_kind(st).is_some(), "topic {t} unmapped");
        }
    }
}
