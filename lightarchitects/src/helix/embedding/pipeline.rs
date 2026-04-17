//! Embedding pipelines — semantic and structural vector generation.
//!
//! - **`SemanticEmbeddingPipeline`**: chunk → embed → mean-pool → write to Neo4j
//! - **`StructuralEmbeddingPipeline`**: GDS `Node2Vec` projection → write to Neo4j

use std::sync::Arc;

use futures_util::{StreamExt as _, stream};
use tracing::{debug, info, instrument, warn};

use crate::helix::db::{HelixDb, HelixDbError};
use crate::helix::types::Step;

use super::chunker::Chunker;
use super::{EmbeddingError, EmbeddingProvider, EmbeddingResult, PrivacyLevel};

// ============================================================================
// Configuration
// ============================================================================

/// Global privacy tier for the embedding pipeline.
///
/// Mirrors `soul::PrivacyTier` without requiring a dependency on the `soul` crate.
/// Callers that load `SoulToml` should convert `soul::PrivacyTier` → this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlobalPrivacyTier {
    /// No cloud operations allowed (fail-safe default).
    #[default]
    Local,
    /// Cloud TTS and embeddings allowed; export blocked.
    Hybrid,
    /// All cloud operations permitted.
    Cloud,
}

impl GlobalPrivacyTier {
    /// Returns `true` if cloud embedding is allowed at this tier.
    ///
    /// Both `Hybrid` and `Cloud` permit cloud embeddings.
    #[must_use]
    pub fn allows_cloud_embed(self) -> bool {
        matches!(self, Self::Hybrid | Self::Cloud)
    }
}

/// Configuration for the embedding pipeline.
#[derive(Debug, Clone)]
pub struct EmbeddingPipelineConfig {
    /// Maximum parallel batches for embedding.
    pub concurrency: usize,
    /// Whether to skip steps that already have embeddings.
    pub skip_existing: bool,
    /// Whether this is a catch-up run (embed all missing).
    pub catch_up: bool,
    /// Global privacy tier from `soul.toml [privacy]`.
    ///
    /// Applied when a step has no per-entry `privacy` metadata field.
    /// Defaults to `Local` (fail-safe — blocks cloud embeddings unless opted in).
    pub global_privacy: GlobalPrivacyTier,
}

impl Default for EmbeddingPipelineConfig {
    fn default() -> Self {
        Self {
            concurrency: 4,
            skip_existing: true,
            catch_up: false,
            global_privacy: GlobalPrivacyTier::Local,
        }
    }
}

/// Report from an embedding pipeline run.
#[derive(Debug, Default)]
pub struct EmbeddingReport {
    /// Steps successfully embedded.
    pub embedded: usize,
    /// Steps skipped (already had embeddings).
    pub skipped: usize,
    /// Steps skipped due to privacy gate.
    pub privacy_blocked: usize,
    /// Errors encountered.
    pub errors: Vec<String>,
}

// ============================================================================
// SemanticEmbeddingPipeline
// ============================================================================

/// Orchestrates semantic embedding: chunk → embed → mean-pool → write.
///
/// For each Step:
/// 1. Chunk content at sentence boundaries (512 tokens, 64-token overlap)
/// 2. Batch-embed chunks via the configured provider (768-dim)
/// 3. Mean-pool chunk embeddings into a single vector per Step
/// 4. Write the vector to the Neo4j Step node (triggers HNSW index update)
pub struct SemanticEmbeddingPipeline {
    provider: Arc<dyn EmbeddingProvider>,
    chunker: Chunker,
    config: EmbeddingPipelineConfig,
}

impl SemanticEmbeddingPipeline {
    /// Create a new semantic embedding pipeline.
    #[must_use]
    pub fn new(
        provider: Arc<dyn EmbeddingProvider>,
        chunker: Chunker,
        config: EmbeddingPipelineConfig,
    ) -> Self {
        Self {
            provider,
            chunker,
            config,
        }
    }

