//! Request and response types for the Vertex AI Search (Discovery Engine) API.

use serde::{Deserialize, Serialize};

// ── Request ───────────────────────────────────────────────────────────────────

/// Top-level search request body.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SearchRequest {
    /// Natural-language or keyword query.
    pub(super) query: String,
    /// Maximum results to return (1–100).
    pub(super) page_size: usize,
    /// Content extraction settings.
    pub(super) content_search_spec: ContentSearchSpec,
}

/// Controls what content is extracted alongside ranked results.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_field_names)]
pub(super) struct ContentSearchSpec {
    /// Inline document snippets.
    pub(super) snippet_spec: SnippetSpec,
    /// Verbatim passage extraction.
    pub(super) extractive_content_spec: ExtractiveContentSpec,
    /// LLM-synthesized summary (optional — incurs Enterprise-tier cost).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) summary_spec: Option<SummarySpec>,
}

/// Snippet extraction settings.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SnippetSpec {
    /// Whether to return snippets.
    pub(super) return_snippet: bool,
    /// How many snippet segments to return per result.
    pub(super) max_snippet_count: u32,
}

/// Extractive content settings (verbatim passages).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExtractiveContentSpec {
    /// Max extractive answers (short, high-confidence passages).
    pub(super) max_extractive_answer_count: u32,
    /// Max extractive segments (longer context windows).
    pub(super) max_extractive_segment_count: u32,
}

/// Summary generation settings (requires Enterprise Edition).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SummarySpec {
    /// How many top results to ground the summary on.
    pub(super) summary_result_count: u32,
    /// Whether to include inline `[N]` citation markers.
    pub(super) include_citations: bool,
    /// Drop adversarial queries before summary generation.
    pub(super) ignore_adversarial_query: bool,
    /// Drop non-answer-seeking queries before summary generation.
    pub(super) ignore_non_summary_seeking_query: bool,
}

// ── Response ──────────────────────────────────────────────────────────────────

/// Top-level search response from the Discovery Engine API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SearchResponse {
    /// Ranked result list.
    #[serde(default)]
    pub(super) results: Vec<SearchResultItem>,
    /// Optional LLM-synthesized summary over results.
    #[serde(default)]
    pub(super) summary: Option<SearchSummary>,
    /// Total matching documents in the index.
    #[serde(default)]
    pub(super) total_size: Option<i32>,
}

/// A single ranked result.
#[derive(Debug, Deserialize)]
pub(super) struct SearchResultItem {
    /// The matched document with extracted content.
    pub(super) document: DocumentItem,
}

/// Document and its derived (extracted) content.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DocumentItem {
    /// Structured fields derived from the document by the indexer.
    pub(super) derived_struct_data: DerivedStructData,
}

/// Indexer-derived fields populated for unstructured documents.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct DerivedStructData {
    /// Document title (from metadata or first heading).
    #[serde(default)]
    pub(super) title: Option<String>,
    /// Document URI (GCS path or web URL).
    #[serde(default)]
    pub(super) link: Option<String>,
    /// Inline snippets with status.
    #[serde(default)]
    pub(super) snippets: Vec<SnippetItem>,
    /// Verbatim extractive answers (short, high-confidence passages).
    #[serde(default)]
    pub(super) extractive_answers: Vec<ExtractiveAnswer>,
}

/// A single inline document snippet.
#[derive(Debug, Deserialize)]
pub(super) struct SnippetItem {
    /// The snippet text.
    pub(super) snippet: String,
    /// `"SUCCESS"` when the snippet was successfully extracted.
    #[serde(default)]
    pub(super) snippet_status: Option<String>,
}

/// A verbatim extractive answer passage.
#[derive(Debug, Deserialize)]
pub(super) struct ExtractiveAnswer {
    /// The passage content.
    pub(super) content: String,
}

/// LLM-synthesized summary over top results.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SearchSummary {
    /// The synthesized answer text, with inline `[N]` citation markers.
    pub(super) summary_text: String,
}

// ── Public output types ───────────────────────────────────────────────────────

/// A single document result from a Vertex AI Search query.
#[derive(Debug, Clone)]
pub struct VertexSearchResult {
    /// Document title extracted from metadata or the first heading.
    pub title: String,
    /// Document URI — GCS object path (`gs://...`) or web URL.
    pub uri: String,
    /// Best available passage: extractive answer, snippet, or empty.
    pub content: String,
    /// One-based citation marker, e.g. `"[1]"`.
    pub citation: String,
    /// Source type tag — always `"corpus"` for data-store results.
    pub source_type: String,
}

/// Output of a [`super::VertexSearchClient::search`] call.
#[derive(Debug, Clone)]
pub struct VertexSearchOutput {
    /// Ranked document results.
    pub results: Vec<VertexSearchResult>,
    /// LLM-synthesized summary (present when `summarize: true` is set and
    /// the data store is on Enterprise Edition).
    pub summary: Option<String>,
    /// Wall-clock latency of the API call in milliseconds.
    pub latency_ms: u64,
    /// Total matching documents reported by the index.
    pub total_size: Option<i32>,
}
