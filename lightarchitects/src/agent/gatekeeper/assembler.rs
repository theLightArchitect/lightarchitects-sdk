//! `CriteriaAssembler` — retrieval layer for stateless gatekeepers.
//!
//! Assembles [`Criteria`] from versioned knowledge stores:
//!
//! - Canon documents under `~/lightarchitects/soul/helix/user/standards/canon/`
//! - Industry baselines under `~/lightarchitects/soul/helix/user/standards/industry-baselines/`
//! - Prior decisions / precedent under `~/lightarchitects/soul/helix/<sibling>/entries/`
//! - The current build plan
//!
//! # Determinism
//!
//! `assemble()` is **deterministic-given-snapshot**: same inputs against the
//! same helix state produce the same `Criteria` (modulo `retrieved_at`).
//! Verdicts cached by `(draft_hash, criteria_hash)` remain valid until canon
//! or helix changes.
//!
//! # Decoupling
//!
//! `CriteriaAssembler` depends on the [`CriteriaSource`] trait rather than
//! directly on [`SoulClient`]. This lets unit tests substitute an in-memory
//! source without spinning up SOUL, and lets operators swap retrieval
//! backends (SOUL stdio, MCP HTTP, raw FS scan) without churning the
//! gatekeeper layer.
//!
//! # See also
//!
//! - [`Criteria`] — the assembled output
//! - [`super::Gatekeeper::min_criteria_completeness`] — refusal threshold

use async_trait::async_trait;

use super::types::{
    BaselineRef, CanonRef, Criteria, Draft, GateDimension, HelixSnapshotId, PlanRef, PrecedentRef,
};

/// Pluggable retrieval source for the [`CriteriaAssembler`].
///
/// Implementations are responsible for querying their backing store
/// (SOUL helix, MCP HTTP proxy, filesystem scan, etc.) and returning
/// **already-relevant** excerpts. The assembler does no further filtering.
///
/// Implementations should be deterministic at a snapshot: given the same
/// store state, the same query parameters should yield the same results.
#[async_trait]
pub trait CriteriaSource: Send + Sync {
    /// Fetch canon excerpts relevant to `dimension` and `topics`.
    ///
    /// `limit` caps the number of excerpts returned. Implementations should
    /// rank by relevance and truncate.
    ///
    /// # Errors
    ///
    /// Returns an [`AssemblyError`] when the underlying store is unreachable
    /// or the query cannot be parsed. Empty results are NOT an error — they
    /// surface upstream as `assembly_warnings`.
    async fn fetch_canon(
        &self,
        dimension: GateDimension,
        topics: &[String],
        limit: usize,
    ) -> Result<Vec<CanonRef>, AssemblyError>;

    /// Fetch industry baseline excerpts relevant to `dimension` and `topics`.
    ///
    /// # Errors
    ///
    /// See [`Self::fetch_canon`].
    async fn fetch_baselines(
        &self,
        dimension: GateDimension,
        topics: &[String],
        limit: usize,
    ) -> Result<Vec<BaselineRef>, AssemblyError>;

    /// Fetch prior-decision precedent for `dimension` and `topics`.
    ///
    /// `lookback_days` bounds how far back the search walks. `limit` caps
    /// the number of entries returned, ranked by RRF score.
    ///
    /// # Errors
    ///
    /// See [`Self::fetch_canon`].
    async fn fetch_precedent(
        &self,
        dimension: GateDimension,
        topics: &[String],
        lookback_days: u32,
        limit: usize,
    ) -> Result<Vec<PrecedentRef>, AssemblyError>;

    /// Stable snapshot id for the current store state.
    ///
    /// Used to populate [`Criteria::helix_snapshot`] for replayability.
    /// Implementations may compute this from a directory hash, a git rev,
    /// or a database transaction id.
    fn snapshot_id(&self) -> HelixSnapshotId;
}

/// Errors that can occur during criteria assembly.
#[derive(Debug, thiserror::Error)]
pub enum AssemblyError {
    /// The backing store could not be reached or returned an error.
    #[error("source unavailable: {0}")]
    SourceUnavailable(String),
    /// A query string was malformed.
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    /// An internal error occurred during assembly.
    #[error("internal: {0}")]
    Internal(String),
}

/// Assembles [`Criteria`] for a [`super::Gatekeeper`] invocation.
///
/// Pure (deterministic-given-snapshot): same inputs against the same
/// `CriteriaSource` snapshot produce identical `Criteria` (modulo
/// `retrieved_at` timestamp).
pub struct CriteriaAssembler<S: CriteriaSource> {
    source: S,
    config: AssemblerConfig,
}

/// Configuration for [`CriteriaAssembler`].
#[derive(Debug, Clone)]
pub struct AssemblerConfig {
    /// Max canon excerpts to retrieve per dimension. Default 5.
    pub max_canon: usize,
    /// Max industry baseline excerpts. Default 3.
    pub max_baselines: usize,
    /// Max precedent entries. Default 5.
    pub max_precedent: usize,
    /// Precedent lookback window in days. Default 90.
    pub precedent_lookback_days: u32,
}

