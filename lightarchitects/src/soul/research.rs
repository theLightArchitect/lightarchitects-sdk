//! Fluent builder for the `soulTools` `research` action.
//!
//! Create a builder via [`crate::soul::SoulClient::research_builder`], configure it,
//! then call `.call().await` to execute.
//!
//! # Example
//!
//! ```no_run
//! # async fn example(client: crate::soul::SoulClient<crate::core::StdioTransport>)
//! # -> Result<(), crate::core::SdkError> {
//! use crate::soul::{DepthLevel, ResearchSource};
//!
//! let result = client
//!     .research_builder("consciousness and identity")?
//!     .source(ResearchSource::Vault)
//!     .source(ResearchSource::ArXiv)
//!     .depth(DepthLevel::Deep)
//!     .strand("meaning")?
//!     .call()
//!     .await?;
//!
//! println!("mode: {:?}", result.mode);
//! # Ok(()) }
//! ```

use crate::core::transport::Transport;
use crate::core::{McpClient, SdkError};

use crate::soul::types::ResearchResult;

// ── ResearchSource ────────────────────────────────────────────────────────────

/// Research source variants supported by the SOUL research pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResearchSource {
    /// SOUL vault digests (pre-ingested research notes).
    Vault,
    /// arXiv preprint server.
    ArXiv,
    /// Hugging Face model and paper index.
    HuggingFace,
    /// Custom source identifier (any string the SOUL server accepts).
    Custom(String),
}

impl ResearchSource {
    /// Returns the source identifier string the SOUL MCP server expects.
    #[must_use]
    fn as_str(&self) -> &str {
        match self {
            Self::Vault => "vault",
            Self::ArXiv => "arxiv",
            Self::HuggingFace => "huggingface",
            Self::Custom(s) => s.as_str(),
        }
    }
}

// ── DepthLevel ────────────────────────────────────────────────────────────────

/// Research depth: how broadly the pipeline searches and corroborates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DepthLevel {
    /// Shallow search — fast, vault-only or limited external fetch.
    Shallow,
    /// Deep search — full multi-source fetch, quarantine, and corroboration.
    Deep,
}

impl DepthLevel {
    /// Returns the mode string the SOUL MCP server expects.
    #[must_use]
    fn as_mode(self) -> &'static str {
        match self {
            Self::Shallow => "search",
            Self::Deep => "refresh",
        }
    }
}

// ── ResearchBuilder ───────────────────────────────────────────────────────────

/// Fluent builder for the `soulTools` `research` action.
///
/// Construction is allocation-free until `.call()`. The `query` string is
/// validated on construction — null bytes and ASCII control characters
/// (except tab and newline) are rejected.
///
/// # Security
///
/// [`ResearchBuilder::new`] rejects queries containing null bytes (`\0`) or
/// ASCII control characters (`0x00`–`0x1F`, excluding `\t` and `\n`). This
/// prevents the query from being misinterpreted as a control sequence by
/// downstream log macros or shell invocations.
///
/// # Example
///
/// ```no_run
/// # async fn example(client: crate::soul::SoulClient<crate::core::StdioTransport>)
/// # -> Result<(), crate::core::SdkError> {
/// use crate::soul::{DepthLevel, ResearchSource};
///
/// let result = client
///     .research_builder("LLM alignment safety")?
///     .source(ResearchSource::ArXiv)
///     .depth(DepthLevel::Deep)
///     .call()
///     .await?;
///
/// println!("status: {:?}", result.status);
/// # Ok(()) }
/// ```
#[must_use]
pub struct ResearchBuilder<'a, T: Transport> {
    inner: &'a McpClient<T>,
    query: String,
    sources: Vec<ResearchSource>,
    depth: Option<DepthLevel>,
    strand: Option<String>,
}

impl<'a, T: Transport> ResearchBuilder<'a, T> {
    /// Create a builder with the given search query.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if `query` contains null bytes or ASCII
    /// control characters other than `\t` and `\n`.
    pub fn new(inner: &'a McpClient<T>, query: impl Into<String>) -> Result<Self, SdkError> {
        let query = query.into();
        validate_query(&query)?;
        Ok(Self {
            inner,
            query,
            sources: Vec::new(),
            depth: None,
            strand: None,
        })
    }

    /// Add a research source (cumulative — multiple calls extend the list).
    pub fn source(mut self, source: ResearchSource) -> Self {
        self.sources.push(source);
        self
    }

