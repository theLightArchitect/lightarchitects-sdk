//! Fluent builder for the `soulTools` `ingest` action.
//!
//! Create a builder via [`crate::SoulClient::ingest_builder`], configure it,
//! then call `.call().await` to execute.
//!
//! # Example
//!
//! ```no_run
//! # async fn example(client: lightarchitects_soul::SoulClient<lightarchitects_core::StdioTransport>)
//! # -> Result<(), lightarchitects_core::SdkError> {
//! use lightarchitects_soul::ingest::ContentType;
//!
//! let result = client
//!     .ingest_builder("~/.soul/helix/eva/entries/my-entry.md")?
//!     .content_type(ContentType::MarkdownNote)
//!     .sibling("eva")
//!     .call()
//!     .await?;
//!
//! println!("ingested {} records", result.report.records_added);
//! # Ok(()) }
//! ```

use lightarchitects_core::transport::Transport;
use lightarchitects_core::{McpClient, SdkError};

use crate::types::IngestResult;

// ── Vault root (compile-time constant) ────────────────────────────────────────

/// Relative vault root suffix appended to `$HOME` for path validation.
const VAULT_ROOT_SUFFIX: &str = "/.soul/";

// ── ContentType ───────────────────────────────────────────────────────────────

/// Content type variants understood by the SOUL ingestion pipeline.
///
/// Controls which ingestor the pipeline uses for the target path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    /// Markdown vault entry (`.md` file or vault directory).
    MarkdownNote,
    /// A CORSO build plan (`plan.md`).
    HelixEntry,
    /// A chat/conversation transcript (`chat-YYYY-MM-DD.md`).
    Conversation,
}

impl ContentType {
    /// Returns the `source_type` string the SOUL MCP server expects.
    #[must_use]
    fn as_source_type(self) -> &'static str {
        match self {
            Self::MarkdownNote => "markdown_vault",
            Self::HelixEntry => "plan",
            Self::Conversation => "chat_transcript",
        }
    }
}

// ── IngestBuilder ─────────────────────────────────────────────────────────────

/// Fluent builder for the `soulTools` `ingest` action.
///
/// Construction is allocation-free until `.call()`. Use [`IngestBuilder::with_path`]
/// to construct a builder; the path is validated during construction.
///
/// # Path security
///
/// [`IngestBuilder::with_path`] rejects any path that does not resolve to a
/// location within `~/.soul/` after tilde expansion. This prevents callers from
/// accidentally (or deliberately) ingesting content outside the vault root.
///
/// # Example
///
/// ```no_run
/// # async fn example(client: lightarchitects_soul::SoulClient<lightarchitects_core::StdioTransport>)
/// # -> Result<(), lightarchitects_core::SdkError> {
/// use lightarchitects_soul::ingest::ContentType;
///
/// let result = client
///     .ingest_builder("~/.soul/helix/corso/entries/plan.md")?
///     .content_type(ContentType::HelixEntry)
///     .sibling("corso")
///     .call()
///     .await?;
///
/// assert!(result.report.records_added > 0);
/// # Ok(()) }
/// ```
#[must_use]
pub struct IngestBuilder<'a, T: Transport> {
    inner: &'a McpClient<T>,
    path: String,
    content_type: Option<ContentType>,
    sibling: Option<String>,
    dry_run: bool,
}

impl<'a, T: Transport> IngestBuilder<'a, T> {
    /// Create a builder with the given (validated) ingestion path.
    ///
    /// Tilde (`~`) is expanded to the value of `$HOME`. The expanded path must
    /// start with `$HOME/.soul/` — any other prefix is rejected.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if `$HOME` is unset, the path contains null
    /// bytes, or the path falls outside the vault root.
    pub fn with_path(inner: &'a McpClient<T>, path: &str) -> Result<Self, SdkError> {
        let home = std::env::var("HOME").map_err(|_| {
            SdkError::Config("$HOME is not set — cannot validate vault path".to_owned())
        })?;
        let validated = validate_vault_path(path, &home)?;
        Ok(Self {
            inner,
            path: validated,
            content_type: None,
            sibling: None,
            dry_run: false,
        })
    }

