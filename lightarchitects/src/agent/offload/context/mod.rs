//! `ContextResolver` — pluggable enrichers that materialise per-pattern,
//! per-sibling context for the offload prompt.
//!
//! # Day 3 scope
//!
//! - [`ContextResolver`] trait, [`ResolvedContext`] struct, [`ContextError`] enum.
//! - [`HelixQueryRunner`] — testability seam wrapping owner-scoped helix queries.
//! - [`HelixSource`] — first resolver implementation (5-minute TTL cache,
//!   200 ms per-call timeout).
//!
//! # Deferred
//!
//! - `CanonSource` + `IndustryBaselineSource` — Day 4.
//! - Multi-source dispatcher (`ContextResolverDispatch`) — Day 4.
//! - Production `HelixQueryRunner` impl on `Arc<HelixStore>` — Day 13 smoke.
//!   The wiring recipe lives in the module rustdoc of [`helix_source`].

use async_trait::async_trait;

use crate::helix::HelixDbError;
use crate::helix::types::Step;

use super::catalog::ContextSource;

mod canon_source;
mod helix_source;
mod industry_baseline_source;
mod section_slicer;

pub use canon_source::CanonSource;
pub use helix_source::HelixSource;
pub use industry_baseline_source::IndustryBaselineSource;
pub use section_slicer::{anchor_prefix, slice_by_anchor_prefix};

/// One source's contribution to an enriched offload prompt.
#[derive(Debug, Clone)]
pub struct ResolvedContext {
    /// Source kind: matches [`ContextSource::kind_str`] (`"helix"`, `"canon"`,
    /// `"industry-baseline"`, `"context7"`).
    pub kind: &'static str,
    /// Human-readable source identifier (e.g. `"owner=corso limit=3"` or
    /// `"canon:builders-cookbook#§63"`).
    pub identifier: String,
    /// Resolved content, truncated to the source's `token_budget`.
    pub content: String,
    /// Approximate token count (4-char/token heuristic).
    pub token_count_estimate: usize,
}

/// Errors raised by [`ContextResolver`] implementations.
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    /// Well-formed source but no data is available (e.g. canon anchor missing).
    #[error("source not found: {0}")]
    NotFound(String),
    /// Backend failure (helix query error, I/O error, parse error).
    #[error("backend error: {0}")]
    Backend(String),
    /// Per-call deadline fired before the source returned.
    #[error("per-call timeout")]
    Timeout,
    /// Resolver received a [`ContextSource`] variant outside its domain.
    #[error("wrong source kind for resolver {resolver:?}: got {got:?}")]
    KindMismatch {
        /// Resolver kind (e.g. `"helix"`).
        resolver: &'static str,
        /// Kind of the source actually received.
        got: &'static str,
    },
}

/// Per-source enricher. Each catalog [`ContextSource`] variant has exactly
/// one implementor.
#[async_trait]
pub trait ContextResolver: Send + Sync {
    /// Source kind this resolver handles — matches [`ContextSource::kind_str`].
    fn kind(&self) -> &'static str;

    /// Resolve the source into a piece of context ready for prompt assembly.
    ///
    /// # Errors
    ///
    /// - [`ContextError::KindMismatch`] if `source.kind_str() != self.kind()`.
    /// - [`ContextError::Timeout`] if the per-call deadline fires.
    /// - [`ContextError::Backend`] on any backend failure.
    /// - [`ContextError::NotFound`] if no data is available.
    async fn resolve(
        &self,
        source: &ContextSource,
        sibling: &str,
    ) -> Result<ResolvedContext, ContextError>;
}

/// Thin trait wrapping owner-scoped helix queries.
///
/// Decouples [`HelixSource`] from a live `HelixStore` so the resolver can be
/// unit-tested without spinning up Neo4j. Production wires
/// `Arc<HelixStore>` on Day 13 (recipe in [`helix_source`] module rustdoc).
#[async_trait]
pub trait HelixQueryRunner: Send + Sync {
    /// Fetch the most-recent `limit` steps owned by `owner`.
    ///
    /// # Errors
    ///
    /// Returns [`HelixDbError`] on any backend failure.
    async fn fetch_by_owner(&self, owner: &str, limit: u32) -> Result<Vec<Step>, HelixDbError>;
}
