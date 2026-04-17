//! Entity extractor — calls an LLM to pull entities and relations from a segment.
//!
//! # Provider strategy
//!
//! Uses a caller-supplied [`CompletionProvider`] trait object.
//! The [`neural_engine::TierRouter`]-based implementation is wired in
//! `soul-mcp/src/tools/graphrag_ingest.rs`; an embedding-signal fallback
//! is available when no completion provider is present.
//!
//! # Output format
//!
//! The LLM is prompted to return JSON:
//! ```json
//! {
//!   "entities": [{"name": "...", "type": "..."}, ...],
//!   "relations": [{"subject": "...", "predicate": "...", "object": "..."}, ...]
//! }
//! ```
//! Malformed responses are logged and return an empty extraction rather than
//! failing the whole document.
//!
//! # Embedding fallback
//!
//! When `extract_with_embedding_signal` is used (no completion provider),
//! noun-phrase heuristics derive candidate entities and a placeholder
//! relation `"co-occurs"` is asserted between adjacent candidates.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::document_parser::Segment;

// ─── CompletionProvider ──────────────────────────────────────────────────────

/// Async LLM completion interface.
///
/// Implemented by the `InferenceBridge` adapter in `soul-mcp`.
/// The lightarchitects-helix crate stays pure — no dependency on `neural-engine`.
#[async_trait]
pub trait CompletionProvider: Send + Sync {
    /// Generate a completion given `system` and `user` prompts.
    ///
    /// Returns the raw response text or an error message.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` when the provider fails or has no configured backend.
    async fn complete(&self, system: &str, user: &str) -> Result<String, String>;
}

// ─── Types ───────────────────────────────────────────────────────────────────

/// A named entity extracted from a segment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    /// Entity name (e.g., "Kevin Francis Tan").
    pub name: String,
    /// Semantic type (e.g., "Person", "Organization", "Concept").
    #[serde(rename = "type")]
    pub entity_type: String,
}

/// A directed relation between two entities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relation {
    /// Source entity name.
    pub subject: String,
    /// Relation predicate (e.g., `"founded"`, `"developed"`, `"cited_by"`).
    pub predicate: String,
    /// Target entity name.
    pub object: String,
}

/// Entities and relations extracted from a single segment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Extraction {
    /// Named entities found in the segment.
    pub entities: Vec<Entity>,
    /// Directed relations between entities.
    pub relations: Vec<Relation>,
}

impl Extraction {
    /// `true` when both entity and relation lists are empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty() && self.relations.is_empty()
    }
}

/// Extraction result for a single segment, including provenance.
#[derive(Debug, Clone)]
pub struct SegmentExtraction {
    /// Zero-based segment index within the document.
    pub segment_index: usize,
    /// Most-recent section heading (from the parser).
    pub section_hint: Option<String>,
    /// Extracted entities and relations.
    pub extraction: Extraction,
}

// ─── EntityExtractor ─────────────────────────────────────────────────────────

/// Extracts entities and relations from document segments.
///
/// When a [`CompletionProvider`] is supplied, calls the LLM for each segment.
/// When none is supplied, falls back to the embedding-signal heuristic
/// (lower recall, zero LLM cost).
pub struct EntityExtractor {
    provider: Option<Arc<dyn CompletionProvider>>,
}

impl EntityExtractor {
    /// Create an extractor backed by the given completion provider.
    #[must_use]
    pub fn new(provider: Arc<dyn CompletionProvider>) -> Self {
        Self {
            provider: Some(provider),
        }
    }

    /// Create an extractor that uses the embedding-signal heuristic fallback.
    ///
    /// Use this when no LLM provider is available. Precision is lower but
    /// no external call is made and extraction never fails.
    #[must_use]
    pub fn with_fallback_only() -> Self {
        Self { provider: None }
    }

    /// Extract entities and relations from all segments of a document.
    ///
    /// Iterates segments in order. LLM failures on individual segments are
    /// logged and produce an empty extraction rather than aborting the whole
    /// document. The extraction strategy (LLM or fallback) is selected once
    /// and applied uniformly to all segments.
    pub async fn extract_all(&self, segments: &[Segment]) -> Vec<SegmentExtraction> {
        let mut results = Vec::with_capacity(segments.len());
        for seg in segments {
            results.push(self.extract_segment(seg).await);
        }
        results
    }

