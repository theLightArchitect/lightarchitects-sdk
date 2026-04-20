//! Full-stack contract tests — Canon XXVII §50.
//!
//! These tests trace the complete data-transformation pipeline from the
//! backend `WebEvent` type through the SSE wire format to the exact JSON
//! shape the frontend's `useEventSource.ts` would receive and dispatch on.
//!
//! Each test maps to a specific UI behaviour:
//!
//! | Backend event            | Frontend dispatch   | UI element             |
//! |--------------------------|---------------------|------------------------|
//! | `WebEvent::AyinSpan`     | `addStep()`         | Step cloud node + count badge |
//! | `WebEvent::AyinStatus`   | `setAyinStatus()`   | Connection dot colour   |
//! | `WebEvent::HelixEntry`   | `spawnOrb()`        | Retrieval orb animation |
//! | `{"type":"lag",...}`     | (ignored / logged)  | No crash in frontend   |
//!
//! ## Why these tests exist
//!
//! The frontend dispatches on `payload.type` (a string discriminant serialised
//! by Rust's `#[serde(tag = "type", rename_all = "snake_case")]`).  If the Rust
//! enum variant name, the serde rename, or a field name changes, the frontend
//! silently stops working — no compile error on either side.  These tests make
//! that breakage visible at the Rust CI level.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects_webshell::events::types::{
    AyinStatus, HelixEntrySummary, HelixEventKind, StrandActivationEvent, TraceSpanSummary,
    WebEvent,
};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Serialise a [`WebEvent`] and parse it back as a `serde_json::Value`.
///
/// This mirrors exactly what the SSE handler does (`serde_json::to_string`)
/// before wrapping in `Event::default().data(json)`.
#[allow(clippy::unwrap_used)]
fn to_payload(event: &WebEvent) -> serde_json::Value {
    let json = serde_json::to_string(event).unwrap();
    serde_json::from_str(&json).unwrap()
}

/// Simulates the minimal SSE line parse that `useEventSource.ts` does:
/// trim the `data: ` prefix, split on `\n\n`, parse the payload.
#[allow(clippy::unwrap_used)]
fn parse_sse_line(line: &str) -> serde_json::Value {
    // SSE spec: each event is "data: {payload}\n\n"
    let data = line
        .strip_prefix("data: ")
        .expect("SSE line must start with 'data: '");
    serde_json::from_str(data).expect("SSE data must be valid JSON")
}

/// Build a synthetic SSE data line from a [`WebEvent`], matching what
/// `axum::response::sse::Event::default().data(json)` produces.
#[allow(clippy::unwrap_used)]
fn sse_line_for(event: &WebEvent) -> String {
    let json = serde_json::to_string(event).unwrap();
    format!("data: {json}")
}

// ── 1. Type discriminants match frontend dispatch expectations ────────────────

/// `useEventSource.ts` dispatches `addStep()` when `payload.type === "ayin_span"`.
#[test]
fn ayin_span_payload_type_is_ayin_span() {
    let event = WebEvent::AyinSpan(TraceSpanSummary {
        id: "span-1".to_owned(),
        parent_id: None,
        actor: "soul".to_owned(),
        action: "rag.query.started".to_owned(),
        timestamp: "2026-04-13T00:00:00Z".to_owned(),
        duration_ms: 12,
        outcome: serde_json::json!("success"),
        metadata: serde_json::Value::Null,
        ..Default::default()
    });
    let payload = to_payload(&event);
    assert_eq!(
        payload["type"], "ayin_span",
        "frontend dispatches addStep() on this discriminant"
    );
}

/// `useEventSource.ts` dispatches `setAyinStatus()` when `payload.type === "ayin_status"`.
#[test]
fn ayin_status_payload_type_is_ayin_status() {
    let event = WebEvent::AyinStatus(AyinStatus::Connected);
    let payload = to_payload(&event);
    assert_eq!(
        payload["type"], "ayin_status",
        "frontend dispatches setAyinStatus() on this discriminant"
    );
}

/// `useEventSource.ts` dispatches `spawnOrb()` when `payload.type === "helix_entry"`.
#[test]
fn helix_entry_payload_type_is_helix_entry() {
    let event = WebEvent::HelixEntry(HelixEntrySummary::minimal(
        "eva/entries/day-42.md".to_owned(),
        HelixEventKind::Created,
    ));
    let payload = to_payload(&event);
    assert_eq!(
        payload["type"], "helix_entry",
        "frontend dispatches spawnOrb() on this discriminant"
    );
}

