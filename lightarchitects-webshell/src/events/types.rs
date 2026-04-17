//! Internal event types for the server-sent event fan-out.
//!
//! All types here implement [`serde::Serialize`] so they can be forwarded
//! verbatim as `data:` payloads on the SSE stream the browser subscribes
//! to via `GET /api/events` (Phase 5).

use serde::{Deserialize, Serialize};

/// Broadcast event emitted by the webshell backend.
///
/// Every variant maps to a distinct browser-visible SSE `data:` payload.
/// The `"type"` discriminant is serialized first so the frontend can
/// dispatch on it without parsing the full body.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebEvent {
    /// A trace span received from the AYIN SSE endpoint.
    AyinSpan(TraceSpanSummary),
    /// AYIN connection lifecycle notification.
    AyinStatus(AyinStatus),
    /// A vault Markdown entry was created or modified (filesystem watcher).
    ///
    /// Emitted by the helix watcher as a fallback when AYIN is unavailable,
    /// or to supplement AYIN spans with raw filesystem signals.
    HelixEntry(HelixEntrySummary),
    /// A build tracking file was created or modified in corso/builds/.
    ///
    /// Emitted by the helix watcher when `active.yaml`, `portfolio.md`,
    /// or `roadmap.html` changes. The frontend should refetch `/api/builds`
    /// to get the latest build data.
    BuildUpdate(BuildUpdateEvent),
    /// A control command from an external process (e.g. Claude Code)
    /// forwarded to the browser for UI state mutation.
    Control(ControlCommand),
}

/// Describes a new or modified helix vault entry detected by the filesystem watcher.
#[derive(Debug, Clone, Serialize)]
pub struct HelixEntrySummary {
    /// Path relative to the helix root (e.g. `"eva/entries/day-42.md"`).
    pub path: String,
    /// What triggered this event.
    pub event_kind: HelixEventKind,
}

/// Filesystem event kind that produced a [`HelixEntrySummary`].
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HelixEventKind {
    /// A new vault entry file was created.
    Created,
    /// An existing vault entry file was modified.
    Modified,
}

/// Describes a build tracking file change detected in the `corso/builds/` directory.
#[derive(Debug, Clone, Serialize)]
pub struct BuildUpdateEvent {
    /// Path relative to the helix root (e.g. `"corso/builds/active.yaml"`).
    pub path: String,
    /// What triggered this event.
    pub event_kind: BuildEventKind,
}

/// Filesystem event kind that produced a [`BuildUpdateEvent`].
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildEventKind {
    /// A new build tracking file was created.
    Created,
    /// An existing build tracking file was modified.
    Modified,
}

/// Slimmed-down view of an AYIN `TraceSpan` forwarded to the browser.
///
/// Field names and serialization format mirror the JSON produced by AYIN so
/// this struct can be deserialized directly from a raw SSE `data:` line
/// without a separate mapping step.
///
/// `outcome` and `metadata` are kept as raw [`serde_json::Value`] to avoid
/// coupling this crate to the AYIN type definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpanSummary {
    /// Span UUID as a hyphenated lowercase string (e.g. `"00112233-…"`).
    pub id: String,
    /// Parent span UUID, if this span is a child of another.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Actor identifier, e.g. `"soul"`, `"claude_code"`, `"corso"`.
    pub actor: String,
    /// Action name, e.g. `"rag.query.started"`, `"tool.call"`.
    pub action: String,
    /// ISO-8601 UTC timestamp string.
    pub timestamp: String,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Outcome forwarded verbatim from AYIN (e.g. `"success"`, `"failure"`).
    pub outcome: serde_json::Value,
    /// Arbitrary extra data forwarded verbatim. Absent when null.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
}

/// AYIN connection lifecycle status.
///
/// Uses internally-tagged serialisation (`#[serde(tag = "status")]`) so that
/// unit variants (`Connected`, `Disconnected`) produce a flat `"status"` field
/// in the parent [`WebEvent`] object rather than being silently dropped.
///
/// Wire format examples:
/// - `{"type":"ayin_status","status":"connected"}`
/// - `{"type":"ayin_status","status":"disconnected"}`
/// - `{"type":"ayin_status","status":"reconnecting","attempt":3}`
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AyinStatus {
    /// Successfully connected and receiving spans.
    Connected,
    /// Connection dropped; the client will attempt to reconnect.
    Disconnected,
    /// Exponential-backoff reconnect is in progress.
    Reconnecting {
        /// 1-based reconnect attempt counter.
        attempt: u32,
    },
}