    /// Embed a batch of steps, writing vectors to the database.
    ///
    /// Uses a single batch existence check to skip already-embedded steps, then
    /// processes the remaining steps with bounded concurrency via `buffer_unordered`.
    ///
    /// # Errors
    /// Returns `EmbeddingError` if the provider fails or the database write fails.
    #[instrument(skip(self, db, steps), fields(count = steps.len()))]
    pub async fn embed_steps(
        &self,
        db: &dyn HelixDb,
        steps: &[Step],
    ) -> EmbeddingResult<EmbeddingReport> {
        let mut report = EmbeddingReport::default();

        // Single round-trip to find already-embedded steps — replaces N individual queries.
        let steps_to_embed: Vec<&Step> = if self.config.skip_existing && !self.config.catch_up {
            let ids: Vec<String> = steps.iter().map(|s| s.id.clone()).collect();
            let already_embedded = db
                .batch_step_ids_with_embeddings(&ids)
                .await
                .map_err(|e| EmbeddingError::Database(e.to_string()))?;
            report.skipped += already_embedded.len();
            steps
                .iter()
                .filter(|s| !already_embedded.contains(&s.id))
                .collect()
        } else {
            steps.iter().collect()
        };

        // Concurrent embedding — up to config.concurrency futures in flight at once.
        let outcomes: Vec<(String, EmbeddingResult<EmbedOutcome>)> = stream::iter(steps_to_embed)
            .map(|step| self.embed_step_with_id(db, step))
            .buffer_unordered(self.config.concurrency)
            .collect()
            .await;

        for (step_id, outcome) in outcomes {
            match outcome {
                Ok(EmbedOutcome::Embedded) => report.embedded += 1,
                Ok(EmbedOutcome::Skipped) => report.skipped += 1,
                Ok(EmbedOutcome::PrivacyBlocked) => report.privacy_blocked += 1,
                Err(e) => report.errors.push(format!("{step_id}: {e}")),
            }
        }

        info!(
            embedded = report.embedded,
            skipped = report.skipped,
            privacy_blocked = report.privacy_blocked,
            errors = report.errors.len(),
            "Semantic embedding batch complete"
        );

        Ok(report)
    }

    /// Embed a single step and return its ID alongside the outcome for error attribution.
    async fn embed_step_with_id(
        &self,
        db: &dyn HelixDb,
        step: &Step,
    ) -> (String, EmbeddingResult<EmbedOutcome>) {
        let id = step.id.clone();
        (id, self.embed_single_step(db, step).await)
    }

    /// Embed a single step.
    async fn embed_single_step(
        &self,
        db: &dyn HelixDb,
        step: &Step,
    ) -> EmbeddingResult<EmbedOutcome> {
        // Privacy gate — per-entry value wins over global tier.
        //
        // A step is blocked from cloud embedding when:
        //   1. The step's own metadata has `privacy: "local"` or `redacted: true`, OR
        //   2. No per-entry privacy field exists AND the global tier is `Local`.
        //
        // This mirrors `soul::DefaultPrivacyGate::can_embed` logic without
        // requiring a dependency on the `soul` crate.
        let is_cloud_provider = self.provider.name() == "cloud";
        if is_cloud_provider {
            let entry_privacy = PrivacyLevel::from_metadata(&step.metadata);
            let blocked = match entry_privacy {
                // Per-entry `local` / `redacted` always blocks, regardless of global tier.
                PrivacyLevel::LocalOnly => true,
                // No per-entry restriction — apply global tier.
                PrivacyLevel::Standard => !self.config.global_privacy.allows_cloud_embed(),
            };
            if blocked {
                debug!(
                    step_id = %step.id,
                    global_tier = ?self.config.global_privacy,
                    "Privacy gate: cloud embedding blocked"
                );
                return Ok(EmbedOutcome::PrivacyBlocked);
            }
        }

        // Skip if already embedded and not in catch-up mode
        if self.config.skip_existing && !self.config.catch_up {
            let has_embedding = db
                .step_has_embedding(&step.id)
                .await
                .map_err(|e| EmbeddingError::Database(e.to_string()))?;
            if has_embedding {
                return Ok(EmbedOutcome::Skipped);
            }
        }

        // Chunk content
        let chunks = self.chunker.chunk(&step.content);
        if chunks.is_empty() {
            return Ok(EmbedOutcome::Skipped);
        }

        // Embed chunks in batches
        let batch_size = self.provider.max_batch_size();
        let mut all_embeddings = Vec::with_capacity(chunks.len());

        for batch in chunks.chunks(batch_size) {
            let texts: Vec<&str> = batch.iter().map(|c| c.text.as_str()).collect();
            let embeddings = self.provider.embed(&texts).await?;
            all_embeddings.extend(embeddings);
        }

        // Mean-pool into single vector
        let embedding = mean_pool(&all_embeddings);

        // Write to database
        db.set_step_embedding(&step.id, &embedding)
            .await
            .map_err(|e| EmbeddingError::Database(e.to_string()))?;

        Ok(EmbedOutcome::Embedded)
    }

    /// Get the configured concurrency level.
    #[must_use]
    pub fn concurrency(&self) -> usize {
        self.config.concurrency
    }
}

/// Outcome of embedding a single step.
enum EmbedOutcome {
    Embedded,
    Skipped,
    PrivacyBlocked,
}

// ============================================================================
// StructuralEmbeddingPipeline
// ============================================================================

/// GDS `Node2Vec` structural embedding pipeline.
///
/// Runs as part of nightly consolidation (Phase 9).
/// Projects the helix subgraph into GDS, runs `Node2Vec`,
/// writes 128-dim structural embeddings back to Step nodes.
pub struct StructuralEmbeddingPipeline;

impl StructuralEmbeddingPipeline {
    /// The GDS projection name used for structural embeddings.
    pub const PROJECTION_NAME: &'static str = "helix-projection";

    /// `Node2Vec` embedding dimensions.
    pub const DIMENSIONS: usize = 128;

