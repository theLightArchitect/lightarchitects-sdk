//! In-process `TurboQuant` semantic index for helix step embeddings.
//!
//! Wraps [`turbovec::TurboQuantIndex`] with:
//! - Slot↔ID and helix-slot mappings for translating turbovec's `i64` slot
//!   indices back to SOUL's string step IDs.
//! - A **per-helix fleet** of micro-indexes — one `TurboQuantIndex` per helix,
//!   holding only that helix's vectors. `search_helix` hits the fleet directly
//!   (O(|helix|) SIMD scan, no mask allocation) instead of constructing a global
//!   boolean mask over all N vectors.
//!
//! # Memory profile (nomic-embed-text, 768-dim, 4-bit)
//!
//! | Vectors | Global index | Per-helix fleet (500 vecs/helix) |
//! |---------|-------------|----------------------------------|
//! | 50 K    | ~20 MB      | ~192 KB per helix (L2-resident)  |
//! | 200 K   | ~80 MB      | ~192 KB per helix (L2-resident)  |
//!
//! The fleet reduces the helix-search working set ~100× (19 MB → 192 KB),
//! fitting comfortably in L2 cache (~512 KB/core on M4 Pro).
//!
//! # Thread safety
//!
//! [`TurboVecIndex`] is `Send + Sync`. `search*` methods take `&self`
//! (`TurboQuant` uses `OnceLock` internally); mutation flows through
//! `&mut self`. Wrap in `tokio::sync::RwLock` for concurrent tokio access.

use std::collections::HashMap;

use turbovec::{ConstructError, TurboQuantIndex};

/// Embedding dimension for nomic-embed-text. Must be a positive multiple of 8.
pub const HELIX_DIM: usize = 768;

/// Quantisation bit-width. 4-bit gives ~7.8× compression at >99% Recall@5.
pub const BIT_WIDTH: usize = 4;

/// In-process `TurboQuant` semantic index carrying the slot↔ID mappings
/// required to integrate with SOUL's string-keyed step graph.
pub struct TurboVecIndex {
    inner: TurboQuantIndex,
    /// Maps turbovec slot index → SOUL step ID string.
    slot_to_step_id: Vec<String>,
    /// Maps SOUL step ID → turbovec slot index (for idempotent upsert).
    step_id_to_slot: HashMap<String, usize>,
    /// Maps `helix_id` → [slot indices] for masked per-helix ANN search (fallback).
    helix_slots: HashMap<String, Vec<usize>>,
    /// Contiguous slot ranges `[start, end)` computed by `prepare()` for
    /// helixes whose slots are fully consecutive (i.e. bulk-loaded in
    /// `helix_id` order via `fetch_all_embeddings ORDER BY helix_id`).
    /// Used in the masked fallback path of `search_helix`.
    /// Cleared on any post-prepare `upsert` for the affected helix.
    helix_ranges: HashMap<String, (usize, usize)>,
    /// Per-helix micro-index fleet.
    ///
    /// Each entry holds only the vectors belonging to that helix, keyed by
    /// helix-local slot index. `search_helix` hits the fleet directly
    /// (no mask construction, no global index scan) when the fleet is warmed.
    /// Built incrementally by dual-writing in `upsert()`.
    fleet: HashMap<String, TurboQuantIndex>,
    /// Per-helix fleet slot → step ID (local numbering, independent of global slots).
    fleet_slot_ids: HashMap<String, Vec<String>>,
}

impl TurboVecIndex {
    /// Create a fresh empty index (768-dim, 4-bit).
    ///
    /// # Errors
    ///
    /// Returns [`ConstructError`] if the dim/bit-width constraints are
    /// violated (they are constants here, so this should never trigger).
    pub fn new() -> Result<Self, ConstructError> {
        Ok(Self {
            inner: TurboQuantIndex::new(HELIX_DIM, BIT_WIDTH)?,
            slot_to_step_id: Vec::new(),
            step_id_to_slot: HashMap::new(),
            helix_slots: HashMap::new(),
            helix_ranges: HashMap::new(),
            fleet: HashMap::new(),
            fleet_slot_ids: HashMap::new(),
        })
    }

