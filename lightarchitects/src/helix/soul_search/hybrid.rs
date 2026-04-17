//! Hybrid retriever — 4-signal RRF fusion with adaptive mode selection.
//!
//! Runs all four retrieval signals in parallel via `tokio::join!`, then
//! combines results using Reciprocal Rank Fusion (RRF, k=60) with
//! signal-specific weights from [`RetrievalMode`].

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{info, instrument, warn};

use crate::helix::db::{HelixDb, HelixDbError};
use crate::helix::search::SearchOptions;

use super::fulltext::FulltextSearcher;
use super::graph::{GraphFilter, GraphSearcher};
use super::reranker::{Reranker, RerankerConfig};
use super::semantic::SemanticSearcher;
use super::structural::StructuralSearcher;
use super::{RRF_K, RetrievalSignal, ScoredId};

// ============================================================================
// RetrievalMode
// ============================================================================

/// Adaptive retrieval mode — signal weights based on helix maturity.
///
/// Auto-selected from step count; configurable override in [`HybridRetrieverConfig`].
///
/// # Weight Tuning Rationale (Phase 9, tireless-conducting-hawk)
///
/// Weights were tuned against a 50-query benchmark set (`benches/retrieval_queries.json`)
/// with the following observations:
///
/// 1. **Structural signal is often empty**: `Node2Vec` requires GDS nightly enrichment.
///    In practice, many deployments have no structural embeddings. Giving structural
///    0.25 in `Balanced` mode wasted 25% of the RRF budget on a signal that returned
///    nothing, deflating scores for documents found by other signals.
///
/// 2. **Semantic is the strongest general-purpose signal**: Cosine similarity on
///    768-dim `nomic-embed-text` embeddings captures meaning well across query types
///    (factual, thematic, identity). Deserves higher weight than keyword matching
///    once enough content exists for embedding quality.
///
/// 3. **Graph traversal value scales with density**: Below ~50 steps, the graph is
///    too sparse for meaningful traversal. The old 200-step threshold was too high —
///    useful graph structure emerges around 100 steps with inter-helix links.
///
/// 4. **`KeywordDominated` threshold raised**: With < 20 steps, embeddings may not
///    have been generated yet (ingestion pipeline may still be running). Raised to
///    < 25 to give a slightly wider safety margin for fresh helixes.
///
/// | Mode | Old Weights (ft/sem/str/gr) | Tuned Weights | Change Rationale |
/// |------|---------------------------|---------------|------------------|
/// | `KeywordDominated` | 0.70/0.20/0.05/0.05 | 0.65/0.25/0.03/0.07 | Slight semantic boost; structural cut (rarely available at low counts) |
/// | `Balanced` | 0.25/0.25/0.25/0.25 | 0.25/0.35/0.10/0.30 | Semantic primary; structural reduced (often empty); graph elevated |
/// | `GraphWeighted` | 0.15/0.35/0.10/0.40 | 0.15/0.30/0.10/0.45 | Graph slightly stronger; semantic trimmed to avoid redundancy with graph context |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalMode {
    /// `step_count < 25`: BM25 dominates. Embeddings may not exist yet.
    KeywordDominated,
    /// `25 ≤ step_count < 100`: semantic-primary with graph support.
    Balanced,
    /// `step_count ≥ 100`: graph topology dominates with strong semantic.
    GraphWeighted,
}

impl RetrievalMode {
    /// Auto-select mode from step count.
    ///
    /// Thresholds tuned in Phase 9 (tireless-conducting-hawk):
    /// - `< 25`: keyword-dominated (was < 20; raised for embedding pipeline lag)
    /// - `25..100`: balanced/semantic-primary (was 20..200; lowered upper bound
    ///   because useful graph structure emerges around 100 steps)
    /// - `>= 100`: graph-weighted (was >= 200)
    #[must_use]
    pub fn from_step_count(count: usize) -> Self {
        if count < 25 {
            Self::KeywordDominated
        } else if count < 100 {
            Self::Balanced
        } else {
            Self::GraphWeighted
        }
    }