    /// Extract from a single segment.
    async fn extract_segment(&self, seg: &Segment) -> SegmentExtraction {
        let extraction = match &self.provider {
            Some(provider) => {
                Self::extract_with_llm(provider.as_ref() as &dyn CompletionProvider, seg).await
            }
            None => extract_with_embedding_signal(seg),
        };

        SegmentExtraction {
            segment_index: seg.index,
            section_hint: seg.section_hint.clone(),
            extraction,
        }
    }

    /// LLM-backed extraction path.
    async fn extract_with_llm(provider: &dyn CompletionProvider, seg: &Segment) -> Extraction {
        let user_prompt = build_user_prompt(seg);

        match provider.complete(SYSTEM_PROMPT, &user_prompt).await {
            Ok(response) => parse_llm_response(&response, seg.index),
            Err(e) => {
                warn!(segment = seg.index, error = %e, "LLM extraction failed, using fallback");
                extract_with_embedding_signal(seg)
            }
        }
    }
}

// ─── Prompts ─────────────────────────────────────────────────────────────────

const SYSTEM_PROMPT: &str = "\
You are a knowledge-graph extraction assistant. Given a text segment, \
extract named entities and directed relations between them.\n\
\n\
Return ONLY valid JSON in this exact shape — no markdown, no explanation:\n\
{\"entities\":[{\"name\":\"...\",\"type\":\"...\"}],\
\"relations\":[{\"subject\":\"...\",\"predicate\":\"...\",\"object\":\"...\"}]}\n\
\n\
Entity types: Person, Organization, Concept, Technology, Event, Location, Other.\n\
Relation predicates: use lowercase_snake_case (e.g. \"developed_by\", \"cited_in\").\n\
Omit entities and relations you are not confident about.";

/// Build the user prompt for a segment.
fn build_user_prompt(seg: &Segment) -> String {
    let mut buf = String::with_capacity(seg.text.len().saturating_add(100));
    if let Some(ref hint) = seg.section_hint {
        buf.push_str("Section: ");
        buf.push_str(hint);
        buf.push('\n');
    }
    buf.push_str("Text:\n");
    buf.push_str(&seg.text);
    buf
}

// ─── LLM response parsing ─────────────────────────────────────────────────────

/// Parse the raw LLM response string into an [`Extraction`].
///
/// Strips markdown fences if present, then deserialises JSON.
/// Returns an empty extraction on any parse failure.
fn parse_llm_response(response: &str, segment_index: usize) -> Extraction {
    let trimmed = strip_markdown_fences(response.trim());

    match serde_json::from_str::<Extraction>(trimmed) {
        Ok(extraction) => {
            debug!(
                segment = segment_index,
                entities = extraction.entities.len(),
                relations = extraction.relations.len(),
                "LLM extraction parsed"
            );
            extraction
        }
        Err(e) => {
            warn!(
                segment = segment_index,
                error = %e,
                raw = %trimmed.chars().take(120).collect::<String>(),
                "Failed to parse LLM response as Extraction JSON"
            );
            Extraction::default()
        }
    }
}

/// Strip ```json ... ``` or ``` ... ``` fences from an LLM response.
fn strip_markdown_fences(text: &str) -> &str {
    let text = text.strip_prefix("```json").unwrap_or(text);
    let text = text.strip_prefix("```").unwrap_or(text);
    let text = text.strip_suffix("```").unwrap_or(text);
    text.trim()
}

// ─── Embedding-signal fallback ────────────────────────────────────────────────

/// Heuristic entity extraction using capitalised noun-phrase detection.
///
/// Identifies runs of Title-Cased tokens as candidate entities (type=`Other`).
/// Asserts `co_occurs` relations between consecutive candidates within the
/// same segment. No external calls — deterministic for any fixed input.
fn extract_with_embedding_signal(seg: &Segment) -> Extraction {
    let candidates = find_capitalised_phrases(&seg.text);

    let entities: Vec<Entity> = candidates
        .iter()
        .map(|name| Entity {
            name: name.clone(),
            entity_type: "Other".to_owned(),
        })
        .collect();

    let relations = build_cooccurrence_relations(&candidates);

    Extraction {
        entities,
        relations,
    }
}

/// Find runs of Title-Cased words as entity candidates.
fn find_capitalised_phrases(text: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    for token in text.split_whitespace() {
        let clean: String = token.chars().filter(|c| c.is_alphabetic()).collect();
        if !clean.is_empty() && clean.starts_with(|c: char| c.is_uppercase()) {
            current.push(token.trim_matches(|c: char| !c.is_alphanumeric()));
        } else {
            if !current.is_empty() {
                candidates.push(current.join(" "));
            }
            current.clear();
        }
    }

    if !current.is_empty() {
        candidates.push(current.join(" "));
    }

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|c| c.len() > 1 && seen.insert(c.clone()))
        .collect()
}

