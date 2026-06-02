//! `VertexSearchClient` — HTTP client for the Vertex AI Search (Discovery Engine) API.
//!
//! Auth: Application Default Credentials via `gcloud auth print-access-token`.
//! No additional Cargo dependencies required beyond `reqwest` (already in the SDK).

use std::time::Instant;

use reqwest::Client;
use tracing::debug;

use crate::core::{SdkError, TransportError};

use super::types::{
    ContentSearchSpec, DerivedStructData, ExtractiveContentSpec, SearchRequest, SearchResponse,
    SnippetSpec, SummarySpec, VertexSearchOutput, VertexSearchResult,
};

const DISCOVERY_ENGINE_BASE: &str = "https://discoveryengine.googleapis.com/v1";

// ── Config ────────────────────────────────────────────────────────────────────

/// Configuration for the Vertex AI Search data store and search engine.
#[derive(Debug, Clone)]
pub struct VertexSearchConfig {
    /// GCP project ID (e.g. `"webshell-497114"`).
    pub project_id: String,
    /// Search engine ID created in the Discovery Engine console.
    pub engine_id: String,
    /// Data store ID backing the engine.
    pub data_store_id: String,
}

impl Default for VertexSearchConfig {
    fn default() -> Self {
        Self {
            project_id: std::env::var("VERTEX_PROJECT_ID")
                .unwrap_or_else(|_| "webshell-497114".to_string()),
            engine_id: std::env::var("VERTEX_ENGINE_ID")
                .unwrap_or_else(|_| "quantum-search".to_string()),
            data_store_id: std::env::var("VERTEX_DATA_STORE_ID")
                .unwrap_or_else(|_| "quantum-security-standards".to_string()),
        }
    }
}

impl VertexSearchConfig {
    fn serving_config_path(&self) -> String {
        format!(
            "projects/{}/locations/global/collections/default_collection\
             /engines/{}/servingConfigs/default_search",
            self.project_id, self.engine_id,
        )
    }
}

// ── Client ────────────────────────────────────────────────────────────────────

/// HTTP client for Vertex AI Search.
///
/// Authenticates via Application Default Credentials — requires `gcloud` to be
/// installed and the caller to have run `gcloud auth application-default login`
/// (or to be running on a GCP instance with an attached service account).
///
/// # Example
///
/// ```no_run
/// # async fn example() -> Result<(), lightarchitects::core::SdkError> {
/// use lightarchitects::vertex_search::VertexSearchClient;
///
/// let client = VertexSearchClient::default();
/// let output = client.search("SQL injection prevention", 10, false).await?;
/// for r in &output.results {
///     println!("[{}] {} — {}", r.citation, r.title, r.uri);
/// }
/// # Ok(()) }
/// ```
#[derive(Debug, Clone)]
pub struct VertexSearchClient {
    http: Client,
    config: VertexSearchConfig,
}

impl Default for VertexSearchClient {
    fn default() -> Self {
        Self {
            http: Client::new(),
            config: VertexSearchConfig::default(),
        }
    }
}

impl VertexSearchClient {
    /// Construct with a custom config (project, engine, data store).
    #[must_use]
    pub fn with_config(config: VertexSearchConfig) -> Self {
        Self {
            http: Client::new(),
            config,
        }
    }

    /// Returns `true` if `gcloud` is reachable on `$PATH`.
    ///
    /// Use this to gate provider registration — if `gcloud` is absent the
    /// provider cannot authenticate and should be skipped.
    #[must_use]
    pub fn is_available() -> bool {
        std::process::Command::new("gcloud")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Execute a search query against the configured data store.
    ///
    /// - `query`: natural-language or keyword query string.
    /// - `max_results`: maximum documents to return (1–100).
    /// - `summarize`: when `true`, requests an LLM-synthesized summary.
    ///   Requires Enterprise Edition; incurs additional per-query cost.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError`] on auth failure, HTTP error, or response parse failure.
    pub async fn search(
        &self,
        query: &str,
        max_results: usize,
        summarize: bool,
    ) -> Result<VertexSearchOutput, SdkError> {
        let token = Self::get_access_token()?;
        let start = Instant::now();

        let body = build_request(query, max_results, summarize);
        let url = format!(
            "{}/{}:search",
            DISCOVERY_ENGINE_BASE,
            self.config.serving_config_path()
        );

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("X-Goog-User-Project", &self.config.project_id)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                SdkError::Transport(TransportError::Http(format!(
                    "Vertex AI Search request failed: {e}"
                )))
            })?;

