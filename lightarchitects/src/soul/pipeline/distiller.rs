//! Retrieval result distiller — reduces a ranked list of hits into a
//! context string suitable for LLM consumption.
//!
//! [`RetrievalDistiller`] sorts, deduplicates, truncates, and formats
//! [`RetrievalHit`][crate::soul::pipeline::RetrievalHit] results into a
//! [`DistilledContext`] ready for prompt injection.

use std::collections::HashSet;

use crate::soul::pipeline::RetrievalHit;

// ============================================================================
// SortBy
// ============================================================================

/// Sort order for distilled results.
#[derive(Debug, Clone, Copy, Default)]
pub enum SortBy {
    /// Sort by final RRF-fused score (highest first).
    #[default]
    Score,
    /// Sort by entry date (most recent first).
    Date,
    /// Sort by entry weight / significance (highest first).
    Weight,
}

// ============================================================================
// DistillerConfig
// ============================================================================

/// Configuration for [`RetrievalDistiller`].
#[derive(Debug, Clone)]
pub struct DistillerConfig {
    /// Maximum number of entries in the distilled context.
    pub max_entries: usize,
    /// Maximum total characters in the context string.
    pub max_chars: usize,
    /// Cosine similarity threshold for content deduplication.
    ///
    /// `0.0` disables deduplication. `0.9` aggressively removes near-duplicates.
    /// The distiller uses a simple content-hash dedup (not cosine) — this field
    /// controls whether *any* dedup is applied.
    pub dedup_threshold: f32,
    /// How to order entries before building the context string.
    pub sort_by: SortBy,
}

impl Default for DistillerConfig {
    fn default() -> Self {
        Self {
            max_entries: 10,
            max_chars: 4096,
            dedup_threshold: 0.0,
            sort_by: SortBy::Score,
        }
    }
}

// ============================================================================
// DistilledContext
// ============================================================================

/// Result of distilling a ranked list of retrieval hits.
///
/// Contains the top-K entries after sorting and deduplication, plus
/// a pre-formatted context string ready for LLM prompt injection.
#[derive(Debug, Clone)]
pub struct DistilledContext {
    /// The selected retrieval hits (sorted, deduped, truncated).
    pub entries: Vec<RetrievalHit>,
    /// Pre-formatted context string for LLM consumption.
    pub context_string: String,
    /// Approximate token count (`context_string.len() / 4`).
    pub token_estimate: usize,
}

// ============================================================================
// RetrievalDistiller
// ============================================================================

/// Distills a list of retrieval hits into an LLM-ready context string.
///
/// Applies sorting, content-hash deduplication, entry-count truncation,
/// character-budget truncation, and context string assembly.
pub struct RetrievalDistiller;

impl RetrievalDistiller {
    /// Distill a ranked list of hits into a [`DistilledContext`].
    ///
    /// Steps:
    /// 1. Sort by `config.sort_by`.
    /// 2. Content-hash deduplication (when `dedup_threshold > 0.0`).
    /// 3. Truncate to `config.max_entries`.
    /// 4. Build context string (title + first 200 chars of content, `\n---\n` separated).
    /// 5. Truncate context string to `config.max_chars`.
    /// 6. Compute `token_estimate = context_string.len() / 4`.
    #[must_use]
    pub fn distill(
        hits: &[RetrievalHit],
        _query: &str,
        config: &DistillerConfig,
    ) -> DistilledContext {
        // Step 1: sort a cloned vec.
        let mut sorted = hits.to_vec();
        Self::sort_hits(&mut sorted, config.sort_by);

        // Step 2: dedup by content hash when enabled.
        let deduped = if config.dedup_threshold > 0.0 {
            Self::dedup_by_content_hash(sorted)
        } else {
            sorted
        };

        // Step 3: truncate to max_entries.
        let truncated: Vec<RetrievalHit> = deduped.into_iter().take(config.max_entries).collect();

        // Step 4: build context string.
        let raw_context = Self::build_context(&truncated);

        // Step 5: truncate to max_chars.
        let context_string = if raw_context.len() <= config.max_chars {
            raw_context
        } else {
            Self::truncate_at_char_boundary(&raw_context, config.max_chars)
        };

        // Step 6: estimate tokens.
        let token_estimate = context_string.len() / 4;

        DistilledContext {
            entries: truncated,
            context_string,
            token_estimate,
        }
    }

    /// Sort hits in-place by the configured [`SortBy`] order.
    fn sort_hits(hits: &mut [RetrievalHit], sort_by: SortBy) {
        match sort_by {
            SortBy::Score => {
                hits.sort_by(|a, b| {
                    b.final_score
                        .partial_cmp(&a.final_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SortBy::Date => {
                hits.sort_by(|a, b| b.entry.created_at.cmp(&a.entry.created_at));
            }
            SortBy::Weight => {
                hits.sort_by(|a, b| {
                    b.entry
                        .significance
                        .partial_cmp(&a.entry.significance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }
    }

    /// Deduplicate hits by a simple hash of entry content.
    ///
    /// First occurrence of each unique content hash is kept; subsequent
    /// entries with the same hash are discarded.
    fn dedup_by_content_hash(hits: Vec<RetrievalHit>) -> Vec<RetrievalHit> {
        let mut seen: HashSet<u64> = HashSet::new();
        hits.into_iter()
            .filter(|h| seen.insert(Self::hash_content(&h.entry.content)))
            .collect()
    }

    /// Compute a simple 64-bit content hash (FNV-1a inspired).
    fn hash_content(content: &str) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in content.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        hash
    }

    /// Build a formatted context string from selected hits.
    ///
    /// Format: `[N] {title}\n{excerpt}\n---\n`
    fn build_context(hits: &[RetrievalHit]) -> String {
        hits.iter()
            .enumerate()
            .map(|(i, h)| {
                let title = h.entry.title.as_deref().unwrap_or("(untitled)");
                let excerpt: String = h.entry.content.chars().take(200).collect();
                format!("[{}] {}\n{}", i.saturating_add(1), title, excerpt)
            })
            .collect::<Vec<_>>()
            .join("\n---\n")
    }

    /// Truncate a string to at most `max_len` bytes, respecting UTF-8 char boundaries.
    fn truncate_at_char_boundary(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            return s.to_owned();
        }
        // Walk char boundaries until we would exceed max_len.
        let mut end = 0;
        for (i, _) in s.char_indices() {
            if i > max_len {
                break;
            }
            end = i;
        }
        s[..end].to_owned()
    }
}