    /// Idempotent upsert — skips if `step_id` is already present.
    ///
    /// Adding duplicate embeddings would shift slot indices and break the
    /// ID maps. The ingest pipeline calls this for every step on startup
    /// and on write-through; idempotency prevents double-insertion on
    /// concurrent restarts.
    ///
    /// # Panics
    ///
    /// `turbovec` panics on non-finite or `|v| ≥ 1e16` values. Callers
    /// should ensure embeddings are L2-normalised or otherwise bounded.
    pub fn upsert(&mut self, step_id: &str, helix_id: &str, embedding: &[f32]) {
        if self.step_id_to_slot.contains_key(step_id) {
            return;
        }
        let slot = self.inner.len();
        self.inner.add(embedding);
        self.slot_to_step_id.push(step_id.to_owned());
        self.step_id_to_slot.insert(step_id.to_owned(), slot);
        self.helix_slots
            .entry(helix_id.to_owned())
            .or_default()
            .push(slot);
        // A new slot after prepare() breaks the cached contiguous range for
        // this helix. Remove it so the masked fallback uses scattered writes.
        self.helix_ranges.remove(helix_id);

        // Dual-write to the per-helix fleet micro-index.
        // Fleet slots are independently numbered (0, 1, ...) per helix so
        // fleet_slot_ids[helix_id][fleet_slot] → step_id.
        let fleet_idx = match self.fleet.entry(helix_id.to_owned()) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                match TurboQuantIndex::new(HELIX_DIM, BIT_WIDTH) {
                    Ok(idx) => e.insert(idx),
                    // HELIX_DIM=768 and BIT_WIDTH=4 are compile-time constants that
                    // satisfy all TurboQuantIndex constraints — this branch is unreachable.
                    Err(_) => return,
                }
            }
        };
        fleet_idx.add(embedding);
        self.fleet_slot_ids
            .entry(helix_id.to_owned())
            .or_default()
            .push(step_id.to_owned());
    }

    /// Global ANN search — no helix filter.
    ///
    /// Returns up to `k` `(score, step_id)` pairs sorted by descending
    /// cosine similarity. Returns an empty vec if the index is empty.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(f32, String)> {
        if self.inner.is_empty() {
            return Vec::new();
        }
        let res = self.inner.search(query, k);
        res.indices_for_query(0)
            .iter()
            .zip(res.scores_for_query(0))
            .filter_map(|(&slot, &score)| {
                usize::try_from(slot)
                    .ok()
                    .and_then(|s| self.slot_to_step_id.get(s).map(|id| (score, id.clone())))
            })
            .collect()
    }

    /// Helix-scoped ANN search.
    ///
    /// **Fast path (fleet)**: if the per-helix micro-index is warmed, searches
    /// only the vectors belonging to `helix_id` — O(|helix|) SIMD scan with no
    /// mask allocation and no global index traversal. The fleet working set is
    /// ~192 KB per helix (500 vecs × 768-dim × 4-bit), fitting in L2 cache.
    ///
    /// **Fallback (global masked scan)**: if the fleet has no entry for this helix
    /// (shouldn't happen after startup bulk-load), falls back to constructing a
    /// boolean mask over the global index and calling `search_with_mask`.
    ///
    /// Returns an empty vec if the helix is unknown or has no embeddings.
    pub fn search_helix(&self, query: &[f32], k: usize, helix_id: &str) -> Vec<(f32, String)> {
        // Fast path: hit the per-helix micro-index directly (no mask needed).
        if let Some((fleet_idx, slot_ids)) = self
            .fleet
            .get(helix_id)
            .zip(self.fleet_slot_ids.get(helix_id))
        {
            if !fleet_idx.is_empty() {
                let res = fleet_idx.search(query, k);
                return res
                    .indices_for_query(0)
                    .iter()
                    .zip(res.scores_for_query(0))
                    .filter_map(|(&slot, &score)| {
                        usize::try_from(slot)
                            .ok()
                            .and_then(|s| slot_ids.get(s).map(|id| (score, id.clone())))
                    })
                    .collect();
            }
        }

        // Fallback: global masked scan (helix not in fleet — e.g., post-load upsert
        // before next prepare() or when turbovec-semantic is partially initialised).
        let slots = match self.helix_slots.get(helix_id) {
            Some(s) if !s.is_empty() => s,
            _ => return Vec::new(),
        };
        let n = self.inner.len();
        let mut mask = vec![false; n];
        if let Some(&(start, end)) = self.helix_ranges.get(helix_id) {
            mask[start..end.min(n)].fill(true);
        } else {
            for &s in slots {
                if s < n {
                    mask[s] = true;
                }
            }
        }
        let res = self.inner.search_with_mask(query, k, Some(&mask));
        res.indices_for_query(0)
            .iter()
            .zip(res.scores_for_query(0))
            .filter_map(|(&slot, &score)| {
                usize::try_from(slot)
                    .ok()
                    .and_then(|s| self.slot_to_step_id.get(s).map(|id| (score, id.clone())))
            })
            .collect()
    }

    /// Eagerly warm the internal SIMD/rotation caches and compute contiguous
    /// helix slot ranges for fast mask construction in `search_helix`.
    ///
    /// Call once after bulk-loading all embeddings at startup. Pays the
    /// `OnceLock` initialisation cost here rather than on the first query.
    /// Also identifies helixes whose slots are fully contiguous (produced by
    /// `fetch_all_embeddings ORDER BY helix_id`) and caches their `[start, end)`
    /// range — these use `fill(true)` instead of scattered writes per query.
    pub fn prepare(&mut self) {
        self.inner.prepare();
        self.helix_ranges.clear();
        for (helix_id, slots) in &self.helix_slots {
            if slots.is_empty() {
                continue;
            }
            let min = slots.iter().copied().min().unwrap_or(0);
            let max = slots.iter().copied().max().unwrap_or(0);
            if max - min + 1 == slots.len() {
                self.helix_ranges.insert(helix_id.clone(), (min, max + 1));
            }
        }
        // Warm SIMD caches in all fleet micro-indexes.
        for fleet_idx in self.fleet.values_mut() {
            fleet_idx.prepare();
        }
    }

    /// Number of vectors in the index.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// `true` if the index contains no vectors.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]