/// A control command sent from an external process (e.g. Claude Code)
/// to mutate the browser UI state via the SSE fan-out.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ControlCommand {
    /// Focus a specific panel (`"terminal"` or `"helix"`).
    FocusPanel {
        /// Panel identifier.
        panel: String,
    },
    /// Set split sizes as percentages (must sum to 100).
    ResizePanels {
        /// Terminal panel size in percent.
        terminal: u8,
        /// Helix panel size in percent.
        helix: u8,
    },
    /// Adjust the helix 3D scene zoom level.
    SetHelixZoom {
        /// Zoom level (camera distance factor).
        level: f32,
    },
    /// Show or hide a panel.
    SetPanelVisibility {
        /// Panel identifier (`"terminal"` or `"helix"`).
        panel: String,
        /// Whether the panel should be visible.
        visible: bool,
    },
    /// Push a transient notification to the browser.
    Notify {
        /// Human-readable message text.
        message: String,
        /// Severity level: `"info"`, `"warn"`, `"error"`.
        level: String,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn web_event_ayin_status_serialises_type_tag() {
        let event = WebEvent::AyinStatus(AyinStatus::Connected);
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            json.contains(r#""type":"ayin_status""#),
            "missing type tag: {json}"
        );
    }

    #[test]
    fn web_event_ayin_span_serialises_type_tag() {
        let span = TraceSpanSummary {
            id: "test".to_owned(),
            parent_id: None,
            actor: "soul".to_owned(),
            action: "rag.query".to_owned(),
            timestamp: "2026-04-13T00:00:00Z".to_owned(),
            duration_ms: 10,
            outcome: serde_json::Value::String("success".to_owned()),
            metadata: serde_json::Value::Null,
        };
        let event = WebEvent::AyinSpan(span);
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            json.contains(r#""type":"ayin_span""#),
            "missing type tag: {json}"
        );
    }

    #[test]
    fn trace_span_summary_null_metadata_omitted() {
        let span = TraceSpanSummary {
            id: "x".to_owned(),
            parent_id: None,
            actor: "a".to_owned(),
            action: "b".to_owned(),
            timestamp: "t".to_owned(),
            duration_ms: 0,
            outcome: serde_json::json!("success"),
            metadata: serde_json::Value::Null,
        };
        let json = serde_json::to_string(&span).unwrap();
        assert!(
            !json.contains("metadata"),
            "null metadata must be omitted: {json}"
        );
    }

    #[test]
    fn trace_span_summary_null_parent_id_omitted() {
        let span = TraceSpanSummary {
            id: "x".to_owned(),
            parent_id: None,
            actor: "a".to_owned(),
            action: "b".to_owned(),
            timestamp: "t".to_owned(),
            duration_ms: 0,
            outcome: serde_json::json!("success"),
            metadata: serde_json::Value::Null,
        };
        let json = serde_json::to_string(&span).unwrap();
        assert!(
            !json.contains("parent_id"),
            "absent parent_id must be omitted: {json}"
        );
    }

    #[test]
    fn reconnecting_status_includes_attempt() {
        let event = WebEvent::AyinStatus(AyinStatus::Reconnecting { attempt: 3 });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("reconnecting"), "{json}");
        assert!(json.contains("attempt"), "{json}");
        assert!(json.contains('3'), "{json}");
    }

    #[test]
    fn helix_entry_event_has_type_tag() {
        let entry = HelixEntrySummary {
            path: "eva/entries/day-1.md".to_owned(),
            event_kind: HelixEventKind::Created,
        };
        let event = WebEvent::HelixEntry(entry);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"helix_entry""#), "{json}");
        assert!(json.contains("created"), "{json}");
    }

    #[test]
    fn helix_event_kind_modified_serialises() {
        let kind = HelixEventKind::Modified;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, r#""modified""#);
    }

    #[test]
    fn build_update_event_has_type_tag() {
        let entry = BuildUpdateEvent {
            path: "corso/builds/active.yaml".to_owned(),
            event_kind: BuildEventKind::Created,
        };
        let event = WebEvent::BuildUpdate(entry);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"build_update""#), "{json}");
        assert!(json.contains("active.yaml"), "{json}");
        assert!(json.contains("created"), "{json}");
    }

    #[test]
    fn build_event_kind_modified_serialises() {
        let kind = BuildEventKind::Modified;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, r#""modified""#);
    }
}
