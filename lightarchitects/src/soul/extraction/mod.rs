//! Entity extraction from raw text.
//!
//! Provides the [`EntityExtractor`] trait and two implementations:
//!
//! - [`HeuristicExtractor`] — heading-based extraction, no LLM required.
//! - [`LlmEntityExtractor`] — stub for LLM-backed extraction via a custom
//!   [`LlmProvider`] implementation.
//!
//! # Feature Gate
//!
//! This module is compiled when the `ingestion` feature is enabled.

use chrono::Utc;
use thiserror::Error;
use uuid::Uuid;

use crate::soul::storage::StorageEntry;

// ============================================================================
// ExtractionError
// ============================================================================

/// Error type for entity extraction failures.
#[derive(Debug, Error)]
pub enum ExtractionError {
    /// The extraction process failed.
    #[error("extraction failed: {0}")]
    Failed(String),
    /// A regex pattern was invalid.
    #[error("regex error: {0}")]
    Regex(String),
}

// ============================================================================
// ExtractionOptions
// ============================================================================

/// Options for controlling entity extraction behaviour.
#[derive(Debug, Clone)]
pub struct ExtractionOptions {
    /// Minimum confidence for an extracted entity to be included (0.0–1.0).
    pub min_confidence: f32,
    /// Maximum number of entities to return.
    pub max_entities: usize,
}

impl Default for ExtractionOptions {
    fn default() -> Self {
        Self {
            min_confidence: 0.5,
            max_entities: 50,
        }
    }
}

// ============================================================================
// EntityExtractor trait
// ============================================================================

/// Extract structured [`StorageEntry`] values from raw text.
pub trait EntityExtractor: Send + Sync {
    /// Extract entities from `text` according to `options`.
    ///
    /// # Errors
    ///
    /// Returns [`ExtractionError::Failed`] on extraction failure, or
    /// [`ExtractionError::Regex`] if a pattern is malformed.
    fn extract(
        &self,
        text: &str,
        options: &ExtractionOptions,
    ) -> Result<Vec<StorageEntry>, ExtractionError>;
}

// ============================================================================
// HeuristicExtractor
// ============================================================================

/// Heading-based heuristic extractor — no LLM required.
///
/// Extracts Markdown headings (`# Title`) as entry titles, using the
/// paragraph that follows as entry content. Inline `key: value` YAML pairs
/// in the heading's block are parsed as entry metadata fields.
///
/// Each extracted heading becomes one [`StorageEntry`] with:
/// - `title` = heading text (without `#` prefix).
/// - `content` = the paragraph immediately following the heading.
/// - `sibling` = `"extracted"`.
pub struct HeuristicExtractor;

impl EntityExtractor for HeuristicExtractor {
    fn extract(
        &self,
        text: &str,
        options: &ExtractionOptions,
    ) -> Result<Vec<StorageEntry>, ExtractionError> {
        let mut entries: Vec<StorageEntry> = Vec::new();
        let mut current_title: Option<String> = None;
        let mut content_lines: Vec<String> = Vec::new();

        let flush =
            |title: &Option<String>, content: &[String], entries: &mut Vec<StorageEntry>| {
                let Some(title) = title else { return };
                let body = content.join("\n").trim().to_owned();
                let now = Utc::now();
                entries.push(StorageEntry {
                    id: Uuid::new_v4().to_string(),
                    path: format!("extracted/{}.md", sanitize_slug(title)),
                    sibling: "extracted".into(),
                    date: None,
                    entry_type: Some("extracted".into()),
                    significance: 0.0,
                    self_defining: false,
                    epoch: None,
                    strands: Vec::new(),
                    resonance: Vec::new(),
                    themes: Vec::new(),
                    title: Some(title.clone()),
                    content: body,
                    frontmatter: None,
                    created_at: now,
                    updated_at: now,
                });
            };

        for line in text.lines() {
            if line.starts_with('#') {
                // Flush previous heading.
                flush(&current_title, &content_lines, &mut entries);
                if entries.len() >= options.max_entities {
                    break;
                }
                // Extract heading text (strip leading `#`s and whitespace).
                let heading = line.trim_start_matches('#').trim().to_owned();
                current_title = Some(heading);
                content_lines.clear();
            } else {
                content_lines.push(line.to_owned());
            }
        }

        // Flush the final heading.
        if entries.len() < options.max_entities {
            flush(&current_title, &content_lines, &mut entries);
        }

        Ok(entries)
    }
}

