//! Internal event types for the server-sent event fan-out.
//!
//! All types here implement [`serde::Serialize`] so they can be forwarded
//! verbatim as `data:` payloads on the SSE stream the browser subscribes
//! to via `GET /api/events` (Phase 5).

use crate::memory::types::PromotionEvent;
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
    /// A strand activation derived from an AYIN span's metadata.
    ///
    /// Emitted by the AYIN client alongside [`WebEvent::AyinSpan`] when the
    /// span's metadata contains a `strand_activations` array. One event per
    /// strand, so a span touching three strands produces three events.
    StrandActivation(StrandActivationEvent),
    /// A hot memo was promoted to the cold helix tier.
    ///
    /// Emitted by `BroadcastingPromoter` in [`crate::memory::promoter_bridge`]
    /// when `SiblingPromoter::promote` returns `PromotionOutcome::Promoted`.
    /// The frontend uses this to optimistically move the memo from the
    /// `hotMemory` store to `coldMemory` and to trigger an orb-spawn animation
    /// in the 3D scene.
    SoulPromotion(PromotionEvent),
    /// A UI event forwarded from the `lightarchitects-gateway` MCP server's
    /// `ui.*` tools.
    ///
    /// The gateway POSTs a raw JSON body to `/api/builds/:id/notify` —
    /// authenticated via `X-LA-Notify-Token` — and the webshell wraps that
    /// body in this variant before broadcasting it on the per-build SSE
    /// channel. The frontend reads `msg.type === "gateway_notify"` then
    /// dispatches on the inner `msg.payload.type` (e.g. `"focus_pillar"`).
    GatewayNotify {
        /// Raw gateway body verbatim — frontend unwraps `.payload.type`
        /// to dispatch (`focus_pillar`, `flag_finding`, `refresh_sitrep`,
        /// `update_conductor`, `set_active_build`, `notify`).
        payload: serde_json::Value,
    },
    /// Streaming progress from a real CORSO pillar run (Phase 15).
    ///
    /// Emitted by [`crate::real_data::trigger_pillar`] as the `corso <cmd>`
    /// subprocess produces output. Three phases per run:
    ///   * `phase: "started"`   — before spawn (single event)
    ///   * `phase: "output"`    — one event per stdout line
    ///   * `phase: "completed"` — final event with exit status + artifact path
    PillarUpdate(PillarUpdateEvent),
    /// Phase 19b.2 — cross-sibling strand convergence detected.
    ///
    /// Emitted by the convergence detector when three or more distinct
    /// siblings activate the same strand within the active hot window.
    /// The UI renders this as a "convergence" pulse in `Helix3D` and an
    /// entry in the convergences tab. Graph materialization of the
    /// convergence (a `:SharedExperience` node + `:PARTICIPATES_IN` edges)
    /// is deferred to Phase 19c / 20.
    StrandConvergence(StrandConvergenceEvent),
    /// Live copilot subprocess activity streamed during a turn.
    ///
    /// Emitted by `run_print_turn` / `run_codex_turn` for each intermediate
    /// `stream-json` event (thinking, `tool_use`, `tool_result`, etc.). The
    /// frontend Activity tab renders these as a live feed.
    CopilotActivity(CopilotActivityEvent),
}

/// Cross-sibling strand convergence event (Phase 19b.2).
///
/// Fired when a strand hits the configured minimum-participants threshold
/// (default 3). `memo_ids` reference the `:HotMemo` nodes that triggered
/// the convergence; the UI can deep-link back to each.
#[derive(Debug, Clone, Serialize)]
pub struct StrandConvergenceEvent {
    /// Strand name, lowercased (e.g. `"analytical"`).
    pub strand: String,
    /// Distinct sibling names currently activating this strand.
    pub siblings: Vec<String>,
    /// `:HotMemo.id` values that participated in the convergence.
    pub memo_ids: Vec<String>,
    /// ISO-8601 UTC timestamp of detection.
    pub detected_at: String,
}

/// Live copilot activity event streamed during a turn (Phase 20 — Activity tab).
///
/// Maps 1:1 to `stream-json` NDJSON lines from `claude --print --verbose`.
/// The frontend Activity tab renders these as a collapsible live feed with
/// verbose/auditable detail levels.
#[derive(Debug, Clone, Serialize)]
pub struct CopilotActivityEvent {
    /// Build this activity belongs to.
    pub build_id: String,
    /// Event category from the stream-json line's `type` field.
    /// Known values: `assistant`, `tool_use`, `tool_result`, `result`, `system`, `error`.
    pub kind: String,
    /// Human-readable summary (first 500 chars of content/thinking/tool name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Full raw JSON line for verbose/auditable mode.
    pub raw: serde_json::Value,
    /// ISO-8601 UTC timestamp of when this event was received.
    pub timestamp: String,
}

