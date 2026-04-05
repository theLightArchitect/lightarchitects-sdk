//! Typed client for the SOUL knowledge-graph MCP server.
//!
//! SOUL exposes a single MCP tool — `soulTools` — with 23 actions. This crate
//! wraps each action in a strongly-typed Rust method and provides fluent
//! builders for the two most parameter-rich actions (`helix` and `query`).
//!
//! # Quick start
//!
//! ```no_run
//! use lightarchitects_soul::SoulClient;
//!
//! # async fn example() -> Result<(), lightarchitects_core::SdkError> {
//! let client = SoulClient::builder().build().await?;
//!
//! // Fluent helix query
//! let entries = client
//!     .helix()
//!     .sibling("eva")
//!     .significance_min(7.0)
//!     .limit(10)
//!     .call()
//!     .await?;
//!
//! // Fluent hybrid-RAG query
//! let result = client
//!     .query("consciousness and identity")
//!     .strand("meaning")
//!     .top_k(5)
//!     .call()
//!     .await?;
//! println!("{}", result.context);
//! # Ok(()) }
//! ```

/// Canonical SOUL action enum — vault operations, queries, voice, research.
pub mod actions;
mod client;
mod helix;
pub mod ingest;
mod query;
pub mod research;
mod types;

// ── Public API surface ────────────────────────────────────────────────────────

pub use actions::SoulAction;
pub use client::{SoulClient, SoulClientBuilder};
pub use helix::{HelixBuilder, HelixEntry};
pub use ingest::{ContentType, IngestBuilder};
pub use query::{QueryBuilder, QueryResult};
pub use research::{DepthLevel, ResearchBuilder, ResearchSource};
pub use types::{
    ChatMessage, ChatResult, ConvergenceEntry, ConvergenceResult, ConverseResult, FrontmatterMatch,
    HealthReport, IngestReport, IngestResult, LinksResult, ManifestContent, NoteContent, NoteEntry,
    NoteList, NoteWritten, QueryFrontmatterResult, QueryHit, RawQueryResult, RelateResult,
    ResearchResult, ScriptTurn, SearchHit, SiblingPrompt, SpeakResult, StatsReport, TagSyncReport,
    ValidateReport, VoiceAudioFile, VoiceProfileEntry, VoiceResult,
};
