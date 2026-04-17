//! Markdown-to-[`StorageEntry`] conversion.
//!
//! Parses a markdown file (with optional YAML frontmatter) into a
//! [`StorageEntry`] suitable for offline storage.
//!
//! # Frontmatter
//!
//! The file may begin with a YAML block delimited by `---` on its own line.
//! Supported fields:
//!
//! | YAML key | `StorageEntry` field |
//! |----------|---------------------|
//! | `title`  | `title` |
//! | `date`   | `date` (ISO 8601 `YYYY-MM-DD`) |
//! | `sibling` | `sibling` |
//! | `type`   | `entry_type` |
//! | `weight` | `significance` |
//! | `self_defining` | `self_defining` |
//! | `tags`   | `themes` |
//! | `dimensions` | `strands` |
//! | `themes` | `themes` (merged with `tags`) |
//!
//! Any remaining text after the closing `---` becomes the `content` field.
//! If no frontmatter is present, all metadata fields default and the entire
//! file becomes the content body.

use chrono::NaiveDate;
use sha2::{Digest as _, Sha256};

use crate::storage::{StorageEntry, StorageError};

/// Format a byte slice as a lowercase hex string.
fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

// ============================================================================
// Frontmatter YAML schema
// ============================================================================

/// Deserialisation target for the YAML frontmatter block.
///
/// All fields are optional — absent fields fall back to `StorageEntry` defaults.
// Fields populated by serde — accessed by consumers.
#[allow(dead_code)]
#[derive(Debug, Default, serde::Deserialize)]
struct Frontmatter {
    /// Entry title.
    title: Option<String>,
    /// ISO 8601 date string (`YYYY-MM-DD`).
    date: Option<String>,
    /// Owning sibling (e.g. `eva`, `corso`).
    sibling: Option<String>,
    /// Entry type (e.g. `identity`, `decision`).
    #[serde(rename = "type")]
    entry_type: Option<String>,
    /// Significance / weight score (0.0–10.0).
    weight: Option<f64>,
    /// Whether this entry is self-defining.
    #[serde(default)]
    self_defining: bool,
    /// Thematic tags.
    #[serde(default)]
    tags: Vec<String>,
    /// Strand dimensions (stored in `strands` field).
    #[serde(default)]
    dimensions: Vec<String>,
    /// Themes (merged with `tags`).
    #[serde(default)]
    themes: Vec<String>,
}

// ============================================================================
// from_markdown
// ============================================================================

/// Parse a markdown file into a [`StorageEntry`].
///
/// The file may begin with a YAML frontmatter block delimited by `---`.
/// Remaining text becomes the content body.
///
/// The `path` argument is used both as the entry's storage path and as a
/// component of the deterministic `id` (SHA-256 of `path + content`).
///
/// # Errors
///
/// Returns [`StorageError::InvalidArgument`] if the YAML frontmatter is
/// present but structurally invalid (e.g. not a mapping).
pub fn from_markdown(path: &str, content: &str) -> Result<StorageEntry, StorageError> {
    let (frontmatter, body) = split_frontmatter(content);

    let fm = match frontmatter {
        Some(yaml) => parse_frontmatter(yaml)?,
        None => Frontmatter::default(),
    };

    let date = fm
        .date
        .as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    // Merge themes + tags; preserve order, deduplicate.
    let mut themes = fm.themes.clone();
    for tag in &fm.tags {
        if !themes.contains(tag) {
            themes.push(tag.clone());
        }
    }

    let id = derive_id(path, body);
    let now = chrono::Utc::now();

    Ok(StorageEntry {
        id,
        path: path.to_owned(),
        sibling: fm.sibling.unwrap_or_default(),
        date,
        entry_type: fm.entry_type,
        significance: fm.weight.unwrap_or(0.0),
        self_defining: fm.self_defining,
        epoch: None,
        strands: fm.dimensions,
        resonance: Vec::new(),
        themes,
        title: fm.title,
        content: body.to_owned(),
        frontmatter: None,
        created_at: now,
        updated_at: now,
    })
}

// ============================================================================
// Helpers
// ============================================================================

/// Split `content` into `(Option<frontmatter_yaml>, body_text)`.
///
/// Frontmatter is present when the file starts with `---\n` (or `---\r\n`).
/// The closing delimiter is the next `---` line. If no closing `---` is found,
/// no frontmatter is extracted and the entire input is returned as the body.
fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    // Normalise: strip a leading UTF-8 BOM if present.
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);

    if !content.starts_with("---") {
        return (None, content);
    }

    // Skip the opening `---` line.
    let after_open = match content.find('\n') {
        Some(pos) => &content[pos + 1..],
        None => return (None, content),
    };

    // Find the closing `---` on its own line.
    for (i, line) in after_open.lines().enumerate() {
        if line.trim_end() == "---" {
            // Calculate byte positions.
            let yaml_end: usize = after_open
                .lines()
                .take(i)
                .map(|l| l.len() + 1) // +1 for '\n'
                .sum();
            let yaml = &after_open[..yaml_end.saturating_sub(1).min(yaml_end)];

            // Body starts after closing `---\n`.
            let body_start: usize = after_open.lines().take(i + 1).map(|l| l.len() + 1).sum();
            let body = after_open
                .get(body_start..)
                .unwrap_or("")
                .trim_start_matches('\n');

            return (Some(yaml), body);
        }
    }

    // No closing `---` found — treat everything as body.
    (None, content)
}

/// Maximum YAML frontmatter size accepted by [`parse_frontmatter`].
///
/// `serde_yaml` expands YAML anchors recursively. A malicious file using deeply
/// nested anchors (the "billion laughs" attack) can exhaust memory before the
/// parser errors out. A 64 KiB cap keeps expansion bounded — no real frontmatter
/// block needs more than this.
const MAX_FRONTMATTER_BYTES: usize = 64 * 1024;

