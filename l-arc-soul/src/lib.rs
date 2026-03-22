//! Typed client for the SOUL knowledge-graph MCP server.
//!
//! SOUL exposes a single MCP tool — `soulTools` — with 23 actions. This crate
//! wraps each action in a strongly-typed Rust method and provides fluent
//! builders for the two most parameter-rich actions (`helix` and `query`).
//!
//! # Quick start
//!
//! ```no_run
//! use l_arc_soul::SoulClient;
//!
//! # async fn example() -> Result<(), l_arc_core::SdkError> {
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

mod client;
mod helix;
mod query;
mod types;

// ── Public API surface ────────────────────────────────────────────────────────

pub use client::{SoulClient, SoulClientBuilder};
pub use helix::{HelixBuilder, HelixEntry};
pub use query::{QueryBuilder, QueryResult};
pub use types::{
    ConverseResult, HealthReport, LinksResult, NoteContent, NoteEntry, NoteList, NoteWritten,
    RelateResult, SearchHit, SpeakResult, StatsReport, TagSyncReport, ValidateReport,
};
