//! Context assembly for copilot prompts.
//!
//! Builds `<recent_events>` and `<ui_context>` prelude blocks that ground copilot
//! responses in the operator's current UI state, satisfying Northstar §P checks 1,
//! 2, 5, 6 and §C check 9 (`northstar.md:261, :490–:495`).

use std::fmt::Write as _;

use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::UiContext;

/// Server-side hard limit on context events (§P check 2; `northstar.md:491`).
/// Frontend caps at 50; server rejects >100 with structured 400.
const MAX_EVENTS: usize = 100;

/// Per-event payload size hard limit in bytes. Requests exceeding this are
/// rejected with a structured 400 — never silently truncated (§P check 2).
const MAX_EVENT_PAYLOAD_BYTES: usize = 16_384;

/// Threshold above which the frontend tray renders an oversize warning chip
/// (§P check 2 visibility; `northstar.md:491`). Pure informational — does NOT
/// cause a server-side rejection on its own.
pub const OVERSIZE_THRESHOLD_BYTES: usize = 4_096;

/// A single event entry sent by the frontend in a copilot request.
///
/// Mirrors the JSON shape of `GlobalEventEntry` as received over the SSE stream.
/// The `event` field is kept as raw JSON so prompt assembly does not couple to
/// individual `WebEvent` variants. TypeScript counterpart: `RecentEvent`.
#[derive(Debug, Deserialize, Serialize)]
pub struct RecentEventEntry {
    /// Monotonically increasing sequence number (from `GlobalEventStore`).
    pub seq: u64,
    /// ISO 8601 UTC timestamp when the event was pushed to the store.
    pub timestamp: String,
    /// Origin of the event (e.g. `"BuildRunner"`, `"GitForest"`).
    pub source: String,
    /// Full event payload as received from the SSE stream; passed verbatim to
    /// the prompt assembler (no server-side truncation, §P check 2).
    pub event: serde_json::Value,
}

/// Structured error returned by [`validate`], mapping to 400 responses.
///
/// Both variants use structured JSON bodies per Cookbook §28 (HTTP error mapping).
#[derive(Debug)]
pub enum CopilotContextError {
    /// More than [`MAX_EVENTS`] entries in `recent_events`.
    TooManyEvents {
        /// Actual number of events received.
        count: usize,
    },
    /// A single event payload exceeds [`MAX_EVENT_PAYLOAD_BYTES`].
    EventPayloadTooLarge {
        /// Zero-based index of the offending entry.
        index: usize,
        /// Serialized byte length of the payload.
        bytes: usize,
    },
}

impl IntoResponse for CopilotContextError {
    fn into_response(self) -> axum::response::Response {
        let body = match &self {
            Self::TooManyEvents { count } => json!({
                "code": "context_too_many_events",
                "message": format!(
                    "received {count} events; server max {MAX_EVENTS}; cap at 50 in client"
                ),
                "recovery": "send fewer events"
            }),
            Self::EventPayloadTooLarge { index, bytes } => json!({
                "code": "context_event_payload_too_large",
                "message": format!(
                    "event at index {index} is {bytes}B; server max {MAX_EVENT_PAYLOAD_BYTES}B"
                ),
                "recovery": "filter verbose event variants or reduce payload size"
            }),
        };
        (StatusCode::BAD_REQUEST, Json(body)).into_response()
    }
}

/// Validate incoming context fields, returning a structured error on violation.
///
/// Enforces the server-side hard limits (§P check 2; `northstar.md:491`):
/// - rejects `recent_events.len() > 100` (server backstop; frontend caps at 50)
/// - rejects any single serialized event payload exceeding 16 KiB
///
/// Empty or absent `recent_events` always passes.
///
/// # Errors
/// Returns [`CopilotContextError`] if limits are exceeded.
pub fn validate(events: &[RecentEventEntry]) -> Result<(), CopilotContextError> {
    if events.len() > MAX_EVENTS {
        return Err(CopilotContextError::TooManyEvents {
            count: events.len(),
        });
    }
    for (i, entry) in events.iter().enumerate() {
        let bytes = entry.event.to_string().len();
        if bytes > MAX_EVENT_PAYLOAD_BYTES {
            return Err(CopilotContextError::EventPayloadTooLarge { index: i, bytes });
        }
    }
    Ok(())
}

