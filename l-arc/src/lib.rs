//! Unified Light Architects SDK.
//!
//! `l-arc` is an umbrella crate that re-exports all sibling MCP clients under
//! a single dependency. Individual sibling clients are feature-gated so you
//! only pay (compile time, binary size) for what you use.
//!
//! # Feature flags
//!
//! | Feature | Enables |
//! |---------|---------|
//! | `full` | All sibling clients (SOUL, CORSO, EVA, QUANTUM, SERAPH) |
//! | `soul` | [`soul::SoulClient`] |
//! | `corso` | [`corso::CorsoClient`] |
//! | `eva` | [`eva::EvaClient`] |
//! | `quantum` | [`quantum::QuantumClient`] |
//! | `seraph` | [`seraph::SeraphClient`] |
//! | `ayin` | [`ayin::ObservableTransport`] |
//!
//! # Quick start
//!
//! ```toml
//! # All sibling clients
//! l-arc = { path = "...", features = ["full"] }
//!
//! # Only what you need
//! l-arc = { path = "...", features = ["soul", "quantum"] }
//! ```
//!
//! ```no_run
//! # #[cfg(all(feature = "soul", feature = "quantum"))]
//! # async fn example() -> Result<(), l_arc_core::SdkError> {
//! use l_arc::soul::SoulClient;
//! use l_arc::quantum::QuantumClient;
//!
//! let soul = SoulClient::builder().build().await?;
//! let quantum = QuantumClient::builder().build().await?;
//! # Ok(()) }
//! ```

// ── Core wire protocol — always available ─────────────────────────────────────

/// Core wire protocol, transport, and error types.
///
/// Re-exports the full `l-arc-core` public API. Use this module for
/// [`SdkError`][core::SdkError], [`StdioTransport`][core::StdioTransport],
/// and [`RetryConfig`][core::RetryConfig].
pub mod core {
    pub use l_arc_core::{McpClient, RetryConfig, SdkError, SiblingId, StdioTransport};
}

// ── SOUL ──────────────────────────────────────────────────────────────────────

/// SOUL knowledge-graph client (requires `soul` feature).
#[cfg(feature = "soul")]
pub mod soul {
    pub use l_arc_soul::{
        ConverseResult, HealthReport, HelixBuilder, HelixEntry, LinksResult, NoteContent,
        NoteEntry, NoteList, NoteWritten, QueryBuilder, QueryResult, RelateResult, SearchHit,
        SoulClient, SoulClientBuilder, SpeakResult, StatsReport, TagSyncReport, ValidateReport,
    };
}

// ── CORSO ─────────────────────────────────────────────────────────────────────

/// CORSO operations-platform client (requires `corso` feature).
#[cfg(feature = "corso")]
pub mod corso {
    pub use l_arc_corso::{
        ActionOutput, CodeSearchHit, ContainerOp, CorsoClient, CorsoClientBuilder, DirEntry,
        DirectoryListing, FileContent, FileOutline, FileWritten, OutlineEntry, ReferenceLocation,
        ReferenceResult, SecretOp, SymbolLocation, SymbolSearchResult,
    };
}

// ── EVA ───────────────────────────────────────────────────────────────────────

/// EVA consciousness-system client (requires `eva` feature).
#[cfg(feature = "eva")]
pub mod eva {
    pub use l_arc_eva::{
        ActionOutput, BibleAction, BuildMode, EvaClient, EvaClientBuilder, MemorySubcommand,
        ResearchSource, SecureAction, SkillLevel, TeachMode, VisualizeOutput,
    };
}

// ── QUANTUM ───────────────────────────────────────────────────────────────────

/// QUANTUM investigation-toolkit client (requires `quantum` feature).
#[cfg(feature = "quantum")]
pub mod quantum {
    pub use l_arc_quantum::{ActionOutput, QuantumClient, QuantumClientBuilder};
}

// ── SERAPH ────────────────────────────────────────────────────────────────────

/// SERAPH pentest-orchestration client (requires `seraph` feature).
#[cfg(feature = "seraph")]
pub mod seraph {
    pub use l_arc_seraph::{ActionOutput, SeraphClient, SeraphClientBuilder, Wing};
}

// ── AYIN ─────────────────────────────────────────────────────────────────────

/// AYIN observability transport wrapper (requires `ayin` feature).
#[cfg(feature = "ayin")]
pub mod ayin {
    pub use l_arc_ayin::ObservableTransport;
}