    /// Generate the Cypher for projecting the helix graph into GDS.
    #[must_use]
    pub fn project_cypher() -> &'static str {
        "CALL gds.graph.project('helix-projection', \
         ['Helix', 'Step', 'Strand', 'SharedExperience'], \
         { IS_HELIX: {}, HAS_SUB_HELIX: {}, LINKS_TO: {properties: ['strength']}, \
           PARTICIPATES_IN: {}, MEMBER_OF: {properties: ['weight']} })"
    }

    /// Generate the Cypher for running `Node2Vec` on the projection.
    #[must_use]
    pub fn node2vec_cypher() -> &'static str {
        "CALL gds.node2vec.write('helix-projection', { \
         embeddingDimension: 128, \
         walkLength: 40, \
         walksPerNode: 5, \
         returnFactor: 0.5, \
         inOutFactor: 2.0, \
         writeProperty: 'struct_embedding' })"
    }

    /// Generate the Cypher for dropping the GDS projection.
    #[must_use]
    pub fn drop_cypher() -> &'static str {
        "CALL gds.graph.drop('helix-projection')"
    }

    /// Check if GDS is available by running `gds.version()`.
    #[must_use]
    pub fn check_gds_cypher() -> &'static str {
        "CALL gds.version() YIELD gdsVersion RETURN gdsVersion"
    }

    /// Run the full structural embedding pipeline.
    ///
    /// Returns `Ok(true)` if `Node2Vec` completed, `Ok(false)` if GDS unavailable.
    ///
    /// # Errors
    /// Returns `HelixDbError` if a Cypher query fails during projection or embedding.
    #[instrument(skip(db))]
    pub async fn run(db: &dyn HelixDb) -> Result<bool, HelixDbError> {
        // Check GDS availability
        match db.execute_cypher(Self::check_gds_cypher()).await {
            Ok(_) => {
                info!("GDS available — running structural embedding pipeline");
            }
            Err(e) => {
                warn!("GDS unavailable ({e}) — structural embeddings skipped");
                return Ok(false);
            }
        }

        // Project graph
        db.execute_cypher(Self::project_cypher()).await?;
        info!("GDS projection created");

        // Run Node2Vec
        db.execute_cypher(Self::node2vec_cypher()).await?;
        info!("Node2Vec embeddings written to Step nodes");

        // Drop projection (free RAM)
        if let Err(e) = db.execute_cypher(Self::drop_cypher()).await {
            warn!("Failed to drop GDS projection: {e}");
        }

        Ok(true)
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Mean-pool a set of vectors into a single vector.
///
/// Returns a zero vector if the input is empty.
#[must_use]
fn mean_pool(vectors: &[Vec<f32>]) -> Vec<f32> {
    if vectors.is_empty() {
        return Vec::new();
    }

    let dims = vectors[0].len();
    let mut sum = vec![0.0_f32; dims];

    for vec in vectors {
        for (i, &v) in vec.iter().enumerate() {
            if i < dims {
                sum[i] += v;
            }
        }
    }

    #[allow(clippy::cast_precision_loss)]
    let count = vectors.len() as f32;
    for v in &mut sum {
        *v /= count;
    }

    sum
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_mean_pool_single() {
        let vecs = vec![vec![1.0, 2.0, 3.0]];
        let result = mean_pool(&vecs);
        assert_eq!(result, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_mean_pool_multiple() {
        let vecs = vec![vec![1.0, 0.0], vec![3.0, 4.0]];
        let result = mean_pool(&vecs);
        assert!((result[0] - 2.0).abs() < f32::EPSILON);
        assert!((result[1] - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_mean_pool_empty() {
        let vecs: Vec<Vec<f32>> = Vec::new();
        let result = mean_pool(&vecs);
        assert!(result.is_empty());
    }

    #[test]
    fn test_structural_cypher_constants() {
        assert!(StructuralEmbeddingPipeline::project_cypher().contains("gds.graph.project"));
        assert!(StructuralEmbeddingPipeline::node2vec_cypher().contains("gds.node2vec.write"));
        assert!(StructuralEmbeddingPipeline::drop_cypher().contains("gds.graph.drop"));
        assert!(StructuralEmbeddingPipeline::check_gds_cypher().contains("gds.version"));
    }

    #[test]
    fn test_structural_dimensions() {
        assert_eq!(StructuralEmbeddingPipeline::DIMENSIONS, 128);
    }

    #[test]
    fn test_default_config() {
        let config = EmbeddingPipelineConfig::default();
        assert_eq!(config.concurrency, 4);
        assert!(config.skip_existing);
        assert!(!config.catch_up);
    }

    #[test]
    fn test_embedding_report_default() {
        let report = EmbeddingReport::default();
        assert_eq!(report.embedded, 0);
        assert_eq!(report.skipped, 0);
        assert_eq!(report.privacy_blocked, 0);
        assert!(report.errors.is_empty());
    }
}