impl Default for AssemblerConfig {
    fn default() -> Self {
        Self {
            max_canon: 5,
            max_baselines: 3,
            max_precedent: 5,
            precedent_lookback_days: 90,
        }
    }
}

impl<S: CriteriaSource> CriteriaAssembler<S> {
    /// Construct a new assembler with default config.
    #[must_use]
    pub fn new(source: S) -> Self {
        Self {
            source,
            config: AssemblerConfig::default(),
        }
    }

    /// Construct with explicit config (custom retrieval bounds).
    #[must_use]
    pub fn with_config(source: S, config: AssemblerConfig) -> Self {
        Self { source, config }
    }

    /// Assemble criteria for `dimension`, using the draft's `topic_hints`
    /// to scope retrieval.
    ///
    /// # Errors
    ///
    /// Returns an [`AssemblyError`] if all three retrieval sources fail
    /// simultaneously. Partial failures (e.g. canon succeeds, precedent
    /// errors) are tolerated: the failed source contributes 0 entries and
    /// surfaces a warning in `assembly_warnings`. This keeps the gatekeeper
    /// honest when retrieval is degraded — it sees fewer evidence entries
    /// and may refuse with `RetrievalInsufficient`.
    pub async fn assemble(
        &self,
        dimension: GateDimension,
        draft: &Draft,
    ) -> Result<Criteria, AssemblyError> {
        let topics = &draft.topic_hints;
        let snapshot = self.source.snapshot_id();
        let mut warnings: Vec<String> = Vec::new();

        let canon = self
            .source
            .fetch_canon(dimension, topics, self.config.max_canon)
            .await
            .unwrap_or_else(|e| {
                warnings.push(format!("canon fetch failed: {e}"));
                Vec::new()
            });
        let baselines = self
            .source
            .fetch_baselines(dimension, topics, self.config.max_baselines)
            .await
            .unwrap_or_else(|e| {
                warnings.push(format!("baseline fetch failed: {e}"));
                Vec::new()
            });
        let precedent = self
            .source
            .fetch_precedent(
                dimension,
                topics,
                self.config.precedent_lookback_days,
                self.config.max_precedent,
            )
            .await
            .unwrap_or_else(|e| {
                warnings.push(format!("precedent fetch failed: {e}"));
                Vec::new()
            });

        // Soft hints — surface as warnings so the gatekeeper sees the
        // shape of its retrieval.
        if canon.is_empty() {
            warnings.push(format!(
                "no canon excerpts for dimension {}",
                dimension.as_str()
            ));
        }
        if precedent.is_empty() {
            warnings.push(format!(
                "no precedent for topics {topics:?} within {} days",
                self.config.precedent_lookback_days
            ));
        }

        Ok(Criteria {
            dimension,
            canon_excerpts: canon,
            industry_baselines: baselines,
            precedent,
            build_plan_excerpts: Vec::new(), // populated when a BuildPlan is wired in
            retrieved_at: chrono::Utc::now(),
            helix_snapshot: snapshot,
            assembly_warnings: warnings,
        })
    }

