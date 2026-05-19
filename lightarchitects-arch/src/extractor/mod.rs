//! Source code extractors — parse Rust, TypeScript, and Python into [`ExtractedFacts`].
//!
//! Each language sub-module uses tree-sitter to walk the AST and produce
//! [`ArchNode`] and [`ArchRelation`] entries without executing the target code.

pub mod python;
pub mod rust;
pub mod typescript;

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
pub fn language_for_path(path: &std::path::Path) -> Language {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => Language::Rust,
        Some("ts" | "tsx") => Language::TypeScript,
        Some("py") => Language::Python,
        _ => Language::Unknown,
    }
}
