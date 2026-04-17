//! Semantic compaction — reduce large entry sets to representative subsets.
//!
//! Provides three compactor implementations:
//!
//! - [`SignificanceCompactor`] — sort by weight × recency decay, take top-N.
//! - [`TemporalCompactor`] — exponential recency weighting, most recent wins.
//! - [`MiniMaxCoverageCompactor`] — k-means clustering with bounded iteration
//!   (Builders Cookbook §: no unbounded loops). Picks the centroid-closest entry
//!   per cluster to maximise semantic diversity.
//!
//! # Feature Gate
//!
//! This module is compiled when the `compaction` feature is enabled.

use std::sync::Arc;

use crate::soul::embedding::EmbeddingProvider;
use crate::soul::storage::StorageEntry;

// ============================================================================
// SemanticCompactor trait
// ============================================================================

/// Reduce a large entry set to a representative subset of `target_count`.
pub trait SemanticCompactor: Send + Sync {
    /// Compact `entries` to at most `target_count` representative entries.
    fn compact(&self, entries: &[StorageEntry], target_count: usize) -> Vec<StorageEntry>;
}

// ============================================================================
// SignificanceCompactor
// ============================================================================

/// Compactor that sorts by `significance × exp(-λ × age_days)` and takes top-N.
///
/// `decay_lambda` controls how aggressively older entries are down-weighted.
/// A value of `0.01` gives roughly half-weight at ~70 days.
pub struct SignificanceCompactor {
    /// Exponential decay factor applied to entry age in days.
    pub decay_lambda: f32,
}

impl Default for SignificanceCompactor {
    fn default() -> Self {
        Self { decay_lambda: 0.01 }
    }
}

impl SemanticCompactor for SignificanceCompactor {
    fn compact(&self, entries: &[StorageEntry], target_count: usize) -> Vec<StorageEntry> {
        if entries.is_empty() || target_count == 0 {
            return Vec::new();
        }

        let now = chrono::Utc::now();
        let mut scored: Vec<(f32, &StorageEntry)> = entries
            .iter()
            .map(|e| {
                // num_days() is i64 — clamp before casting to avoid precision loss.
                #[allow(clippy::cast_precision_loss)]
                let age_days = (now - e.created_at)
                    .num_days()
                    .max(0)
                    .min(i64::from(i32::MAX)) as f32;
                #[allow(clippy::cast_possible_truncation)]
                let sig = e.significance as f32;
                let score = sig * (-self.decay_lambda * age_days).exp();
                (score, e)
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored
            .into_iter()
            .take(target_count)
            .map(|(_, e)| e.clone())
            .collect()
    }
}

// ============================================================================
// TemporalCompactor
// ============================================================================

/// Compactor using exponential recency weighting — most recent entries ranked highest.
///
/// Score = `exp(-λ × age_days)` — significance is not considered.
pub struct TemporalCompactor {
    /// Exponential decay factor applied to entry age in days.
    pub decay_lambda: f32,
}

impl Default for TemporalCompactor {
    fn default() -> Self {
        Self { decay_lambda: 0.01 }
    }
}

impl SemanticCompactor for TemporalCompactor {
    fn compact(&self, entries: &[StorageEntry], target_count: usize) -> Vec<StorageEntry> {
        if entries.is_empty() || target_count == 0 {
            return Vec::new();
        }

        let now = chrono::Utc::now();
        let mut scored: Vec<(f32, &StorageEntry)> = entries
            .iter()
            .map(|e| {
                #[allow(clippy::cast_precision_loss)]
                let age_days = (now - e.created_at)
                    .num_days()
                    .max(0)
                    .min(i64::from(i32::MAX)) as f32;
                let score = (-self.decay_lambda * age_days).exp();
                (score, e)
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored
            .into_iter()
            .take(target_count)
            .map(|(_, e)| e.clone())
            .collect()
    }
}

// ============================================================================
// MiniMaxCoverageCompactor
// ============================================================================

/// K-means clustering compactor — maximise semantic diversity, minimise redundancy.
///
/// Embeds all entries via the configured [`EmbeddingProvider`], runs k-means
/// clustering (deterministic initialisation, **bounded** to `max_iter` iterations),
/// then picks the entry closest to each cluster centroid.
///
/// # Bounded Iterations (Builders Cookbook §)
///
/// The inner loop is bounded to `max_iter` (default: 100). The algorithm
/// converges early when no assignments change between iterations.
pub struct MiniMaxCoverageCompactor {
    /// Embedding provider used to vectorise entries.
    pub embedder: Arc<dyn EmbeddingProvider>,
    /// Maximum k-means iterations. **MUST be bounded** — default 100.
    ///
    /// Per Builders Cookbook §: no unbounded loops in production code.
    pub max_iter: usize,
}

impl SemanticCompactor for MiniMaxCoverageCompactor {
    fn compact(&self, entries: &[StorageEntry], target_count: usize) -> Vec<StorageEntry> {
        if entries.is_empty() || target_count == 0 {
            return Vec::new();
        }
        if entries.len() <= target_count {
            return entries.to_vec();
        }

        // Embed all entries synchronously (blocking inside the sync trait).
        let texts: Vec<&str> = entries.iter().map(|e| e.content.as_str()).collect();
        let vecs = match tokio::runtime::Handle::try_current() {
            Ok(handle) => match handle.block_on(self.embedder.embed(&texts)) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "MiniMaxCoverageCompactor embedding failed, falling back to significance sort");
                    return fallback_compact(entries, target_count);
                }
            },
            Err(_) => {
                // No tokio runtime — return a simple slice.
                return fallback_compact(entries, target_count);
            }
        };

        if vecs.is_empty() {
            return fallback_compact(entries, target_count);
        }

        // k = ceil(sqrt(target_count)), minimum 1.
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_sign_loss,
            clippy::cast_possible_truncation
        )]
        let k = ((target_count as f32).sqrt().ceil() as usize)
            .max(1)
            .min(vecs.len());

