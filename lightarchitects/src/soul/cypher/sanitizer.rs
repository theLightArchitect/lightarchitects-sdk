//! Mutation guard for LLM-generated Cypher queries.
//!
//! LLM-generated queries are untrusted. This module blocks mutation keywords
//! before the query reaches the driver. The Bolt parameter protocol provides a
//! second layer: values bound via `$param` are never parsed as Cypher — this
//! sanitizer defends the **query template** itself.
//!
//! # Design
//!
//! 1. Strip quoted string literals — prevents false positives for keywords
//!    appearing inside content patterns (`CONTAINS 'call'` ≠ `CALL proc()`).
//! 2. Whole-word blocklist scan on the stripped, uppercased query.
//! 3. Structural check: first keyword must be `MATCH`, `OPTIONAL`, or `WITH`.

use thiserror::Error;

// ============================================================================
// Error
// ============================================================================

/// Error returned when a query fails the sanitizer.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SanitizeError {
    /// Query contains a mutation keyword that is not permitted.
    #[error("mutation keyword '{0}' is not permitted in LLM-generated queries")]
    MutationKeyword(String),
    /// Query is empty after trimming.
    #[error("query is empty")]
    Empty,
    /// Query does not begin with a recognised read-only structural keyword.
    #[error("query must start with MATCH, OPTIONAL MATCH, or WITH")]
    NoMatchClause,
}

// ============================================================================
// Blocked keywords
// ============================================================================

/// Keywords that indicate a mutation attempt.
///
/// All checks are **whole-word** (surrounded by non-alphanumeric characters or
/// string boundary) and **case-insensitive** (query normalised to uppercase).
/// Quoted string literals are stripped first so keywords embedded in content
/// patterns (e.g., `CONTAINS 'call'`) do not trigger false positives.
const BLOCKED: &[&str] = &[
    "CREATE", "MERGE", "DETACH", "DELETE", "SET", "REMOVE", "DROP", "LOAD", "CALL", "FOREACH",
    "IMPORT",
];

// ============================================================================
// Public API
// ============================================================================

