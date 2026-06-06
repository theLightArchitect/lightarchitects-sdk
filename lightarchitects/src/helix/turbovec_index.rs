//! In-process `TurboQuant` semantic index for helix step embeddings.
//!
//! Wraps [`turbovec::TurboQuantIndex`] with the slot-to-step-id and
//! helix-to-slots mappings needed to translate turbovec's `i64` slot
//! indices back to SOUL's string step IDs, and to restrict ANN search
//! to a single helix via [`TurboQuantIndex::search_with_mask`].
//!
//! # Memory profile (nomic-embed-text, 768-dim, 4-bit)
//!
//! | Vectors | Index | Rotation matrix |
//! |---------|-------|-----------------|
//! | 50 K    | ~20 MB | 2.4 MB (shared) |
//! | 200 K   | ~80 MB | 2.4 MB (shared) |
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
    /// Maps `helix_id` → [slot indices] for masked per-helix ANN search.
    helix_slots: HashMap<String, Vec<usize>>,
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

    /// Helix-scoped ANN search via boolean mask.
    ///
    /// Constructs a `bool` mask covering only the slots that belong to
    /// `helix_id` and calls [`TurboQuantIndex::search_with_mask`]. This
    /// replaces the O(n) `vector.similarity.cosine` Cypher scan with an
    /// in-process SIMD scan over the compressed codes — no Bolt round-trip.
    ///
    /// Returns an empty vec if the helix is unknown or has no embeddings.
    pub fn search_helix(&self, query: &[f32], k: usize, helix_id: &str) -> Vec<(f32, String)> {
        let slots = match self.helix_slots.get(helix_id) {
            Some(s) if !s.is_empty() => s,
            _ => return Vec::new(),
        };
        let n = self.inner.len();
        let mut mask = vec![false; n];
        for &s in slots {
            if s < n {
                mask[s] = true;
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

    /// Eagerly warm the internal SIMD/rotation caches.
    ///
    /// Call once after bulk-loading all embeddings at startup. Pays the
    /// `OnceLock` initialisation cost here rather than on the first query.
    pub fn prepare(&self) {
        self.inner.prepare();
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