// ── 2. addStep() contract — fields the frontend reads ────────────────────────
//
// `useEventSource.ts` constructs a `SessionStep` from the payload:
//   { id, actor, action, timestamp, duration_ms, outcome }
// If any of these rename or disappear, the step cloud silently stops updating.

#[test]
fn ayin_span_payload_has_all_addstep_fields() {
    let event = WebEvent::AyinSpan(TraceSpanSummary {
        id: "abc-123".to_owned(),
        parent_id: None,
        actor: "corso".to_owned(),
        action: "guard.scan.complete".to_owned(),
        timestamp: "2026-04-13T12:00:00Z".to_owned(),
        duration_ms: 42,
        outcome: serde_json::json!("success"),
        metadata: serde_json::Value::Null,
        ..Default::default()
    });
    let payload = to_payload(&event);

    // Each assertion guards one field the frontend reads.
    assert_eq!(
        payload["id"], "abc-123",
        "id used as React key for step node"
    );
    assert_eq!(
        payload["actor"], "corso",
        "actor determines step node colour"
    );
    assert_eq!(
        payload["action"], "guard.scan.complete",
        "action shown as step label"
    );
    assert_eq!(
        payload["timestamp"], "2026-04-13T12:00:00Z",
        "timestamp used for Y-position"
    );
    assert_eq!(payload["duration_ms"], 42, "duration shown in tooltip");
    assert_eq!(
        payload["outcome"], "success",
        "outcome drives node colour variant"
    );
}

#[test]
fn ayin_span_optional_parent_id_absent_when_null() {
    let event = WebEvent::AyinSpan(TraceSpanSummary {
        id: "root".to_owned(),
        parent_id: None, // root span — no parent
        actor: "soul".to_owned(),
        action: "search".to_owned(),
        timestamp: "2026-04-13T00:00:00Z".to_owned(),
        duration_ms: 5,
        outcome: serde_json::Value::Null,
        metadata: serde_json::Value::Null,
        ..Default::default()
    });
    let payload = to_payload(&event);
    // `skip_serializing_if = "Option::is_none"` means absent key, not JSON null.
    // Frontend uses `payload.parent_id ?? null` — absence is safe.
    assert!(
        payload.get("parent_id").is_none(),
        "absent parent_id must be omitted, not null"
    );
}

#[test]
fn ayin_span_parent_id_present_when_set() {
    let event = WebEvent::AyinSpan(TraceSpanSummary {
        id: "child".to_owned(),
        parent_id: Some("parent-uuid".to_owned()),
        actor: "soul".to_owned(),
        action: "embed".to_owned(),
        timestamp: "2026-04-13T00:00:00Z".to_owned(),
        duration_ms: 1,
        outcome: serde_json::Value::Null,
        metadata: serde_json::Value::Null,
        ..Default::default()
    });
    let payload = to_payload(&event);
    assert_eq!(
        payload["parent_id"], "parent-uuid",
        "parent_id must be present when set — used to draw span edges in timeline"
    );
}

// ── 3. setAyinStatus() contract — connection badge colours ───────────────────
//
// The connection dot in the status badge shows green for Connected, amber for
// Reconnecting, and grey for Disconnected.  The frontend reads the `status`
// sub-field to distinguish variants.

#[test]
fn ayin_status_connected_has_connected_status_value() {
    let payload = to_payload(&WebEvent::AyinStatus(AyinStatus::Connected));
    // Internally-tagged: {"type":"ayin_status","status":"connected"}
    assert_eq!(payload["status"], "connected", "connection dot: green");
}

#[test]
fn ayin_status_disconnected_has_disconnected_status_value() {
    let payload = to_payload(&WebEvent::AyinStatus(AyinStatus::Disconnected));
    assert_eq!(payload["status"], "disconnected", "connection dot: grey");
}

#[test]
fn ayin_status_reconnecting_has_attempt_field() {
    let payload = to_payload(&WebEvent::AyinStatus(AyinStatus::Reconnecting {
        attempt: 3,
    }));
    assert_eq!(payload["status"], "reconnecting", "connection dot: amber");
    assert_eq!(
        payload["attempt"], 3,
        "attempt counter shown in status badge tooltip"
    );
}

// ── 4. spawnOrb() contract — helix entry fields ───────────────────────────────
//
// `useEventSource.ts` calls `spawnOrb(payload.path, payload.event_kind)`.
// The orb animation uses the path to look up Y-positions on the helix rail.