/// Validate that a Cypher string is read-only.
///
/// Strips quoted string literals, uppercases the result, then checks for
/// mutation keywords using a whole-word scan. Also verifies that the first
/// structural keyword is `MATCH`, `OPTIONAL`, or `WITH`.
///
/// # Errors
///
/// Returns [`SanitizeError`] on the first violation found.
pub fn sanitize(cypher: &str) -> Result<(), SanitizeError> {
    let trimmed = cypher.trim();
    if trimmed.is_empty() {
        return Err(SanitizeError::Empty);
    }

    // Strip quoted literals before keyword scan to avoid false positives such
    // as `CONTAINS 'call'` triggering the CALL check.
    let stripped = strip_string_literals(trimmed);
    let upper = stripped.to_uppercase();

    for &kw in BLOCKED {
        if contains_word(&upper, kw) {
            return Err(SanitizeError::MutationKeyword(kw.to_owned()));
        }
    }

    // Structural check uses the original (not stripped) uppercased query so
    // that the real first keyword (MATCH / OPTIONAL / WITH) is preserved.
    let orig_upper = trimmed.to_uppercase();
    let first = orig_upper.split_whitespace().next().unwrap_or("");
    if first != "MATCH" && first != "OPTIONAL" && first != "WITH" {
        return Err(SanitizeError::NoMatchClause);
    }

    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

/// Remove single-quoted and double-quoted string literals from a Cypher string.
///
/// Each literal is replaced with a single space character to preserve the word
/// boundaries around the surrounding Cypher tokens. Backslash-escaped quotes
/// inside literals are skipped correctly.
fn strip_string_literals(cypher: &str) -> String {
    let mut out = String::with_capacity(cypher.len());
    let mut chars = cypher.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\'' || ch == '"' {
            let quote = ch;
            // Consume the literal body.
            loop {
                match chars.next() {
                    None => break,
                    Some('\\') => {
                        chars.next(); // skip escaped char
                    }
                    Some(c) if c == quote => break,
                    Some(_) => {}
                }
            }
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

/// Whole-word keyword check in an already-uppercased string.
///
/// Returns `true` only when `keyword` appears surrounded by non-alphanumeric
/// characters (or the string boundary) to avoid false matches like
/// `"DATASET"` containing `"SET"`.
fn contains_word(upper: &str, keyword: &str) -> bool {
    let kw = keyword.as_bytes();
    let src = upper.as_bytes();
    let kw_len = kw.len();
    let src_len = src.len();

    if kw_len > src_len {
        return false;
    }

    let limit = src_len - kw_len;
    let mut i = 0usize;
    while i <= limit {
        if src[i..i + kw_len] == *kw {
            let before_ok = i == 0 || !src[i - 1].is_ascii_alphanumeric();
            let after = i + kw_len;
            let after_ok = after >= src_len || !src[after].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_match_query_accepted() {
        assert!(
            sanitize(
                "MATCH (s:Step {helix_id: $helix_id}) \
             WHERE toLower(s.content) CONTAINS $kw \
             RETURN DISTINCT s.title AS session_id"
            )
            .is_ok()
        );
    }

    #[test]
    fn with_clause_accepted() {
        assert!(sanitize("WITH 1 AS x MATCH (n) WHERE n.id = x RETURN n").is_ok());
    }

    #[test]
    fn optional_match_accepted() {
        assert!(sanitize("OPTIONAL MATCH (s:Step) WHERE s.id = $id RETURN s.title").is_ok());
    }

    #[test]
    fn create_blocked() {
        assert_eq!(
            sanitize("CREATE (n:Node {id: 'x'}) RETURN n"),
            Err(SanitizeError::MutationKeyword("CREATE".into()))
        );
    }

    #[test]
    fn delete_blocked() {
        assert_eq!(
            sanitize("MATCH (n) DELETE n"),
            Err(SanitizeError::MutationKeyword("DELETE".into()))
        );
    }

    #[test]
    fn detach_blocked() {
        assert_eq!(
            sanitize("MATCH (n) DETACH DELETE n"),
            Err(SanitizeError::MutationKeyword("DETACH".into()))
        );
    }

    #[test]
    fn merge_blocked() {
        assert_eq!(
            sanitize("MERGE (n:Step {id: $id}) RETURN n"),
            Err(SanitizeError::MutationKeyword("MERGE".into()))
        );
    }

    #[test]
    fn set_blocked_as_keyword() {
        assert_eq!(
            sanitize("MATCH (n) SET n.x = 1 RETURN n"),
            Err(SanitizeError::MutationKeyword("SET".into()))
        );
    }

    #[test]
    fn contains_set_string_not_blocked() {
        // "set" inside a quoted literal should NOT trigger the SET check.
        assert!(
            sanitize(
                "MATCH (s:Step {helix_id: $hid}) \
                 WHERE toLower(s.content) CONTAINS 'set' \
                 RETURN DISTINCT s.title"
            )
            .is_ok()
        );
    }

    #[test]
    fn contains_call_string_not_blocked() {
        // "call" inside a quoted literal should NOT trigger the CALL check.
        assert!(
            sanitize(
                "MATCH (s:Step {helix_id: $hid}) \
                 WHERE toLower(s.content) CONTAINS 'call' \
                 RETURN DISTINCT s.title"
            )
            .is_ok()
        );
    }

    #[test]
    fn dataset_not_blocked_by_set() {
        // Property name "dataset" in a WHERE clause is not the SET keyword.
        assert!(sanitize("MATCH (s:Step) WHERE s.dataset = $d RETURN s").is_ok());
    }

    #[test]
    fn empty_query_rejected() {
        assert_eq!(sanitize(""), Err(SanitizeError::Empty));
        assert_eq!(sanitize("   "), Err(SanitizeError::Empty));
    }

    #[test]
    fn return_only_rejected() {
        assert_eq!(sanitize("RETURN 1"), Err(SanitizeError::NoMatchClause));
    }

    #[test]
    fn call_procedure_blocked() {
        assert_eq!(
            sanitize("MATCH (n) WITH n CALL db.index.fulltext.search() YIELD node RETURN node"),
            Err(SanitizeError::MutationKeyword("CALL".into()))
        );
    }
}
