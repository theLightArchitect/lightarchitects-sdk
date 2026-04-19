//! YAML frontmatter parser for markdown files.
//!
//! Extracts structured metadata from `---` delimited frontmatter blocks
//! at the top of markdown files.

use std::collections::HashMap;
use std::fmt;

use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

// ============================================================================
// Frontmatter
// ============================================================================

/// Parsed YAML frontmatter from a markdown helix entry.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Frontmatter {
    /// Sibling or owner name.
    pub sibling: Option<String>,
    /// Entry title.
    pub title: Option<String>,
    /// Date of the entry (YYYY-MM-DD).
    pub date: Option<String>,
    /// Significance score (0.0-10.0).
    pub significance: Option<f64>,
    /// Strand names.
    #[serde(default)]
    pub strands: Vec<String>,
    /// Resonance tags (emotional qualities, energy states).
    ///
    /// Accepts both `resonance:` and legacy `emotions:` keys via serde alias.
    #[serde(default, alias = "emotions")]
    pub resonance: Vec<String>,
    /// Themes — accepts plain strings or `[[wikilink|Display]]` nested arrays.
    #[serde(default, deserialize_with = "deser_flat_string_vec")]
    pub themes: Vec<String>,
    /// Epoch name.
    pub epoch: Option<String>,
    /// Whether this is a self-defining moment.
    pub self_defining: Option<bool>,
    /// Entry number (for indexed ordering).
    pub entry_number: Option<i64>,
    /// Convergence points with other helixes.
    ///
    /// Many entries use `convergence: false` or `convergence: true` as a boolean
    /// flag rather than an array of refs. This deserializer accepts both forms:
    /// `bool` → empty `Vec` (treated as "no explicit convergence refs"),
    /// `[...]` → `Vec<ConvergenceRef>`.
    #[serde(default, deserialize_with = "deser_convergence")]
    pub convergence: Vec<ConvergenceRef>,
    /// Typed link references.
    #[serde(default)]
    pub links: Vec<LinkRef>,
    /// Privacy tier for this entry.
    ///
    /// Controls whether cloud operations (TTS, embeddings, export) are allowed
    /// for this specific entry. Overrides the global tier in `soul.toml`.
    ///
    /// Accepted values: `"local"` | `"hybrid"` | `"cloud"`.
    /// When absent, the global tier from `soul.toml [privacy]` applies.
    ///
    /// Example frontmatter:
    /// ```yaml
    /// ---
    /// sibling: eva
    /// privacy: local
    /// ---
    /// ```
    pub privacy: Option<String>,
    /// Semantic entry type for graph queries.
    ///
    /// Deserializes from `type:` (the vault's primary key) with `entry_type:` as a
    /// forward-compatible alias. `type` is a Rust keyword so the field is named
    /// `entry_type` in code. Values: `"hub"`, `"experience"`, `"reference"`,
    /// `"milestone"`, `"convergence"`, `"growth-summary"`, `"journal"`, etc.
    #[serde(rename = "type", alias = "entry_type", default)]
    pub entry_type: Option<String>,
    /// Catch-all for extra fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// A convergence reference in frontmatter.
#[derive(Debug, Clone, Deserialize)]
pub struct ConvergenceRef {
    /// Step IDs participating in the convergence.
    #[serde(default)]
    pub step_ids: Vec<String>,
    /// Label for the convergence.
    pub label: Option<String>,
    /// Weight (default 1.0).
    pub weight: Option<f64>,
}

/// A typed link reference in frontmatter.
#[derive(Debug, Clone, Deserialize)]
pub struct LinkRef {
    /// Target step or entry path.
    pub target: String,
    /// Link type (wikilink, reference, dependency, etc.).
    #[serde(rename = "type")]
    pub link_type: Option<String>,
    /// Link strength (0.0-1.0, default 1.0).
    pub strength: Option<f64>,
}

// ============================================================================
// Custom Deserializers
// ============================================================================

/// Deserializes the `convergence` field, which appears in two forms in the vault:
///
/// - `convergence: false` / `convergence: true` — a boolean flag used by older
///   entries to indicate "this is/isn't a convergence point". Treated as no refs.
/// - `convergence: [{step_ids: [...], label: "..."}]` — explicit `ConvergenceRef` array.
fn deser_convergence<'de, D>(deserializer: D) -> Result<Vec<ConvergenceRef>, D::Error>
where
    D: Deserializer<'de>,
{
    struct ConvergenceVisitor;

    impl<'de> Visitor<'de> for ConvergenceVisitor {
        type Value = Vec<ConvergenceRef>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "a bool or a sequence of ConvergenceRef")
        }

        fn visit_bool<E: serde::de::Error>(self, _val: bool) -> Result<Self::Value, E> {
            // `convergence: false/true` is a boolean flag — no explicit refs.
            Ok(Vec::new())
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut result = Vec::new();
            while let Some(item) = seq.next_element::<ConvergenceRef>()? {
                result.push(item);
            }
            Ok(result)
        }
    }

    deserializer.deserialize_any(ConvergenceVisitor)
}