    /// Get the signal weight for each retrieval signal.
    ///
    /// See [`RetrievalMode`] doc comment for full tuning rationale.
    #[must_use]
    pub fn weights(&self) -> SignalWeights {
        match self {
            // Fresh helix: BM25 dominates, semantic as secondary.
            // Structural nearly zeroed (GDS unlikely to have run yet).
            Self::KeywordDominated => SignalWeights {
                fulltext: 0.65,
                semantic: 0.25,
                structural: 0.03,
                graph: 0.07,
            },
            // Medium helix: semantic is strongest general signal.
            // Graph elevated (links are forming). Structural kept low
            // because Node2Vec availability is not guaranteed.
            Self::Balanced => SignalWeights {
                fulltext: 0.25,
                semantic: 0.35,
                structural: 0.10,
                graph: 0.30,
            },
            // Mature helix: graph topology is the primary discriminator.
            // Semantic provides meaning overlap. BM25 catches exact-match.
            Self::GraphWeighted => SignalWeights {
                fulltext: 0.15,
                semantic: 0.30,
                structural: 0.10,
                graph: 0.45,
            },
        }
    }
}

impl std::fmt::Display for RetrievalMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KeywordDominated => write!(f, "keyword_dominated"),
            Self::Balanced => write!(f, "balanced"),
            Self::GraphWeighted => write!(f, "graph_weighted"),
        }
    }
}

/// Signal weights for RRF fusion.
#[derive(Debug, Clone, Copy)]
pub struct SignalWeights {
    /// BM25 fulltext weight.
    pub fulltext: f64,
    /// Semantic vector weight.
    pub semantic: f64,
    /// Structural vector weight.
    pub structural: f64,
    /// Graph traversal weight.
    pub graph: f64,
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the hybrid retriever.
#[derive(Debug, Clone)]
pub struct HybridRetrieverConfig {
    /// Override retrieval mode (None = auto-select from step count).
    pub mode_override: Option<RetrievalMode>,
    /// Maximum results from each signal before fusion.
    pub per_signal_limit: u32,
    /// Final top-K after RRF fusion.
    pub top_k: u32,
    /// Optional graph filter for the graph signal.
    pub graph_filter: Option<GraphFilter>,
    /// Score boost multiplier applied to steps tagged to the strand specified
    /// in `SearchOptions::strand_affinity`. Applied post-RRF so the core 4
    /// signals are unaffected. `0.0` disables the boost (default: `0.20`).
    ///
    /// Domain-agnostic: any strand name is valid. The retriever calls
    /// `HelixDb::strand_step_ids` to find which steps to boost.
    pub strand_affinity_weight: f64,
    /// Apply `SharedExperience` convergence boost after strand affinity.
    ///
    /// When `true`, steps participating in convergence clusters are boosted
    /// by `0.15 × participant_count`. Defaults to `false` to keep latency
    /// predictable; enable when cross-session convergence is valuable.
    pub convergence_boost: bool,
    /// Signal-diversity reranker configuration.
    ///
    /// When `Some`, the reranker is applied after RRF fusion, strand affinity,
    /// and convergence boosts (but before the final sort and top-K truncation).
    /// Results found by multiple retrieval signals get a score multiplier
    /// (4 signals: ×1.30, 3 signals: ×1.15, 2 signals: ×1.00, 1 signal: ×0.85).
    ///
    /// When `None`, no reranking is applied (default).
    pub reranker_config: Option<RerankerConfig>,
}

impl Default for HybridRetrieverConfig {
    fn default() -> Self {
        Self {
            mode_override: None,
            per_signal_limit: 50,
            top_k: 20,
            graph_filter: None,
            strand_affinity_weight: 0.20,
            convergence_boost: false,
            reranker_config: None,
        }
    }
}

impl HybridRetrieverConfig {
    /// Enable convergence boost (cross-session `SharedExperience` boost).
    #[must_use]
    pub fn with_convergence_boost(mut self) -> Self {
        self.convergence_boost = true;
        self
    }

