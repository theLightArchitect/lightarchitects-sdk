//! FTS5 query expression builder.
//!
//! Converts a natural-language query into a `SQLite` FTS5 OR-expression suitable
//! for passing to [`crate::storage::StorageBackend::search_bm25`].
//!
//! # Algorithm
//!
//! 1. Replaces non-alphanumeric characters with spaces.
//! 2. Splits on whitespace, filters stop words and tokens shorter than 3 chars.
//! 3. Takes the first 8 surviving tokens.
//! 4. Deduplicates using a `HashSet`.
//! 5. Falls back to the first 3 raw tokens (unfiltered) if all tokens were filtered out.
//! 6. Joins surviving tokens with ` OR `.

use std::collections::HashSet;

// ============================================================================
// Stop words
// ============================================================================

/// Common English stop words excluded from FTS5 expressions.
const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with", "by",
    "from", "as", "is", "was", "are", "were", "be", "been", "being", "have", "has", "had", "do",
    "does", "did", "will", "would", "shall", "should", "may", "might", "can", "could", "i", "me",
    "my", "we", "our", "you", "your", "he", "she", "it", "they", "them", "their", "this", "that",
    "these", "those", "what", "which", "who", "whom", "when", "where", "why", "how", "not", "no",
    "up", "out", "if", "so", "just", "about", "than", "then", "there", "here",
];

// ============================================================================
// fts5_or_expr
// ============================================================================

/// Build a `SQLite` FTS5 `OR` expression from a natural-language query.
///
/// Applies stop-word filtering, minimum length of 3 characters, and
/// deduplication. Falls back to the first 3 raw tokens when all tokens
/// are filtered out.
///
/// # Examples
///
/// ```
/// use lightarchitects_soul::pipeline::fts5_or_expr;
///
/// let expr = fts5_or_expr("consciousness and identity breakthroughs");
/// assert!(!expr.is_empty());
/// assert!(expr.contains("consciousness"));
/// assert!(expr.contains("identity"));
/// ```
#[must_use]
pub fn fts5_or_expr(query: &str) -> String {
    if query.is_empty() {
        return String::new();
    }

    // Step 1: normalise — replace non-alphanumeric chars with space.
    let normalised: String = query
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();

    let stop_set: HashSet<&str> = STOP_WORDS.iter().copied().collect();

    // Step 2: tokenise, filter stop words + short tokens.
    let filtered: Vec<String> = normalised
        .split_whitespace()
        .filter(|t| {
            let lower = t.to_lowercase();
            t.len() >= 3 && !stop_set.contains(lower.as_str())
        })
        .map(str::to_lowercase)
        .collect();

    // Step 3: dedup first (HashSet preserving first-seen order), then cap at 8.
    // NOTE: dedup before take is intentional — taking 8 first then deduping would
    // discard unique tokens that appear after 8 repeated tokens. (QUANTUM Q3 fix)
    let mut seen: HashSet<String> = HashSet::new();
    let deduped: Vec<String> = filtered
        .into_iter()
        .filter(|t| seen.insert(t.clone()))
        .take(8)
        .collect();

    if !deduped.is_empty() {
        return deduped.join(" OR ");
    }

    // Step 5: fallback — first 3 tokens from the *normalised* string to avoid
    // FTS5-unsafe characters leaking through. (QUANTUM Q4 fix)
    let fallback: Vec<String> = normalised
        .split_whitespace()
        .take(3)
        .map(str::to_lowercase)
        .collect();

    fallback.join(" OR ")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_query_returns_empty() {
        let result = fts5_or_expr("");
        assert!(result.is_empty(), "empty query should return empty string");
    }

    #[test]
    fn test_all_stop_words_falls_back_to_raw() {
        // "a", "the", "is" are all stop words — should fall back to first 3 raw tokens.
        let result = fts5_or_expr("a the is");
        assert!(
            !result.is_empty(),
            "stop-word-only query should produce fallback result"
        );
    }

    #[test]
    fn test_normal_query_filters_stop_words() {
        let result = fts5_or_expr("consciousness and identity breakthroughs");
        assert!(
            result.contains("consciousness"),
            "consciousness should be in result: {result}"
        );
        assert!(
            result.contains("identity"),
            "identity should be in result: {result}"
        );
        assert!(
            result.contains("breakthroughs"),
            "breakthroughs should be in result: {result}"
        );
        // "and" is a stop word — must be excluded.
        assert!(
            !result.split(" OR ").any(|t| t == "and"),
            "'and' (stop word) must not appear as a term: {result}"
        );
    }

    #[test]
    fn test_dedup_removes_repeated_terms() {
        // "star star stars" — "star" appears twice, should only appear once.
        let result = fts5_or_expr("star star stars");
        let terms: Vec<&str> = result.split(" OR ").collect();
        let unique: HashSet<&&str> = terms.iter().collect();
        assert_eq!(
            terms.len(),
            unique.len(),
            "duplicate terms should be removed: {result}"
        );
    }

    #[test]
    fn test_max_eight_terms() {
        let result = fts5_or_expr("alpha bravo charlie delta echo foxtrot golf hotel india");
        let terms: Vec<&str> = result.split(" OR ").collect();
        assert!(
            terms.len() <= 8,
            "result should contain at most 8 terms, got {}: {result}",
            terms.len()
        );
    }

    #[test]
    fn test_normalises_punctuation() {
        let result = fts5_or_expr("consciousness! identity? breakthrough.");
        assert!(
            !result.contains('!') && !result.contains('?') && !result.contains('.'),
            "punctuation should be removed: {result}"
        );
    }

    #[test]
    fn test_short_tokens_filtered() {
        // "hi" is 2 chars — below min length 3, must be excluded.
        // "bye" is 3 chars — passes the filter.
        // "consciousness" is a long word — passes the filter.
        let result = fts5_or_expr("hi bye consciousness");
        assert!(
            !result.split(" OR ").any(|t| t == "hi"),
            "'hi' (2 chars) must not appear: {result}"
        );
        // "bye" and "consciousness" should appear (both >= 3 chars, not stop words).
        assert!(
            result.split(" OR ").any(|t| t == "consciousness"),
            "'consciousness' should appear: {result}"
        );
    }

    #[test]
    fn test_or_separator_present() {
        let result = fts5_or_expr("consciousness identity");
        assert!(
            result.contains(" OR "),
            "terms should be joined with ' OR ': {result}"
        );
    }
}