        let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SdkError::Transport(TransportError::Http(format!(
                "Vertex AI Search returned {status}: {text}"
            ))));
        }

        let raw: SearchResponse = resp.json().await.map_err(|e| {
            SdkError::Transport(TransportError::Http(format!(
                "Failed to parse Vertex response: {e}"
            )))
        })?;

        debug!(
            results = raw.results.len(),
            latency_ms, "Vertex AI Search returned"
        );

        Ok(build_output(raw, latency_ms))
    }

    /// Obtain a short-lived ADC access token via the `gcloud` CLI.
    fn get_access_token() -> Result<String, SdkError> {
        let out = std::process::Command::new("gcloud")
            .args(["auth", "print-access-token"])
            .output()
            .map_err(|e| {
                SdkError::Transport(TransportError::Http(format!("gcloud not found: {e}")))
            })?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(SdkError::Transport(TransportError::Http(format!(
                "gcloud auth print-access-token failed: {}. \
                 Run: gcloud auth application-default login",
                stderr.trim()
            ))));
        }

        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_request(query: &str, max_results: usize, summarize: bool) -> SearchRequest {
    SearchRequest {
        query: query.to_string(),
        page_size: max_results.min(100),
        content_search_spec: ContentSearchSpec {
            snippet_spec: SnippetSpec {
                return_snippet: true,
                max_snippet_count: 3,
            },
            extractive_content_spec: ExtractiveContentSpec {
                max_extractive_answer_count: 1,
                max_extractive_segment_count: 2,
            },
            summary_spec: summarize.then_some(SummarySpec {
                summary_result_count: 5,
                include_citations: true,
                ignore_adversarial_query: true,
                ignore_non_summary_seeking_query: true,
            }),
        },
    }
}

fn build_output(raw: SearchResponse, latency_ms: u64) -> VertexSearchOutput {
    let results = raw
        .results
        .into_iter()
        .enumerate()
        .map(|(i, item)| map_result(i, item.document.derived_struct_data))
        .collect();

    VertexSearchOutput {
        results,
        summary: raw.summary.map(|s| s.summary_text),
        latency_ms,
        total_size: raw.total_size,
    }
}

fn map_result(index: usize, data: DerivedStructData) -> VertexSearchResult {
    let title = data
        .title
        .unwrap_or_else(|| format!("Document {}", index + 1));
    let uri = data.link.unwrap_or_default();

    // Prefer extractive answers (shorter, higher-confidence); fall back to snippets.
    let content = data
        .extractive_answers
        .into_iter()
        .next()
        .map(|a| a.content)
        .or_else(|| {
            data.snippets
                .into_iter()
                .find(|s| s.snippet_status.as_deref() == Some("SUCCESS"))
                .map(|s| s.snippet)
        })
        .unwrap_or_default();

    VertexSearchResult {
        title,
        uri,
        content,
        citation: format!("[{}]", index + 1),
        source_type: "corpus".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertex_search::types::SearchResponse;

    #[allow(clippy::unwrap_used)]
    #[test]
    fn test_serving_config_path() {
        let cfg = VertexSearchConfig {
            project_id: "webshell-497114".to_string(),
            engine_id: "quantum-search".to_string(),
            data_store_id: "quantum-security-standards".to_string(),
        };
        let path = cfg.serving_config_path();
        assert!(path.contains("webshell-497114"));
        assert!(path.contains("quantum-search"));
        assert!(path.contains("servingConfigs/default_search"));
    }

    #[allow(clippy::unwrap_used)]
    #[test]
    fn test_build_request_summarize_false() {
        let req = build_request("CVE-2024-1234", 5, false);
        assert_eq!(req.query, "CVE-2024-1234");
        assert_eq!(req.page_size, 5);
        assert!(req.content_search_spec.summary_spec.is_none());
    }

    #[allow(clippy::unwrap_used)]
    #[test]
    fn test_build_request_summarize_true() {
        let req = build_request("SQL injection", 10, true);
        assert!(req.content_search_spec.summary_spec.is_some());
    }

    #[allow(clippy::unwrap_used)]
    #[test]
    fn test_build_output_empty() {
        let raw = SearchResponse {
            results: vec![],
            summary: None,
            total_size: Some(0),
        };
        let out = build_output(raw, 42);
        assert!(out.results.is_empty());
        assert!(out.summary.is_none());
        assert_eq!(out.latency_ms, 42);
    }

    #[allow(clippy::unwrap_used)]
    #[test]
    fn test_page_size_capped_at_100() {
        let req = build_request("test", 999, false);
        assert_eq!(req.page_size, 100);
    }
}
