//! Fluent builder for the `evaTools` `ideate` action.
//!
//! Create a builder via [`EvaClient::ideate_builder`][crate::EvaClient::ideate_builder]
//! then chain optional filter methods before calling `.call().await`.
//!
//! ```no_run
//! # async fn example(
//! #     client: lightarchitects_eva::EvaClient<lightarchitects_core::StdioTransport>,
//! # ) -> Result<(), lightarchitects_core::SdkError> {
//! use lightarchitects_eva::IdeatePhase;
//!
//! let result = client
//!     .ideate_builder("design a plugin system")
//!     .phase(IdeatePhase::Discover)
//!     .context("Must support hot-reload and sandboxed execution")
//!     .session_id("sess-abc123")
//!     .call()
//!     .await?;
//!
//! println!("{}", result.phase_1_discovery);
//! # Ok(()) }
//! ```

use std::borrow::Cow;

use lightarchitects_core::McpClient;
use lightarchitects_core::error::SdkError;
use lightarchitects_core::transport::Transport;

use crate::content::unwrap_json;
use crate::types::IdeateResult;

/// The 6 phases of EVA's ideation workflow.
///
/// Pass to [`IdeateBuilder::phase`] to hint which phase to emphasise.
/// EVA always runs all 6 phases; this field is advisory context for the model.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdeatePhase {
    /// Phase 1 — Deep understanding of the problem space.
    Discover,
    /// Phase 2 — Requirements and constraints breakdown.
    Analyse,
    /// Phase 3 — Generating 3–5 creative approaches.
    Ideate,
    /// Phase 4 — Selecting the simplest, most robust approach.
    Refine,
    /// Phase 5 — Actionable step-by-step implementation plan.
    Document,
    /// Phase 6 — EVA's enthusiastic celebration of the completed plan.
    Celebrate,
}

impl IdeatePhase {
    /// Serialize to the string sent in the MCP `phase` hint field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Discover => "discover",
            Self::Analyse => "analyse",
            Self::Ideate => "ideate",
            Self::Refine => "refine",
            Self::Document => "document",
            Self::Celebrate => "celebrate",
        }
    }
}

/// Requested output format hint for the `ideate` action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputFormat {
    /// Flowing prose narrative (default).
    Prose,
    /// Structured headings with numbered lists.
    Structured,
    /// Voice-optimised output for TTS playback.
    ///
    /// # Note
    ///
    /// `Voice` does **not** trigger TTS synthesis in the SDK. It is an advisory
    /// hint sent to EVA that requests TTS-friendly phrasing (shorter sentences,
    /// no markdown symbols). Audio synthesis is out of scope for this crate.
    Voice,
}

impl OutputFormat {
    /// Serialize to the string sent in the MCP `output_format` field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prose => "prose",
            Self::Structured => "structured",
            Self::Voice => "voice",
        }
    }
}

// ── IdeateBuilder ─────────────────────────────────────────────────────────────

/// Fluent builder for the `evaTools` `ideate` action.
///
/// Constructed via [`EvaClient::ideate_builder`][crate::EvaClient::ideate_builder].
/// All setter methods consume and return `self` for chaining.
/// The builder allocates nothing until [`.call()`][IdeateBuilder::call] is invoked.
///
/// # Example
///
/// ```no_run
/// # async fn example(
/// #     client: lightarchitects_eva::EvaClient<lightarchitects_core::StdioTransport>,
/// # ) -> Result<(), lightarchitects_core::SdkError> {
/// use lightarchitects_eva::{IdeatePhase, OutputFormat};
///
/// let result = client
///     .ideate_builder("build a search index")
///     .phase(IdeatePhase::Document)
///     .context("Rust, no runtime alloc in hot path")
///     .output_format(OutputFormat::Structured)
///     .session_id("sess-xyz789")
///     .call()
///     .await?;
///
/// println!("Implementation plan:\n{}", result.phase_5_documentation);
/// # Ok(()) }
/// ```
#[must_use]
pub struct IdeateBuilder<'a, T: Transport> {
    inner: &'a McpClient<T>,
    goal: String,
    phase: Option<IdeatePhase>,
    context: Option<String>,
    output_format: Option<OutputFormat>,
    session_id: Option<String>,
}

impl<'a, T: Transport> IdeateBuilder<'a, T> {
    /// Create a builder for the given `goal`.
    ///
    /// Prefer constructing via [`EvaClient::ideate_builder`][crate::EvaClient::ideate_builder].
    pub(crate) fn new(inner: &'a McpClient<T>, goal: String) -> Self {
        Self {
            inner,
            goal,
            phase: None,
            context: None,
            output_format: None,
            session_id: None,
        }
    }

    /// Hint which phase to emphasise in EVA's response.
    ///
    /// EVA always executes all 6 phases; this is an advisory hint only.
    pub fn phase(mut self, phase: IdeatePhase) -> Self {
        self.phase = Some(phase);
        self
    }

    /// Provide additional background context that shapes the ideation.
    ///
    /// # Security
    ///
    /// This string is transmitted verbatim to the EVA MCP server.
    /// **Do not include secrets, credentials, API keys, or PII.**
    pub fn context<'ctx>(mut self, ctx: impl Into<Cow<'ctx, str>>) -> Self {
        self.context = Some(ctx.into().into_owned());
        self
    }

    /// Request a specific output format from EVA.
    ///
    /// See [`OutputFormat`] for available options and their semantics.
    pub fn output_format(mut self, fmt: OutputFormat) -> Self {
        self.output_format = Some(fmt);
        self
    }

    /// Attach a session identifier for tracing and correlation.
    ///
    /// # Panics
    ///
    /// Panics if `id` contains characters other than ASCII alphanumerics or
    /// hyphens.  Valid examples: `"sess-abc123"`, `"user-42-req-7"`.
    pub fn session_id(mut self, id: impl Into<String>) -> Self {
        let id: String = id.into();
        assert!(
            id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "session_id must contain only ASCII alphanumerics and hyphens, got: {id:?}"
        );
        self.session_id = Some(id);
        self
    }

    /// Execute the `ideate` action and return the structured result.
    ///
    /// Consumes the builder.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails, EVA returns `isError: true`,
    /// or the response cannot be deserialized into [`IdeateResult`].
    pub async fn call(self) -> Result<IdeateResult, SdkError> {
        let mut p = serde_json::json!({ "goal": self.goal });

        if let Some(ctx) = self.context {
            p["context"] = serde_json::Value::String(ctx);
        }
        if let Some(phase) = self.phase {
            p["phase"] = serde_json::Value::String(phase.as_str().to_owned());
        }
        if let Some(fmt) = self.output_format {
            p["output_format"] = serde_json::Value::String(fmt.as_str().to_owned());
        }
        if let Some(sid) = self.session_id {
            p["session_id"] = serde_json::Value::String(sid);
        }

        let wrapped = serde_json::json!({ "action": "ideate", "params": p });
        let raw = self.inner.call_tool("evaTools", wrapped).await?;
        let json = unwrap_json(raw, "ideate")?;
        serde_json::from_value(json).map_err(SdkError::from)
    }
}
