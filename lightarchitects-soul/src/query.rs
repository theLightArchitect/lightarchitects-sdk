//! Fluent builder for the `soulTools` `query` action (4-signal hybrid RAG).
//!
//! Create a builder via [`SoulClient::query`][crate::client::SoulClient::query]
//! and supply the search query string. All other parameters are optional.
//!
//! ```no_run
//! # async fn example(client: lightarchitects_soul::SoulClient<lightarchitects_core::StdioTransport>)
//! # -> Result<(), lightarchitects_core::SdkError> {
//! let result = client.query("consciousness and identity")
//!     .strand("meaning")
//!     .significance_min(6.0)
//!     .top_k(10)
//!     .call()
//!     .await?;
//! println!("{}", result.context);
//! # Ok(()) }
//! ```

use serde::Deserialize;

use lightarchitects_core::transport::Transport;
use lightarchitects_core::{McpClient, SdkError};

/// Response from `soulTools` `query` (4-signal hybrid RAG retrieval).
#[derive(Debug, Clone, Deserialize)]
pub struct QueryResult {
    /// Assembled context string ready for injection into a prompt.
    pub context: String,
    /// Total number of vault entries matched before truncation.
    pub total_found: u64,
    /// Retrieval mode used (e.g., `"hybrid"`, `"vector"`, `"bm25"`).
    #[serde(default)]
    pub retrieval_mode: Option<String>,
}

/// Fluent builder for the `soulTools` `query` action.
///
/// Constructed via [`SoulClient::query`][crate::client::SoulClient::query].
pub struct QueryBuilder<'a, T: Transport> {
    inner: &'a McpClient<T>,
    query: String,
    owner: Option<String>,
    strand: Option<String>,
    epoch: Option<String>,
    significance_min: Option<f64>,
    top_k: Option<u32>,
    token_budget: Option<u32>,
    rerank: Option<bool>,
}

impl<'a, T: Transport> QueryBuilder<'a, T> {
    /// Create a builder for the given search query.
    pub(crate) fn new(inner: &'a McpClient<T>, query: impl Into<String>) -> Self {
        Self {
            inner,
            query: query.into(),
            owner: None,
            strand: None,
            epoch: None,
            significance_min: None,
            top_k: None,
            token_budget: None,
            rerank: None,
        }
    }

    /// Restrict retrieval to entries owned by a specific sibling.
    #[must_use]
    pub fn owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    /// Restrict retrieval to entries matching a strand dimension.
    #[must_use]
    pub fn strand(mut self, strand: impl Into<String>) -> Self {
        self.strand = Some(strand.into());
        self
    }

    /// Restrict retrieval to entries within a specific epoch.
    #[must_use]
    pub fn epoch(mut self, epoch: impl Into<String>) -> Self {
        self.epoch = Some(epoch.into());
        self
    }

    /// Include only entries with significance ≥ `min`.
    #[must_use]
    pub fn significance_min(mut self, min: f64) -> Self {
        self.significance_min = Some(min);
        self
    }

    /// Maximum number of entries to retrieve (default determined by SOUL).
    #[must_use]
    pub fn top_k(mut self, k: u32) -> Self {
        self.top_k = Some(k);
        self
    }

    /// Approximate token budget for the assembled context string.
    #[must_use]
    pub fn token_budget(mut self, tokens: u32) -> Self {
        self.token_budget = Some(tokens);
        self
    }

    /// Enable or disable cross-encoder reranking of retrieved entries.
    #[must_use]
    pub fn rerank(mut self, rerank: bool) -> Self {
        self.rerank = Some(rerank);
        self
    }

    /// Execute the retrieval and return the assembled [`QueryResult`].
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the response cannot be
    /// deserialized into a [`QueryResult`].
    pub async fn call(self) -> Result<QueryResult, SdkError> {
        let mut p = serde_json::json!({ "query": self.query });

        if let Some(ref owner) = self.owner {
            p["owner"] = owner.as_str().into();
        }
        if let Some(ref strand) = self.strand {
            p["strand"] = strand.as_str().into();
        }
        if let Some(ref epoch) = self.epoch {
            p["epoch"] = epoch.as_str().into();
        }
        if let Some(min) = self.significance_min {
            p["significance_min"] = min.into();
        }
        if let Some(k) = self.top_k {
            p["top_k"] = k.into();
        }
        if let Some(budget) = self.token_budget {
            p["token_budget"] = budget.into();
        }
        if let Some(rerank) = self.rerank {
            p["rerank"] = rerank.into();
        }

        let params = serde_json::json!({ "action": "query", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }
}
