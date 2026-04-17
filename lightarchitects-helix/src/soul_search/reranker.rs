//! Reranker — signal-diversity reranking of RRF results.
//!
//! **Gated behind `rerank` feature** (`soul-helix/rerank`). When enabled,
//! applies a diversity bonus/penalty to RRF-fused results based on how many
//! distinct retrieval signals contributed to each result.
//!
//! # Rationale
//!
//! A document found by 3 out of 4 signals (BM25 + semantic + graph) is far
//! more likely to be genuinely relevant than one found by a single signal.
//! The RRF fusion already accounts for this somewhat (sum of per-signal RRF
//! scores), but the diversity reranker amplifies the effect:
//!
//! - **4 signals**: score *= 1.30 (strong cross-signal agreement)
//! - **3 signals**: score *= 1.15
//! - **2 signals**: score *= 1.00 (no change)
//! - **1 signal**:  score *= 0.85 (penalize single-signal results)
//!
//! These multipliers were chosen to be conservative — they reorder within
//! close-scoring clusters but do not override large RRF score gaps.
//!
//! # Usage
//!
//! ```rust,no_run
//! use lightarchitects_helix::soul_search::reranker::{Reranker, RerankerConfig};
//! use lightarchitects_helix::soul_search::hybrid::FusedResult;
//!
//! let reranker = Reranker::new(RerankerConfig::default());
//! let fused_results: Vec<FusedResult> = vec![]; // populated from hybrid RRF retrieval
//! let reranked = reranker.rerank(fused_results, "query text");
//! ```

use serde::{Deserialize, Serialize};
use tracing::debug;

use super::hybrid::FusedResult;

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the reranker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankerConfig {
    /// Whether reranking is enabled.
    ///
    /// Always `false` when the `rerank` feature is not compiled in.
    /// When the feature IS compiled in, defaults to `true`.
    pub enabled: bool,

    /// Multiplier for results found by all 4 signals.
    pub boost_4_signals: f64,

    /// Multiplier for results found by 3 signals.
    pub boost_3_signals: f64,

    /// Multiplier for results found by 2 signals (neutral).
    pub boost_2_signals: f64,

    /// Multiplier for results found by only 1 signal (penalty).
    pub boost_1_signal: f64,
}

impl Default for RerankerConfig {
    fn default() -> Self {
        Self {
            #[cfg(feature = "rerank")]
            enabled: true,
            #[cfg(not(feature = "rerank"))]
            enabled: false,
            boost_4_signals: 1.30,
            boost_3_signals: 1.15,
            boost_2_signals: 1.00,
            boost_1_signal: 0.85,
        }
    }
}

impl RerankerConfig {
    /// Validate that multipliers are positive and ordered.
    ///
    /// Returns `None` if valid, or `Some(error_message)` if invalid.
    #[must_use]
    pub fn validate(&self) -> Option<String> {
        if self.boost_1_signal <= 0.0
            || self.boost_2_signals <= 0.0
            || self.boost_3_signals <= 0.0
            || self.boost_4_signals <= 0.0
        {
            return Some("All boost multipliers must be positive".into());
        }
        if self.boost_1_signal > self.boost_2_signals
            || self.boost_2_signals > self.boost_3_signals
            || self.boost_3_signals > self.boost_4_signals
        {
            return Some(
                "Boost multipliers must be non-decreasing: 1-signal <= 2 <= 3 <= 4".into(),
            );
        }
        None
    }
}

// ============================================================================
// Reranker
// ============================================================================

/// Signal-diversity reranker.
///
/// Applies a multiplier to each result's RRF score based on the number of
/// distinct retrieval signals that contributed. Results are then re-sorted.
///
/// When disabled (default without `rerank` feature), acts as a pass-through.
pub struct Reranker {
    config: RerankerConfig,
}

impl Reranker {
    /// Create a new reranker with the given configuration.
    #[must_use]
    pub fn new(config: RerankerConfig) -> Self {
        Self { config }
    }

    /// Create a disabled reranker (pass-through).
    #[must_use]
    pub fn disabled() -> Self {
        Self::new(RerankerConfig {
            enabled: false,
            ..RerankerConfig::default()
        })
    }

    /// Whether reranking is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Rerank results using signal-diversity scoring.
    ///
    /// If disabled, returns the input unchanged.
    #[must_use]
    pub fn rerank(&self, mut results: Vec<FusedResult>, _query: &str) -> Vec<FusedResult> {
        if !self.config.enabled || results.is_empty() {
            return results;
        }

        // Count unique signals per result and apply multiplier
        for result in &mut results {
            let unique_signals = count_unique_signals(&result.signals);
            let multiplier = self.multiplier_for_count(unique_signals);
            result.score *= multiplier;
        }

        // Re-sort by adjusted score (descending)
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        debug!(
            count = results.len(),
            "Reranker applied signal-diversity multipliers"
        );

        results
    }

    /// Get the multiplier for a given unique-signal count.
    fn multiplier_for_count(&self, unique_count: usize) -> f64 {
        match unique_count {
            // 0 shouldn't happen (result always has >= 1 signal), but safe default
            0 | 1 => self.config.boost_1_signal,
            2 => self.config.boost_2_signals,
            3 => self.config.boost_3_signals,
            _ => self.config.boost_4_signals, // 4+
        }
    }
}

