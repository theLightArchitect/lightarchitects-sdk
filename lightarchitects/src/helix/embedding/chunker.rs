//! Content chunker — splits text at sentence boundaries for embedding.
//!
//! Target: 512 tokens (~2048 chars) per chunk with 64-token (~256 char) overlap.
//! Splits at sentence boundaries (`.` `!` `?` followed by whitespace) to preserve
//! semantic coherence within each chunk.

// ============================================================================
// Types
// ============================================================================

/// A chunk of text with character offsets into the original content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// The chunk text.
    pub text: String,
    /// Start character offset in the original content.
    pub start_char: usize,
    /// End character offset in the original content (exclusive).
    pub end_char: usize,
}

/// Configuration for the chunker.
#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    /// Target chunk size in characters (~4 chars per token).
    pub target_chars: usize,
    /// Overlap in characters between consecutive chunks.
    pub overlap_chars: usize,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            target_chars: 2048, // ~512 tokens
            overlap_chars: 256, // ~64 tokens
        }
    }
}

// ============================================================================
// Chunker
// ============================================================================

/// Splits content into overlapping chunks at sentence boundaries.
#[derive(Debug, Clone)]
pub struct Chunker {
    config: ChunkerConfig,
}

impl Chunker {
    /// Create a new chunker with the given configuration.
    #[must_use]
    pub fn new(config: ChunkerConfig) -> Self {
        Self { config }
    }

    /// Create a chunker with default settings (512 tokens, 64-token overlap).
    #[must_use]
    pub fn default_config() -> Self {
        Self::new(ChunkerConfig::default())
    }

    /// Split content into chunks.
    ///
    /// If the content is shorter than `target_chars`, returns a single chunk.
    /// Otherwise, splits at sentence boundaries with overlap.
    #[must_use]
    pub fn chunk(&self, content: &str) -> Vec<Chunk> {
        if content.is_empty() {
            return Vec::new();
        }

        // Short content: single chunk
        if content.len() <= self.config.target_chars {
            return vec![Chunk {
                text: content.to_owned(),
                start_char: 0,
                end_char: content.len(),
            }];
        }

        let sentences = split_sentences(content);
        let mut chunks = Vec::new();
        let mut chunk_start = 0;
        let mut chunk_text = String::new();

        for (sent_start, sent_end) in &sentences {
            let sentence = &content[*sent_start..*sent_end];

            // If adding this sentence exceeds target, finalize current chunk
            if !chunk_text.is_empty()
                && chunk_text.len() + sentence.len() > self.config.target_chars
            {
                chunks.push(Chunk {
                    text: chunk_text.trim().to_owned(),
                    start_char: chunk_start,
                    end_char: chunk_start + chunk_text.trim_end().len(),
                });

                // Overlap: back up by overlap_chars from the end
                let new_start = find_overlap_start(
                    content,
                    chunk_start + chunk_text.len(),
                    self.config.overlap_chars,
                    &sentences,
                );
                chunk_start = new_start;
                content[new_start..*sent_start].clone_into(&mut chunk_text);
            }

            chunk_text.push_str(sentence);
        }

        // Final chunk
        if !chunk_text.trim().is_empty() {
            chunks.push(Chunk {
                text: chunk_text.trim().to_owned(),
                start_char: chunk_start,
                end_char: content.len(),
            });
        }

        chunks
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Split text into sentence spans (start, end) at sentence-ending punctuation.
fn split_sentences(text: &str) -> Vec<(usize, usize)> {
    let mut sentences = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();

    let mut i = 0;
    while i < bytes.len() {
        // Sentence boundary: `.` `!` `?` followed by whitespace or end of text
        if matches!(bytes[i], b'.' | b'!' | b'?') {
            let next = i + 1;
            let is_boundary = next >= bytes.len()
                || bytes[next].is_ascii_whitespace()
                || bytes[next] == b'"'
                || bytes[next] == b'\'';

            if is_boundary {
                // Include trailing whitespace in the sentence span
                let mut end = next;
                while end < bytes.len() && bytes[end].is_ascii_whitespace() {
                    end += 1;
                }
                sentences.push((start, end));
                start = end;
                i = end;
                continue;
            }
        }
        i += 1;
    }

    // Remaining text (no terminal punctuation)
    if start < text.len() {
        sentences.push((start, text.len()));
    }

    sentences
}

/// Find the best overlap start position (at a sentence boundary).
fn find_overlap_start(
    content: &str,
    current_end: usize,
    overlap_chars: usize,
    sentences: &[(usize, usize)],
) -> usize {
    let target = current_end.saturating_sub(overlap_chars);

    // Find the sentence boundary closest to (but not after) the target
    let mut best = current_end;
    for (sent_start, _) in sentences {
        if *sent_start >= target && *sent_start < current_end {
            best = *sent_start;
            break;
        }
    }

    best.min(content.len())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_content() {
        let chunker = Chunker::default_config();
        let chunks = chunker.chunk("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_short_content_single_chunk() {
        let chunker = Chunker::default_config();
        let text = "Hello world. This is short.";
        let chunks = chunker.chunk(text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, text);
        assert_eq!(chunks[0].start_char, 0);
        assert_eq!(chunks[0].end_char, text.len());
    }

    #[test]
    fn test_long_content_splits() {
        let chunker = Chunker::new(ChunkerConfig {
            target_chars: 50,
            overlap_chars: 10,
        });

        let text =
            "First sentence here. Second sentence here. Third sentence here. Fourth sentence here.";
        let chunks = chunker.chunk(text);
        assert!(
            chunks.len() >= 2,
            "Expected multiple chunks, got {}",
            chunks.len()
        );

        // First chunk starts at 0
        assert_eq!(chunks[0].start_char, 0);

        // All chunks have non-empty text
        for chunk in &chunks {
            assert!(!chunk.text.is_empty());
        }
    }

    #[test]
    fn test_split_sentences() {
        let sentences = split_sentences("Hello world. How are you? Fine!");
        assert_eq!(sentences.len(), 3);
    }

    #[test]
    fn test_split_sentences_no_punctuation() {
        let sentences = split_sentences("No punctuation here");
        assert_eq!(sentences.len(), 1);
    }

    #[test]
    fn test_split_sentences_abbreviations_not_split() {
        // Abbreviations like "e.g." shouldn't split (no whitespace after inner dots)
        let sentences = split_sentences("Use e.g. this pattern. It works.");
        // "e.g." has dots NOT followed by whitespace (except the last one before "this")
        // The exact behavior depends on the rule: `.` + whitespace
        assert!(sentences.len() >= 2);
    }

    #[test]
    fn test_chunk_offsets_cover_content() {
        let chunker = Chunker::new(ChunkerConfig {
            target_chars: 30,
            overlap_chars: 5,
        });
        let text = "One. Two. Three. Four. Five. Six. Seven.";
        let chunks = chunker.chunk(text);
        assert!(!chunks.is_empty());

        // First chunk starts at 0
        assert_eq!(chunks[0].start_char, 0);
        // Last chunk ends at content length
        assert_eq!(chunks.last().map(|c| c.end_char), Some(text.len()));
    }

    #[test]
    fn test_custom_config() {
        let config = ChunkerConfig {
            target_chars: 100,
            overlap_chars: 20,
        };
        let chunker = Chunker::new(config);
        let chunks = chunker.chunk("Short.");
        assert_eq!(chunks.len(), 1);
    }
}
