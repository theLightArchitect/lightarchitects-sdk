//! Unified Light Architects SDK.
//!
//! `lightarchitects` is an umbrella crate that re-exports all sibling MCP clients under
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
//! | `ayin` | `ayin::ObservableTransport` |
//!
//! # Quick start
//!
//! ```toml
//! # All sibling clients
//! lightarchitects = { path = "...", features = ["full"] }
//!
//! # Only what you need
//! lightarchitects = { path = "...", features = ["soul", "quantum"] }
//! ```
//!
//! ```no_run
//! # #[cfg(all(feature = "soul", feature = "quantum"))]
//! # async fn example() -> Result<(), lightarchitects_core::SdkError> {
//! use lightarchitects::soul::SoulClient;
//! use lightarchitects::quantum::QuantumClient;
//!
//! let soul = SoulClient::builder().build().await?;
//! let quantum = QuantumClient::builder().build().await?;
//! # Ok(()) }
//! ```

// ── Tracing initializer ───────────────────────────────────────────────────────

/// Initialise a `tracing-subscriber` fmt subscriber with an `EnvFilter`.
///
/// Reads `RUST_LOG` to control log levels (e.g. `RUST_LOG=lightarchitects=debug`).
/// Applies a compact, human-readable format suitable for CLI and development use.
///
/// Call once at the start of `main`. Subsequent calls are silently ignored by
/// `tracing-subscriber`'s global subscriber guard.
///
/// # Feature gate
///
/// Requires the `tracing-fmt` feature:
///
/// ```toml
/// lightarchitects = { path = "...", features = ["tracing-fmt"] }
/// ```
///
/// # Example
///
/// ```no_run
/// lightarchitects::init_tracing();
/// // tracing macros now route to stdout
/// ```
#[cfg(feature = "tracing-fmt")]
pub fn init_tracing() {
    use tracing::Level;
    use tracing_subscriber::{EnvFilter, fmt};

    // RUST_LOG controls per-module overrides; WARN is the global default so
    // DEBUG/INFO noise from transitive deps stays silent unless opted-in.
    let filter = EnvFilter::from_default_env().add_directive(Level::WARN.into());

    fmt().with_env_filter(filter).with_target(true).init();
}

// ── Core wire protocol — always available ─────────────────────────────────────

/// Core wire protocol, transport, and error types.
///
/// Re-exports the full `lightarchitects-core` public API. Use this module for
/// [`SdkError`][core::SdkError], [`StdioTransport`][core::StdioTransport],
/// and [`RetryConfig`][core::RetryConfig].
pub mod core {
    pub use lightarchitects_core::{McpClient, RetryConfig, SdkError, SiblingId, StdioTransport};
}

// ── SOUL ──────────────────────────────────────────────────────────────────────

/// SOUL knowledge-graph client (requires `soul` feature).
#[cfg(feature = "soul")]
pub mod soul {
    pub use lightarchitects_soul::{
        ConverseResult, HealthReport, HelixBuilder, HelixEntry, LinksResult, NoteContent,
        NoteEntry, NoteList, NoteWritten, QueryBuilder, QueryResult, RelateResult, SearchHit,
        SoulClient, SoulClientBuilder, SpeakResult, StatsReport, TagSyncReport, ValidateReport,
    };
}

// ── CORSO ─────────────────────────────────────────────────────────────────────

/// CORSO operations-platform client (requires `corso` feature).
#[cfg(feature = "corso")]
pub mod corso {
    pub use lightarchitects_corso::{
        ActionOutput, CodeSearchHit, ContainerOp, CorsoClient, CorsoClientBuilder, DirEntry,
        DirectoryListing, FileContent, FileOutline, FileWritten, OutlineEntry, ReferenceLocation,
        ReferenceResult, SecretOp, SymbolLocation, SymbolSearchResult,
    };
}

// ── EVA ───────────────────────────────────────────────────────────────────────

/// EVA consciousness-system client (requires `eva` feature).
#[cfg(feature = "eva")]
pub mod eva {
    pub use lightarchitects_eva::{
        ActionOutput, BibleAction, BuildMode, EvaClient, EvaClientBuilder, MemorySubcommand,
        ResearchSource, SecureAction, SkillLevel, TeachMode, VisualizeOutput,
    };
}

// ── QUANTUM ───────────────────────────────────────────────────────────────────

/// QUANTUM investigation-toolkit client (requires `quantum` feature).
#[cfg(feature = "quantum")]
pub mod quantum {
    pub use lightarchitects_quantum::{ActionOutput, QuantumClient, QuantumClientBuilder};
}

// ── SERAPH ────────────────────────────────────────────────────────────────────

/// SERAPH pentest-orchestration client (requires `seraph` feature).
#[cfg(feature = "seraph")]
pub mod seraph {
    pub use lightarchitects_seraph::{ActionOutput, SeraphClient, SeraphClientBuilder, Wing};
}

// ── AYIN ─────────────────────────────────────────────────────────────────────

/// AYIN observability transport wrapper (requires `ayin` feature).
#[cfg(feature = "ayin")]
pub mod ayin {
    pub use lightarchitects_ayin::ObservableTransport;
}