/// Build the XML-style context prelude prepended to the copilot prompt.
///
/// Produces `<recent_events>…</recent_events>` + `<ui_context>…</ui_context>` blocks.
/// Event payloads are embedded verbatim — no server-side truncation (§P check 2).
///
/// Returns an empty string when both `events` is empty and `ui` is `None`.
///
/// # Arguments
/// * `events` — slice of entries (frontend caps at 50; server validated ≤100).
/// * `ui` — optional [`UiContext`] snapshot captured at submit time.
#[must_use]
pub fn assemble_prompt_prelude(events: &[RecentEventEntry], ui: Option<&UiContext>) -> String {
    if events.is_empty() && ui.is_none() {
        return String::new();
    }

    let mut out = String::with_capacity(1024);

    if !events.is_empty() {
        out.push_str("<recent_events>\n");
        for entry in events {
            let _ = write!(
                out,
                "  [{}] seq={} source={}\n  {}\n",
                entry.timestamp, entry.seq, entry.source, entry.event,
            );
        }
        out.push_str("</recent_events>\n");
    }

    if let Some(ctx) = ui {
        out.push_str("<ui_context>\n");
        let _ = writeln!(out, "  route: {}", ctx.route);
        if let Some(sel) = &ctx.selection {
            let _ = writeln!(out, "  selection: {sel}");
        }
        if let Some(view) = &ctx.view {
            let _ = writeln!(out, "  view: {view}");
        }
        if !ctx.degraded.is_empty() {
            let _ = writeln!(out, "  degraded: {}", ctx.degraded.join(", "));
        }
        out.push_str("</ui_context>\n");
    }

    out
}

/// Return the indices of events whose serialized `event` payload exceeds `threshold_bytes`.
///
/// Used by the frontend tray to render oversize warning chips before submit
/// (§P check 2; `northstar.md:491`). Also called server-side to populate
/// AYIN telemetry response tags.
#[must_use]
pub fn oversize_event_indices(events: &[RecentEventEntry], threshold_bytes: usize) -> Vec<usize> {
    events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            if e.event.to_string().len() > threshold_bytes {
                Some(i)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use serde_json::json as sjson;

    use super::*;

    fn entry(seq: u64, payload: serde_json::Value) -> RecentEventEntry {
        RecentEventEntry {
            seq,
            timestamp: "2026-05-19T17:00:00Z".to_owned(),
            source: "BuildRunner".to_owned(),
            event: payload,
        }
    }

    #[test]
    fn context_assembly_empty() {
        assert!(assemble_prompt_prelude(&[], None).is_empty());
    }

    #[test]
    fn context_assembly_events_only() {
        let events = vec![entry(1, sjson!({"type": "BuildStarted"}))];
        let out = assemble_prompt_prelude(&events, None);
        assert!(out.contains("<recent_events>"));
        assert!(out.contains("seq=1"));
        assert!(out.contains("BuildRunner"));
        assert!(!out.contains("<ui_context>"));
    }

    #[test]
    fn context_assembly_full() {
        let events = vec![entry(42, sjson!({"type": "ToolUse", "tool": "Bash"}))];
        let ctx = UiContext {
            route: "/builds/abc".to_owned(),
            selection: Some("build-abc".to_owned()),
            view: Some("activity".to_owned()),
            degraded: vec![],
        };
        let out = assemble_prompt_prelude(&events, Some(&ctx));
        assert!(out.contains("<recent_events>"));
        assert!(out.contains("seq=42"));
        assert!(out.contains("<ui_context>"));
        assert!(out.contains("route: /builds/abc"));
        assert!(out.contains("selection: build-abc"));
        assert!(out.contains("view: activity"));
    }

    #[test]
    fn context_assembly_oversize_rejected() {
        let oversized = "x".repeat(MAX_EVENT_PAYLOAD_BYTES + 1);
        let events = vec![entry(1, sjson!({ "data": oversized }))];
        let err = validate(&events).unwrap_err();
        assert!(matches!(
            err,
            CopilotContextError::EventPayloadTooLarge { index: 0, .. }
        ));
    }

    #[test]
    fn validate_too_many_events() {
        let events: Vec<_> = (0..=MAX_EVENTS)
            .map(|i| entry(i as u64, sjson!({"type": "x"})))
            .collect();
        let err = validate(&events).unwrap_err();
        assert!(matches!(err, CopilotContextError::TooManyEvents { .. }));
    }

    #[test]
    fn oversize_indices_returns_correct_positions() {
        let big = "y".repeat(OVERSIZE_THRESHOLD_BYTES + 10);
        let entries = vec![
            entry(1, sjson!({"x": "small"})),
            entry(2, sjson!({ "data": big })),
            entry(3, sjson!({"z": "also_small"})),
        ];
        let oversized = oversize_event_indices(&entries, OVERSIZE_THRESHOLD_BYTES);
        assert_eq!(oversized, vec![1]);
    }
}
