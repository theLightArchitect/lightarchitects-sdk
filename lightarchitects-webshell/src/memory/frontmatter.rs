//! YAML front-matter parser for helix entry files.
//!
//! Every helix entry follows this shape:
//!
//! ```markdown
//! ---
//! id: {uuid}
//! date: 2026-04-19
//! sibling: eva
//! significance: 7.5
//! strands:
//!   - Methodical
//!   - Contextual
//! ---
//!
//! body text…
//! ```
//!
//! The parser is intentionally permissive: any file that doesn't start with
//! the literal `---\n` delimiter is treated as a bodyless entry (`None` for
//! every enrichment field). Malformed YAML inside the front-matter falls back
//! the same way. The goal is never to block an SSE event over a bad file.

use std::path::Path;

use tokio::io::AsyncReadExt;

use super::types::EnrichedEntry;

/// Soft cap on excerpt length. 280 chars is the Twitter-era UI preview budget.
const EXCERPT_MAX_CHARS: usize = 280;

/// Enriched projection of a helix entry file.
///
/// Strictly-fallible `Option` fields reflect the reality that vault entries
/// accumulated over years may have inconsistent front-matter shapes. Callers
/// should treat missing fields as non-fatal.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrontMatterFields {
    /// Owning sibling.
    pub sibling: Option<String>,
    /// Significance in `[0.0, 1.0]` (rescaled from the YAML `0..10` convention).
    pub significance: Option<f32>,
    /// Lowercased strand tags.
    pub strands: Vec<String>,
    /// ISO-8601 UTC timestamp — from front-matter `date:` (promoted to RFC3339).
    pub created_at: Option<String>,
    /// Typed classification from front-matter `type:` — Phase 14.1.
    ///
    /// Populated from the YAML `type:` key. Common canonical values:
    /// `entry`, `plan`, `standard`, `review`, `lesson`, `reference`,
    /// `scrum-assessment`. `None` when the field is absent.
    pub entry_type: Option<String>,
    /// Raw front-matter YAML as a JSON value (null when absent or malformed).
    pub raw: serde_json::Value,
}

/// Parse a raw markdown string into front-matter fields + body excerpt.
///
/// Returns `(fields, body_excerpt)` where `body_excerpt` is the first 280 chars
/// of the body after the closing `---` delimiter. Both halves are best-effort:
/// a file with no front-matter returns `(FrontMatterFields::default(), excerpt_of_full_text)`.
#[must_use]
pub fn parse(source: &str) -> (FrontMatterFields, Option<String>) {
    let Some(rest) = source.strip_prefix("---\n") else {
        // No front-matter delimiter — treat the whole file as body.
        return (FrontMatterFields::default(), Some(excerpt(source)));
    };

    let Some(end_idx) = rest.find("\n---\n") else {
        return (FrontMatterFields::default(), Some(excerpt(source)));
    };

    let (yaml, body) = rest.split_at(end_idx);
    let body = body.strip_prefix("\n---\n").unwrap_or(body);

    let raw: serde_json::Value = serde_yaml::from_str::<serde_yaml::Value>(yaml)
        .ok()
        .and_then(|v| serde_json::to_value(v).ok())
        .unwrap_or(serde_json::Value::Null);

    let fields = FrontMatterFields {
        sibling: raw
            .get("sibling")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        significance: raw
            .get("significance")
            .and_then(serde_json::Value::as_f64)
            .map(normalise_significance),
        strands: raw
            .get("strands")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_lowercase))
                    .collect()
            })
            .unwrap_or_default(),
        created_at: raw.get("date").and_then(|v| v.as_str()).map(|date| {
            // Front-matter dates are typically `YYYY-MM-DD`; promote to
            // RFC3339 UTC by appending the T00:00:00Z boundary.
            if date.len() == 10 {
                format!("{date}T00:00:00Z")
            } else {
                date.to_owned()
            }
        }),
        entry_type: raw
            .get("type")
            .and_then(|v| v.as_str())
            .map(str::to_lowercase),
        raw,
    };

    (fields, Some(excerpt(body)))
}

/// Async helper — reads the file at `path`, parses it, returns an `EnrichedEntry`.
///
/// Derives `sibling` from the path's top-level directory when front-matter
/// doesn't supply it. Returns `None` if the file cannot be read.
#[allow(clippy::missing_errors_doc)]
pub async fn enrich_file(helix_rel_path: &str, abs_path: &Path) -> Option<EnrichedEntry> {
    let mut file = tokio::fs::File::open(abs_path).await.ok()?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).await.ok()?;

    let (fields, body) = parse(&buf);

    let sibling_from_path = helix_rel_path
        .split('/')
        .next()
        .map(str::to_owned)
        .unwrap_or_default();

    let sibling = fields.sibling.clone().unwrap_or(sibling_from_path);

    Some(EnrichedEntry {
        path: helix_rel_path.to_owned(),
        sibling,
        significance: fields.significance,
        strands: fields.strands,
        content_excerpt: body,
        created_at: fields.created_at,
        frontmatter_raw: fields.raw,
    })
}

