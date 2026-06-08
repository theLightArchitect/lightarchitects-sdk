//! Pure-fn markdown section slicer.
//!
//! Extracts the body of an `h2` section from a markdown doc by **anchor
//! prefix** matching (not exact heading match). Designed for the case where
//! catalog YAML carries operator-readable anchors (`§63 — Rust patterns`)
//! while real headings shed the human-readable suffix
//! (`## §63 Untrusted-Input Operational Patterns (P1–P5)`).
//!
//! # Algorithm
//!
//! 1. Extract the **identifier prefix** from `anchor` — everything before the
//!    first ` — ` (em-dash with surrounding spaces) or ` - ` (hyphen with
//!    spaces). If neither separator is present, the whole anchor is the prefix.
//! 2. Walk the doc line-by-line; find `h2` headings (lines beginning with `## `).
//! 3. The first heading whose body starts with the prefix (case-insensitive,
//!    em-dash and hyphen normalised, leading whitespace trimmed) marks the
//!    section start.
//! 4. Capture all subsequent lines until the next `h2` heading or EOF.
//! 5. Return the captured body trimmed of leading and trailing whitespace.

/// Slice the body of the `h2` section whose heading starts with the
/// anchor's identifier prefix.
///
/// Returns `None` if no matching heading is found.
#[must_use]
pub fn slice_by_anchor_prefix(doc: &str, anchor: &str) -> Option<String> {
    let prefix = anchor_prefix(anchor);
    let mut in_section = false;
    let mut captured: Vec<&str> = Vec::new();

    for line in doc.lines() {
        if let Some(heading_body) = line.strip_prefix("## ") {
            if in_section {
                // Hit the next h2 — stop.
                break;
            }
            if heading_matches_prefix(heading_body, prefix) {
                in_section = true;
                continue;
            }
        }
        if in_section {
            captured.push(line);
        }
    }

    if !in_section {
        return None;
    }
    let body = captured.join("\n");
    let trimmed = body.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

/// Extract the leading identifier from `anchor`.
///
/// Returns everything before the first ` — ` (em-dash) or ` - ` (hyphen),
/// or the whole anchor if neither separator is present.
#[must_use]
pub fn anchor_prefix(anchor: &str) -> &str {
    if let Some(idx) = anchor.find(" — ") {
        &anchor[..idx]
    } else if let Some(idx) = anchor.find(" - ") {
        &anchor[..idx]
    } else {
        anchor
    }
    .trim()
}

/// Case-insensitive prefix match with em-dash and hyphen normalisation.
fn heading_matches_prefix(heading_body: &str, prefix: &str) -> bool {
    let h = normalise(heading_body);
    let p = normalise(prefix);
    h.starts_with(&p)
}

/// Lowercase + replace em-dash with hyphen + collapse internal whitespace
/// for prefix-comparison.
fn normalise(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            '\u{2014}' | '\u{2013}' => '-',
            other => other,
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const DOC: &str = "\
# Top\n\
\n\
## 7. Agentic Architecture Patterns\n\
body for section 7\n\
more body\n\
\n\
## §63 Untrusted-Input Operational Patterns (P1–P5)\n\
section 63 body\n\
\n\
## Part XIV — Pre-Finalization Quality Gate (C1–C8 Rubric)\n\
quality gate body\n\
\n\
## 16. File & Code Documentation Standards\n\
docs body\n\
final line\n\
";

    #[test]
    fn slice_returns_body_for_part_prefix() {
        let s = slice_by_anchor_prefix(DOC, "Part XIV — C2 cross-validation").unwrap();
        assert!(s.contains("quality gate body"));
        assert!(!s.contains("section 63"));
    }

    #[test]
    fn slice_returns_body_for_section_sigil_prefix() {
        let s = slice_by_anchor_prefix(DOC, "§63 — Rust patterns").unwrap();
        assert!(s.contains("section 63 body"));
        assert!(!s.contains("body for section 7"));
    }

    #[test]
    fn slice_returns_body_for_numeric_prefix() {
        let s = slice_by_anchor_prefix(DOC, "16. — File & Code Documentation Standards").unwrap();
        assert!(s.contains("docs body"));
        assert!(s.contains("final line"));
    }

    #[test]
    fn slice_returns_none_for_unmatched_anchor() {
        assert!(slice_by_anchor_prefix(DOC, "§999 — Bogus").is_none());
    }

    #[test]
    fn slice_stops_at_next_h2() {
        let s = slice_by_anchor_prefix(DOC, "7. — Agentic").unwrap();
        assert!(s.contains("body for section 7"));
        assert!(s.contains("more body"));
        assert!(
            !s.contains("section 63"),
            "must not bleed into next h2 body"
        );
    }

    #[test]
    fn anchor_prefix_strips_em_dash_suffix() {
        assert_eq!(anchor_prefix("§63 — Rust patterns"), "§63");
        assert_eq!(anchor_prefix("Part XIV — Quality Gate"), "Part XIV");
        assert_eq!(anchor_prefix("7. — Agentic Patterns"), "7.");
        assert_eq!(anchor_prefix("no-separator-anchor"), "no-separator-anchor");
    }

    #[test]
    fn heading_matches_em_dash_and_hyphen_variants() {
        // Heading uses em-dash; anchor uses ASCII hyphen — must still match.
        assert!(heading_matches_prefix(
            "Part XIV — Pre-Finalization Quality Gate",
            "Part XIV"
        ));
        assert!(heading_matches_prefix(
            "§63 Untrusted-Input Operational Patterns",
            "§63"
        ));
        // Case-insensitive.
        assert!(heading_matches_prefix(
            "part xiv — quality gate",
            "Part XIV"
        ));
    }

    #[test]
    fn empty_section_returns_none() {
        // Heading exists but no body before the next h2 → None.
        let doc = "## §99 Empty\n## §100 Next\nbody\n";
        assert!(slice_by_anchor_prefix(doc, "§99 — Whatever").is_none());
    }
}