/// Deserializes `themes` (and similarly structured string-list fields) that may
/// contain Obsidian wikilink notation: `[[path/to/file|Display Name]]`.
///
/// YAML parses `[[a|b]]` as a nested sequence `[["a|b"]]`. This visitor
/// flattens nested sequences and extracts the display name (after `|`) so
/// Neo4j stores human-readable labels like "Architecture" rather than
/// "hubs/themes/_hub-theme-architecture|Architecture".
fn deser_flat_string_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct FlatVecVisitor;

    impl<'de> Visitor<'de> for FlatVecVisitor {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "a string or a (possibly nested) sequence of strings")
        }

        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(vec![wikilink_display(v)])
        }

        fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
            Ok(vec![wikilink_display(&v)])
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut result = Vec::new();
            while let Some(elem) = seq.next_element::<serde_json::Value>()? {
                flatten_json_value(&elem, &mut result);
            }
            Ok(result)
        }
    }

    deserializer.deserialize_any(FlatVecVisitor)
}

/// Recursively flattens a JSON value into strings, extracting wikilink display names.
fn flatten_json_value(val: &serde_json::Value, out: &mut Vec<String>) {
    match val {
        serde_json::Value::String(s) => out.push(wikilink_display(s)),
        serde_json::Value::Array(arr) => {
            for v in arr {
                flatten_json_value(v, out);
            }
        }
        _ => {}
    }
}

/// Extracts the display name from an Obsidian wikilink target.
///
/// `"hubs/themes/_hub-theme-architecture|Architecture"` → `"Architecture"`.
/// Plain strings (no `|`) are returned as-is.
fn wikilink_display(s: &str) -> String {
    if let Some(pipe_idx) = s.rfind('|') {
        s[pipe_idx.saturating_add(1)..].trim().to_owned()
    } else {
        s.trim().to_owned()
    }
}

// ============================================================================
// Parser
// ============================================================================