        let selected_indices = kmeans_pick(&vecs, k, self.max_iter);

        // Return at most target_count entries.
        selected_indices
            .into_iter()
            .take(target_count)
            .filter_map(|i| entries.get(i).cloned())
            .collect()
    }
}

// ============================================================================
// K-means helpers
// ============================================================================

/// Run Lloyd's k-means with deterministic initialisation (first k points).
///
/// Returns the index of the entry closest to each cluster centroid.
/// Loop is bounded to `max_iter` — Builders Cookbook §: no unbounded loops.
fn kmeans_pick(vecs: &[Vec<f32>], k: usize, max_iter: usize) -> Vec<usize> {
    let n = vecs.len();
    if k >= n {
        return (0..n).collect();
    }

    let dims = vecs[0].len();

    // Deterministic initialisation: use first k vectors as centroids.
    let mut centroids: Vec<Vec<f32>> = vecs[..k].to_vec();
    let mut assignments = vec![0usize; n];

    // Bounded Lloyd's iteration — Builders Cookbook § no unbounded loops.
    for _ in 0..max_iter {
        let mut changed = false;

        // Assignment step.
        for (i, vec) in vecs.iter().enumerate() {
            let nearest = centroids
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    euclidean_sq(vec, a)
                        .partial_cmp(&euclidean_sq(vec, b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map_or(0, |(j, _)| j);
            if assignments[i] != nearest {
                assignments[i] = nearest;
                changed = true;
            }
        }

        if !changed {
            break; // Converged early.
        }

        // Update step: recompute centroids as mean of assigned vectors.
        let mut new_centroids = vec![vec![0.0f32; dims]; k];
        let mut counts = vec![0usize; k];
        for (i, &cluster) in assignments.iter().enumerate() {
            counts[cluster] = counts[cluster].saturating_add(1);
            for d in 0..dims.min(vecs[i].len()) {
                new_centroids[cluster][d] += vecs[i][d];
            }
        }
        for c in 0..k {
            if counts[c] > 0 {
                #[allow(clippy::cast_precision_loss)]
                let count = counts[c] as f32;
                for d in 0..dims {
                    new_centroids[c][d] /= count;
                }
                centroids[c].clone_from(&new_centroids[c]);
            }
        }
    }

    // Pick the entry closest to each centroid.
    let mut best: Vec<Option<(usize, f32)>> = vec![None; k];
    for (i, &cluster) in assignments.iter().enumerate() {
        let dist = euclidean_sq(&vecs[i], &centroids[cluster]);
        match best[cluster] {
            None => best[cluster] = Some((i, dist)),
            Some((_, best_dist)) if dist < best_dist => best[cluster] = Some((i, dist)),
            _ => {}
        }
    }

    best.into_iter().flatten().map(|(i, _)| i).collect()
}

/// Squared Euclidean distance between two equal-length float vectors.
fn euclidean_sq(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    a[..len]
        .iter()
        .zip(b[..len].iter())
        .map(|(x, y)| (x - y) * (x - y))
        .sum()
}

/// Fallback compactor: sort by significance descending, take top-N.
fn fallback_compact(entries: &[StorageEntry], target_count: usize) -> Vec<StorageEntry> {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| {
        b.significance
            .partial_cmp(&a.significance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted.truncate(target_count);
    sorted
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::cast_precision_loss)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;

    fn make_entry(sig: f64, content: &str) -> StorageEntry {
        StorageEntry {
            id: Uuid::new_v4().to_string(),
            path: format!("test/{content}.md"),
            sibling: "test".into(),
            date: None,
            entry_type: Some("context".into()),
            significance: sig,
            self_defining: false,
            epoch: None,
            strands: Vec::new(),
            resonance: Vec::new(),
            themes: Vec::new(),
            title: Some(content.to_owned()),
            content: content.to_owned(),
            frontmatter: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_significance_compactor_picks_top() {
        let entries = vec![
            make_entry(5.0, "medium"),
            make_entry(9.0, "high"),
            make_entry(1.0, "low"),
        ];
        let c = SignificanceCompactor::default();
        let result = c.compact(&entries, 2);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].title.as_deref(), Some("high"));
    }

    #[test]
    fn test_significance_compactor_empty() {
        let c = SignificanceCompactor::default();
        assert!(c.compact(&[], 5).is_empty());
    }

    #[test]
    fn test_significance_compactor_target_zero() {
        let entries = vec![make_entry(9.0, "entry")];
        let c = SignificanceCompactor::default();
        assert!(c.compact(&entries, 0).is_empty());
    }

    #[test]
    fn test_temporal_compactor_most_recent_wins() {
        // All entries created at the same time in tests; just verify count.
        let entries: Vec<StorageEntry> = (0..5).map(|i| make_entry(f64::from(i), "e")).collect();
        let c = TemporalCompactor::default();
        let result = c.compact(&entries, 3);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_significance_compactor_fewer_than_target() {
        let entries = vec![make_entry(9.0, "only one")];
        let c = SignificanceCompactor::default();
        let result = c.compact(&entries, 10);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_euclidean_sq_zero_for_identical() {
        let v = vec![1.0f32, 2.0, 3.0];
        assert!((euclidean_sq(&v, &v)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_kmeans_pick_returns_k_or_fewer() {
        let vecs: Vec<Vec<f32>> = (0..10).map(|i| vec![i as f32, (i * 2) as f32]).collect();
        let result = kmeans_pick(&vecs, 3, 100);
        assert!(result.len() <= 3);
    }
}