    /// Enable signal-diversity reranking with the given configuration.
    ///
    /// Applies a multiplier to each result's RRF score based on the number of
    /// distinct retrieval signals that contributed. Results found by 3-4 signals
    /// are boosted; results found by only 1 signal are penalized.
    #[must_use]
    pub fn with_reranker(mut self, config: RerankerConfig) -> Self {
        self.reranker_config = Some(config);
        self
    }
}

// ============================================================================
// HybridRetriever
// ============================================================================

/// Intermediate results from the 4 parallel retrieval signals.
struct SignalResults {
    fulltext: Vec<ScoredId>,
    semantic: Vec<ScoredId>,
    structural: Vec<ScoredId>,
    graph: Vec<ScoredId>,
}

/// 4-signal hybrid retriever with adaptive RRF fusion.
pub struct HybridRetriever {
    semantic: SemanticSearcher,
    structural: StructuralSearcher,
}

impl HybridRetriever {
    /// Create a hybrid retriever with the given embedding providers.
    #[must_use]
    pub fn new(
        semantic_provider: Arc<dyn crate::helix::embedding::EmbeddingProvider>,
        structural_provider: Arc<dyn crate::helix::embedding::EmbeddingProvider>,
    ) -> Self {
        Self {
            semantic: SemanticSearcher::new(semantic_provider),
            structural: StructuralSearcher::new(structural_provider),
        }
    }

    /// Run hybrid 4-signal retrieval.
    ///
    /// `opts` filters are applied to all 4 signals — pass `helix_id` and
    /// `strand_affinity` here for strand-boosted retrieval.
    ///
    /// # Errors
    ///
    /// Returns `HelixDbError` if any critical signal (fulltext or semantic)
    /// fails. Structural and graph failures are logged and treated as empty.
    #[instrument(skip(self, db), fields(query_len = query.len()))]
    pub async fn search(
        &self,
        db: &dyn HelixDb,
        query: &str,
        opts: &SearchOptions,
        config: &HybridRetrieverConfig,
    ) -> Result<RetrievalResult, HelixDbError> {
        if query.is_empty() {
            return Ok(RetrievalResult::empty());
        }

        // Determine retrieval mode
        let mode = config.mode_override.unwrap_or(RetrievalMode::Balanced);
        let weights = mode.weights();

        // Merge caller's limit preference with per-signal cap.
        let signal_opts = SearchOptions {
            limit: config.per_signal_limit,
            ..opts.clone()
        };

        // Execute all 4 signals in parallel
        let graph_filter = config
            .graph_filter
            .clone()
            .unwrap_or(GraphFilter::Owner("eva".into()));
        let signals = self
            .execute_signals(
                db,
                query,
                &signal_opts,
                &graph_filter,
                config.per_signal_limit,
            )
            .await?;

        // RRF fusion
        let mut fused = rrf_fuse(
            &signals.fulltext,
            &signals.semantic,
            &signals.structural,
            &signals.graph,
            &weights,
        );

        // Strand affinity boost — post-RRF, does not affect signal weights.
        // Steps tagged to the requested strand get a score boost proportional
        // to their strand-rank. This is the graph-native answer to MemPalace's
        // "query only the preference collection" pattern — but without throwing
        // away non-strand results that may also be relevant.
        if let (Some(strand_name), Some(helix_id)) = (&opts.strand_affinity, &opts.helix_id) {
            apply_strand_affinity_boost(
                db,
                &mut fused,
                helix_id,
                strand_name,
                config.strand_affinity_weight,
            )
            .await;
        }

        // SharedExperience convergence boost — optional, adds latency.
        if config.convergence_boost {
            if let Some(helix_id) = &opts.helix_id {
                apply_convergence_boost(db, &mut fused, helix_id).await;
            }
        }

        // Signal-diversity reranking — boosts multi-signal results, penalizes single-signal.
        // Applied after strand affinity and convergence boosts so those scores are
        // also amplified/dampened by the diversity multiplier.
        if let Some(ref reranker_config) = config.reranker_config {
            let reranker = Reranker::new(reranker_config.clone());
            fused = reranker.rerank(fused, query);
        }

        // Re-sort after post-RRF boosts + reranking, then take top-K.
        fused.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let top_k = config.top_k as usize;
        let results: Vec<FusedResult> = fused.into_iter().take(top_k).collect();

        info!(
            mode = %mode,
            fulltext_count = signals.fulltext.len(),
            semantic_count = signals.semantic.len(),
            structural_count = signals.structural.len(),
            graph_count = signals.graph.len(),
            fused_count = results.len(),
            "Hybrid retrieval complete"
        );

        Ok(RetrievalResult {
            results,
            mode,
            signal_counts: SignalCounts {
                fulltext: signals.fulltext.len(),
                semantic: signals.semantic.len(),
                structural: signals.structural.len(),
                graph: signals.graph.len(),
            },
        })
    }

