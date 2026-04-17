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
//! | `full` | All published sibling clients (SOUL, CORSO, EVA, QUANTUM) |
//! | `soul` | [`soul::SoulClient`] |
//! | `corso` | [`corso::CorsoClient`] |
//! | `eva` | [`eva::EvaClient`] |
//! | `quantum` | [`quantum::QuantumClient`] |
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
/// [`RetryConfig`][core::RetryConfig], and the [`AuthProvider`][core::AuthProvider] trait.
pub mod core {
    pub use lightarchitects_core::{
        // Auth types — always available (trait + type-erased wrapper)
        AuthChecker,
        AuthProvider,
        AuthStatus,
        McpClient,
        RetryConfig,
        SdkError,
        SiblingId,
        StdioTransport,
    };
}

// ── Auth guard — opt-in (`auth` feature) ─────────────────────────────────────

/// Concrete auth guard — 3-tier key validation with grace-period degradation.
///
/// Requires the `auth` feature. Provides [`AuthGuard`][auth::AuthGuard], which
/// implements [`core::AuthProvider`] and can be passed to `.auth()` on any
/// sibling client builder.
///
/// # Example
///
/// ```no_run
/// # #[cfg(all(feature = "auth", feature = "soul"))]
/// # async fn example() -> Result<(), lightarchitects_core::SdkError> {
/// use lightarchitects::auth::AuthGuard;
/// use lightarchitects::soul::SoulClient;
///
/// let guard = AuthGuard::new(Default::default());
/// let client = SoulClient::builder().auth(guard).build().await?;
/// # Ok(()) }
/// ```
#[cfg(feature = "auth")]
pub mod auth {
    pub use lightarchitects_auth::{AuthConfig, AuthGuard, AuthTier};
}

// ── SOUL ──────────────────────────────────────────────────────────────────────

/// SOUL knowledge-graph client (requires `soul` feature).
#[cfg(feature = "soul")]
pub mod soul {
    // ── Client entry point ────────────────────────────────────────────────────
    pub use lightarchitects_soul::{SoulAction, SoulClient, SoulClientBuilder};

    // ── Fluent query builders ─────────────────────────────────────────────────
    pub use lightarchitects_soul::{
        GraphRagIngestBuilder, HelixBuilder, IngestBuilder, QueryBuilder, ResearchBuilder,
    };

    // ── Builder parameter types ───────────────────────────────────────────────
    pub use lightarchitects_soul::{
        ContentType, DepthLevel, IngestSource, ResearchSource, TextFormat,
    };

    // ── Query / retrieval result types ────────────────────────────────────────
    pub use lightarchitects_soul::{
        ConvergenceEntry, ConvergenceResult, FrontmatterMatch, HelixEntry, IngestReport,
        IngestResult, LinksResult, QueryFrontmatterResult, QueryHit, QueryResult, RawQueryResult,
        RelateResult, ResearchResult, SearchHit, StatsReport,
    };

    // ── Note / vault result types ─────────────────────────────────────────────
    pub use lightarchitects_soul::{
        ManifestContent, NoteContent, NoteEntry, NoteList, NoteWritten, TagSyncReport,
        ValidateReport,
    };

    // ── Voice / conversation result types ─────────────────────────────────────
    pub use lightarchitects_soul::{
        ChatMessage, ChatResult, ConverseResult, GraphRagIngestResult, HealthReport, ScriptTurn,
        SiblingPrompt, SpeakResult, VoiceAudioFile, VoiceProfileEntry, VoiceResult,
    };
}

// ── Helix — Neo4j graph backend ───────────────────────────────────────────────

/// Neo4j graph backend — [`HelixStore`][helix::HelixStore] and the 5 helix
/// primitives (requires `helix` feature).
///
/// # Quick start
///
/// ```no_run
/// # #[cfg(feature = "helix")]
/// # async fn example() -> Result<(), lightarchitects_helix::HelixStoreError> {
/// use lightarchitects::helix::HelixStore;
///
/// let store = HelixStore::connect("bolt://localhost:7687", "neo4j", "password").await?;
/// let hits = store.search("consciousness").top(10).call().await?;
/// # Ok(()) }
/// ```
#[cfg(feature = "helix")]
pub mod helix {
    // ── Ergonomic entry point ─────────────────────────────────────────────────
    pub use lightarchitects_helix::{HelixSearchBuilder, HelixStore, HelixStoreError};

    // ── Offline entry point (re-exported here for convenience) ───────────────
    pub use lightarchitects_soul::{SearchBuilder, SoulDb, SoulError};

    // ── 5 helix primitives ────────────────────────────────────────────────────
    pub use lightarchitects_helix::{
        Helix, HelixLink, HelixOrderingMode, SharedExperience, Step, Strand,
    };

    // ── Neo4j database trait + implementation ─────────────────────────────────
    pub use lightarchitects_helix::{HelixDb, HelixDbError, HelixNeo4j, Neo4jConfig};

    // ── Search + retrieval types ──────────────────────────────────────────────
    pub use lightarchitects_helix::{ScoredResult, SearchOptions};
    pub use lightarchitects_soul::{RetrievalHit, RetrievalSignal};
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
        ActionOutput, EvaClient, EvaClientBuilder, SkillLevel, TeachMode, VisualizeOutput,
    };
}

// ── QUANTUM ───────────────────────────────────────────────────────────────────

/// QUANTUM investigation-toolkit client (requires `quantum` feature).
#[cfg(feature = "quantum")]
pub mod quantum {
    pub use lightarchitects_quantum::{
        ActionOutput, InvestigationPhase, QuantumClient, QuantumClientBuilder, QuantumInvestigation,
    };
}

// ── AYIN ─────────────────────────────────────────────────────────────────────

/// AYIN observability transport wrapper (requires `ayin` feature).
///
/// The `ayin` feature provides `ObservableTransport` for instrumenting MCP calls.
/// The `ayin-http` feature additionally provides `AyinClient` for querying the
/// AYIN viewer REST API at `localhost:3742`.
#[cfg(feature = "ayin")]
pub mod ayin {
    pub use lightarchitects_ayin::ObservableTransport;

    /// HTTP client for the AYIN viewer REST API (requires `ayin-http` feature).
    ///
    /// Queries `GET /api/sessions` and `GET /api/spans/:actor/:date` on the AYIN
    /// viewer running at `localhost:3742`.
    #[cfg(feature = "ayin-http")]
    pub use lightarchitects_ayin::{AyinClient, SessionEntry, SessionList, SpanList, SpanRecord};
}
