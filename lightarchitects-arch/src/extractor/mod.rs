//! Source code extractors — parse Rust, TypeScript, and Python into [`ExtractedFacts`].
//!
//! Each language sub-module uses tree-sitter to walk the AST and produce
//! [`ArchNode`] and [`ArchRelation`] entries without executing the target code.

pub mod python;
pub mod rust;
pub mod typescript;

use std::path::Path;

use crate::model::{ExtractedFacts, Language};

/// Common configuration for all extractors.
#[derive(Debug, Clone)]
pub struct ExtractorConfig {
    /// Maximum file size to attempt parsing (bytes). Files above this limit are skipped.
    pub max_file_bytes: usize,
    /// Maximum number of warnings to accumulate before stopping extraction.
    pub max_warnings: usize,
}

impl Default for ExtractorConfig {
    fn default() -> Self {
        Self {
            max_file_bytes: 1_024 * 1_024, // 1 MiB
            max_warnings: 100,
        }
    }
}

/// Errors returned by extractors.
#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    /// Source file could not be read.
    #[error("I/O error reading '{path}': {source}")]
    Io {
        /// File path that failed.
        path: String,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },

    /// tree-sitter failed to produce a valid parse tree.
    #[error("parse error in '{path}': tree-sitter returned no tree")]
    ParseFailed {
        /// File path that failed.
        path: String,
    },

    /// File exceeds configured size limit.
    #[error("file '{path}' exceeds max_file_bytes limit ({size} bytes)")]
    FileTooLarge {
        /// File path.
        path: String,
        /// Actual file size.
        size: usize,
    },
}

/// Merges `other` into `base`, appending nodes, relations, and warnings.
pub fn merge_facts(base: &mut ExtractedFacts, other: ExtractedFacts) {
    base.nodes.extend(other.nodes);
    base.relations.extend(other.relations);
    base.warnings.extend(other.warnings);
}

/// Returns the [`Language`] inferred from a file extension.
#[must_use]
pub fn language_for_path(path: &Path) -> Language {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => Language::Rust,
        Some("ts" | "tsx") => Language::TypeScript,
        Some("py") => Language::Python,
        _ => Language::Unknown,
    }
}

/// Bucketed file count label for bounded-cardinality AYIN spans.
fn file_count_bucket(n: usize) -> &'static str {
    match n {
        0 => "0",
        1..=10 => "1-10",
        11..=100 => "11-100",
        101..=1_000 => "101-1000",
        _ => "1001+",
    }
}

/// Walks `root` recursively, extracts facts from every supported source file,
/// and merges them into a single [`ExtractedFacts`].
///
/// Files that fail extraction (parse error, too large, I/O error) are logged
/// and their error message appended to [`ExtractedFacts::warnings`] — they do
/// not abort the walk.
///
/// # Errors
///
/// Returns [`ExtractError::Io`] if `root` cannot be read at all.
#[tracing::instrument(skip(config), fields(root = %root.display()))]
pub fn walk_and_extract(
    root: &Path,
    config: &ExtractorConfig,
) -> Result<ExtractedFacts, ExtractError> {
    let mut facts = ExtractedFacts::default();
    let mut file_count: usize = 0;

    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let lang = language_for_path(path);
        if lang == Language::Unknown {
            continue;
        }

        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("I/O error reading '{}': {e}", path.display());
                tracing::warn!(path = %path.display(), error = %e, "skipping file due to I/O error");
                facts.warnings.push(msg);
                continue;
            }
        };

        let result = match lang {
            Language::Rust => rust::extract_file(path, &source, config),
            Language::TypeScript => typescript::extract_file(path, &source, config),
            Language::Python => python::extract_file(path, &source, config),
            Language::Unknown => unreachable!("filtered above"),
        };

        match result {
            Ok(file_facts) => {
                merge_facts(&mut facts, file_facts);
                file_count += 1;
            }
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "skipping file due to extraction error"
                );
                facts.warnings.push(e.to_string());
            }
        }
    }

    tracing::debug!(
        file_count,
        file_count_bucket = file_count_bucket(file_count),
        nodes = facts.nodes.len(),
        relations = facts.relations.len(),
        "walk complete"
    );
    Ok(facts)
}