    /// Set the research depth. Defaults to SOUL server default when absent.
    pub fn depth(mut self, depth: DepthLevel) -> Self {
        self.depth = Some(depth);
        self
    }

    /// Restrict research to entries matching a specific strand dimension.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if `strand` is empty, exceeds 128 characters,
    /// or contains control characters.
    pub fn strand(mut self, strand: impl Into<String>) -> Result<Self, SdkError> {
        let strand: String = strand.into();
        if strand.is_empty() {
            return Err(SdkError::Config("strand must not be empty".to_owned()));
        }
        if strand.len() > 128 {
            return Err(SdkError::Config(
                "strand must not exceed 128 characters".to_owned(),
            ));
        }
        if strand.bytes().any(|b| b < 0x20)
            || strand
                .chars()
                .any(|c| c == '\x7F' || ('\u{0080}'..='\u{009F}').contains(&c))
        {
            return Err(SdkError::Config(
                "strand contains control character".to_owned(),
            ));
        }
        self.strand = Some(strand);
        Ok(self)
    }

    /// Execute the research query and return the [`ResearchResult`].
    ///
    /// Consumes `self` — build a new [`ResearchBuilder`] for each call.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SOUL rejects the request.
    pub async fn call(self) -> Result<ResearchResult, SdkError> {
        let mode = self.depth.map(DepthLevel::as_mode);

        let mut p = serde_json::json!({ "query": self.query });

        if let Some(mode) = mode {
            p["mode"] = mode.into();
        }
        if !self.sources.is_empty() {
            let source_strs: Vec<&str> = self.sources.iter().map(ResearchSource::as_str).collect();
            p["sources"] = serde_json::Value::Array(
                source_strs
                    .into_iter()
                    .map(|s| serde_json::Value::String(s.to_owned()))
                    .collect(),
            );
        }
        if let Some(ref strand) = self.strand {
            p["strand"] = strand.as_str().into();
        }

        let params = serde_json::json!({ "action": "soul_search", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }
}

// ── Query validation ──────────────────────────────────────────────────────────

/// Reject queries containing null bytes, ASCII control characters other than
/// tab (`\t`, 0x09) and newline (`\n`, 0x0A), U+007F (DEL), or C1 controls
/// (U+0080–U+009F).
fn validate_query(query: &str) -> Result<(), SdkError> {
    for byte in query.bytes() {
        if byte < 0x20 && byte != 0x09 && byte != 0x0A {
            return Err(SdkError::Config(format!(
                "research query must not contain ASCII control characters (found 0x{byte:02X})"
            )));
        }
    }
    // Check DEL (U+007F) and C1 controls (U+0080–U+009F) via char iteration
    // to avoid false-positive matches against UTF-8 continuation bytes.
    if query
        .chars()
        .any(|c| c == '\x7F' || ('\u{0080}'..='\u{009F}').contains(&c))
    {
        return Err(SdkError::Config(
            "research query contains invalid control character".to_owned(),
        ));
    }
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn rejects_null_byte_in_query() {
        let result = validate_query("valid\0invalid");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("0x00"));
    }

    #[test]
    fn rejects_control_character_in_query() {
        // 0x01 (SOH) should be rejected
        let result = validate_query("abc\x01def");
        assert!(result.is_err());
    }

    #[test]
    fn accepts_tab_in_query() {
        assert!(validate_query("query\twith\ttabs").is_ok());
    }

    #[test]
    fn accepts_newline_in_query() {
        assert!(validate_query("multi\nline\nquery").is_ok());
    }

    #[test]
    fn accepts_plain_query() {
        assert!(validate_query("LLM safety alignment").is_ok());
    }

    #[test]
    fn accepts_unicode_query() {
        assert!(validate_query("consciousness and idëntity — σφαίρα").is_ok());
    }

    #[test]
    fn research_source_strings() {
        assert_eq!(ResearchSource::Vault.as_str(), "vault");
        assert_eq!(ResearchSource::ArXiv.as_str(), "arxiv");
        assert_eq!(ResearchSource::HuggingFace.as_str(), "huggingface");
        assert_eq!(
            ResearchSource::Custom("custom-db".into()).as_str(),
            "custom-db"
        );
    }

    #[test]
    fn depth_level_modes() {
        assert_eq!(DepthLevel::Shallow.as_mode(), "search");
        assert_eq!(DepthLevel::Deep.as_mode(), "refresh");
    }
}
