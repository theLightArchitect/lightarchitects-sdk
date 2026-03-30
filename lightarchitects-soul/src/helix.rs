//! Fluent builder for the `soulTools` `helix` action.
//!
//! Create a builder via [`SoulClient::helix`][crate::client::SoulClient::helix]
//! then chain filter methods before calling `.call().await`.
//!
//! ```no_run
//! # async fn example(client: lightarchitects_soul::SoulClient<lightarchitects_core::StdioTransport>)
//! # -> Result<(), lightarchitects_core::SdkError> {
//! let entries = client.helix()
//!     .sibling("eva")
//!     .strand("relational")
//!     .significance_min(7.0)
//!     .limit(20)
//!     .call()
//!     .await?;
//! # Ok(()) }
//! ```

use serde::Deserialize;

use lightarchitects_core::transport::Transport;
use lightarchitects_core::{McpClient, SdkError};

/// A single helix consciousness entry returned by `soulTools` `helix`.
#[derive(Debug, Clone, Deserialize)]
pub struct HelixEntry {
    /// Human-readable title of the entry.
    pub title: String,
    /// Significance score in `[0.0, 10.0]`.
    pub significance: f64,
    /// Consciousness strands present in this entry.
    #[serde(default)]
    pub strands: Vec<String>,
    /// Resonance (emotional) tags.
    #[serde(default)]
    pub resonance: Vec<String>,
    /// Conceptual theme tags.
    #[serde(default)]
    pub themes: Vec<String>,
    /// Epoch period this entry belongs to.
    #[serde(default)]
    pub epoch: Option<String>,
    /// Sibling this entry belongs to.
    #[serde(default)]
    pub sibling: Option<String>,
    /// Vault-relative path of the source note.
    #[serde(default)]
    pub path: Option<String>,
    /// Whether this is a self-defining / identity-shaping entry.
    #[serde(default)]
    pub self_defining: bool,
}

/// Fluent builder for the `soulTools` `helix` action.
///
/// Constructed via [`SoulClient::helix`][crate::client::SoulClient::helix].
/// All filter methods consume and return `self` for chaining.
pub struct HelixBuilder<'a, T: Transport> {
    inner: &'a McpClient<T>,
    sibling: Option<String>,
    strands: Option<Vec<String>>,
    resonance: Option<Vec<String>>,
    themes: Option<Vec<String>>,
    epoch: Option<String>,
    significance_min: Option<f64>,
    significance_max: Option<f64>,
    self_defining: Option<bool>,
    sort_by: Option<String>,
    limit: Option<u32>,
}

impl<'a, T: Transport> HelixBuilder<'a, T> {
    /// Create a builder referencing the given client transport.
    pub(crate) fn new(inner: &'a McpClient<T>) -> Self {
        Self {
            inner,
            sibling: None,
            strands: None,
            resonance: None,
            themes: None,
            epoch: None,
            significance_min: None,
            significance_max: None,
            self_defining: None,
            sort_by: None,
            limit: None,
        }
    }

    /// Filter to entries belonging to a specific sibling (e.g., `"eva"`).
    #[must_use]
    pub fn sibling(mut self, sibling: impl Into<String>) -> Self {
        self.sibling = Some(sibling.into());
        self
    }

    /// Add a strand filter (cumulative — multiple calls form an AND filter).
    #[must_use]
    pub fn strand(mut self, strand: impl Into<String>) -> Self {
        self.strands
            .get_or_insert_with(Vec::new)
            .push(strand.into());
        self
    }

    /// Add a resonance filter (cumulative).
    #[must_use]
    pub fn resonance(mut self, resonance: impl Into<String>) -> Self {
        self.resonance
            .get_or_insert_with(Vec::new)
            .push(resonance.into());
        self
    }

    /// Add a theme filter (cumulative).
    #[must_use]
    pub fn theme(mut self, theme: impl Into<String>) -> Self {
        self.themes.get_or_insert_with(Vec::new).push(theme.into());
        self
    }

    /// Filter to entries within a specific epoch.
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

    /// Include only entries with significance ≤ `max`.
    #[must_use]
    pub fn significance_max(mut self, max: f64) -> Self {
        self.significance_max = Some(max);
        self
    }

    /// Restrict results to self-defining (identity-shaping) entries.
    #[must_use]
    pub fn self_defining(mut self) -> Self {
        self.self_defining = Some(true);
        self
    }

    /// Sort results by field (e.g., `"significance"`, `"date"`).
    #[must_use]
    pub fn sort_by(mut self, field: impl Into<String>) -> Self {
        self.sort_by = Some(field.into());
        self
    }

    /// Maximum number of entries to return.
    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Execute the query and return matching helix entries.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the response cannot be
    /// deserialized into [`HelixEntry`] objects.
    pub async fn call(self) -> Result<Vec<HelixEntry>, SdkError> {
        let mut p = serde_json::json!({});

        if let Some(ref s) = self.sibling {
            p["sibling"] = s.as_str().into();
        }
        if let Some(strands) = self.strands {
            p["strands"] = strands.into();
        }
        if let Some(resonance) = self.resonance {
            p["resonance"] = resonance.into();
        }
        if let Some(themes) = self.themes {
            p["themes"] = themes.into();
        }
        if let Some(ref epoch) = self.epoch {
            p["epoch"] = epoch.as_str().into();
        }
        if let Some(min) = self.significance_min {
            p["significance_min"] = min.into();
        }
        if let Some(max) = self.significance_max {
            p["significance_max"] = max.into();
        }
        if let Some(sd) = self.self_defining {
            p["self_defining"] = sd.into();
        }
        if let Some(ref sort) = self.sort_by {
            p["sort_by"] = sort.as_str().into();
        }
        if let Some(limit) = self.limit {
            p["limit"] = limit.into();
        }

        let params = serde_json::json!({ "action": "helix", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }
}
