//! Light Architects server-side modules.
//!
//! This crate is a focused re-export facade that exposes the server-facing subset of the
//! [`lightarchitects`] unified SDK. It exists so downstream MCP servers and gateway
//! binaries can depend on only the heavy server modules (auth, arena, helix, turnlog,
//! oracle) without enabling the full client SDK surface.
//!
//! # Feature flags
//!
//! | Feature      | Enables                                                      |
//! |--------------|--------------------------------------------------------------|
//! | `auth`       | API key authentication (default on, no heavy deps)           |
//! | `arena`      | Training data factory (default on)                           |
//! | `helix`      | Neo4j graph backend — `HelixStore`, 5 primitives, 4-signal RRF |
//! | `turnlog`    | Ephemeral transactional log with HMAC chaining (default on)  |
//! | `oracle`     | Multi-model mathematical verification oracle (default on)    |
//! | `fastembed`  | ONNX-backed local embeddings (off by default — 100 MB cache) |
//!
//! # Quick start
//!
//! ```toml
//! [dependencies]
//! lightarchitects-server = { version = "0.1", default-features = false, features = ["auth"] }
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! use lightarchitects_server::auth::{AuthConfig, KeyReader, KeyValidator};
//! ```

// ── Auth ─────────────────────────────────────────────────────────────────────

/// API key authentication — 3-tier degradation (`NoKey` / `GracePeriod` / `Valid`).
///
/// Re-exports [`lightarchitects::auth`] unchanged. Provides [`AuthConfig`],
/// [`KeyReader`], [`KeyValidator`], [`RevocationWatcher`], and [`AuthGuard`].
#[cfg(feature = "auth")]
pub mod auth {
    pub use lightarchitects::auth::*;
}

// ── Arena ────────────────────────────────────────────────────────────────────

/// MCP training data factory — discover → generate → execute → score → export.
///
/// Re-exports [`lightarchitects::research_arena`] unchanged. Provides arena configuration,
/// MCP server auto-discovery, exercise generation, execution engine, reward scoring,
/// and SFT/DPO/RL export formats.
#[cfg(feature = "arena")]
pub mod research_arena {
    pub use lightarchitects::research_arena::*;
}

// ── Helix ─────────────────────────────────────────────────────────────────────

/// Neo4j graph backend — `HelixStore`, 5 primitives, 4-signal RRF retrieval.
///
/// Re-exports [`lightarchitects::helix`] unchanged. Provides [`HelixStore`],
/// the 5 helix primitives (`Helix`, `Step`, `Strand`, `HelixLink`, `SharedExperience`),
/// hybrid retrieval (BM25 + semantic + structural + graph traversal), ingestion pipeline,
/// and embedding providers (Ollama, fastembed behind the `fastembed` feature).
///
/// Requires the `helix` feature (enables Neo4j + `SQLite` + search deps).
/// The `fastembed` feature additionally enables the ONNX-backed embedding provider.
#[cfg(feature = "helix")]
pub mod helix {
    pub use lightarchitects::helix::*;
}

// ── Turnlog ───────────────────────────────────────────────────────────────────

/// Tier-1 ephemeral transactional log with HMAC chaining and helix promotion.
///
/// Re-exports [`lightarchitects::turnlog`] unchanged. Provides [`TurnLogStore`],
/// HMAC-chained [`TurnLogEntry`] types, chain verification, and promotion to the
/// helix graph backend.
#[cfg(feature = "turnlog")]
pub mod turnlog {
    pub use lightarchitects::turnlog::*;
}

// ── Oracle ────────────────────────────────────────────────────────────────────

/// Multi-model mathematical verification oracle (Lean 4 + `DeepSeek` + Qwen + Kimi).
///
/// Re-exports [`lightarchitects::oracle`] unchanged. Provides [`OracleClient`],
/// the [`OracleVerdict`] type, and multi-model consensus verification for formal
/// proofs and numerical correctness.
#[cfg(feature = "oracle")]
pub mod oracle {
    pub use lightarchitects::oracle::*;
}