    /// Override the content type (auto-detected from the file extension when absent).
    pub fn content_type(mut self, ct: ContentType) -> Self {
        self.content_type = Some(ct);
        self
    }

    /// Override the sibling owner for the ingested content.
    ///
    /// When absent, SOUL derives the owner from the path.
    pub fn sibling(mut self, sibling: impl Into<String>) -> Self {
        self.sibling = Some(sibling.into());
        self
    }

    /// Enable dry-run mode: validate without writing to the database.
    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    /// Execute the ingestion and return the [`IngestResult`].
    ///
    /// Consumes `self` — build a new [`IngestBuilder`] for each call.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or SOUL rejects the request.
    pub async fn call(self) -> Result<IngestResult, SdkError> {
        let mut p = serde_json::json!({ "path": self.path });

        if let Some(ct) = self.content_type {
            p["source_type"] = ct.as_source_type().into();
        }
        if let Some(ref sib) = self.sibling {
            p["sibling"] = sib.as_str().into();
        }
        if self.dry_run {
            p["dry_run"] = true.into();
        }

        let params = serde_json::json!({ "action": "ingest", "params": p });
        let raw = self.inner.call_tool("soulTools", params).await?;
        serde_json::from_value(raw).map_err(SdkError::from)
    }
}

// ── Path validation ───────────────────────────────────────────────────────────

/// Expand a tilde prefix and verify the path is within `~/.soul/`.
///
/// Takes an explicit `home` argument so callers (including tests) can inject
/// any home directory without requiring environment mutation.
///
/// Rejects paths containing null bytes or path traversal components (`..`).
fn validate_vault_path(path: &str, home: &str) -> Result<String, SdkError> {
    if path.contains('\0') {
        return Err(SdkError::Config(
            "path must not contain null bytes".to_owned(),
        ));
    }

    // Reject any path containing a traversal component to prevent escaping
    // the vault root even if the prefix check would otherwise pass.
    if path.split('/').any(|c| c == "..") {
        return Err(SdkError::Config(
            "ingest path must not contain '..' traversal components".to_owned(),
        ));
    }

    let expanded = if let Some(rest) = path.strip_prefix("~/") {
        format!("{home}/{rest}")
    } else if path == "~" {
        home.to_owned()
    } else {
        path.to_owned()
    };

    let vault_root = format!("{home}{VAULT_ROOT_SUFFIX}");
    if !expanded.starts_with(&vault_root) {
        return Err(SdkError::Config(format!(
            "ingest path must be within ~/.soul/ (got: {expanded})"
        )));
    }

    Ok(expanded)
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/home/user";

    #[test]
    fn rejects_null_byte() {
        let result = validate_vault_path("/home/user/.soul/entry\0bad", HOME);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("null bytes"));
    }

    #[test]
    fn rejects_path_outside_vault() {
        let result = validate_vault_path("/home/user/Projects/evil.md", HOME);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("~/.soul/"), "message was: {msg}");
    }

    #[test]
    fn rejects_traversal_attempt() {
        // Path starts correctly but escapes the vault via `..` traversal.
        // The explicit `..` component check fires before the prefix check.
        let result = validate_vault_path("/home/user/.soul/../Projects/evil.md", HOME);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("traversal"), "message was: {msg}");
    }

    #[test]
    fn accepts_tilde_path() {
        let result = validate_vault_path("~/.soul/helix/eva/entry.md", HOME);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(result.unwrap(), "/home/user/.soul/helix/eva/entry.md");
    }

    #[test]
    fn accepts_absolute_vault_path() {
        let result = validate_vault_path("/home/user/.soul/helix/corso/plan.md", HOME);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn content_type_source_strings() {
        assert_eq!(ContentType::MarkdownNote.as_source_type(), "markdown_vault");
        assert_eq!(ContentType::HelixEntry.as_source_type(), "plan");
        assert_eq!(
            ContentType::Conversation.as_source_type(),
            "chat_transcript"
        );
    }
}