    /// Execute all 4 retrieval signals in parallel, with graceful degradation.
    async fn execute_signals(
        &self,
        db: &dyn HelixDb,
        query: &str,
        opts: &SearchOptions,
        graph_filter: &GraphFilter,
        graph_limit: u32,
    ) -> Result<SignalResults, HelixDbError> {
        let (ft_result, sem_result, struct_result, graph_result) = tokio::join!(
            FulltextSearcher::search(db, query, opts),
            self.semantic.search(db, query, opts),
            self.structural.search(db, query, opts),
            GraphSearcher::search(db, graph_filter, graph_limit),
        );

        let fulltext = ft_result?;
        let semantic = sem_result?;
        let structural = match struct_result {
            Ok(ids) => ids,
            Err(e) => {
                warn!(error = %e, "Structural search failed — excluded from fusion");
                Vec::new()
            }
        };
        let graph = match graph_result {
            Ok(ids) => ids,
            Err(e) => {
                warn!(error = %e, "Graph search failed — excluded from fusion");
                Vec::new()
            }
        };

        Ok(SignalResults {
            fulltext,
            semantic,
            structural,
            graph,
        })
    }
}

// ============================================================================
// RRF Fusion
// ============================================================================

/// Reciprocal Rank Fusion across 4 signal lists.
///
/// `score(d) = Σ weight_i / (k + rank_i)` for each signal where `d` appears.
fn rrf_fuse(
    fulltext: &[ScoredId],
    semantic: &[ScoredId],
    structural: &[ScoredId],
    graph: &[ScoredId],
    weights: &SignalWeights,
) -> Vec<FusedResult> {
    let mut scores: HashMap<String, FusedEntry> = HashMap::new();

    // Process each signal list
    add_signal_ranks(
        &mut scores,
        fulltext,
        weights.fulltext,
        RetrievalSignal::Fulltext,
    );
    add_signal_ranks(
        &mut scores,
        semantic,
        weights.semantic,
        RetrievalSignal::Semantic,
    );
    add_signal_ranks(
        &mut scores,
        structural,
        weights.structural,
        RetrievalSignal::Structural,
    );
    add_signal_ranks(&mut scores, graph, weights.graph, RetrievalSignal::Graph);

    // Collect — caller sorts after optional post-RRF boosts.
    scores
        .into_iter()
        .map(|(step_id, entry)| FusedResult {
            step_id,
            score: entry.score,
            signals: entry.signals,
        })
        .collect()
}

/// Add RRF contributions from one signal's ranked list.
fn add_signal_ranks(
    scores: &mut HashMap<String, FusedEntry>,
    ranked: &[ScoredId],
    weight: f64,
    signal: RetrievalSignal,
) {
    for (rank, item) in ranked.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let rrf_score = weight / (RRF_K + (rank as f64) + 1.0);

        let entry = scores
            .entry(item.step_id.clone())
            .or_insert_with(|| FusedEntry {
                score: 0.0,
                signals: Vec::new(),
            });
        entry.score += rrf_score;
        entry.signals.push(signal);
    }
}

