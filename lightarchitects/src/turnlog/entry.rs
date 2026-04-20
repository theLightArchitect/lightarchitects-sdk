//! On-disk entry types — the schema every line of an NDJSON session file conforms to.
//!
//! # Design contract
//!
//! Every event in a turnlog session file is an [`ayin::TraceSpan`] wrapped in a
//! [`TurnEntry`] that adds the HMAC chain fields (`seq`, `parent_seq`,
//! `hmac_prev`, `hmac_self`).  There is no separate payload enum — the span's
//! `action` tag carries the kind, and `metadata` carries all rich payload data.
//!
//! Canonical action tags (greppable with `jq '.span.action'`):
//!
//! | Action | Semantic kind |
//! |--------|--------------|
//! | `turn.user` | User message |
//! | `turn.assistant` | Final assistant response |
//! | `tool_result` | Tool call result |
//! | `span` | Fine-grained AYIN span |
//! | `compaction` | Context compaction cycle |
//! | `reflection` | Post-turn reflection / helix candidate |
//! | `security_event` | Security-relevant event |
//! | `session_start` | First entry of session |
//! | `turn_start` | Begin of a turn |
//! | `turn_end` | End of a turn |
//! | `session_paused` | Session paused (resume candidate) |
//! | `session_resumed` | Session resumed |
//! | `session_ended` | Clean shutdown |
//!
//! # Wire format guarantees
//!
//! * Action tag is stable — never renamed between protocol versions.
//! * `metadata` field is a `serde_json::Value` with BTreeMap-backed objects —
//!   key order is deterministic, which is load-bearing for HMAC chain integrity.
//! * [`lightarchitects::turnlog::chain::signable_bytes`] binds the HMAC to the serialised
//!   [`ayin::TraceSpan`], not to a custom byte layout — AYIN and turnlog share
//!   the same span encoding.

use crate::ayin::span::TraceSpan;
use serde::{Deserialize, Serialize};

// ── Top-level entry ─────────────────────────────────────────────────────────────

/// One line of a session's NDJSON log.
///
/// Wraps an [`ayin::TraceSpan`] with chain-integrity fields. Entries are
/// append-only: once written, they are never mutated. The HMAC chain
/// (`hmac_prev`, `hmac_self`) makes mutation detectable at verify time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnEntry {
    /// Monotonic sequence within the session. Starts at 0 for the first entry.
    pub seq: u64,

    /// Optional pointer to a parent entry (e.g. tool_result inside a turn).
    /// `None` for session-level entries like `session_start` / `session_ended`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent_seq: Option<u64>,

    /// The span record. `span.action` identifies the kind; `span.metadata`
    /// carries rich payload. `span.timestamp` is the canonical wall-clock time.
    pub span: TraceSpan,

    /// Hex-encoded HMAC-SHA256 digest of the previous entry.
    /// For seq=0, this is the genesis block's `hmac_genesis` value.
    pub hmac_prev: String,

    /// Hex-encoded HMAC-SHA256 of this entry's canonical bytes, keyed by the
    /// per-session secret derived from the store-level pepper via HKDF.
    pub hmac_self: String,
}

impl TurnEntry {
    /// Nanoseconds since Unix epoch, derived from the span's `timestamp`.
    ///
    /// Returns 0 if the timestamp is out of the i64 range (which would
    /// require a year > 2262 — safe to treat as a sentinel).
    #[must_use]
    pub fn ts_ns(&self) -> i64 {
        self.span.timestamp.timestamp_nanos_opt().unwrap_or(0)
    }

    /// Milliseconds since Unix epoch — AYIN trace viewer granularity.
    #[must_use]
    pub fn ts_ms(&self) -> i64 {
        self.ts_ns() / 1_000_000
    }

    /// Semantic kind derived from `span.action`.
    #[must_use]
    pub fn kind(&self) -> EntryKind {
        EntryKind::from_action(&self.span.action)
    }

    /// Whether this entry is a candidate for Tier-2 helix promotion.
    ///
    /// Uses the compile-time [`crate::turnlog::promotion::SIGNIFICANCE_AUTO_FLOOR`].
    /// For policy-aware callers that read the floor from a hot-reloadable YAML
    /// config, use [`Self::is_helix_promotable_with_floor`] instead.
    #[must_use]
    pub fn is_helix_promotable(&self) -> bool {
        self.is_helix_promotable_with_floor(crate::turnlog::promotion::SIGNIFICANCE_AUTO_FLOOR)
    }