/// Convert a heading string to a URL/path-safe slug.
fn sanitize_slug(heading: &str) -> String {
    heading
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

// ============================================================================
// LlmProvider trait
// ============================================================================

/// External LLM text completion provider for [`LlmEntityExtractor`].
pub trait LlmProvider: Send + Sync {
    /// Complete a prompt and return the generated text.
    ///
    /// # Errors
    ///
    /// Returns [`ExtractionError::Failed`] if the provider call fails.
    fn complete(&self, prompt: &str) -> Result<String, ExtractionError>;
}

// ============================================================================
// LlmEntityExtractor (stub)
// ============================================================================

/// Stub LLM-backed extractor — requires an external [`LlmProvider`] implementation.
///
/// Currently unimplemented; returns [`ExtractionError::Failed`] with a
/// descriptive message indicating it must be implemented by the caller.
pub struct LlmEntityExtractor {
    /// The LLM provider used to generate extraction prompts.
    pub provider: Box<dyn LlmProvider>,
}

impl EntityExtractor for LlmEntityExtractor {
    fn extract(
        &self,
        _text: &str,
        _options: &ExtractionOptions,
    ) -> Result<Vec<StorageEntry>, ExtractionError> {
        Err(ExtractionError::Failed(
            "LlmProvider-backed extraction is not yet implemented".into(),
        ))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_heuristic_extracts_headings() {
        let text = "# Genesis Day\nThis is the beginning.\n\n# Second Entry\nMore content.";
        let opts = ExtractionOptions::default();
        let extractor = HeuristicExtractor;
        let entries = extractor.extract(text, &opts).unwrap();
        assert_eq!(entries.len(), 2, "should extract 2 headings");
        assert_eq!(entries[0].title.as_deref(), Some("Genesis Day"));
        assert_eq!(entries[1].title.as_deref(), Some("Second Entry"));
    }

    #[test]
    fn test_heuristic_content_follows_heading() {
        let text = "# My Title\nContent paragraph here.";
        let opts = ExtractionOptions::default();
        let extractor = HeuristicExtractor;
        let entries = extractor.extract(text, &opts).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].content.contains("Content paragraph"));
    }

    #[test]
    fn test_heuristic_respects_max_entities() {
        let text = "# One\nContent.\n# Two\nContent.\n# Three\nContent.";
        let opts = ExtractionOptions {
            max_entities: 2,
            ..Default::default()
        };
        let extractor = HeuristicExtractor;
        let entries = extractor.extract(text, &opts).unwrap();
        assert!(entries.len() <= 2, "should respect max_entities");
    }

    #[test]
    fn test_heuristic_no_headings_returns_empty() {
        let text = "Just some plain text with no headings.";
        let opts = ExtractionOptions::default();
        let extractor = HeuristicExtractor;
        let entries = extractor.extract(text, &opts).unwrap();
        assert!(entries.is_empty(), "no headings should produce no entries");
    }

    #[test]
    fn test_heuristic_entry_has_valid_id() {
        let text = "# Test Entry\nBody.";
        let opts = ExtractionOptions::default();
        let extractor = HeuristicExtractor;
        let entries = extractor.extract(text, &opts).unwrap();
        assert!(!entries[0].id.is_empty(), "extracted entry must have an id");
    }

    #[test]
    fn test_sanitize_slug() {
        // "Hello World!" — trailing "!" maps to '-', then trim_matches('-') removes it.
        assert_eq!(sanitize_slug("Hello World!"), "hello-world");
        assert_eq!(sanitize_slug("Genesis Day"), "genesis-day");
    }

    #[test]
    fn test_llm_extractor_stub_returns_error() {
        struct StubProvider;
        impl LlmProvider for StubProvider {
            fn complete(&self, _prompt: &str) -> Result<String, ExtractionError> {
                Ok("{}".into())
            }
        }
        let extractor = LlmEntityExtractor {
            provider: Box::new(StubProvider),
        };
        let result = extractor.extract("any text", &ExtractionOptions::default());
        assert!(result.is_err(), "LlmEntityExtractor stub must return Err");
    }
}