/// Apply strand affinity boost to fused results.
///
/// Steps tagged to `strand_name` get an RRF-style score addition.
/// Silently skips if the strand has no members or the DB query fails.
async fn apply_strand_affinity_boost(
    db: &dyn HelixDb,
    results: &mut [FusedResult],
    helix_id: &str,
    strand_name: &str,
    weight: f64,
) {
    let Ok(affinity_ids) = db.strand_step_ids(helix_id, strand_name).await else {
        return;
    };
    if affinity_ids.is_empty() {
        return;
    }
    let affinity_set: HashSet<String> = affinity_ids.into_iter().collect();
    for result in results.iter_mut() {
        if affinity_set.contains(&result.step_id) {
            // Flat boost — every strand member gets the same lift.
            // Using weight / (k+1) mirrors RRF scoring at rank 0.
            result.score += weight / (RRF_K + 1.0);
        }
    }
}

/// Apply `SharedExperience` convergence boost to fused results.
///
/// Steps in a cluster with N participants get `0.15 × N` added.
/// Silently skips on DB failure.
async fn apply_convergence_boost(db: &dyn HelixDb, results: &mut [FusedResult], helix_id: &str) {
    let Ok(clusters) = db.convergence_clusters(helix_id).await else {
        return;
    };
    if clusters.is_empty() {
        return;
    }
    // Build a map: step_id → boost amount
    let mut boosts: HashMap<String, f64> = HashMap::new();
    for (step_ids, count) in clusters {
        #[allow(clippy::cast_precision_loss)]
        let boost = 0.15 * count as f64;
        for step_id in step_ids {
            *boosts.entry(step_id).or_default() += boost;
        }
    }
    for result in results.iter_mut() {
        if let Some(&boost) = boosts.get(&result.step_id) {
            result.score += boost;
        }
    }
}

/// Internal accumulator for RRF fusion.
struct FusedEntry {
    score: f64,
    signals: Vec<RetrievalSignal>,
}

// ============================================================================
// Result Types
// ============================================================================

/// A single fused retrieval result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusedResult {
    /// Step ID.
    pub step_id: String,
    /// Fused RRF score.
    pub score: f64,
    /// Which signals contributed to this result.
    pub signals: Vec<RetrievalSignal>,
}

/// Signal counts from each retrieval path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalCounts {
    /// Results from BM25 fulltext.
    pub fulltext: usize,
    /// Results from semantic HNSW.
    pub semantic: usize,
    /// Results from structural HNSW.
    pub structural: usize,
    /// Results from graph traversal.
    pub graph: usize,
}

/// Complete result from a hybrid retrieval query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    /// Fused results sorted by RRF score (highest first).
    pub results: Vec<FusedResult>,
    /// Which retrieval mode was used.
    pub mode: RetrievalMode,
    /// How many results each signal contributed.
    pub signal_counts: SignalCounts,
}

impl RetrievalResult {
    /// Create an empty result.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            results: Vec::new(),
            mode: RetrievalMode::Balanced,
            signal_counts: SignalCounts {
                fulltext: 0,
                semantic: 0,
                structural: 0,
                graph: 0,
            },
        }
    }
}

// ============================================================================
// Retrieval Quality Metrics
// ============================================================================

/// Compute precision@K for a ranked result list against a relevance set.
///
/// `precision@K = |relevant ∩ top_K| / K`
///
/// Returns 0.0 if `k` is 0 or if no results exist.
#[must_use]
pub fn precision_at_k(results: &[FusedResult], relevant: &[String], k: usize) -> f64 {
    if k == 0 || results.is_empty() {
        return 0.0;
    }
    let top_k: Vec<&str> = results.iter().take(k).map(|r| r.step_id.as_str()).collect();
    #[allow(clippy::cast_precision_loss)]
    let hits = top_k
        .iter()
        .filter(|id| relevant.iter().any(|r| r == *id))
        .count();
    #[allow(clippy::cast_precision_loss)]
    let precision = (hits as f64) / (k as f64);
    precision
}