/// Parse YAML frontmatter and body from a markdown string.
///
/// Returns `(frontmatter, body)`. If no frontmatter is found,
/// returns a default `Frontmatter` and the full content as body.
#[must_use]
pub fn parse(content: &str) -> (Frontmatter, &str) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (Frontmatter::default(), content);
    }

    // Find closing delimiter
    let after_open = &trimmed[3..];
    let close_idx = after_open.find("\n---");
    let Some(close_idx) = close_idx else {
        return (Frontmatter::default(), content);
    };

    let yaml_str = &after_open[..close_idx];
    let body_start = 3 + close_idx + 4; // "---" + yaml + "\n---"
    let body = trimmed[body_start..].trim_start_matches('\n');

    // Guard against YAML anchor DoS ("billion laughs"): serde_yaml expands
    // anchors before returning an error, so a 1KB bomb can allocate gigabytes.
    // 64 KiB is generous for any real helix frontmatter block.
    #[allow(clippy::items_after_statements)]
    const MAX_FRONTMATTER_BYTES: usize = 64 * 1024;
    if yaml_str.len() > MAX_FRONTMATTER_BYTES {
        tracing::warn!(
            bytes = yaml_str.len(),
            limit = MAX_FRONTMATTER_BYTES,
            "YAML frontmatter exceeds size limit — skipping parse"
        );
        return (Frontmatter::default(), body);
    }

    match serde_yaml::from_str(yaml_str) {
        Ok(fm) => (fm, body),
        // Even when YAML parsing fails, return `body` (text after the closing `---`)
        // rather than the full `content` (which includes the raw frontmatter block).
        // This prevents YAML source from being stored verbatim in Step.content.
        Err(_) => (Frontmatter::default(), body),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_with_frontmatter() {
        let md = "---\ntitle: Test Entry\nsibling: eva\nsignificance: 7.5\nstrands:\n  - emotional\n  - growth\n---\nBody content here.";
        let (fm, body) = parse(md);
        assert_eq!(fm.title.as_deref(), Some("Test Entry"));
        assert_eq!(fm.sibling.as_deref(), Some("eva"));
        assert_eq!(fm.significance, Some(7.5));
        assert_eq!(fm.strands, vec!["emotional", "growth"]);
        assert_eq!(body, "Body content here.");
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let md = "Just plain content.";
        let (fm, body) = parse(md);
        assert!(fm.title.is_none());
        assert_eq!(body, "Just plain content.");
    }

    #[test]
    fn test_parse_empty_frontmatter() {
        let md = "---\n---\nBody after empty front.";
        let (fm, body) = parse(md);
        assert!(fm.title.is_none());
        assert_eq!(body, "Body after empty front.");
    }

    #[test]
    fn test_parse_with_convergence() {
        let md = "---\ntitle: Shared Moment\nconvergence:\n  - step_ids: [step-1, step-2]\n    label: Trust deepening\n---\nContent.";
        let (fm, _) = parse(md);
        assert_eq!(fm.convergence.len(), 1);
        assert_eq!(fm.convergence[0].step_ids, vec!["step-1", "step-2"]);
        assert_eq!(fm.convergence[0].label.as_deref(), Some("Trust deepening"));
    }

    #[test]
    fn test_parse_with_links() {
        let md = "---\nlinks:\n  - target: eva/2026-01-15\n    type: reference\n    strength: 0.8\n---\nContent.";
        let (fm, _) = parse(md);
        assert_eq!(fm.links.len(), 1);
        assert_eq!(fm.links[0].target, "eva/2026-01-15");
        assert_eq!(fm.links[0].link_type.as_deref(), Some("reference"));
        assert_eq!(fm.links[0].strength, Some(0.8));
    }

    /// `convergence: false` (boolean) was the root cause of ~275 corrupted Step nodes.
    /// The custom deserializer must accept `bool` and return an empty Vec.
    #[test]
    fn test_parse_convergence_bool_false() {
        let md = "---\ntitle: CORSO Entry\nconvergence: false\ndate: 2026-02-16\n---\nBody text.";
        let (fm, body) = parse(md);
        assert_eq!(fm.title.as_deref(), Some("CORSO Entry"));
        assert!(
            fm.convergence.is_empty(),
            "bool convergence should yield empty vec"
        );
        assert_eq!(body, "Body text.");
    }

    #[test]
    fn test_parse_convergence_bool_true() {
        let md = "---\ntitle: EVA Entry\nconvergence: true\n---\nBody.";
        let (fm, body) = parse(md);
        assert!(fm.convergence.is_empty());
        assert_eq!(body, "Body.");
    }

    /// `themes: [[path|Display]], [[path2|Display2]]` — the Obsidian inline wikilink
    /// notation is INVALID YAML. The `, [[` after the first flow sequence close `]]`
    /// is unexpected per spec. `serde_yaml` fails, falling back to default (empty themes)
    /// but correctly extracting the body text. Vault entries with this format should
    /// be updated to use block sequence format instead.
    #[test]
    fn test_parse_themes_wikilink_invalid_yaml_body_still_extracted() {
        let md = "---\ntitle: Themed Entry\nthemes: [[hubs/themes/_hub-theme-architecture|Architecture]], [[hubs/themes/_hub-theme-security|Security]]\n---\nBody.";
        let (fm, body) = parse(md);
        // Themes will be empty because the YAML is invalid — that's acceptable.
        // The critical guarantee is that the body is extracted rather than raw YAML stored.
        assert!(
            fm.themes.is_empty(),
            "wikilink themes cause YAML parse failure → default"
        );
        assert_eq!(body, "Body.", "body must be extracted even when YAML fails");
        assert!(
            !body.contains("---"),
            "raw frontmatter must not appear in body"
        );
    }

    #[test]
    fn test_parse_themes_plain_strings() {
        let md = "---\nthemes:\n  - prompt-compression\n  - security-assessment\n---\nBody.";
        let (fm, _) = parse(md);
        assert_eq!(fm.themes, vec!["prompt-compression", "security-assessment"]);
    }

    /// When YAML parse fails, body (not full content) must be returned.
    /// This prevents raw YAML from being stored in Step.content.
    #[test]
    fn test_parse_yaml_error_returns_body_not_full_content() {
        // Deliberately malformed YAML: type mismatch that older code couldn't handle
        let md = "---\nsignificance: not-a-number\n---\nActual body text.";
        let (fm, body) = parse(md);
        assert!(fm.significance.is_none(), "default used on parse error");
        assert_eq!(
            body, "Actual body text.",
            "body extracted even when YAML fails"
        );
        assert!(
            !body.contains("---"),
            "raw frontmatter must not appear in body on parse error"
        );
    }

    #[test]
    fn test_parse_type_field_mapped_to_entry_type() {
        let md = "---\ntitle: A hub\ntype: hub\n---\nContent.";
        let (fm, _) = parse(md);
        assert_eq!(
            fm.entry_type.as_deref(),
            Some("hub"),
            "YAML `type:` must deserialize into entry_type field"
        );
    }

    #[test]
    fn test_parse_entry_type_alias_accepted() {
        let md = "---\ntitle: A future entry\nentry_type: experience\n---\nContent.";
        let (fm, _) = parse(md);
        assert_eq!(
            fm.entry_type.as_deref(),
            Some("experience"),
            "`entry_type:` alias must also deserialize correctly"
        );
    }

    #[test]
    fn test_parse_type_experience_and_significance() {
        let md = "---\nsibling: corso\ntype: experience\nsignificance: 7.5\n---\nBody.";
        let (fm, _) = parse(md);
        assert_eq!(fm.entry_type.as_deref(), Some("experience"));
        assert_eq!(fm.significance, Some(7.5));
    }

    #[test]
    fn test_parse_oversized_frontmatter_skipped_gracefully() {
        // A frontmatter block exceeding 64 KiB is rejected without calling serde_yaml.
        // This guards against YAML anchor DoS ("billion laughs") attacks.
        let huge_yaml = "a: ".repeat(25_000); // ~75 KiB, well above 64 KiB limit
        let md = format!("---\n{huge_yaml}\n---\nBody after oversized block.");
        let (fm, body) = parse(&md);
        assert!(
            fm.significance.is_none(),
            "oversized frontmatter returns default"
        );
        assert_eq!(body, "Body after oversized block.", "body still extracted");
    }
}
