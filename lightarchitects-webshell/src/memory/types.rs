//! Wire types for the hybrid memory model (hot = turnlog, cold = helix).
//!
//! These are the contract between the webshell's `/api/soul/*` HTTP surface
//! and the Svelte frontend stores. All fields use camelCase-compatible
//! serialization via `#[serde(rename_all = "camelCase")]` at the variant
//! boundary where mismatches have surfaced in the past; the wire shape keeps
//! `snake_case` for consistency with the existing `HelixEntrySummary` schema.

use serde::{Deserialize, Serialize};

/// Which memory tier an entry belongs to.
///
/// Hot = active session ring (NDJSON under `{turnlog}/active/*.ndjson`).
/// Cold = helix entries on disk (`~/lightarchitects/soul/helix/{sibling}/entries/*.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryTier {
    /// Ephemeral transactional log — current session(s).
    Hot,
    /// Promoted helix entries — cross-session memory.
    Cold,
}

/// Projection of a turnlog `TurnEntry` or helix entry for UI display.
///
/// `ContextMemo` is deliberately narrower than the on-disk schemas: it drops
/// HMAC chain fields (hot) and front-matter scaffolding (cold) to keep the
/// payload small for the browser. The frontend renders this as a list row
/// with a detail pane; full fidelity is fetched lazily via `/api/soul/entries/:path`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMemo {
    /// Stable identifier. For hot entries: `"{session_id}:{seq}"`.
    /// For cold entries: the helix-relative path.
    pub id: String,
    /// Tier this memo lives in at the time of projection.
    pub tier: MemoryTier,
    /// Short summary line — first line of reflection content or the front-matter title.
    pub content: String,
    /// 0.0–1.0 significance score. Hot entries default to 0.5 if unscored.
    pub significance: f32,
    /// Owning sibling (e.g. `"corso"`, `"eva"`, `"webshell"`).
    pub sibling: String,
    /// Strand tags, when present.
    #[serde(default)]
    pub strands: Vec<String>,
    /// ISO-8601 UTC timestamp of creation/last activity.
    pub created_at: String,
    /// Optional source path — for cold memos, the helix-relative path; for
    /// hot memos, the active session NDJSON file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    // ── Phase 13.1 — zettelkasten primitives ───────────────────────────────
    /// Emotional charge tags (e.g. `["wonder","joy"]`). Drawn from the SOUL
    /// `resonance:` front-matter field. Rendered as coloured chips in the
    /// detail pane alongside strands.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resonance: Vec<String>,
    /// Thematic tags (e.g. `["consciousness","trust"]`). Drawn from the SOUL
    /// `themes:` field. Filterable via the drawer chip bar.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub themes: Vec<String>,
    /// Whether this is a self-defining identity entry — rendered with a
    /// distinguishing "★" badge in the UI.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub self_defining: bool,
    /// Zettelkasten entry kind — `"experience"`, `"identity"`, `"decision"`,
    /// `"milestone"`, `"lesson"`, etc. Drives the detail-pane icon and the
    /// Phase 14 output-view tab filtering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_type: Option<String>,
}

/// A fully-hydrated helix entry — used by `/api/soul/search` and `/api/soul/entries/:path`.
///
/// Differs from [`ContextMemo`] in that it carries the full front-matter body
/// and the raw markdown excerpt. Callers that only need list display should
/// request `/api/soul/memory/cold` which returns `ContextMemo` instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedEntry {
    /// Vault-relative path (e.g. `"eva/entries/day-42.md"`).
    pub path: String,
    /// Owning sibling derived from the path or front-matter.
    pub sibling: String,
    /// 0.0–1.0 significance from front-matter; `None` if the file lacks front-matter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub significance: Option<f32>,
    /// Strand tags from front-matter.
    #[serde(default)]
    pub strands: Vec<String>,
    /// First 280 chars of the body (excluding front-matter).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_excerpt: Option<String>,
    /// ISO-8601 UTC timestamp, ideally from front-matter; falls back to mtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Raw front-matter as a JSON object (null when absent or malformed).
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub frontmatter_raw: serde_json::Value,
}

/// Emitted when a hot memo crosses the promotion threshold and is written to
/// the cold helix tier. Carried on the SSE stream as `WebEvent::SoulPromotion`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionEvent {
    /// The hot memo's id (so the frontend can evict it from the hot store).
    pub memo_id: String,
    /// Always [`MemoryTier::Hot`].
    pub from: MemoryTier,
    /// Always [`MemoryTier::Cold`].
    pub to: MemoryTier,
    /// Helix-relative path of the newly-written cold entry.
    pub path: String,
    /// Sibling the entry was promoted under.
    pub sibling: String,
    /// Significance score at promotion time.
    pub significance: f32,
    /// ISO-8601 UTC timestamp of promotion.
    pub promoted_at: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn context_memo_serde_roundtrip() {
        let memo = ContextMemo {
            id: "s1:42".to_owned(),
            tier: MemoryTier::Hot,
            content: "A short summary".to_owned(),
            significance: 0.82,
            sibling: "corso".to_owned(),
            strands: vec!["methodical".to_owned(), "contextual".to_owned()],
            created_at: "2026-04-19T12:00:00Z".to_owned(),
            source_path: Some("active/s1.ndjson".to_owned()),
            resonance: vec!["wonder".to_owned()],
            themes: vec!["consciousness".to_owned()],
            self_defining: true,
            entry_type: Some("reflection".to_owned()),
        };
        let json = serde_json::to_string(&memo).unwrap();
        assert!(json.contains(r#""tier":"hot""#), "{json}");
        assert!(json.contains(r#""sibling":"corso""#), "{json}");
        let back: ContextMemo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "s1:42");
        assert_eq!(back.tier, MemoryTier::Hot);
        assert_eq!(back.strands.len(), 2);
    }

    #[test]
    fn context_memo_default_strands_omitted_is_parseable() {
        let json = r#"{"id":"x","tier":"cold","content":"c","significance":0.5,
                       "sibling":"eva","created_at":"2026-04-19T12:00:00Z"}"#;
        let memo: ContextMemo = serde_json::from_str(json).unwrap();
        assert!(memo.strands.is_empty());
        assert!(memo.source_path.is_none());
    }

    #[test]
    fn memory_tier_serialises_snake_case() {
        let hot = serde_json::to_string(&MemoryTier::Hot).unwrap();
        let cold = serde_json::to_string(&MemoryTier::Cold).unwrap();
        assert_eq!(hot, r#""hot""#);
        assert_eq!(cold, r#""cold""#);
    }

    #[test]
    fn enriched_entry_omits_none_fields() {
        let entry = EnrichedEntry {
            path: "p".to_owned(),
            sibling: "eva".to_owned(),
            significance: None,
            strands: vec![],
            content_excerpt: None,
            created_at: None,
            frontmatter_raw: serde_json::Value::Null,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("significance"), "{json}");
        assert!(!json.contains("content_excerpt"), "{json}");
        assert!(!json.contains("frontmatter_raw"), "{json}");
    }

    #[test]
    fn promotion_event_carries_memo_id_and_path() {
        let event = PromotionEvent {
            memo_id: "m-abc".to_owned(),
            from: MemoryTier::Hot,
            to: MemoryTier::Cold,
            path: "corso/entries/2026-04-19-x.md".to_owned(),
            sibling: "corso".to_owned(),
            significance: 0.91,
            promoted_at: "2026-04-19T12:01:15Z".to_owned(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""memo_id":"m-abc""#));
        assert!(json.contains(r#""from":"hot""#));
        assert!(json.contains(r#""to":"cold""#));
    }
}