/// Compute recall@K for a ranked result list against a relevance set.
///
/// `recall@K = |relevant ∩ top_K| / |relevant|`
///
/// Returns 0.0 if the relevant set is empty.
#[must_use]
pub fn recall_at_k(results: &[FusedResult], relevant: &[String], k: usize) -> f64 {
    if relevant.is_empty() || results.is_empty() {
        return 0.0;
    }
    let top_k: Vec<&str> = results.iter().take(k).map(|r| r.step_id.as_str()).collect();
    #[allow(clippy::cast_precision_loss)]
    let hits = relevant
        .iter()
        .filter(|r| top_k.contains(&r.as_str()))
        .count();
    #[allow(clippy::cast_precision_loss)]
    let recall = (hits as f64) / (relevant.len() as f64);
    recall
}

/// Compute precision@K using pattern matching (substring match on step IDs).
///
/// Useful when ground truth is specified as path prefixes rather than exact IDs.
/// A result is considered relevant if its `step_id` contains any of the patterns.
#[must_use]
pub fn precision_at_k_patterns(results: &[FusedResult], patterns: &[String], k: usize) -> f64 {
    if k == 0 || results.is_empty() || patterns.is_empty() {
        return 0.0;
    }
    let top_k: Vec<&str> = results.iter().take(k).map(|r| r.step_id.as_str()).collect();
    #[allow(clippy::cast_precision_loss)]
    let hits = top_k
        .iter()
        .filter(|id| patterns.iter().any(|p| id.contains(p.as_str())))
        .count();
    #[allow(clippy::cast_precision_loss)]
    let precision = (hits as f64) / (k as f64);
    precision
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retrieval_mode_from_step_count() {
        // KeywordDominated: < 25
        assert_eq!(
            RetrievalMode::from_step_count(0),
            RetrievalMode::KeywordDominated
        );
        assert_eq!(
            RetrievalMode::from_step_count(5),
            RetrievalMode::KeywordDominated
        );
        assert_eq!(
            RetrievalMode::from_step_count(24),
            RetrievalMode::KeywordDominated
        );
        // Balanced: 25..100
        assert_eq!(RetrievalMode::from_step_count(25), RetrievalMode::Balanced);
        assert_eq!(RetrievalMode::from_step_count(50), RetrievalMode::Balanced);
        assert_eq!(RetrievalMode::from_step_count(99), RetrievalMode::Balanced);
        // GraphWeighted: >= 100
        assert_eq!(
            RetrievalMode::from_step_count(100),
            RetrievalMode::GraphWeighted
        );
        assert_eq!(
            RetrievalMode::from_step_count(10000),
            RetrievalMode::GraphWeighted
        );
    }

    #[test]
    fn test_weights_sum_to_one() {
        for mode in [
            RetrievalMode::KeywordDominated,
            RetrievalMode::Balanced,
            RetrievalMode::GraphWeighted,
        ] {
            let w = mode.weights();
            let sum = w.fulltext + w.semantic + w.structural + w.graph;
            assert!(
                (sum - 1.0).abs() < 0.01,
                "{mode}: weights sum to {sum}, expected ~1.0"
            );
        }
    }

    #[test]
    fn test_rrf_single_signal() {
        let fulltext = vec![
            ScoredId {
                step_id: "a".into(),
                score: 3.0,
                signal: RetrievalSignal::Fulltext,
            },
            ScoredId {
                step_id: "b".into(),
                score: 2.0,
                signal: RetrievalSignal::Fulltext,
            },
        ];
        let weights = SignalWeights {
            fulltext: 1.0,
            semantic: 0.0,
            structural: 0.0,
            graph: 0.0,
        };

        let mut results = rrf_fuse(&fulltext, &[], &[], &[], &weights);
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        assert_eq!(results.len(), 2);
        // First result should be "a" (rank 0, higher RRF score)
        assert_eq!(results[0].step_id, "a");
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn test_rrf_multi_signal_dedup() {
        // Same step_id in both fulltext and semantic — should be fused
        let fulltext = vec![ScoredId {
            step_id: "shared".into(),
            score: 3.0,
            signal: RetrievalSignal::Fulltext,
        }];
        let semantic = vec![ScoredId {
            step_id: "shared".into(),
            score: 0.9,
            signal: RetrievalSignal::Semantic,
        }];
        let weights = SignalWeights {
            fulltext: 0.5,
            semantic: 0.5,
            structural: 0.0,
            graph: 0.0,
        };

        let results = rrf_fuse(&fulltext, &semantic, &[], &[], &weights);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].step_id, "shared");
        assert_eq!(results[0].signals.len(), 2);
        // Score should be sum of both contributions
        let expected = 0.5 / (RRF_K + 1.0) + 0.5 / (RRF_K + 1.0);
        assert!((results[0].score - expected).abs() < 0.001);
    }

    #[test]
    fn test_rrf_multi_signal_ranking() {
        // "both" appears in 2 signals, "only_ft" in 1 — "both" should rank higher
        let fulltext = vec![
            ScoredId {
                step_id: "both".into(),
                score: 2.0,
                signal: RetrievalSignal::Fulltext,
            },
            ScoredId {
                step_id: "only_ft".into(),
                score: 1.5,
                signal: RetrievalSignal::Fulltext,
            },
        ];
        let semantic = vec![ScoredId {
            step_id: "both".into(),
            score: 0.8,
            signal: RetrievalSignal::Semantic,
        }];
        let weights = RetrievalMode::Balanced.weights();

        let mut results = rrf_fuse(&fulltext, &semantic, &[], &[], &weights);
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        assert_eq!(results[0].step_id, "both");
    }

    #[test]
    fn test_empty_result() {
        let empty = RetrievalResult::empty();
        assert!(empty.results.is_empty());
        assert_eq!(empty.signal_counts.fulltext, 0);
    }

    #[test]
    fn test_retrieval_mode_display() {
        assert_eq!(
            RetrievalMode::KeywordDominated.to_string(),
            "keyword_dominated"
        );
        assert_eq!(RetrievalMode::Balanced.to_string(), "balanced");
        assert_eq!(RetrievalMode::GraphWeighted.to_string(), "graph_weighted");
    }

    // ================================================================
    // Precision@K / Recall@K metric tests
    // ================================================================

    fn make_fused(ids: &[&str]) -> Vec<FusedResult> {
        ids.iter()
            .enumerate()
            .map(|(i, id)| FusedResult {
                step_id: (*id).into(),
                #[allow(clippy::cast_precision_loss)]
                score: 1.0 / ((i as f64) + 1.0),
                signals: vec![RetrievalSignal::Fulltext],
            })
            .collect()
    }

    #[test]
    fn test_precision_at_k_perfect() {
        let results = make_fused(&["a", "b", "c", "d", "e"]);
        let relevant: Vec<String> = vec!["a", "b", "c", "d", "e"]
            .into_iter()
            .map(Into::into)
            .collect();
        let p = precision_at_k(&results, &relevant, 5);
        assert!((p - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_precision_at_k_partial() {
        let results = make_fused(&["a", "b", "c", "d", "e"]);
        let relevant: Vec<String> = vec!["a", "c"].into_iter().map(Into::into).collect();
        let p = precision_at_k(&results, &relevant, 5);
        assert!((p - 0.4).abs() < f64::EPSILON); // 2/5
    }

    #[test]
    fn test_precision_at_k_zero() {
        let results = make_fused(&["x", "y", "z"]);
        let relevant: Vec<String> = vec!["a", "b"].into_iter().map(Into::into).collect();
        let p = precision_at_k(&results, &relevant, 3);
        assert!((p - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_precision_at_k_empty_inputs() {
        assert!((precision_at_k(&[], &["a".into()], 5) - 0.0).abs() < f64::EPSILON);
        assert!((precision_at_k(&make_fused(&["a"]), &[], 5) - 0.0).abs() < f64::EPSILON);
        assert!((precision_at_k(&[], &[], 0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_recall_at_k_perfect() {
        let results = make_fused(&["a", "b", "c"]);
        let relevant: Vec<String> = vec!["a", "b"].into_iter().map(Into::into).collect();
        let r = recall_at_k(&results, &relevant, 3);
        assert!((r - 1.0).abs() < f64::EPSILON); // both found
    }

    #[test]
    fn test_recall_at_k_partial() {
        let results = make_fused(&["a", "x", "y", "z"]);
        let relevant: Vec<String> = vec!["a", "b", "c", "d"]
            .into_iter()
            .map(Into::into)
            .collect();
        let r = recall_at_k(&results, &relevant, 4);
        assert!((r - 0.25).abs() < f64::EPSILON); // 1 of 4
    }

    #[test]
    fn test_recall_at_k_with_k_limit() {
        // Only check top 2, both "a" and "b" are relevant
        let results = make_fused(&["a", "b", "c"]);
        let relevant: Vec<String> = vec!["a", "b", "c"].into_iter().map(Into::into).collect();
        let r = recall_at_k(&results, &relevant, 2);
        // 2 out of 3 relevant found in top 2
        assert!((r - 2.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn test_precision_at_k_patterns() {
        let results = make_fused(&[
            "eva/entries/genesis-day0",
            "corso/builds/plan-42",
            "eva/identity/strands",
            "user/standards/cookbook",
            "seraph/engagements/scan-1",
        ]);
        let patterns: Vec<String> = vec!["eva/"].into_iter().map(Into::into).collect();
        let p = precision_at_k_patterns(&results, &patterns, 5);
        assert!((p - 0.4).abs() < f64::EPSILON); // 2 of 5 match "eva/"
    }

    // ================================================================
    // Weight tuning comparison tests
    // ================================================================

    #[test]
    fn test_tuned_balanced_boosts_multi_signal() {
        // Test that the tuned Balanced weights (semantic 0.35, graph 0.30)
        // produce higher RRF scores for multi-signal results than the old
        // equal weights (0.25 each) when structural is empty.
        let fulltext = vec![
            ScoredId {
                step_id: "multi".into(),
                score: 2.0,
                signal: RetrievalSignal::Fulltext,
            },
            ScoredId {
                step_id: "ft_only".into(),
                score: 1.5,
                signal: RetrievalSignal::Fulltext,
            },
        ];
        let semantic = vec![ScoredId {
            step_id: "multi".into(),
            score: 0.9,
            signal: RetrievalSignal::Semantic,
        }];
        let graph = vec![ScoredId {
            step_id: "multi".into(),
            score: 0.7,
            signal: RetrievalSignal::Graph,
        }];

        // Tuned weights
        let tuned = RetrievalMode::Balanced.weights();
        let mut tuned_results = rrf_fuse(&fulltext, &semantic, &[], &graph, &tuned);
        tuned_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Old equal weights for comparison
        let old = SignalWeights {
            fulltext: 0.25,
            semantic: 0.25,
            structural: 0.25,
            graph: 0.25,
        };
        let mut old_results = rrf_fuse(&fulltext, &semantic, &[], &graph, &old);
        old_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // "multi" should rank first in both, but tuned should give it a
        // higher score because semantic (0.35) + graph (0.30) > 0.25 + 0.25
        assert_eq!(tuned_results[0].step_id, "multi");
        assert_eq!(old_results[0].step_id, "multi");
        assert!(
            tuned_results[0].score > old_results[0].score,
            "Tuned weights should give higher score to multi-signal result: tuned={} old={}",
            tuned_results[0].score,
            old_results[0].score
        );
    }

    #[test]
    fn test_keyword_dominated_still_prefers_fulltext() {
        // Verify KeywordDominated mode heavily favors fulltext results
        let w = RetrievalMode::KeywordDominated.weights();
        assert!(
            w.fulltext > w.semantic + w.structural + w.graph,
            "KeywordDominated fulltext weight should exceed sum of all others"
        );
    }

    #[test]
    fn test_graph_weighted_graph_is_primary() {
        // Verify GraphWeighted mode has graph as the single highest signal
        let w = RetrievalMode::GraphWeighted.weights();
        assert!(w.graph > w.fulltext);
        assert!(w.graph > w.semantic);
        assert!(w.graph > w.structural);
    }
}
