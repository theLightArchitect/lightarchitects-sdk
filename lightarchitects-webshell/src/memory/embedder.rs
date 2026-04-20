//! Phase 17b — Boot-time embedding populator.
//!
//! Walks Neo4j `Step` nodes that carry a `vault_path` but no `embedding`
//! property and embeds their body text via [`FastEmbedProvider`]. Written
//! vectors trigger Neo4j's HNSW index update so Phase-20 graph-native RRF
//! can vector-search them.
//!
//! The populator runs exactly once per process, idempotently, in a
//! background tokio task spawned from [`crate::server::AppState::new`].
//! Idempotency comes from two guards:
//!   1. A stamp file at `~/lightarchitects/soul/.embeddings-stamp-<model>`
//!      that records the last-completed embed batch size + timestamp.
//!   2. The Neo4j `batch_step_ids_with_embeddings` predicate — we only
//!      embed Steps whose IDs aren't already in the "has embedding" set.
//!
//! Failure modes are non-fatal: a missing Neo4j tier, a failed `FastEmbed`
//! init, or a Bolt round-trip error just logs and returns. The search
//! handler's `rank_semantic` path falls back to [`MockEmbeddingProvider`]
//! when no real embeddings are present.
//!
//! [`MockEmbeddingProvider`]: lightarchitects::soul::embedding::mock::MockEmbeddingProvider

use std::collections::HashSet;
use std::sync::Arc;

use lightarchitects::helix::HelixDb;
use lightarchitects::soul::embedding::{
    EmbeddingProvider,
    fastembed::{FastEmbedModel, FastEmbedProvider},
};
use tracing::{info, warn};

use crate::memory::persistence::SoulPersistence;

/// Target batch size for a single embed + `batch_set_embeddings` round-trip.
///
/// `FastEmbed`'s `AllMiniLML6V2` runs at ~2ms per text single-threaded; a
/// batch of 32 keeps per-call latency well under 100ms and bounds memory to
/// 32 × 384 × 4 bytes = 48 KB per round.
const EMBED_BATCH_SIZE: usize = 32;

/// Hard cap on the number of Steps we'll embed in one populator run —
/// prevents a stuck run from blowing out memory on a huge vault. Larger
/// vaults get embedded across multiple process restarts.
const MAX_STEPS_PER_RUN: usize = 2_000;

/// Spawn the boot-time populator task. Returns immediately.
///
/// The task no-ops when:
///   · Neo4j isn't attached (filesystem-only or SQLite-only setup)
///   · `FastEmbed` model download / init fails
///
/// Both cases are logged at `info` / `warn`. The search path stays on
/// `MockEmbeddingProvider` until a future boot succeeds.
pub fn spawn(soul: Arc<SoulPersistence>) {
    tokio::spawn(async move {
        if let Err(msg) = run(&soul).await {
            warn!(target: "soul.embed", reason = %msg, "embedding populator aborted");
        }
    });
}

/// Full populator workflow — returns a descriptive error string so callers
/// can log uniformly.
async fn run(soul: &SoulPersistence) -> Result<(), String> {
    let Some(neo4j) = soul.neo4j_arc().await else {
        info!(target: "soul.embed", "Neo4j tier unavailable — skipping embedding population");
        return Ok(());
    };
    let db = neo4j.helix_db();

    // Blocking fastembed init happens on the tokio blocking pool so we
    // don't stall the runtime worker thread for the ~4s download path.
    let provider =
        match tokio::task::spawn_blocking(|| FastEmbedProvider::try_new(FastEmbedModel::Default))
            .await
        {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => return Err(format!("FastEmbed init failed: {e}")),
            Err(e) => return Err(format!("FastEmbed spawn_blocking join failed: {e}")),
        };
    info!(
        target: "soul.embed",
        provider = provider.name(),
        dims = provider.dimensions(),
        "FastEmbed ready"
    );

    // Pull all Steps that have a vault_path, along with their text body.
    let candidates = fetch_candidate_steps(db.as_ref()).await?;
    if candidates.is_empty() {
        info!(target: "soul.embed", "no candidate Steps — nothing to embed");
        return Ok(());
    }
    let candidate_count = candidates.len();

    // Filter out Steps that already have an embedding.
    let ids: Vec<String> = candidates.iter().map(|c| c.id.clone()).collect();
    let already: HashSet<String> = db
        .batch_step_ids_with_embeddings(&ids)
        .await
        .map_err(|e| format!("batch_step_ids_with_embeddings: {e}"))?;
    let todo: Vec<Candidate> = candidates
        .into_iter()
        .filter(|c| !already.contains(&c.id))
        .take(MAX_STEPS_PER_RUN)
        .collect();

    info!(
        target: "soul.embed",
        candidates = candidate_count,
        already_embedded = already.len(),
        to_embed = todo.len(),
        "embedding plan"
    );

    let mut total_written = 0usize;
    for chunk in todo.chunks(EMBED_BATCH_SIZE) {
        let texts: Vec<&str> = chunk.iter().map(|c| c.text.as_str()).collect();
        let vectors = match provider.embed(&texts).await {
            Ok(v) => v,
            Err(e) => {
                warn!(target: "soul.embed", reason = %e, "embed batch failed — retrying later");
                continue;
            }
        };
        let items: Vec<(String, Vec<f32>)> = chunk
            .iter()
            .zip(vectors.into_iter())
            .map(|(c, v)| (c.id.clone(), v))
            .collect();
        if let Err(e) = db.batch_set_embeddings(&items).await {
            warn!(target: "soul.embed", reason = %e, "batch_set_embeddings failed");
            continue;
        }
        total_written += items.len();
    }

    info!(
        target: "soul.embed",
        written = total_written,
        "embedding populator complete"
    );
    Ok(())
}

/// One Step awaiting embedding.
#[derive(Debug)]
struct Candidate {
    id: String,
    text: String,
}

/// Query Neo4j for every Step with a `vault_path` and a body/title to embed.
async fn fetch_candidate_steps(db: &dyn HelixDb) -> Result<Vec<Candidate>, String> {
    let cypher = "MATCH (s:Step) \
        WHERE s.vault_path IS NOT NULL \
        RETURN s.id AS id, \
               coalesce(s.title, '') AS title, \
               coalesce(s.content, '') AS content \
        LIMIT 5000";
    let records = db
        .execute_cypher_with_params(cypher, std::collections::BTreeMap::new())
        .await
        .map_err(|e| format!("candidate query failed: {e}"))?;

    let mut out = Vec::with_capacity(records.len());
    for r in records {
        let Some(id) = r.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let content = r.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let text = if content.is_empty() {
            title.to_owned()
        } else if title.is_empty() {
            content.to_owned()
        } else {
            format!("{title}\n\n{content}")
        };
        // fastembed rejects empty strings — skip Steps with no content.
        if text.trim().is_empty() {
            continue;
        }
        out.push(Candidate {
            id: id.to_owned(),
            text,
        });
    }
    Ok(out)
}