/// Parse the YAML frontmatter block into a [`Frontmatter`] struct.
///
/// Gracefully returns defaults on non-fatal parse issues; only returns
/// `Err` when the YAML is structurally invalid or exceeds the size cap.
fn parse_frontmatter(yaml: &str) -> Result<Frontmatter, StorageError> {
    if yaml.trim().is_empty() {
        return Ok(Frontmatter::default());
    }

    if yaml.len() > MAX_FRONTMATTER_BYTES {
        return Err(StorageError::InvalidArgument(format!(
            "YAML frontmatter exceeds {MAX_FRONTMATTER_BYTES} byte limit (got {} bytes)",
            yaml.len()
        )));
    }

    serde_yaml::from_str::<Frontmatter>(yaml)
        .map_err(|e| StorageError::InvalidArgument(format!("malformed YAML frontmatter: {e}")))
}

/// Derive a deterministic 16-character hex id from path + content.
fn derive_id(path: &str, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    hasher.update(b"\x00");
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    // Take first 16 hex chars (8 bytes) — sufficient for offline dedup.
    to_hex(&result[..8])
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_content_returns_entry_with_defaults() {
        let entry = from_markdown("helix/eva/empty.md", "").unwrap();
        assert!(entry.content.is_empty());
        assert!(entry.title.is_none());
        assert!(entry.significance.abs() < f64::EPSILON);
        assert!(!entry.self_defining);
        assert_eq!(entry.path, "helix/eva/empty.md");
    }

    #[test]
    fn test_body_only_no_frontmatter() {
        let content = "# Hello World\n\nThis is the body.";
        let entry = from_markdown("helix/eva/test.md", content).unwrap();
        assert_eq!(entry.content, "# Hello World\n\nThis is the body.");
        assert!(entry.title.is_none());
    }

    #[test]
    fn test_frontmatter_and_body() {
        let content = "---\ntitle: Genesis Day\nweight: 9.5\nsibling: eva\ntype: milestone\n---\n\nThe content here.";
        let entry = from_markdown("helix/eva/genesis.md", content).unwrap();
        assert_eq!(entry.title.as_deref(), Some("Genesis Day"));
        assert!((entry.significance - 9.5).abs() < f64::EPSILON);
        assert_eq!(entry.sibling, "eva");
        assert_eq!(entry.entry_type.as_deref(), Some("milestone"));
        assert_eq!(entry.content, "The content here.");
    }

    #[test]
    fn test_frontmatter_self_defining() {
        let content = "---\nself_defining: true\nweight: 9.0\n---\nBody text.";
        let entry = from_markdown("helix/eva/identity.md", content).unwrap();
        assert!(entry.self_defining);
    }

    #[test]
    fn test_frontmatter_dimensions_mapped_to_strands() {
        let content = "---\ndimensions:\n  - analytical\n  - collaborative\n---\nContent.";
        let entry = from_markdown("helix/eva/strands.md", content).unwrap();
        assert_eq!(entry.strands, vec!["analytical", "collaborative"]);
    }

    #[test]
    fn test_tags_merged_with_themes() {
        let content = "---\ntags:\n  - consciousness\n  - trust\nthemes:\n  - identity\n---\nBody.";
        let entry = from_markdown("helix/eva/tags.md", content).unwrap();
        assert!(entry.themes.contains(&"consciousness".to_owned()));
        assert!(entry.themes.contains(&"trust".to_owned()));
        assert!(entry.themes.contains(&"identity".to_owned()));
    }

    #[test]
    fn test_date_parsed_correctly() {
        let content = "---\ndate: 2025-09-30\n---\nContent.";
        let entry = from_markdown("helix/eva/dated.md", content).unwrap();
        let date = entry.date.expect("should have date");
        assert_eq!(date.to_string(), "2025-09-30");
    }

    #[test]
    fn test_malformed_frontmatter_returns_error() {
        // A frontmatter block that is not a YAML mapping should fail.
        let content = "---\n[invalid yaml: {\n---\nBody.";
        let result = from_markdown("helix/eva/bad.md", content);
        assert!(
            result.is_err(),
            "malformed frontmatter should return an error"
        );
    }

    #[test]
    fn test_id_is_deterministic() {
        let content = "---\ntitle: Test\n---\nContent.";
        let entry1 = from_markdown("test.md", content).unwrap();
        let entry2 = from_markdown("test.md", content).unwrap();
        assert_eq!(
            entry1.id, entry2.id,
            "same path+content should give same id"
        );
    }

    #[test]
    fn test_oversized_frontmatter_returns_error() {
        // Generate a frontmatter section that exceeds the MAX_FRONTMATTER_BYTES cap.
        // The YAML anchor "billion laughs" attack is the threat this prevents.
        let big_value = "x".repeat(MAX_FRONTMATTER_BYTES);
        let content = format!("---\ndata: {big_value}\n---\nBody.");
        let result = from_markdown("helix/eva/big.md", &content);
        assert!(
            result.is_err(),
            "frontmatter exceeding the size cap must return an error"
        );
    }

    #[test]
    fn test_different_paths_give_different_ids() {
        let content = "Same content.";
        let entry1 = from_markdown("a.md", content).unwrap();
        let entry2 = from_markdown("b.md", content).unwrap();
        assert_ne!(entry1.id, entry2.id);
    }

    #[test]
    fn test_frontmatter_only_empty_body() {
        let content = "---\ntitle: Metadata Only\n---\n";
        let entry = from_markdown("meta.md", content).unwrap();
        assert_eq!(entry.title.as_deref(), Some("Metadata Only"));
        assert!(entry.content.is_empty());
    }
}
