//! BGE-projected structural embeddings pipeline.
//!
//! Reads BGE-384 embeddings already stored on `Step` nodes (written by the
//! semantic embedding stage), projects each through the two-layer `ReLU` MLP
//! defined by [`ProjectionWeights`], and writes the resulting 128-dim vector
//! back as `sage_embedding` — ensuring the query-time [`StructuralSearcher`]
//! and the `step-sage-embeddings` HNSW index live in the same vector space.
//!
//! This replaces the GDS `graphSage.train()` approach, which used `significance`
//! (a scalar node property) as its sole feature and produced vectors incompatible
//! with the BGE-MLP query encoder.
//!
//! [`StructuralSearcher`]: crate::helix::soul_search::StructuralSearcher

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tracing::{debug, instrument, warn};

use super::graphsage::ProjectionWeights;
use crate::helix::db::{HelixDb, HelixDbError};

/// Steps fetched per Cypher SKIP/LIMIT page.
const PAGE_SIZE: usize = 500;

/// Projects stored BGE-384 embeddings to 128-dim `sage_embedding` vectors.
///
/// Instantiate via [`BgeSageProjectionPipeline::load_or_default`], then call
/// [`BgeSageProjectionPipeline::project_all`] with a connected [`HelixDb`].
///
/// The pipeline paginates through Neo4j to avoid loading the entire vault
/// into memory at once, making it safe for vaults of any size.
pub struct BgeSageProjectionPipeline {
    weights: ProjectionWeights,
}

impl BgeSageProjectionPipeline {
    /// Load projection weights from `path`, falling back to random-stable weights.
    ///
    /// See [`ProjectionWeights::load_or_default`] for fallback behaviour.
    #[must_use]
    pub fn load_or_default(path: &Path) -> Self {
        Self {
            weights: ProjectionWeights::load_or_default(path),
        }
    }

    /// Return the canonical path for `sage_projection.bin`.
    ///
    /// Uses `$HOME` rather than `dirs-next` because the consolidator crate
    /// does not declare that dependency.
    #[must_use]
    pub fn default_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
        PathBuf::from(home)
            .join(".lightarchitects")
            .join("sage_projection.bin")
    }

    /// Project all Steps with a BGE embedding to 128-dim `sage_embedding`.
    ///
    /// Pages through Neo4j in batches of [`PAGE_SIZE`] to avoid OOM on large
    /// vaults. Returns the total number of Step nodes written.
    ///
    /// # Errors
    ///
    /// Returns [`HelixDbError`] on any Neo4j query or write failure.
    #[instrument(skip(self, db), fields(page_size = PAGE_SIZE))]
    pub async fn project_all(&self, db: &dyn HelixDb) -> Result<usize, HelixDbError> {
        let mut total = 0usize;
        let mut skip = 0usize;

        loop {
            let updated = self.project_page(db, skip, PAGE_SIZE).await?;
            total = total.saturating_add(updated);
            if updated < PAGE_SIZE {
                break;
            }
            skip = skip.saturating_add(PAGE_SIZE);
        }

        debug!(
            total_projected = total,
            "BgeSageProjectionPipeline complete"
        );
        Ok(total)
    }

    #[instrument(skip(self, db), fields(skip, limit))]
    async fn project_page(
        &self,
        db: &dyn HelixDb,
        skip: usize,
        limit: usize,
    ) -> Result<usize, HelixDbError> {
        // Parameterized Cypher — no user input reaches this query.
        let cypher = "MATCH (s:Step) WHERE s.embedding IS NOT NULL \
                      RETURN s.id AS id, s.embedding AS embedding \
                      SKIP $skip LIMIT $limit";

        let skip_i64 = i64::try_from(skip)
            .map_err(|_| HelixDbError::Validation("skip value overflows i64".into()))?;
        let limit_i64 = i64::try_from(limit)
            .map_err(|_| HelixDbError::Validation("limit value overflows i64".into()))?;

        let mut params = BTreeMap::new();
        params.insert("skip".into(), serde_json::json!(skip_i64));
        params.insert("limit".into(), serde_json::json!(limit_i64));

        let records = db.execute_cypher_with_params(cypher, params).await?;

        let mut count = 0usize;
        for record in &records {
            let Some(id) = record.get("id").and_then(serde_json::Value::as_str) else {
                warn!("BGE sage pipeline: record missing id — skipping");
                continue;
            };

            let Some(bge_vec) = extract_f32_array(record.get("embedding")) else {
                warn!(
                    step_id = id,
                    "BGE sage pipeline: cannot extract embedding — skipping"
                );
                continue;
            };

            let projected = self.weights.project(&bge_vec);
            db.set_step_sage_embedding(id, &projected).await?;
            count = count.saturating_add(1);
        }

        Ok(count)
    }
}

/// Extract a `Vec<f32>` from a Neo4j JSON value (array of numbers).
///
/// BGE embeddings are stored as JSON arrays of f64 (widened from f32 at write
/// time). The narrowing cast back to f32 is intentional — precision loss is
/// negligible for cosine-similarity distance computations.
#[allow(clippy::cast_possible_truncation)]
fn extract_f32_array(value: Option<&serde_json::Value>) -> Option<Vec<f32>> {
    let arr = value?.as_array()?;
    if arr.is_empty() {
        return None;
    }
    arr.iter()
        .map(|v| v.as_f64().map(|f| f as f32))
        .collect::<Option<Vec<f32>>>()
}
