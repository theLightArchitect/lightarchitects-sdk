//! Wikilink parser — extracts `[[target]]` and `[[target|label]]` patterns.
//!
//! Used by [`MarkdownVaultIngester`] to discover inter-step links within content.

use regex::Regex;
use std::sync::OnceLock;

// ============================================================================
// WikilinkMatch
// ============================================================================

/// A parsed wikilink from markdown content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikilinkMatch {
    /// The link target (path or title).
    pub target: String,
    /// Optional display label (from `[[target|label]]` syntax).
    pub label: Option<String>,
    /// The raw matched text including brackets.
    pub raw: String,
}

// ============================================================================
// Parser
// ============================================================================

/// Get or compile the wikilink regex.
#[allow(clippy::expect_used)] // static regex literal — infallible at runtime
fn wikilink_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\[\[([^\]\|]+?)(?:\|([^\]]+?))?\]\]").expect("valid wikilink regex")
    })
}

/// Extract all wikilinks from markdown content.
#[must_use]
pub fn extract(content: &str) -> Vec<WikilinkMatch> {
    wikilink_re()
        .captures_iter(content)
        .map(|cap| {
            let target = cap[1].trim().to_owned();
            let label = cap.get(2).map(|m| m.as_str().trim().to_owned());
            let raw = cap[0].to_owned();
            WikilinkMatch { target, label, raw }
        })
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_wikilink() {
        let links = extract("See [[eva/identity]] for details.");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "eva/identity");
        assert!(links[0].label.is_none());
        assert_eq!(links[0].raw, "[[eva/identity]]");
    }

    #[test]
    fn test_labeled_wikilink() {
        let links = extract("Refer to [[eva/identity|EVA's identity doc]].");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "eva/identity");
        assert_eq!(links[0].label.as_deref(), Some("EVA's identity doc"));
    }

    #[test]
    fn test_multiple_wikilinks() {
        let links = extract("Link to [[a]] and [[b|B Label]] and [[c]].");
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].target, "a");
        assert_eq!(links[1].target, "b");
        assert_eq!(links[1].label.as_deref(), Some("B Label"));
        assert_eq!(links[2].target, "c");
    }

    #[test]
    fn test_no_wikilinks() {
        let links = extract("No links in this text.");
        assert!(links.is_empty());
    }

    #[test]
    fn test_empty_content() {
        let links = extract("");
        assert!(links.is_empty());
    }

    #[test]
    fn test_nested_brackets_ignored() {
        let links = extract("Some [regular] brackets and [[valid]].");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "valid");
    }
}