/// Build `co_occurs` relations between consecutive entity candidates.
fn build_cooccurrence_relations(candidates: &[String]) -> Vec<Relation> {
    candidates
        .windows(2)
        .map(|pair| Relation {
            subject: pair[0].clone(),
            predicate: "co_occurs".to_owned(),
            object: pair[1].clone(),
        })
        .collect()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    struct MockProvider {
        response: String,
    }

    #[async_trait::async_trait]
    impl CompletionProvider for MockProvider {
        async fn complete(&self, _system: &str, _user: &str) -> Result<String, String> {
            Ok(self.response.clone())
        }
    }

    struct FailProvider;

    #[async_trait::async_trait]
    impl CompletionProvider for FailProvider {
        async fn complete(&self, _system: &str, _user: &str) -> Result<String, String> {
            Err("provider unavailable".to_owned())
        }
    }

    fn seg(text: &str) -> Segment {
        Segment {
            text: text.to_owned(),
            index: 0,
            section_hint: None,
            start_char: 0,
            end_char: text.len(),
        }
    }

    #[test]
    fn extraction_is_empty_by_default() {
        let e = Extraction::default();
        assert!(e.is_empty());
    }

    #[tokio::test]
    async fn llm_provider_parses_valid_json() {
        let json = r#"{"entities":[{"name":"Alice","type":"Person"}],"relations":[]}"#;
        let provider = Arc::new(MockProvider {
            response: json.to_owned(),
        });
        let extractor = EntityExtractor::new(provider);
        let results = extractor
            .extract_all(&[seg("Alice founded the company.")])
            .await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].extraction.entities.len(), 1);
        assert_eq!(results[0].extraction.entities[0].name, "Alice");
    }

    #[tokio::test]
    async fn llm_provider_parses_fenced_json() {
        let json = "```json\n{\"entities\":[],\"relations\":[{\"subject\":\"A\",\"predicate\":\"b\",\"object\":\"C\"}]}\n```";
        let provider = Arc::new(MockProvider {
            response: json.to_owned(),
        });
        let extractor = EntityExtractor::new(provider);
        let results = extractor.extract_all(&[seg("text")]).await;
        assert_eq!(results[0].extraction.relations.len(), 1);
    }

    #[tokio::test]
    async fn provider_failure_falls_back_gracefully() {
        let extractor = EntityExtractor::new(Arc::new(FailProvider));
        // Should not panic; returns fallback extraction
        let results = extractor.extract_all(&[seg("Alice meets Bob.")]).await;
        assert_eq!(results.len(), 1);
        // fallback produces something or nothing — no crash
    }

    #[test]
    fn fallback_extracts_capitalised_phrases() {
        let result = extract_with_embedding_signal(&seg(
            "Kevin Francis Tan built the Light Architects platform.",
        ));
        assert!(
            !result.entities.is_empty(),
            "expected at least one entity candidate"
        );
    }

    #[tokio::test]
    async fn fallback_only_extractor_does_not_panic() {
        let extractor = EntityExtractor::with_fallback_only();
        let results = extractor.extract_all(&[seg("Test text here.")]).await;
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn cooccurrence_relations_connect_consecutive_candidates() {
        let candidates = vec!["Alice".to_owned(), "Bob".to_owned(), "Charlie".to_owned()];
        let rels = build_cooccurrence_relations(&candidates);
        assert_eq!(rels.len(), 2);
        assert_eq!(rels[0].subject, "Alice");
        assert_eq!(rels[0].object, "Bob");
        assert_eq!(rels[1].subject, "Bob");
        assert_eq!(rels[1].object, "Charlie");
    }

    #[test]
    fn strip_markdown_fences_handles_json_prefix() {
        let fenced = "```json\n{}\n```";
        assert_eq!(strip_markdown_fences(fenced), "{}");
    }

    #[test]
    fn strip_markdown_fences_leaves_plain_json() {
        let plain = r#"{"entities":[],"relations":[]}"#;
        assert_eq!(strip_markdown_fences(plain), plain);
    }

    #[test]
    fn build_user_prompt_includes_section() {
        let mut s = seg("Some text.");
        s.section_hint = Some("Introduction".to_owned());
        let prompt = build_user_prompt(&s);
        assert!(prompt.contains("Introduction"));
        assert!(prompt.contains("Some text."));
    }
}