mod tests {
    use super::*;

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// L2-normalised one-hot vector. Cosine similarity between two one-hots
    /// with different hot indices is 0.0; with the same index it is 1.0.
    fn one_hot(hot: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; HELIX_DIM];
        v[hot] = 1.0;
        v
    }

    /// Deterministic pseudo-random unit vector — used for recall tests.
    /// Uses a simple LCG so there are no external rand dependencies in tests.
    fn pseudo_unit(seed: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(HELIX_DIM);
        let mut x = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        for _ in 0..HELIX_DIM {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            v.push(((x >> 33) as f32) / (u32::MAX as f32) * 2.0 - 1.0);
        }
        l2_normalize(&mut v);
        v
    }

    fn l2_normalize(v: &mut [f32]) {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-9 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
    }

    /// Build a populated + prepared index with `n` pseudo-random vectors
    /// distributed across `n_helixes` helixes round-robin.
    fn populated(n: usize, n_helixes: usize) -> (TurboVecIndex, Vec<Vec<f32>>) {
        let mut idx = TurboVecIndex::new().unwrap();
        let mut vecs = Vec::with_capacity(n);
        for i in 0..n {
            let v = pseudo_unit(i);
            idx.upsert(
                &format!("step-{i}"),
                &format!("helix-{}", i % n_helixes),
                &v,
            );
            vecs.push(v);
        }
        idx.prepare();
        (idx, vecs)
    }

    // ── Empty-index edge cases ────────────────────────────────────────────────

    #[test]
    fn empty_global_search_returns_empty() {
        let idx = TurboVecIndex::new().unwrap();
        assert!(idx.search(&one_hot(0), 10).is_empty());
    }

    #[test]
    fn empty_helix_search_returns_empty() {
        let idx = TurboVecIndex::new().unwrap();
        assert!(idx.search_helix(&one_hot(0), 10, "helix-a").is_empty());
    }

    #[test]
    fn is_empty_and_len_on_new_index() {
        let idx = TurboVecIndex::new().unwrap();
        assert!(idx.is_empty());
        assert_eq!(idx.len(), 0);
    }

    // ── Upsert correctness ────────────────────────────────────────────────────

    #[test]
    fn upsert_increments_len() {
        let mut idx = TurboVecIndex::new().unwrap();
        idx.upsert("step-0", "h", &one_hot(0));
        assert_eq!(idx.len(), 1);
        assert!(!idx.is_empty());
        idx.upsert("step-1", "h", &one_hot(1));
        assert_eq!(idx.len(), 2);
    }

    #[test]
    fn upsert_is_idempotent_same_step_id() {
        let mut idx = TurboVecIndex::new().unwrap();
        idx.upsert("step-0", "h", &one_hot(0));
        idx.upsert("step-0", "h", &one_hot(0)); // duplicate
        idx.upsert("step-0", "h", &one_hot(1)); // duplicate with different vec
        assert_eq!(idx.len(), 1, "duplicate upserts must not grow the index");
    }

    #[test]
    fn upsert_after_prepare_is_accepted() {
        let mut idx = TurboVecIndex::new().unwrap();
        idx.upsert("step-0", "h", &one_hot(0));
        idx.prepare();
        idx.upsert("step-1", "h", &one_hot(1));
        assert_eq!(idx.len(), 2);
        // Index is still searchable after post-prepare insertion.
        let hits = idx.search(&one_hot(0), 2);
        assert!(!hits.is_empty());
    }

    // ── Search ordering ───────────────────────────────────────────────────────

    #[test]
    fn global_search_scores_are_descending() {
        let (idx, _) = populated(50, 1);
        let q = pseudo_unit(99_999);
        let hits = idx.search(&q, 20);
        assert!(!hits.is_empty());
        for w in hits.windows(2) {
            assert!(
                w[0].0 >= w[1].0,
                "scores not descending: {} > {}",
                w[0].0,
                w[1].0
            );
        }
    }

    #[test]
    fn exact_vector_ranks_first() {
        let mut idx = TurboVecIndex::new().unwrap();
        // Use one-hot vectors so cosine = 1.0 for exact match, 0.0 for others.
        for i in 0..20 {
            idx.upsert(&format!("step-{i}"), "h", &one_hot(i));
        }
        idx.prepare();

        let hits = idx.search(&one_hot(5), 20);
        assert_eq!(hits[0].1, "step-5", "exact match must be top-ranked");
    }

    #[test]
    fn k_larger_than_n_returns_at_most_n_results() {
        let mut idx = TurboVecIndex::new().unwrap();
        idx.upsert("a", "h", &one_hot(0));
        idx.upsert("b", "h", &one_hot(1));
        idx.prepare();

        let hits = idx.search(&one_hot(0), 1000);
        assert!(hits.len() <= 2, "cannot return more results than vectors");
    }

    // ── Helix isolation ───────────────────────────────────────────────────────

    #[test]
    fn helix_search_returns_only_target_helix() {
        let mut idx = TurboVecIndex::new().unwrap();
        for i in 0..10 {
            idx.upsert(&format!("a-{i}"), "helix-a", &one_hot(i));
        }
        for i in 10..20 {
            idx.upsert(&format!("b-{i}"), "helix-b", &one_hot(i));
        }
        idx.prepare();

        let hits = idx.search_helix(&one_hot(0), 20, "helix-a");
        assert!(!hits.is_empty());
        for (_, id) in &hits {
            assert!(
                id.starts_with("a-"),
                "helix-b step leaked into helix-a results: {id}"
            );
        }

        let hits_b = idx.search_helix(&one_hot(10), 20, "helix-b");
        for (_, id) in &hits_b {
            assert!(
                id.starts_with("b-"),
                "helix-a step leaked into helix-b results: {id}"
            );
        }
    }

    #[test]
    fn helix_search_unknown_helix_returns_empty() {
        let (idx, vecs) = populated(20, 2);
        let hits = idx.search_helix(&vecs[0], 10, "helix-does-not-exist");
        assert!(hits.is_empty());
    }

    #[test]
    fn helix_search_scores_are_descending() {
        let (idx, _) = populated(100, 4);
        let q = pseudo_unit(12345);
        let hits = idx.search_helix(&q, 25, "helix-0");
        for w in hits.windows(2) {
            assert!(w[0].0 >= w[1].0, "helix search scores not descending");
        }
    }

    // ── Range-based fast path ─────────────────────────────────────────────────

    /// Verify that `prepare()` detects contiguous ranges: helixes inserted in
    /// sorted order should have `helix_ranges` populated; a write-through
    /// upsert after prepare must still return correct results (fallback path).
    #[test]
    fn prepare_detects_contiguous_ranges_and_fallback_correct() {
        let mut idx = TurboVecIndex::new().unwrap();
        // Helix-a: slots 0..5 (contiguous).
        for i in 0..5 {
            idx.upsert(&format!("a-{i}"), "helix-a", &one_hot(i));
        }
        // Helix-b: slots 5..10 (contiguous).
        for i in 0..5 {
            idx.upsert(&format!("b-{i}"), "helix-b", &one_hot(i + 5));
        }
        idx.prepare();

        // After prepare(), both helixes should have contiguous ranges.
        assert!(
            idx.helix_ranges.contains_key("helix-a"),
            "helix-a slots 0..5 are contiguous — range should be cached"
        );
        assert!(
            idx.helix_ranges.contains_key("helix-b"),
            "helix-b slots 5..10 are contiguous — range should be cached"
        );

        // Write-through upsert on helix-a after prepare() invalidates its range.
        idx.upsert("a-new", "helix-a", &one_hot(20));
        assert!(
            !idx.helix_ranges.contains_key("helix-a"),
            "post-prepare upsert must evict the cached range"
        );
        // helix-b range unchanged.
        assert!(idx.helix_ranges.contains_key("helix-b"));

        // Correctness: helix-a must still return only its own steps.
        idx.prepare(); // re-warm for search
        let hits = idx.search_helix(&one_hot(0), 10, "helix-a");
        assert!(!hits.is_empty());
        for (_, id) in &hits {
            assert!(id.starts_with("a-"), "helix-b leaked: {id}");
        }
    }

    // ── Recall@5 property ─────────────────────────────────────────────────────
    //
    // Validates that TurboQuant 4-bit compression retains enough fidelity for
    // clustered data — the regime that matches real SOUL helix workloads.
    //
    // WHY a structured corpus instead of random vectors:
    // In 768-dim space, random unit vectors have cosine similarities in
    // [-1/√768, 1/√768] ≈ ±0.036. The gap between rank-5 and rank-6 is
    // ~0.01 — smaller than 4-bit quantisation noise. Text embeddings live in
    // ~50–100 effective semantic dimensions; the top-5 gap is much larger and
    // 4-bit TurboQuant achieves ≥ 96.2% recall (LongMemEval, 23 854 steps).
    //
    // The test uses 50 semantic clusters × 10 members each. Each cluster center
    // is a one-hot basis vector; members are the center plus a small perturbation
    // (cosine sim to center ≈ 0.995). Background vectors (400) occupy distinct
    // one-hot directions far from any query, making the ranking gap large enough
    // to survive quantisation. Expected recall@5: 100% for well-separated clusters.

    /// Build a cluster member: center + `noise_scale` × pseudorandom perturbation.
    fn make_cluster_member(center_dim: usize, member_idx: usize, noise_scale: f32) -> Vec<f32> {
        let noise = pseudo_unit(center_dim * 10_000 + member_idx);
        let mut v = vec![0.0f32; HELIX_DIM];
        v[center_dim] = 1.0;
        for (vi, ni) in v.iter_mut().zip(noise.iter()) {
            *vi += noise_scale * ni;
        }
        l2_normalize(&mut v);
        v
    }

    #[test]
    fn recall_at_5_above_90_percent_clustered_corpus() {
        const N_CLUSTERS: usize = 50; // 50 semantic topics
        const MEMBERS_PER_CLUSTER: usize = 10; // 10 texts per topic
        const N_BACKGROUND: usize = 400; // random background noise
        const NOISE_SCALE: f32 = 0.05; // members stay within ~5° of center
        const K: usize = 5;
        const MIN_RECALL: f64 = 0.90;

        let mut idx = TurboVecIndex::new().unwrap();

        // Insert cluster members (cluster i uses basis vector at dim i).
        for c in 0..N_CLUSTERS {
            for m in 0..MEMBERS_PER_CLUSTER {
                let v = make_cluster_member(c, m, NOISE_SCALE);
                idx.upsert(&format!("c{c}-m{m}"), "helix-a", &v);
            }
        }
        // Insert background vectors at dims N_CLUSTERS..N_CLUSTERS+N_BACKGROUND.
        for b in 0..N_BACKGROUND {
            let v = one_hot(N_CLUSTERS + b);
            idx.upsert(&format!("bg-{b}"), "helix-a", &v);
        }
        idx.prepare();

        // Query: exact cluster center (one-hot) — top-5 must be cluster members.
        let mut total_recall = 0.0f64;
        for c in 0..N_CLUSTERS {
            let query = one_hot(c);
            let ann = idx.search(&query, K);
            let hits = ann
                .iter()
                .filter(|(_, id)| id.starts_with(&format!("c{c}-")))
                .count();
            total_recall += hits as f64 / K as f64;
        }

        let mean_recall = total_recall / N_CLUSTERS as f64;
        assert!(
            mean_recall >= MIN_RECALL,
            "Recall@5 = {mean_recall:.3} on clustered corpus is below {MIN_RECALL}. \
             This indicates 4-bit compression is scrambling well-separated neighbors."
        );
    }
}
