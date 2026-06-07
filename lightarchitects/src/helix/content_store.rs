//! Two-tier in-process step retrieval cache.
//!
//! # Tier 1 — Step ID cache
//!
//! [`HashMap<String, Arc<Step>>`][HashMap] keyed by step ID. Eliminates the Neo4j
//! Bolt round-trip required to hydrate steps after an in-process ANN search.
//! Populated lazily: [`get_batch`][ContentStore::get_batch] returns misses that
//! the caller fetches from Neo4j and re-inserts. Evicted write-through: any
//! [`upsert_step`][`crate::helix::db::HelixNeo4j`] call removes the stale entry.
//!
//! # Tier 2 — Traversal memoization
//!
//! [`HashMap<TraversalKey, Vec<Arc<Step>>>`][HashMap] keyed by a SHA-256 hash of
//! `(root_step_id, depth, edge_filter)`. Populated by graph traversal callers.
//! Evicted when any step in the result set is written.
//!
//! # Boundary rule
//!
//! | Scenario | Path |
//! |---|---|
//! | Step IDs known after ANN (stable content) | Tier 1, ~0µs |
//! | Repeated traversal from same root + depth | Tier 2, ~0µs |
//! | Step content written (`upsert_step`) | Evict Tier 1 + affected Tier 2 |
//! | First traversal of any path | Neo4j → populate Tier 2 on return |

use std::collections::HashMap;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::helix::types::Step;

/// 32-byte SHA-256 traversal cache key.
///
/// Hash inputs (null-byte separated): `root_step_id \x00 depth_le4 \x00 edge_filter`.
pub type TraversalKey = [u8; 32];

/// Compute a [`TraversalKey`] from the root step ID, traversal depth, and edge filter.
#[must_use]
pub fn traversal_key(root_step_id: &str, depth: u32, edge_filter: &str) -> TraversalKey {
    let mut hasher = Sha256::new();
    hasher.update(root_step_id.as_bytes());
    hasher.update(b"\x00");
    hasher.update(depth.to_le_bytes());
    hasher.update(b"\x00");
    hasher.update(edge_filter.as_bytes());
    hasher.finalize().into()
}

/// Two-tier in-process retrieval cache for helix [`Step`] objects.
#[derive(Default)]
pub struct ContentStore {
    /// Tier 1: `step_id` → cached Step.
    step_cache: HashMap<String, Arc<Step>>,
    /// Tier 2: traversal results keyed by traversal signature.
    traversal_cache: HashMap<TraversalKey, Vec<Arc<Step>>>,
    /// Reverse index for Tier 2 eviction: `step_id` → traversal keys that contain it.
    id_to_traversal_keys: HashMap<String, Vec<TraversalKey>>,
}

impl ContentStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) a step in Tier 1.
    pub fn insert(&mut self, step: Arc<Step>) {
        self.step_cache.insert(step.id.clone(), step);
    }

    /// Look up a single step by ID in Tier 1. Returns `None` on miss.
    #[must_use]
    pub fn get(&self, step_id: &str) -> Option<Arc<Step>> {
        self.step_cache.get(step_id).cloned()
    }

    /// Batch Tier 1 lookup.
    ///
    /// Returns `(found, missed)` where `found` contains all cached
    /// [`Arc<Step>`] hits and `missed` contains the IDs that were absent.
    /// Callers should fetch missed IDs from Neo4j then call [`insert`][Self::insert]
    /// for each returned step.
    #[must_use]
    pub fn get_batch<'a>(&self, ids: &'a [String]) -> (Vec<Arc<Step>>, Vec<&'a str>) {
        let mut found = Vec::with_capacity(ids.len());
        let mut missed = Vec::new();
        for id in ids {
            match self.step_cache.get(id.as_str()) {
                Some(step) => found.push(Arc::clone(step)),
                None => missed.push(id.as_str()),
            }
        }
        (found, missed)
    }

    /// Evict a step from Tier 1 and from every Tier 2 traversal that includes it.
    ///
    /// Call on every `upsert_step` write to prevent stale cache reads.
    pub fn evict(&mut self, step_id: &str) {
        self.step_cache.remove(step_id);
        if let Some(tkeys) = self.id_to_traversal_keys.remove(step_id) {
            for key in tkeys {
                self.traversal_cache.remove(&key);
            }
        }
    }

    /// Store traversal results in Tier 2.
    ///
    /// `step_ids` must enumerate every step ID in `results` so the reverse
    /// eviction index is fully populated.
    pub fn insert_traversal(
        &mut self,
        key: TraversalKey,
        results: Vec<Arc<Step>>,
        step_ids: &[String],
    ) {
        for id in step_ids {
            self.id_to_traversal_keys
                .entry(id.clone())
                .or_default()
                .push(key);
        }
        self.traversal_cache.insert(key, results);
    }

    /// Look up traversal results in Tier 2. Returns `None` on miss.
    #[must_use]
    pub fn get_traversal(&self, key: &TraversalKey) -> Option<&[Arc<Step>]> {
        self.traversal_cache.get(key).map(Vec::as_slice)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn make_step(id: &str) -> Arc<Step> {
        Arc::new(Step {
            id: id.to_owned(),
            helix_id: "helix-a".to_owned(),
            title: None,
            content: format!("content of {id}"),
            significance: 5.0,
            step_date: None,
            step_index: None,
            community_id: None,
            expires: None,
            created_at: chrono::Utc::now(),
            metadata: serde_json::Value::Object(serde_json::Map::default()),
            vault_path: None,
            graph_embedding: None,
        })
    }

    #[test]
    fn tier1_insert_and_get() {
        let mut store = ContentStore::new();
        let step = make_step("step-1");
        store.insert(Arc::clone(&step));
        let got = store.get("step-1").expect("should be cached");
        assert_eq!(got.id, "step-1");
    }

    #[test]
    fn tier1_miss_returns_none() {
        let store = ContentStore::new();
        assert!(store.get("step-missing").is_none());
    }

    #[test]
    fn get_batch_splits_hit_and_miss() {
        let mut store = ContentStore::new();
        store.insert(make_step("a"));
        store.insert(make_step("b"));
        let ids = vec!["a".to_owned(), "c".to_owned(), "b".to_owned()];
        let (found, missed) = store.get_batch(&ids);
        assert_eq!(found.len(), 2);
        assert_eq!(missed, vec!["c"]);
    }

    #[test]
    fn evict_removes_from_tier1() {
        let mut store = ContentStore::new();
        store.insert(make_step("step-1"));
        assert!(store.get("step-1").is_some());
        store.evict("step-1");
        assert!(store.get("step-1").is_none());
    }

    #[test]
    fn evict_cascades_to_tier2() {
        let mut store = ContentStore::new();
        let step = make_step("step-1");
        store.insert(Arc::clone(&step));
        let key = traversal_key("step-1", 2, "");
        store.insert_traversal(key, vec![Arc::clone(&step)], &["step-1".to_owned()]);
        assert!(store.get_traversal(&key).is_some());
        store.evict("step-1");
        assert!(
            store.get_traversal(&key).is_none(),
            "Tier 2 should be evicted"
        );
    }

    #[test]
    fn traversal_key_is_deterministic() {
        let k1 = traversal_key("root", 3, "HAS_STEP");
        let k2 = traversal_key("root", 3, "HAS_STEP");
        assert_eq!(k1, k2);
    }

    #[test]
    fn traversal_key_differs_on_depth() {
        let k1 = traversal_key("root", 2, "");
        let k2 = traversal_key("root", 3, "");
        assert_ne!(k1, k2);
    }
}