/// Count unique signal types in a signal list.
///
/// Uses a small fixed-size approach (4 signals max) instead of `HashSet`.
fn count_unique_signals(signals: &[super::RetrievalSignal]) -> usize {
    use super::RetrievalSignal;

    let mut seen_fulltext = false;
    let mut seen_semantic = false;
    let mut seen_structural = false;
    let mut seen_graph = false;

    for signal in signals {
        match signal {
            RetrievalSignal::Fulltext => seen_fulltext = true,
            RetrievalSignal::Semantic => seen_semantic = true,
            RetrievalSignal::Structural => seen_structural = true,
            RetrievalSignal::Graph => seen_graph = true,
        }
    }

    usize::from(seen_fulltext)
        + usize::from(seen_semantic)
        + usize::from(seen_structural)
        + usize::from(seen_graph)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soul_search::RetrievalSignal;

    #[test]
    fn test_default_config_validity() {
        let config = RerankerConfig::default();
        assert!(
            config.validate().is_none(),
            "Default config should be valid"
        );
    }

    #[test]
    fn test_config_validation_negative() {
        let config = RerankerConfig {
            enabled: true,
            boost_4_signals: -1.0,
            ..RerankerConfig::default()
        };
        assert!(config.validate().is_some());
    }

    #[test]
    fn test_config_validation_wrong_order() {
        let config = RerankerConfig {
            enabled: true,
            boost_1_signal: 2.0, // higher than boost_2
            boost_2_signals: 1.0,
            boost_3_signals: 1.5,
            boost_4_signals: 1.8,
        };
        assert!(config.validate().is_some());
    }

    #[test]
    fn test_passthrough_when_disabled() {
        let reranker = Reranker::disabled();
        assert!(!reranker.is_enabled());

        let results = vec![
            FusedResult {
                step_id: "a".into(),
                score: 0.9,
                signals: vec![RetrievalSignal::Fulltext],
            },
            FusedResult {
                step_id: "b".into(),
                score: 0.7,
                signals: vec![RetrievalSignal::Semantic],
            },
        ];

        let output = reranker.rerank(results, "test query");
        assert_eq!(output.len(), 2);
        assert_eq!(output[0].step_id, "a");
        assert_eq!(output[1].step_id, "b");
    }

    #[test]
    fn test_diversity_boost_reorders() {
        let reranker = Reranker::new(RerankerConfig {
            enabled: true,
            ..RerankerConfig::default()
        });

        // "b" has lower RRF score but 3 signals; "a" has higher score but 1 signal
        let results = vec![
            FusedResult {
                step_id: "a".into(),
                score: 0.020,
                signals: vec![RetrievalSignal::Fulltext],
            },
            FusedResult {
                step_id: "b".into(),
                score: 0.019,
                signals: vec![
                    RetrievalSignal::Fulltext,
                    RetrievalSignal::Semantic,
                    RetrievalSignal::Graph,
                ],
            },
        ];

        let output = reranker.rerank(results, "test query");
        // "b" should now rank higher: 0.019 * 1.15 = 0.02185 > 0.020 * 0.85 = 0.017
        assert_eq!(output[0].step_id, "b");
        assert_eq!(output[1].step_id, "a");
    }

    #[test]
    fn test_4_signal_result_boosted_highest() {
        let reranker = Reranker::new(RerankerConfig {
            enabled: true,
            ..RerankerConfig::default()
        });

        let results = vec![
            FusedResult {
                step_id: "single".into(),
                score: 0.025,
                signals: vec![RetrievalSignal::Fulltext],
            },
            FusedResult {
                step_id: "quad".into(),
                score: 0.020,
                signals: vec![
                    RetrievalSignal::Fulltext,
                    RetrievalSignal::Semantic,
                    RetrievalSignal::Structural,
                    RetrievalSignal::Graph,
                ],
            },
        ];

        let output = reranker.rerank(results, "test query");
        // quad: 0.020 * 1.30 = 0.026 > single: 0.025 * 0.85 = 0.02125
        assert_eq!(output[0].step_id, "quad");
    }

    #[test]
    fn test_count_unique_signals() {
        assert_eq!(count_unique_signals(&[]), 0);
        assert_eq!(
            count_unique_signals(&[RetrievalSignal::Fulltext, RetrievalSignal::Fulltext]),
            1
        );
        assert_eq!(
            count_unique_signals(&[RetrievalSignal::Fulltext, RetrievalSignal::Semantic]),
            2
        );
        assert_eq!(
            count_unique_signals(&[
                RetrievalSignal::Fulltext,
                RetrievalSignal::Semantic,
                RetrievalSignal::Structural,
                RetrievalSignal::Graph,
            ]),
            4
        );
    }

    #[test]
    fn test_empty_results_passthrough() {
        let reranker = Reranker::new(RerankerConfig {
            enabled: true,
            ..RerankerConfig::default()
        });
        let output = reranker.rerank(Vec::new(), "query");
        assert!(output.is_empty());
    }

    #[test]
    fn test_large_score_gap_not_overridden() {
        // If RRF score gap is large enough, diversity shouldn't flip the order
        let reranker = Reranker::new(RerankerConfig {
            enabled: true,
            ..RerankerConfig::default()
        });

        let results = vec![
            FusedResult {
                step_id: "strong_single".into(),
                score: 0.100, // very high
                signals: vec![RetrievalSignal::Fulltext],
            },
            FusedResult {
                step_id: "weak_quad".into(),
                score: 0.010, // much lower
                signals: vec![
                    RetrievalSignal::Fulltext,
                    RetrievalSignal::Semantic,
                    RetrievalSignal::Structural,
                    RetrievalSignal::Graph,
                ],
            },
        ];

        let output = reranker.rerank(results, "test");
        // 0.100 * 0.85 = 0.085 > 0.010 * 1.30 = 0.013 — order preserved
        assert_eq!(output[0].step_id, "strong_single");
    }
}