/// Normalise a significance value from the YAML convention (0–10) into `[0, 1]`.
///
/// A value already between 0 and 1 is returned unchanged. Values above 1.0 are
/// divided by 10. Values above 10 are clamped to 1.0.
#[allow(clippy::cast_possible_truncation)]
fn normalise_significance(v: f64) -> f32 {
    let normalised = if v <= 1.0 {
        v
    } else if v <= 10.0 {
        v / 10.0
    } else {
        1.0
    };
    normalised.max(0.0) as f32
}

fn excerpt(body: &str) -> String {
    let trimmed = body.trim_start();
    let mut out: String = trimmed.chars().take(EXCERPT_MAX_CHARS).collect();
    if trimmed.chars().count() > EXCERPT_MAX_CHARS {
        out.push('…');
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    const SAMPLE: &str = "---\nid: abc-123\ndate: 2026-04-19\nsibling: eva\nsignificance: 8.5\nstrands:\n  - Methodical\n  - Contextual\n---\n\nThis is the body text of the entry.";

    #[test]
    fn parses_complete_frontmatter() {
        let (fields, body) = parse(SAMPLE);
        assert_eq!(fields.sibling.as_deref(), Some("eva"));
        assert_eq!(fields.significance, Some(0.85)); // 8.5 / 10
        assert_eq!(fields.strands, vec!["methodical", "contextual"]);
        assert_eq!(fields.created_at.as_deref(), Some("2026-04-19T00:00:00Z"));
        assert!(body.as_ref().unwrap().starts_with("This is the body"));
    }

    #[test]
    fn parses_zero_significance() {
        let src = "---\nsignificance: 0.0\nsibling: corso\n---\nbody";
        let (fields, _) = parse(src);
        assert_eq!(fields.significance, Some(0.0));
    }

    #[test]
    fn no_delimiter_treats_full_text_as_body() {
        let (fields, body) = parse("Just some plain text.");
        assert!(fields.sibling.is_none());
        assert!(fields.significance.is_none());
        assert!(fields.strands.is_empty());
        assert_eq!(body.as_deref(), Some("Just some plain text."));
    }

    #[test]
    fn malformed_yaml_degrades_gracefully() {
        // The content between --- markers is invalid YAML (tabs, unclosed
        // quotes). Parser should still produce a valid bodyless projection.
        let src = "---\n\t\tnot: \"valid\nyaml\n---\nbody after";
        let (fields, body) = parse(src);
        assert!(fields.sibling.is_none());
        assert!(body.is_some());
    }

    #[test]
    fn strands_are_lowercased() {
        let src = "---\nstrands:\n  - Analytical\n  - PRECISION\n---\nbody";
        let (fields, _) = parse(src);
        assert_eq!(fields.strands, vec!["analytical", "precision"]);
    }

    #[test]
    fn excerpt_truncates_long_body() {
        let body = "x".repeat(500);
        let src = format!("---\nsibling: eva\n---\n{body}");
        let (_, excerpt_body) = parse(&src);
        let excerpt = excerpt_body.unwrap();
        assert!(excerpt.ends_with('…'));
        assert_eq!(excerpt.chars().count(), EXCERPT_MAX_CHARS + 1);
    }

    #[test]
    fn significance_rescales_common_ranges() {
        assert_eq!(normalise_significance(0.0), 0.0);
        assert_eq!(normalise_significance(0.85), 0.85);
        assert_eq!(normalise_significance(7.5), 0.75);
        assert_eq!(normalise_significance(10.0), 1.0);
        assert_eq!(normalise_significance(99.9), 1.0);
        assert_eq!(normalise_significance(-1.0), 0.0);
    }

    #[tokio::test]
    async fn enrich_file_derives_sibling_from_path_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("note.md");
        tokio::fs::write(&file_path, "just body").await.unwrap();
        let entry = enrich_file("corso/entries/note.md", &file_path)
            .await
            .unwrap();
        // Path-derived sibling — front-matter was absent.
        assert_eq!(entry.sibling, "corso");
        assert!(entry.content_excerpt.is_some());
    }

    #[tokio::test]
    async fn enrich_file_returns_none_on_missing_file() {
        let entry = enrich_file("x/entries/absent.md", Path::new("/nonexistent-xyz")).await;
        assert!(entry.is_none());
    }
}
