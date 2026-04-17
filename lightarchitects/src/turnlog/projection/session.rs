//! Session summary projection — replaces `SessionState::persist_to_helix`.
//!
//! Reduces a session's entries to a `SessionProjection` struct equivalent to
//! the old `SessionState.yaml` — model, turn count, compaction cycles, weight,
//! status. Populated entirely by folding over log entries; no side channel consulted.

use crate::turnlog::entry::{EntryKind, TurnEntry};
use serde::{Deserialize, Serialize};

/// Aggregate summary of a completed or paused session.
///
/// All fields populated by folding over the session's log entries. No side
/// channel is consulted.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionProjection {
    /// Session UUID.
    pub session_id: String,
    /// Project root at session start.
    pub project_root: String,
    /// Model identifier from `SessionStart`.
    pub model: String,
    /// Provider name from `SessionStart`.
    pub provider: String,
    /// Number of complete user→assistant rounds (i.e. `TurnEnd` lifecycle entries).
    pub turns: u32,
    /// Number of `compaction` entries produced during this session.
    pub compaction_cycles: u32,
    /// Max weight observed across `Reflection` entries.
    pub max_weight: f64,
    /// Whether the session ended cleanly (terminal `SessionEnded` present).
    pub clean_exit: bool,
    /// Whether a `SessionPaused` entry is the last entry.
    pub paused: bool,
}

impl SessionProjection {
    /// Reduce a session's log entries into a summary projection.
    ///
    /// This is total: it never fails on malformed logs. Missing optional entries
    /// result in defaults (empty strings, zeros, false). The caller can inspect
    /// `session_id.is_empty()` to detect an empty or corrupt session.
    ///
    /// # Panics
    ///
    /// Does not panic — tolerant of any input.
    #[must_use]
    pub fn from_log(entries: &[TurnEntry]) -> Self {
        let mut proj = Self::default();

        for entry in entries {
            match entry.kind() {
                EntryKind::SessionStart => {
                    proj.session_id = entry.span.session_id.clone().unwrap_or_default();
                    let meta = &entry.span.metadata;
                    meta.get("model")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .clone_into(&mut proj.model);
                    meta.get("provider")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .clone_into(&mut proj.provider);
                    meta.get("project_root")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .clone_into(&mut proj.project_root);
                }
                EntryKind::TurnEnd => {
                    proj.turns = proj.turns.saturating_add(1);
                }
                EntryKind::Compaction => {
                    proj.compaction_cycles = proj.compaction_cycles.saturating_add(1);
                }
                EntryKind::Reflection => {
                    let weight = entry
                        .span
                        .metadata
                        .get("weight")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0);
                    if weight > proj.max_weight {
                        proj.max_weight = weight;
                    }
                }
                EntryKind::SessionEnded => {
                    proj.clean_exit = true;
                }
                EntryKind::SessionPaused => {
                    proj.paused = true;
                }
                _ => {}
            }
        }

        // If the last non-span entry is SessionPaused, mark paused = true.
        // If SessionEnded appears anywhere, it overrides paused.
        if let Some(last) = entries.last() {
            if last.kind() == EntryKind::SessionPaused && !proj.clean_exit {
                proj.paused = true;
            }
        }

        proj
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ayin::span::{Actor, TraceOutcome};
    use chrono::Utc;
    use uuid::Uuid;

    fn span(action: &str, session_id: &str, meta: serde_json::Value) -> TurnEntry {
        TurnEntry {
            seq: 0,
            parent_seq: None,
            span: ayin::TraceSpan {
                id: Uuid::new_v4(),
                parent_id: None,
                session_id: Some(session_id.to_owned()),
                actor: Actor::claude(),
                action: action.to_owned(),
                timestamp: Utc::now(),
                duration_ms: 1,
                decision_points: Vec::new(),
                strand_activations: Vec::new(),
                outcome: TraceOutcome::Continue,
                metadata: meta,
            },
            hmac_prev: String::new(),
            hmac_self: String::new(),
        }
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn from_log_empty_entries_returns_default() {
        let proj = SessionProjection::from_log(&[]);
        assert!(proj.session_id.is_empty());
        assert_eq!(proj.turns, 0);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn from_log_session_start_populates_fields() {
        let entries = vec![span(
            "session_start",
            "sess-1",
            serde_json::json!({
                "model": "claude-opus-4-6",
                "provider": "anthropic",
                "project_root": "/home/user/project"
            }),
        )];
        let proj = SessionProjection::from_log(&entries);
        assert_eq!(proj.session_id, "sess-1");
        assert_eq!(proj.model, "claude-opus-4-6");
        assert_eq!(proj.provider, "anthropic");
        assert_eq!(proj.project_root, "/home/user/project");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn from_log_counts_turns_and_compactions() {
        let entries = vec![
            span("session_start", "s1", serde_json::json!({})),
            span("turn_end", "s1", serde_json::json!({})),
            span("turn_end", "s1", serde_json::json!({})),
            span("compaction", "s1", serde_json::json!({})),
            span("turn_end", "s1", serde_json::json!({})),
        ];
        let proj = SessionProjection::from_log(&entries);
        assert_eq!(proj.turns, 3);
        assert_eq!(proj.compaction_cycles, 1);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn from_log_tracks_max_weight_from_reflections() {
        let entries = vec![
            span("session_start", "s1", serde_json::json!({})),
            span("reflection", "s1", serde_json::json!({ "weight": 0.7 })),
            span("reflection", "s1", serde_json::json!({ "weight": 0.9 })),
            span("reflection", "s1", serde_json::json!({ "weight": 0.5 })),
        ];
        let proj = SessionProjection::from_log(&entries);
        assert!((proj.max_weight - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn from_log_detects_clean_exit() {
        let entries = vec![
            span("session_start", "s1", serde_json::json!({})),
            span("session_ended", "s1", serde_json::json!({})),
        ];
        let proj = SessionProjection::from_log(&entries);
        assert!(proj.clean_exit);
        assert!(!proj.paused);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn from_log_detects_paused_session() {
        let entries = vec![
            span("session_start", "s1", serde_json::json!({})),
            span("session_paused", "s1", serde_json::json!({})),
        ];
        let proj = SessionProjection::from_log(&entries);
        assert!(!proj.clean_exit);
        assert!(proj.paused);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn from_log_tolerates_missing_metadata() {
        let entries = vec![span("session_start", "s1", serde_json::json!({}))];
        let proj = SessionProjection::from_log(&entries);
        assert_eq!(proj.model, "");
        assert_eq!(proj.provider, "");
    }
}