    /// Whether this entry is a candidate for Tier-2 helix promotion given a
    /// specific significance `floor`.
    ///
    /// An entry qualifies when its kind is a known promotable class
    /// (`Reflection`, `SessionPaused`, `BuildComplete`, `ScrumVerdict`), OR
    /// when its metadata declares `significance >= floor`.
    ///
    /// This is the floor-parameterised variant used by policy-aware promotion
    /// paths (see [`crate::turnlog::policy::PromotionPolicy::floor_for`]).
    #[must_use]
    pub fn is_helix_promotable_with_floor(&self, floor: f64) -> bool {
        let kind = self.kind();
        if matches!(
            kind,
            EntryKind::Reflection
                | EntryKind::SessionPaused
                | EntryKind::BuildComplete
                | EntryKind::ScrumVerdict
        ) {
            return true;
        }
        self.span
            .metadata
            .get("significance")
            .and_then(serde_json::Value::as_f64)
            .is_some_and(|v| v >= floor)
    }
}

// ── Semantic kind ───────────────────────────────────────────────────────────────

/// Semantic kind parsed from a span's `action` tag.
///
/// Variants map 1:1 to the canonical action strings. `Other` captures any
/// action not in this list — callers that only care about a specific kind
/// should match on the relevant variants and ignore `Other`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EntryKind {
    /// User message (`turn.user`).
    TurnUser,
    /// Final assistant response (`turn.assistant`).
    TurnAssistant,
    /// Tool call result (`tool_result`).
    ToolResult,
    /// Fine-grained span (`span`).
    Span,
    /// Context compaction cycle (`compaction`).
    Compaction,
    /// Post-turn reflection — primary helix promotion candidate (`reflection`).
    Reflection,
    /// Security-relevant event (`security_event`).
    SecurityEvent,
    /// Session start lifecycle marker (`session_start`).
    SessionStart,
    /// Turn start lifecycle marker (`turn_start`).
    TurnStart,
    /// Turn end lifecycle marker (`turn_end`).
    TurnEnd,
    /// Session paused — resume candidate (`session_paused`).
    SessionPaused,
    /// Session resumed from a pause (`session_resumed`).
    SessionResumed,
    /// Clean session shutdown (`session_ended`).
    SessionEnded,
    /// CORSO build finished — Phase 19 typed trigger (`build_complete`).
    /// Carries `build_id`, `status`, `plan_ids` in span metadata.
    BuildComplete,
    /// SCRUM review verdict — Phase 19 typed trigger (`scrum_verdict`).
    /// Carries `plan_ids: [...]` in span metadata for REVIEWS_PLAN edge wire-up.
    ScrumVerdict,
    /// Any action not matched by the variants above.
    Other(String),
}

impl EntryKind {
    /// Parse an action tag string into a [`EntryKind`].
    #[must_use]
    pub fn from_action(action: &str) -> Self {
        match action {
            "turn.user" => Self::TurnUser,
            "turn.assistant" => Self::TurnAssistant,
            "tool_result" => Self::ToolResult,
            "span" => Self::Span,
            "compaction" => Self::Compaction,
            "reflection" => Self::Reflection,
            "security_event" => Self::SecurityEvent,
            "session_start" => Self::SessionStart,
            "turn_start" => Self::TurnStart,
            "turn_end" => Self::TurnEnd,
            "session_paused" => Self::SessionPaused,
            "session_resumed" => Self::SessionResumed,
            "session_ended" => Self::SessionEnded,
            "build_complete" => Self::BuildComplete,
            "scrum_verdict" => Self::ScrumVerdict,
            other => Self::Other(other.to_owned()),
        }
    }

