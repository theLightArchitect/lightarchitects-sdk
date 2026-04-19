//! Promotion-gate enrichment — evaluates whether a hot memo qualifies for cold storage.
//!
//! This is the Rust-side half of the `ContextMemo → EnrichedEntry` transition.
//! Callers pass a [`ContextMemo`] and receive either:
//! - `Ok(EnrichedEntry)` — memo crossed the significance threshold; the returned
//!   entry carries the projected path + front-matter suitable for helix write.
//! - `Err(EnrichError::NotPromoted)` — below threshold; the memo stays hot.
//!
//! # Why not `SoulClient`?
//!
//! `SoulClient::search()` in the SDK is an MCP client that requires the SOUL
//! daemon to be running over stdio transport. The webshell's memory layer
//! operates directly on the filesystem helix, so we skip the MCP hop to avoid
//! adding a runtime dependency on the SOUL binary.

use chrono::Utc;

use super::types::{ContextMemo, EnrichedEntry};

/// Significance threshold above which a memo is promoted from hot to cold.
///
/// Matches the `>= 7.0` gate (rescaled to `[0, 1]`) used in
/// `SiblingPromoter::promotion_reason_for` for
/// `SignificantReflection` weight handling.
pub const PROMOTION_THRESHOLD: f32 = 0.7;

/// Maximum excerpt length preserved in the `EnrichedEntry`. Matches
/// [`crate::memory::hot::CONTENT_MAX_CHARS`] so the wire shape is consistent.
const EXCERPT_MAX_CHARS: usize = 280;

/// Why a memo failed to enrich.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum EnrichError {
    /// Significance below [`PROMOTION_THRESHOLD`] — not a promotion candidate.
    #[error("significance {0} below promotion threshold {1}")]
    NotPromoted(f32, f32),
}

/// Evaluate a memo and build an `EnrichedEntry` if it passes the promotion gate.
///
/// The produced `EnrichedEntry` projects a canonical helix path:
/// `{sibling}/entries/{YYYY-MM-DD}-{memo_id_stem}.md`
///
/// where `memo_id_stem` is derived from `memo.id` by splitting on `:` and
/// taking the first segment (session id for hot memos, path stem for cold).
///
/// The returned `frontmatter_raw` shape mirrors the YAML front-matter format
/// that `SiblingPromoter` writes:
///
/// ```yaml
/// sibling: {sibling}
/// significance: {0.0-1.0}
/// strands: [...]
/// created_at: {ISO-8601}
/// ```
///
/// # Errors
///
/// Returns [`EnrichError::NotPromoted`] if `memo.significance < PROMOTION_THRESHOLD`.
#[allow(clippy::missing_errors_doc)]
pub fn enrich(memo: &ContextMemo) -> Result<EnrichedEntry, EnrichError> {
    if memo.significance < PROMOTION_THRESHOLD {
        return Err(EnrichError::NotPromoted(
            memo.significance,
            PROMOTION_THRESHOLD,
        ));
    }

    let date = Utc::now().format("%Y-%m-%d").to_string();
    let stem = memo.id.split(':').next().unwrap_or("memo");
    let path = format!("{}/entries/{}-{}.md", memo.sibling, date, stem);

    let excerpt = truncate_chars(&memo.content, EXCERPT_MAX_CHARS);

    let frontmatter_raw = serde_json::json!({
        "sibling": memo.sibling,
        "significance": memo.significance,
        "strands": memo.strands,
        "created_at": memo.created_at,
    });

    Ok(EnrichedEntry {
        path,
        sibling: memo.sibling.clone(),
        significance: Some(memo.significance),
        strands: memo.strands.clone(),
        content_excerpt: Some(excerpt),
        created_at: Some(memo.created_at.clone()),
        frontmatter_raw,
    })
}

/// Async alias — keeps the API consistent with the rest of the memory module
/// (callers may wrap enrichment inside a `tokio::spawn_blocking` for heavier
/// implementations later).
///
/// # Errors
///
/// See [`enrich`].
pub async fn enrich_async(memo: &ContextMemo) -> Result<EnrichedEntry, EnrichError> {
    enrich(memo)
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push('…');
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::memory::types::MemoryTier;

    fn memo(significance: f32) -> ContextMemo {
        ContextMemo {
            id: "session-1:42".to_owned(),
            tier: MemoryTier::Hot,
            content: "A reflection worth remembering".to_owned(),
            significance,
            sibling: "corso".to_owned(),
            strands: vec!["methodical".to_owned()],
            created_at: "2026-04-19T12:00:00Z".to_owned(),
            source_path: None,
            resonance: Vec::new(),
            themes: Vec::new(),
            self_defining: false,
            entry_type: None,
        }
    }

    #[test]
    fn below_threshold_returns_not_promoted() {
        let result = enrich(&memo(0.5));
        assert!(matches!(result, Err(EnrichError::NotPromoted(_, _))));
    }

    #[test]
    fn at_threshold_succeeds() {
        let result = enrich(&memo(PROMOTION_THRESHOLD));
        assert!(result.is_ok());
    }

    #[test]
    fn above_threshold_projects_canonical_path() {
        let entry = enrich(&memo(0.85)).unwrap();
        assert!(entry.path.starts_with("corso/entries/"));
        assert!(entry.path.ends_with("-session-1.md"));
        assert_eq!(entry.sibling, "corso");
        assert_eq!(entry.significance, Some(0.85));
        assert_eq!(entry.strands, vec!["methodical".to_owned()]);
    }

    #[test]
    fn frontmatter_raw_contains_expected_fields() {
        // Use 0.75 — exactly representable in f32, avoids f32→f64 coercion drift
        // that makes 0.9f32 serialize as 0.8999999761581421.
        let entry = enrich(&memo(0.75)).unwrap();
        assert_eq!(entry.frontmatter_raw["sibling"], "corso");
        assert_eq!(entry.frontmatter_raw["significance"], 0.75);
        assert_eq!(entry.frontmatter_raw["strands"][0], "methodical");
    }

    #[test]
    fn content_excerpt_truncates_long_content() {
        let mut m = memo(0.9);
        m.content = "x".repeat(500);
        let entry = enrich(&m).unwrap();
        let excerpt = entry.content_excerpt.unwrap();
        assert_eq!(excerpt.chars().count(), EXCERPT_MAX_CHARS + 1); // +ellipsis
        assert!(excerpt.ends_with('…'));
    }

    #[tokio::test]
    async fn enrich_async_matches_enrich() {
        let m = memo(0.8);
        let sync = enrich(&m).unwrap();
        let asyn = enrich_async(&m).await.unwrap();
        assert_eq!(sync.path, asyn.path);
        assert_eq!(sync.significance, asyn.significance);
    }
}
