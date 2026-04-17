//! Document parser — segments PDF, Markdown, and plaintext into [`ParsedDocument`].
//!
//! Detects format from file extension or inline content type and delegates
//! to the appropriate parser. All parsers emit [`Segment`] slices with
//! character offsets preserved from the original source.
//!
//! # Supported formats
//!
//! | Format | Detection |
//! |--------|-----------|
//! | Markdown | `.md`, `.markdown` extension, or detected `#` headings |
//! | Plaintext | All other input (fallback) |
//!
//! PDF support requires a native PDF parsing dependency. Until that is added,
//! `.pdf` files are rejected with [`ParseError::UnsupportedFormat`].

use crate::embedding::chunker::{Chunk, Chunker};

// Re-export `ChunkerConfig` so the graphrag module can expose it publicly.
pub use crate::embedding::chunker::ChunkerConfig;

// ─── Errors ─────────────────────────────────────────────────────────────────

/// Fatal parsing errors — prevent the document from being processed.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// File extension or content type is not supported by this parser.
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    /// I/O error reading the source file.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Content is empty after stripping control characters.
    #[error("empty content after normalisation")]
    EmptyContent,
}

/// Result alias for parsing operations.
pub type ParseResult<T> = Result<T, ParseError>;

// ─── Segment ────────────────────────────────────────────────────────────────

/// A contiguous segment of a parsed document.
///
/// Multiple segments from the same document share the same `source_id`.
/// `section_hint` records the most-recent Markdown heading (if any) so the
/// entity extractor can include it as contextual metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// Segment text (trimmed, never empty).
    pub text: String,
    /// Zero-based segment index within its document.
    pub index: usize,
    /// Most-recent Markdown heading before this segment, if any.
    pub section_hint: Option<String>,
    /// Character offset of the segment start in the original content.
    pub start_char: usize,
    /// Character offset of the segment end (exclusive) in the original content.
    pub end_char: usize,
}

// ─── ParsedDocument ──────────────────────────────────────────────────────────

/// The output of a successful document parse.
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    /// Unique source identifier (file stem or `"inline"`).
    pub source_id: String,
    /// Detected content format.
    pub format: DocumentFormat,
    /// Ordered non-empty segments ready for entity extraction.
    pub segments: Vec<Segment>,
}

impl ParsedDocument {
    /// Total character count across all segments.
    #[must_use]
    pub fn total_chars(&self) -> usize {
        self.segments.iter().map(|s| s.text.len()).sum()
    }
}

// ─── DocumentFormat ──────────────────────────────────────────────────────────

/// Detected document format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentFormat {
    /// Markdown with optional frontmatter.
    Markdown,
    /// Plain UTF-8 text.
    Plaintext,
}

// ─── DocumentParser ──────────────────────────────────────────────────────────

/// Parses documents into segments for downstream entity extraction.
///
/// Use [`DocumentParser::parse_file`] for file sources or
/// [`DocumentParser::parse_inline`] for inline text.
#[derive(Debug, Clone)]
pub struct DocumentParser {
    chunker: Chunker,
}

impl Default for DocumentParser {
    fn default() -> Self {
        Self::new(ChunkerConfig::default())
    }
}

impl DocumentParser {
    /// Create a parser with a custom chunker configuration.
    #[must_use]
    pub fn new(config: ChunkerConfig) -> Self {
        Self {
            chunker: Chunker::new(config),
        }
    }

    /// Parse a file from disk.
    ///
    /// Extension detection order: `.pdf` → rejected, `.md`/`.markdown` →
    /// Markdown, everything else → Plaintext.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::UnsupportedFormat`] for `.pdf` files.
    /// Returns [`ParseError::Io`] if the file cannot be read.
    /// Returns [`ParseError::EmptyContent`] if the file is blank.
    pub fn parse_file(&self, path: &std::path::Path) -> ParseResult<ParsedDocument> {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        if ext == "pdf" {
            return Err(ParseError::UnsupportedFormat(
                "PDF parsing requires a native dependency not yet available; \
                 convert to Markdown or plaintext first"
                    .to_owned(),
            ));
        }

        let content = std::fs::read_to_string(path).map_err(ParseError::Io)?;

        let format = if matches!(ext.as_str(), "md" | "markdown") {
            DocumentFormat::Markdown
        } else {
            DocumentFormat::Plaintext
        };

        let source_id = path.file_stem().map_or_else(
            || "unknown".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );

        self.parse_content(&content, &source_id, format)
    }