    /// Canonical action string for this kind.
    ///
    /// For `Other`, returns the underlying action string unchanged.
    #[must_use]
    pub fn as_action(&self) -> &str {
        match self {
            Self::TurnUser => "turn.user",
            Self::TurnAssistant => "turn.assistant",
            Self::ToolResult => "tool_result",
            Self::Span => "span",
            Self::Compaction => "compaction",
            Self::Reflection => "reflection",
            Self::SecurityEvent => "security_event",
            Self::SessionStart => "session_start",
            Self::TurnStart => "turn_start",
            Self::TurnEnd => "turn_end",
            Self::SessionPaused => "session_paused",
            Self::SessionResumed => "session_resumed",
            Self::SessionEnded => "session_ended",
            Self::BuildComplete => "build_complete",
            Self::ScrumVerdict => "scrum_verdict",
            Self::Other(s) => s.as_str(),
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use crate::ayin::span::{Actor, TraceContext, TraceOutcome};

    use super::*;

    fn make_span(action: &str) -> TraceSpan {
        TraceContext::new(Actor::claude(), action)
            .outcome(TraceOutcome::Continue)
            .finish()
            .expect("span construction must succeed in tests")
    }

    #[test]
    fn kind_roundtrips_all_canonical_actions() {
        let actions = [
            ("turn.user", EntryKind::TurnUser),
            ("turn.assistant", EntryKind::TurnAssistant),
            ("tool_result", EntryKind::ToolResult),
            ("span", EntryKind::Span),
            ("compaction", EntryKind::Compaction),
            ("reflection", EntryKind::Reflection),
            ("security_event", EntryKind::SecurityEvent),
            ("session_start", EntryKind::SessionStart),
            ("turn_start", EntryKind::TurnStart),
            ("turn_end", EntryKind::TurnEnd),
            ("session_paused", EntryKind::SessionPaused),
            ("session_resumed", EntryKind::SessionResumed),
            ("session_ended", EntryKind::SessionEnded),
        ];
        for (action, expected) in actions {
            let kind = EntryKind::from_action(action);
            assert_eq!(kind, expected, "from_action({action:?}) mismatch");
            assert_eq!(
                kind.as_action(),
                action,
                "as_action() roundtrip failed for {action:?}"
            );
        }
    }

    #[test]
    fn other_kind_preserves_action_string() {
        let kind = EntryKind::from_action("custom.my_tool");
        assert!(matches!(kind, EntryKind::Other(_)));
        assert_eq!(kind.as_action(), "custom.my_tool");
    }

    #[test]
    fn reflection_is_helix_promotable() {
        let span = make_span("reflection");
        let entry = TurnEntry {
            seq: 0,
            parent_seq: None,
            span,
            hmac_prev: String::new(),
            hmac_self: String::new(),
        };
        assert!(entry.is_helix_promotable());
    }

    #[test]
    fn session_paused_is_helix_promotable() {
        let span = make_span("session_paused");
        let entry = TurnEntry {
            seq: 0,
            parent_seq: None,
            span,
            hmac_prev: String::new(),
            hmac_self: String::new(),
        };
        assert!(entry.is_helix_promotable());
    }

    #[test]
    fn turn_user_is_not_helix_promotable() {
        let span = make_span("turn.user");
        let entry = TurnEntry {
            seq: 0,
            parent_seq: None,
            span,
            hmac_prev: String::new(),
            hmac_self: String::new(),
        };
        assert!(!entry.is_helix_promotable());
    }

    #[test]
    fn ts_ms_derived_from_span_timestamp() {
        let span = make_span("turn.user");
        let entry = TurnEntry {
            seq: 0,
            parent_seq: None,
            span,
            hmac_prev: String::new(),
            hmac_self: String::new(),
        };
        // ts_ms() should be non-negative (current time in ms since epoch).
        assert!(entry.ts_ms() > 0);
        assert_eq!(entry.ts_ms(), entry.ts_ns() / 1_000_000);
    }

    #[test]
    fn turn_entry_serde_roundtrip() {
        let span = make_span("turn.assistant");
        let entry = TurnEntry {
            seq: 42,
            parent_seq: Some(41),
            span: span.clone(),
            hmac_prev: "aabbcc".to_owned(),
            hmac_self: "ddeeff".to_owned(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: TurnEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.seq, 42);
        assert_eq!(back.parent_seq, Some(41));
        assert_eq!(back.span.action, "turn.assistant");
        assert_eq!(back.hmac_prev, "aabbcc");
        assert_eq!(back.hmac_self, "ddeeff");
    }

    #[test]
    fn turn_entry_parent_seq_none_omitted_in_json() {
        let span = make_span("session_start");
        let entry = TurnEntry {
            seq: 0,
            parent_seq: None,
            span,
            hmac_prev: String::new(),
            hmac_self: String::new(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        // `parent_seq` must be absent when None — the field is skip_serializing_if.
        assert!(
            !json.contains("parent_seq"),
            "parent_seq should be omitted when None"
        );
    }
}