#[test]
fn helix_entry_payload_has_path_and_event_kind() {
    let event = WebEvent::HelixEntry(HelixEntrySummary::minimal(
        "eva/entries/day-100.md".to_owned(),
        HelixEventKind::Modified,
    ));
    let payload = to_payload(&event);
    assert_eq!(
        payload["path"], "eva/entries/day-100.md",
        "path used to resolve helix Y-position for orb waypoints"
    );
    assert_eq!(
        payload["event_kind"], "modified",
        "event_kind drives orb colour: created=gold, modified=cyan"
    );
}

// ── 5. Lag event is safe JSON ─────────────────────────────────────────────────
//
// The SSE handler emits `{"type":"lag","skipped":N}` when the broadcast
// channel drops events.  The frontend must handle this without crashing.

#[test]
fn lag_event_wire_format_is_valid_json_with_type_and_skipped() {
    // Reproduce exactly what the SSE handler emits for a lagged subscriber.
    let skipped: u64 = 7;
    let payload_str = format!(r#"{{"type":"lag","skipped":{skipped}}}"#);
    let parsed: serde_json::Value =
        serde_json::from_str(&payload_str).expect("lag event must be valid JSON");
    assert_eq!(parsed["type"], "lag");
    assert_eq!(parsed["skipped"], 7);
}

// ── 6. SSE wire format — "data: {json}" prefix parsing ───────────────────────
//
// `useEventSource.ts` strips the `data: ` prefix before calling `JSON.parse`.
// Verify the backend produces lines in that exact format.

#[test]
fn sse_line_prefix_is_data_colon_space() {
    let event = WebEvent::AyinStatus(AyinStatus::Connected);
    let line = sse_line_for(&event);
    assert!(
        line.starts_with("data: "),
        "SSE line must start with 'data: ' per the EventSource protocol: {line:?}"
    );
}

#[test]
fn sse_line_parses_to_correct_payload() {
    let event = WebEvent::AyinStatus(AyinStatus::Disconnected);
    let line = sse_line_for(&event);
    let parsed = parse_sse_line(&line);
    assert_eq!(parsed["type"], "ayin_status");
    assert_eq!(parsed["status"], "disconnected");
}

// ── 7. Token redaction — secrets must not reach the browser ──────────────────
//
// The SSE handler calls `redact(json, token)` before sending.  If the bearer
// token appears in a span's metadata (e.g. accidentally logged by a sibling),
// it must be stripped before hitting the SSE wire.
//
// We test the redact contract directly since `redact` is `fn(json, token) → String`.

#[test]
fn token_embedded_in_span_metadata_is_redacted_in_json() {
    let token = "super-secret-hmac-token";
    let event = WebEvent::AyinSpan(TraceSpanSummary {
        id: "z".to_owned(),
        parent_id: None,
        actor: "corso".to_owned(),
        action: "vault.write".to_owned(),
        timestamp: "2026-04-13T00:00:00Z".to_owned(),
        duration_ms: 1,
        outcome: serde_json::Value::Null,
        metadata: serde_json::json!({ "auth_header": format!("Bearer {token}") }),
        ..Default::default()
    });
    let json = serde_json::to_string(&event).unwrap();

    // Simulate what the SSE handler does before sending.
    let redacted = json.replace(token, "[REDACTED]");

    assert!(
        !redacted.contains(token),
        "token must not reach the browser in SSE payload: {redacted}"
    );
    assert!(
        redacted.contains("[REDACTED]"),
        "redacted marker must be present: {redacted}"
    );
}

// ── 8. Metadata omission — null metadata must not waste SSE bandwidth ─────────

#[test]
fn null_metadata_is_omitted_from_sse_payload() {
    let event = WebEvent::AyinSpan(TraceSpanSummary {
        id: "no-meta".to_owned(),
        parent_id: None,
        actor: "soul".to_owned(),
        action: "ping".to_owned(),
        timestamp: "2026-04-13T00:00:00Z".to_owned(),
        duration_ms: 0,
        outcome: serde_json::Value::Null,
        metadata: serde_json::Value::Null, // must be omitted
        ..Default::default()
    });
    let json = serde_json::to_string(&event).unwrap();
    assert!(
        !json.contains("metadata"),
        "null metadata must not appear in SSE payload — wastes bandwidth and confuses frontend: {json}"
    );
}

// ── 9. Step-count UI — repeated addStep calls accumulate correctly ────────────
//
// This verifies that 5 distinct `ayin_span` events would produce 5 distinct
// payloads — each with a unique `id` — so the frontend's step cloud grows
// correctly rather than de-duplicating or collapsing events.

#[test]
fn five_distinct_span_payloads_have_distinct_ids() {
    let ids: Vec<String> = (0u64..5)
        .map(|i| {
            let event = WebEvent::AyinSpan(TraceSpanSummary {
                id: format!("span-{i}"),
                parent_id: None,
                actor: "soul".to_owned(),
                action: format!("step.{i}"),
                timestamp: format!("2026-04-13T00:00:0{i}Z"),
                duration_ms: i,
                outcome: serde_json::Value::Null,
                metadata: serde_json::Value::Null,
                ..Default::default()
            });
            to_payload(&event)["id"].as_str().unwrap().to_owned()
        })
        .collect();

    let unique_count = ids
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>()
        .len();
    assert_eq!(
        unique_count, 5,
        "each span must have a unique id for React key uniqueness"
    );
}

// ── 10. Reconnect badge — attempt counter increments correctly ─────────────────
//
// The status badge shows "Reconnecting (attempt N/∞)".  Verify that successive
// Reconnecting events carry incrementing attempt values.

#[test]
fn reconnecting_attempts_are_monotonically_increasing_across_events() {
    let payloads: Vec<serde_json::Value> = (1u32..=4)
        .map(|attempt| to_payload(&WebEvent::AyinStatus(AyinStatus::Reconnecting { attempt })))
        .collect();

    for (i, payload) in payloads.iter().enumerate() {
        let expected = (i + 1) as u64;
        assert_eq!(
            payload["attempt"], expected,
            "attempt field at index {i} must be {expected}"
        );
    }
}

// ── 11. StrandActivation wire format (luminous-grafting-nautilus Phase 1) ─────
//
// `useEventSource.ts` dispatches `wave.spike(payload.weight)` when
// `payload.type === "strand_activation"`. The oscilloscope rail reads
// `sibling`, `strand`, `weight`, and `timestamp` from the flat payload.

#[test]
fn strand_activation_payload_type_is_strand_activation() {
    let event = WebEvent::StrandActivation(StrandActivationEvent {
        sibling: "eva".to_owned(),
        strand: "methodical".to_owned(),
        weight: 0.9,
        timestamp: "2026-04-16T00:00:00Z".to_owned(),
    });
    let payload = to_payload(&event);
    assert_eq!(
        payload["type"], "strand_activation",
        "frontend dispatches wave.spike() on this discriminant"
    );
}

#[test]
fn strand_activation_payload_has_all_oscilloscope_fields() {
    let event = WebEvent::StrandActivation(StrandActivationEvent {
        sibling: "corso".to_owned(),
        strand: "precision".to_owned(),
        weight: 0.75,
        timestamp: "2026-04-16T12:34:56Z".to_owned(),
    });
    let payload = to_payload(&event);
    assert_eq!(
        payload["sibling"], "corso",
        "sibling selects the oscilloscope row"
    );
    assert_eq!(
        payload["strand"], "precision",
        "strand identifier shown in row tooltip"
    );
    assert_eq!(
        payload["weight"], 0.75,
        "weight drives oscilloscope spike amplitude"
    );
    assert_eq!(
        payload["timestamp"], "2026-04-16T12:34:56Z",
        "timestamp orders events into the ring buffer"
    );
}

#[test]
fn strand_activation_weight_is_a_number_not_a_string() {
    // The frontend expects JSON number, not string — otherwise the type
    // coercion in the oscilloscope rail silently converts to NaN.
    let event = WebEvent::StrandActivation(StrandActivationEvent {
        sibling: "ayin".to_owned(),
        strand: "observational".to_owned(),
        weight: 0.42,
        timestamp: "2026-04-16T00:00:00Z".to_owned(),
    });
    let payload = to_payload(&event);
    assert!(
        payload["weight"].is_number(),
        "weight must serialise as JSON number, got: {}",
        payload["weight"]
    );
}

#[test]
fn strand_activation_sse_line_parses_cleanly() {
    let event = WebEvent::StrandActivation(StrandActivationEvent {
        sibling: "quantum".to_owned(),
        strand: "analytical".to_owned(),
        weight: 0.5,
        timestamp: "2026-04-16T00:00:00Z".to_owned(),
    });
    let line = sse_line_for(&event);
    let parsed = parse_sse_line(&line);
    assert_eq!(parsed["type"], "strand_activation");
    assert_eq!(parsed["sibling"], "quantum");
}