    /// Parse inline text content.
    ///
    /// `source_id` is an arbitrary caller-supplied identifier (e.g. a title
    /// or URL slug). `format` defaults to `Plaintext` for raw text callers;
    /// use `Markdown` when the content contains `#` headings.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::EmptyContent`] if `text` is blank after normalisation.
    pub fn parse_inline(
        &self,
        text: &str,
        source_id: &str,
        format: DocumentFormat,
    ) -> ParseResult<ParsedDocument> {
        self.parse_content(text, source_id, format)
    }

    // ─── Private ─────────────────────────────────────────────────────────────

    /// Core parse routine shared by file and inline paths.
    fn parse_content(
        &self,
        content: &str,
        source_id: &str,
        format: DocumentFormat,
    ) -> ParseResult<ParsedDocument> {
        let normalised = normalise(content);
        // Reject content that is empty or only whitespace after normalisation
        if normalised.trim().is_empty() {
            return Err(ParseError::EmptyContent);
        }

        let raw_chunks = self.chunker.chunk(&normalised);
        let sections = extract_sections(&normalised, format);
        let segments = build_segments(raw_chunks, &sections, &normalised);

        Ok(ParsedDocument {
            source_id: source_id.to_owned(),
            format,
            segments,
        })
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Strip Windows line endings and null bytes; collapse runs of blank lines.
fn normalise(text: &str) -> String {
    let without_cr = text.replace('\r', "");
    // Remove null bytes (not valid in UTF-8 text context)
    without_cr.replace('\0', "")
}

/// Extract `(char_offset, heading_text)` pairs from Markdown.
fn extract_sections(content: &str, format: DocumentFormat) -> Vec<(usize, String)> {
    if format != DocumentFormat::Markdown {
        return Vec::new();
    }

    let mut sections = Vec::new();
    let mut offset = 0_usize;

    for line in content.lines() {
        if line.starts_with('#') {
            let heading = line
                .trim_start_matches('#')
                .trim()
                .chars()
                .filter(|c| !c.is_control())
                .collect::<String>();
            if !heading.is_empty() {
                sections.push((offset, heading));
            }
        }
        // +1 for the newline character consumed by `lines()`
        offset = offset.saturating_add(line.len()).saturating_add(1);
    }

    sections
}

/// Build [`Segment`] list from chunks, annotating each with its nearest section.
fn build_segments(
    chunks: Vec<Chunk>,
    sections: &[(usize, String)],
    _content: &str,
) -> Vec<Segment> {
    chunks
        .into_iter()
        .filter(|c| !c.text.trim().is_empty())
        .enumerate()
        .map(|(idx, chunk)| {
            let section_hint = nearest_section(chunk.start_char, sections);
            Segment {
                text: chunk.text,
                index: idx,
                section_hint,
                start_char: chunk.start_char,
                end_char: chunk.end_char,
            }
        })
        .collect()
}

/// Find the heading whose offset is closest to (but not after) `pos`.
fn nearest_section(pos: usize, sections: &[(usize, String)]) -> Option<String> {
    sections
        .iter()
        .filter(|(offset, _)| *offset <= pos)
        .next_back()
        .map(|(_, heading)| heading.clone())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn parser() -> DocumentParser {
        DocumentParser::new(ChunkerConfig {
            target_chars: 200,
            overlap_chars: 20,
        })
    }

    #[test]
    fn parse_inline_short_plaintext() {
        let doc = parser()
            .parse_inline(
                "Hello world. This is a test.",
                "test",
                DocumentFormat::Plaintext,
            )
            .expect("parse ok");
        assert_eq!(doc.source_id, "test");
        assert_eq!(doc.format, DocumentFormat::Plaintext);
        assert!(!doc.segments.is_empty());
        assert!(doc.segments[0].section_hint.is_none());
    }

    #[test]
    fn parse_inline_markdown_with_heading() {
        let md = "# Introduction\n\nThis is the intro paragraph. It has content.\n\n\
                  ## Methods\n\nThe methods section explains everything.";
        let doc = parser()
            .parse_inline(md, "paper", DocumentFormat::Markdown)
            .expect("parse ok");
        assert_eq!(doc.format, DocumentFormat::Markdown);
        // At least one segment should carry a section hint
        let has_hint = doc.segments.iter().any(|s| s.section_hint.is_some());
        assert!(has_hint, "expected section hints from headings");
    }

    #[test]
    fn parse_inline_empty_returns_error() {
        let result = parser().parse_inline("  \n\n  ", "empty", DocumentFormat::Plaintext);
        assert!(result.is_err());
    }

    #[test]
    fn parse_file_pdf_rejected() {
        let path = std::path::Path::new("document.pdf");
        let result = parser().parse_file(path);
        assert!(matches!(result, Err(ParseError::UnsupportedFormat(_))));
    }

    #[test]
    fn segment_indices_are_sequential() {
        let content = "Sentence one. Sentence two. Sentence three. Sentence four. Sentence five.";
        let doc = parser()
            .parse_inline(content, "seq", DocumentFormat::Plaintext)
            .expect("parse ok");
        for (i, seg) in doc.segments.iter().enumerate() {
            assert_eq!(seg.index, i);
        }
    }

    #[test]
    fn segment_indices_are_sequential_with_blank_leading_chunk() {
        // Simulate the bug: enumerate-before-filter would assign index 1 to the
        // first surviving segment when the chunk list starts with an empty entry.
        // build_segments is called directly so we can inject a synthetic empty
        // leading chunk without relying on the chunker producing one.
        let chunks = vec![
            crate::embedding::chunker::Chunk {
                text: "   ".to_owned(),
                start_char: 0,
                end_char: 3,
            },
            crate::embedding::chunker::Chunk {
                text: "First real segment.".to_owned(),
                start_char: 3,
                end_char: 22,
            },
            crate::embedding::chunker::Chunk {
                text: "Second real segment.".to_owned(),
                start_char: 22,
                end_char: 42,
            },
        ];
        let segments = build_segments(chunks, &[], "");
        assert_eq!(segments.len(), 2, "blank chunk must be filtered out");
        for (expected_idx, seg) in segments.iter().enumerate() {
            assert_eq!(
                seg.index, expected_idx,
                "segment index must be post-filter position, not pre-filter position"
            );
        }
    }

    #[test]
    fn nearest_section_returns_closest_preceding() {
        let sections = vec![(0, "Intro".to_owned()), (50, "Methods".to_owned())];
        assert_eq!(nearest_section(30, &sections), Some("Intro".to_owned()));
        assert_eq!(nearest_section(60, &sections), Some("Methods".to_owned()));
    }

    #[test]
    fn nearest_section_before_any_heading() {
        let sections = vec![(100, "Later".to_owned())];
        assert_eq!(nearest_section(10, &sections), None);
    }

    #[test]
    fn normalise_strips_windows_endings() {
        let text = "line one\r\nline two\r\n";
        let result = normalise(text);
        assert!(!result.contains('\r'));
    }

    #[test]
    fn total_chars_sums_segments() {
        let doc = parser()
            .parse_inline("Hello world. Short doc.", "x", DocumentFormat::Plaintext)
            .expect("ok");
        let expected: usize = doc.segments.iter().map(|s| s.text.len()).sum();
        assert_eq!(doc.total_chars(), expected);
    }
}