/// Incremental pillar-run update broadcast over SSE (Phase 15).
///
/// The frontend subscribes on the per-build SSE channel and matches on
/// `build_id` + `pillar` to update the matching UI card.
#[derive(Debug, Clone, Serialize)]
pub struct PillarUpdateEvent {
    /// Build this pillar run belongs to.
    pub build_id: String,
    /// Pillar name (`arch`, `sec`, `qual`, `perf`, `test`, `doc`, `ops`).
    pub pillar: String,
    /// Lifecycle marker — `started` · `output` · `completed`.
    pub phase: String,
    /// One line of stdout when `phase == "output"`; omitted otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// Process exit code when `phase == "completed"`; omitted otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Relative artifact path (e.g. `pillar-arch.json`) when completed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<String>,
}

/// A single strand activation derived from an AYIN span.
///
/// Produced by the strand parser in [`crate::events::strand`]. The parser
/// is the validation boundary — `weight` is always clamped to `[0.0, 1.0]`
/// before construction, so downstream consumers can trust the value.
#[derive(Debug, Clone, Serialize)]
pub struct StrandActivationEvent {
    /// Sibling identifier, e.g. `"eva"`, `"corso"`. Taken verbatim from
    /// the source span's `actor` field.
    pub sibling: String,
    /// Strand name, e.g. `"methodical"`, `"contextual"`. Taken from the
    /// `strand_activations[].strand` field of the source span's metadata.
    pub strand: String,
    /// Activation magnitude in `[0.0, 1.0]`. Clamped by the parser.
    pub weight: f32,
    /// ISO-8601 UTC timestamp, mirrored from the source span.
    pub timestamp: String,
}