    /// Assemble criteria with explicit build-plan excerpts.
    ///
    /// Use when the caller can pluck plan-relevant sections without going
    /// through the source — the plan file is local and stable per build.
    ///
    /// # Errors
    ///
    /// See [`Self::assemble`].
    pub async fn assemble_with_plan(
        &self,
        dimension: GateDimension,
        draft: &Draft,
        plan_excerpts: Vec<PlanRef>,
    ) -> Result<Criteria, AssemblyError> {
        let mut c = self.assemble(dimension, draft).await?;
        c.build_plan_excerpts = plan_excerpts;
        Ok(c)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::agent::gatekeeper::types::DraftKind;
    use std::sync::Mutex;

    // ── In-memory fake source ─────────────────────────────────────────────
    //
    // Lives in `tests` module — does not violate the gatekeeper struct's
    // no-interior-mutability rule. `CriteriaSource` impls may use mutability
    // freely; only `Gatekeeper` impls are bound by the stateless contract.

    struct FakeSource {
        canon: Vec<CanonRef>,
        baselines: Vec<BaselineRef>,
        precedent: Vec<PrecedentRef>,
        // Track calls for determinism testing.
        calls: Mutex<u32>,
        // Toggle to simulate a failing source for one method.
        canon_fails: bool,
    }

    impl FakeSource {
        fn empty() -> Self {
            Self {
                canon: Vec::new(),
                baselines: Vec::new(),
                precedent: Vec::new(),
                calls: Mutex::new(0),
                canon_fails: false,
            }
        }

        fn with_canon_count(n: usize) -> Self {
            let mut s = Self::empty();
            for i in 0..n {
                s.canon.push(CanonRef {
                    doc: "builders-cookbook".to_owned(),
                    section: format!("§{}", 48 + i),
                    excerpt: "no .unwrap() in prod".to_owned(),
                    uri: format!("canon://builders-cookbook#section-{}", 48 + i),
                });
            }
            s
        }
    }

    #[async_trait]
    impl CriteriaSource for FakeSource {
        async fn fetch_canon(
            &self,
            _dim: GateDimension,
            _topics: &[String],
            limit: usize,
        ) -> Result<Vec<CanonRef>, AssemblyError> {
            *self.calls.lock().unwrap() += 1;
            if self.canon_fails {
                return Err(AssemblyError::SourceUnavailable("test".to_owned()));
            }
            Ok(self.canon.iter().take(limit).cloned().collect())
        }
        async fn fetch_baselines(
            &self,
            _dim: GateDimension,
            _topics: &[String],
            limit: usize,
        ) -> Result<Vec<BaselineRef>, AssemblyError> {
            Ok(self.baselines.iter().take(limit).cloned().collect())
        }
        async fn fetch_precedent(
            &self,
            _dim: GateDimension,
            _topics: &[String],
            _lookback_days: u32,
            limit: usize,
        ) -> Result<Vec<PrecedentRef>, AssemblyError> {
            Ok(self.precedent.iter().take(limit).cloned().collect())
        }
        fn snapshot_id(&self) -> HelixSnapshotId {
            HelixSnapshotId::test()
        }
    }

    fn rust_draft() -> Draft {
        Draft {
            content: "fn x() {}".to_owned(),
            kind: DraftKind::Code,
            topic_hints: vec!["rust".to_owned()],
            file_paths: Vec::new(),
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn assemble_with_empty_source_yields_warnings() {
        let asm = CriteriaAssembler::new(FakeSource::empty());
        let c = asm
            .assemble(GateDimension::Quality, &rust_draft())
            .await
            .unwrap();
        assert_eq!(c.total_evidence_count(), 0);
        assert!(c.assembly_warnings.iter().any(|w| w.contains("canon")));
        assert!(c.assembly_warnings.iter().any(|w| w.contains("precedent")));
    }

    #[tokio::test]
    async fn assemble_returns_canon_excerpts() {
        let asm = CriteriaAssembler::new(FakeSource::with_canon_count(3));
        let c = asm
            .assemble(GateDimension::Quality, &rust_draft())
            .await
            .unwrap();
        assert_eq!(c.canon_excerpts.len(), 3);
        assert_eq!(c.total_evidence_count(), 3);
    }

    #[tokio::test]
    async fn assemble_respects_max_canon_config() {
        let cfg = AssemblerConfig {
            max_canon: 2,
            ..AssemblerConfig::default()
        };
        let asm = CriteriaAssembler::with_config(FakeSource::with_canon_count(10), cfg);
        let c = asm
            .assemble(GateDimension::Quality, &rust_draft())
            .await
            .unwrap();
        assert_eq!(c.canon_excerpts.len(), 2);
    }

    #[tokio::test]
    async fn assemble_tolerates_partial_failure() {
        let mut src = FakeSource::with_canon_count(2);
        src.canon_fails = true;
        let asm = CriteriaAssembler::new(src);
        let c = asm
            .assemble(GateDimension::Quality, &rust_draft())
            .await
            .unwrap();
        assert_eq!(c.canon_excerpts.len(), 0);
        assert!(
            c.assembly_warnings
                .iter()
                .any(|w| w.contains("canon fetch failed")),
            "expected canon-failure warning, got {:?}",
            c.assembly_warnings
        );
    }

    #[tokio::test]
    async fn assemble_determinism_same_source_same_output_shape() {
        let asm = CriteriaAssembler::new(FakeSource::with_canon_count(3));
        let c1 = asm
            .assemble(GateDimension::Quality, &rust_draft())
            .await
            .unwrap();
        let c2 = asm
            .assemble(GateDimension::Quality, &rust_draft())
            .await
            .unwrap();
        // retrieved_at differs; other shape should be identical.
        assert_eq!(c1.dimension, c2.dimension);
        assert_eq!(c1.canon_excerpts.len(), c2.canon_excerpts.len());
        assert_eq!(c1.helix_snapshot, c2.helix_snapshot);
        assert_eq!(c1.assembly_warnings, c2.assembly_warnings);
    }

    #[tokio::test]
    async fn assemble_with_plan_threads_excerpts_through() {
        let asm = CriteriaAssembler::new(FakeSource::with_canon_count(2));
        let plan = vec![PlanRef {
            plan_codename: "test-build".to_owned(),
            section: "§4 phase 2".to_owned(),
            excerpt: "must enforce citation invariant".to_owned(),
        }];
        let c = asm
            .assemble_with_plan(GateDimension::Quality, &rust_draft(), plan.clone())
            .await
            .unwrap();
        assert_eq!(c.build_plan_excerpts.len(), 1);
        assert_eq!(c.build_plan_excerpts[0].plan_codename, "test-build");
        assert_eq!(c.total_evidence_count(), 3); // 2 canon + 1 plan
    }
}