/// Describes a new or modified helix vault entry detected by the filesystem watcher.
///
/// Phase 9.3 enriched this shape with front-matter fields so the Svelte webshell
/// can render real memory tiles without a secondary fetch. All enrichment fields
/// are best-effort — a malformed or missing YAML front-matter still produces a
/// valid event with the core `path` + `event_kind`, and `None`/empty values
/// elsewhere.
#[derive(Debug, Clone, Serialize)]
pub struct HelixEntrySummary {
    /// Path relative to the helix root (e.g. `"eva/entries/day-42.md"`).
    pub path: String,
    /// What triggered this event.
    pub event_kind: HelixEventKind,
    /// Owning sibling derived from the path's top-level directory or the
    /// front-matter `sibling:` field (front-matter wins).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling: Option<String>,
    /// Significance score from front-matter. Normalised to `[0.0, 1.0]`:
    /// values between 0 and 10 in the YAML are divided by 10 on ingest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub significance: Option<f32>,
    /// Strand tags from the front-matter `strands:` list (lowercased).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strands: Vec<String>,
    /// First 280 chars of the body (excluding front-matter), for UI hover preview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_excerpt: Option<String>,
    /// ISO-8601 UTC timestamp from front-matter `date:` or file mtime fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Typed classification — Phase 14.1. Populated from the front-matter
    /// `type:` field when present; otherwise inferred from the path shape.
    ///
    /// Canonical values recognised by the UI: `entry`, `plan`, `standard`,
    /// `review`, `lesson`, `reference`. Unknown types are passed through as
    /// lowercase strings so new categories don't require a frontend deploy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
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

impl HelixEntrySummary {
    /// Build a minimal summary — used when the file can't be read or parsed.
    ///
    /// Enrichment fields default to `None` / empty. The Svelte frontend is
    /// responsible for rendering a graceful fallback when fields are absent.
    #[must_use]
    pub fn minimal(path: String, event_kind: HelixEventKind) -> Self {
        Self {
            path,
            event_kind,
            sibling: None,
            significance: None,
            strands: Vec::new(),
            content_excerpt: None,
            created_at: None,
            kind: None,
        }
    }
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// Top-level strand activations as emitted by AYIN's native `TraceSpan`.
    ///
    /// AYIN puts this at the top level of every span it writes. Older code
    /// paths may still embed the field under `metadata.strand_activations`
    /// for test-fixture compatibility, so the parser checks both locations
    /// (top-level wins). Empty when absent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strand_activations: Vec<serde_json::Value>,
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
    /// Open a local file in the system default editor (or the editor
    /// referenced by the `$EDITOR` env var if set).
    ///
    /// The backend executes this locally and also broadcasts the event so
    /// SSE listeners can observe file-open activity.
    OpenInEditor {
        /// Absolute or workspace-relative file path.
        file: String,
        /// Optional 1-based line number to jump to.
        line: Option<u32>,
    },
    /// Reveal a local path in the system file manager (Finder on macOS).
    ///
    /// The backend executes this locally and also broadcasts the event.
    RevealInFinder {
        /// Absolute or workspace-relative path to reveal.
        path: String,
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
            strand_activations: Vec::new(),
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
            strand_activations: Vec::new(),
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
            strand_activations: Vec::new(),
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
        let entry =
            HelixEntrySummary::minimal("eva/entries/day-1.md".to_owned(), HelixEventKind::Created);
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

    #[test]
    fn gateway_notify_wraps_raw_json_under_payload() {
        let event = WebEvent::GatewayNotify {
            payload: serde_json::json!({"type": "focus_pillar", "pillar": "ARCH"}),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            json.contains(r#""type":"gateway_notify""#),
            "outer tag must be gateway_notify: {json}"
        );
        // Parse back and confirm `payload.type` is preserved for the frontend
        // to dispatch on (e.g. `msg.payload.type === "focus_pillar"`).
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["payload"]["type"], "focus_pillar");
        assert_eq!(parsed["payload"]["pillar"], "ARCH");
    }

    #[test]
    fn strand_activation_has_type_tag_and_flat_fields() {
        let event = WebEvent::StrandActivation(StrandActivationEvent {
            sibling: "eva".to_owned(),
            strand: "methodical".to_owned(),
            weight: 0.9,
            timestamp: "2026-04-16T00:00:00Z".to_owned(),
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"strand_activation""#), "{json}");
        assert!(json.contains(r#""sibling":"eva""#), "{json}");
        assert!(json.contains(r#""strand":"methodical""#), "{json}");
        assert!(json.contains(r#""weight":0.9"#), "{json}");
    }

    /// SSE contract canary (#50) — trip-wire test that enumerates EVERY
    /// `WebEvent` variant and asserts the serialised `"type"` tag matches the
    /// exact string the frontend's `EventType` union expects.
    ///
    /// **If this test fails** you must update `EventType` in
    /// `lightarchitects-webshell-ui/src/lib/types.ts` to match before merging.
    ///
    /// The canonical FE set at time of writing (2026-04-30):
    ///   `ayin_span`, `ayin_status`, `helix_entry`, `build_update`, `control`,
    ///   `strand_activation`, `soul_promotion`, `gateway_notify`, `pillar_update`,
    ///   `strand_convergence`, `copilot_activity`
    #[test]
    fn sse_contract_all_web_event_variants_have_known_type_tags() {
        // Helper: extract the `type` field from a serialised WebEvent.
        fn type_tag(event: &WebEvent) -> String {
            let json = serde_json::to_string(event).unwrap();
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            v["type"].as_str().unwrap_or("").to_owned()
        }

        let span = TraceSpanSummary {
            id: "x".to_owned(),
            parent_id: None,
            actor: "soul".to_owned(),
            action: "a".to_owned(),
            timestamp: "t".to_owned(),
            duration_ms: 0,
            outcome: serde_json::Value::Null,
            metadata: serde_json::Value::Null,
            strand_activations: Vec::new(),
        };
        let helix = HelixEntrySummary::minimal("p".to_owned(), HelixEventKind::Created);
        let build_ev = BuildUpdateEvent {
            path: "p".to_owned(),
            event_kind: BuildEventKind::Created,
        };
        let ctrl = ControlCommand::Notify {
            message: "m".to_owned(),
            level: "info".to_owned(),
        };
        let strand = StrandActivationEvent {
            sibling: "s".to_owned(),
            strand: "t".to_owned(),
            weight: 1.0,
            timestamp: "t".to_owned(),
        };
        let pillar = PillarUpdateEvent {
            build_id: "b".to_owned(),
            pillar: "arch".to_owned(),
            phase: "started".to_owned(),
            line: None,
            exit_code: None,
            artifact: None,
        };
        let convergence = StrandConvergenceEvent {
            strand: "analytical".to_owned(),
            siblings: vec!["eva".to_owned()],
            memo_ids: Vec::new(),
            detected_at: "t".to_owned(),
        };
        let activity = CopilotActivityEvent {
            build_id: "b".to_owned(),
            kind: "assistant".to_owned(),
            summary: None,
            raw: serde_json::Value::Null,
            timestamp: "t".to_owned(),
        };
        let promotion = crate::memory::types::PromotionEvent {
            memo_id: "m".to_owned(),
            from: crate::memory::types::MemoryTier::Hot,
            to: crate::memory::types::MemoryTier::Cold,
            sibling: "eva".to_owned(),
            significance: 0.9,
            path: "p".to_owned(),
            promoted_at: "t".to_owned(),
        };

        // Canonical mapping: Rust variant → expected serialised `type` string.
        // Update this list AND the FE EventType whenever a new variant is added.
        let cases: &[(&str, WebEvent)] = &[
            ("ayin_span", WebEvent::AyinSpan(span)),
            ("ayin_status", WebEvent::AyinStatus(AyinStatus::Connected)),
            ("helix_entry", WebEvent::HelixEntry(helix)),
            ("build_update", WebEvent::BuildUpdate(build_ev)),
            ("control", WebEvent::Control(ctrl)),
            ("strand_activation", WebEvent::StrandActivation(strand)),
            ("soul_promotion", WebEvent::SoulPromotion(promotion)),
            (
                "gateway_notify",
                WebEvent::GatewayNotify {
                    payload: serde_json::Value::Null,
                },
            ),
            ("pillar_update", WebEvent::PillarUpdate(pillar)),
            (
                "strand_convergence",
                WebEvent::StrandConvergence(convergence),
            ),
            ("copilot_activity", WebEvent::CopilotActivity(activity)),
        ];

        for (expected_tag, event) in cases {
            let actual = type_tag(event);
            assert_eq!(
                actual, *expected_tag,
                "WebEvent variant serialised as '{actual}' but contract expects '{expected_tag}'. \
                 Update EventType in lightarchitects-webshell-ui/src/lib/types.ts.",
            );
        }
    }
}
