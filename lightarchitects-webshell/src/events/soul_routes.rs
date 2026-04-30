//! `/api/soul/*` HTTP handlers — Phase 9.5 of the `SOUL` vault hybrid backend.
//!
//! Four authenticated endpoints surface the hybrid memory model to the Svelte
//! `MemoryDrawer`:
//!
//! | Method | Path                           | Purpose                        |
//! |--------|--------------------------------|--------------------------------|
//! | GET    | `/api/soul/search`             | regex search across helix     |
//! | GET    | `/api/soul/entries/*path`      | detail view of one entry      |
//! | GET    | `/api/soul/memory/hot`         | active-session memo snapshot  |
//! | GET    | `/api/soul/memory/cold`        | promoted helix entries        |
//!
//! All four return 401 without a valid Bearer token (same pattern as
//! [`crate::events::builds_handler`]). Empty results are 200 with an empty
//! array — never 404 — so the frontend can render a "no results" state.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use lightarchitects::soul::embedding::EmbeddingProvider;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    auth,
    memory::{
        cold, frontmatter,
        types::{ContextMemo, EnrichedEntry},
    },
    server::AppState,
};
use lightarchitects::helix::HelixDb;

// ── Query parameter shapes ──────────────────────────────────────────────────

/// Search mode for `GET /api/soul/search`. Phase 17a.
#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// Lexical substring match against content, sibling, and strands.
    /// Default — preserves the Phase 9 baseline.
    #[default]
    Bm25,
    /// Cosine-similarity ranking of candidate entries against the query's
    /// embedding vector. Phase 17a uses `MockEmbeddingProvider` (FNV-1a +
    /// LCG); Phase 17b swaps in `fastembed` behind the same trait.
    Semantic,
    /// Reciprocal-rank fusion (`RRF`) of the top-K bm25 + top-K semantic
    /// rankings. Returns `rrf_used: true` in the response envelope so the
    /// UI can surface which retrieval strategy actually ran.
    Hybrid,
}

/// Query parameters for `GET /api/soul/search`.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Regex/substring pattern to match against entry body + front-matter.
    #[serde(default)]
    pub q: String,
    /// Maximum results to return. Server caps at 100.
    #[serde(default)]
    pub limit: Option<u8>,
    /// Retrieval strategy — see [`SearchMode`]. Defaults to `Bm25`.
    #[serde(default)]
    pub mode: SearchMode,
}

/// Query parameters for `GET /api/soul/memory/hot`.
#[derive(Debug, Deserialize)]
pub struct HotMemoryQuery {
    /// Maximum memos to return. Server caps at 200.
    #[serde(default)]
    pub limit: Option<u16>,
}

/// Query parameters for `GET /api/soul/memory/cold`.
#[derive(Debug, Deserialize)]
pub struct ColdMemoryQuery {
    /// Optional sibling filter (`"eva"`, `"corso"`, etc.).
    #[serde(default)]
    pub sibling: Option<String>,
    /// Maximum entries to return. Server caps at 500.
    #[serde(default)]
    pub limit: Option<u16>,
}

/// Query parameters for `GET /api/soul/edges` — Phase 12 static lineage.
#[derive(Debug, Deserialize)]
pub struct EdgeListQuery {
    /// Maximum edges to return. Server caps at 5,000 to bound Three.js
    /// draw-call cost on the `Hero3D` scene. Default 500.
    #[serde(default)]
    pub limit: Option<u16>,
}

/// Query parameters for `GET /api/soul/convergences` — Phase 13.3.
#[derive(Debug, Deserialize)]
pub struct ConvergenceListQuery {
    /// Minimum participant count (default 2). The consolidator writes 3+
    /// for Louvain-discovered convergences; user-declared convergences may
    /// have exactly 2 participants.
    #[serde(default)]
    pub min_participants: Option<u8>,
    /// Maximum convergences to return (default 50, cap 200).
    #[serde(default)]
    pub limit: Option<u16>,
}

/// One participant of a `SharedExperience` — a step + its owning sibling.
#[derive(Debug, Serialize)]
pub struct ConvergenceParticipant {
    /// Step UUID.
    pub step_id: String,
    /// Step title (may be absent).
    pub title: Option<String>,
    /// Vault-relative path — `None` for pre-Phase-11.5 Steps.
    pub vault_path: Option<String>,
    /// First path segment of `vault_path` — `unknown` for Steps that
    /// predate vault-path population.
    pub sibling: String,
}

/// One `SharedExperience` projected for the `MemoryDrawer` convergence view.
#[derive(Debug, Serialize)]
pub struct ConvergenceSummary {
    /// `SharedExperience` UUID.
    pub id: String,
    /// Convergence weight (0.0-1.0, higher = stronger alignment).
    pub weight: f64,
    /// Total participant count reported by Neo4j.
    pub participant_count: usize,
    /// Discovery method label (`"louvain"`, `"declared"`, `"embedding_ann"`).
    pub discovered_by: String,
    /// Optional human-readable label.
    pub label: Option<String>,
    /// When this convergence was materialised.
    pub created_at: String,
    /// Participants with sibling info — enables cross-sibling side-by-side render.
    pub participants: Vec<ConvergenceParticipant>,
    /// Distinct sibling names across the participants (primary sort key for the UI).
    pub siblings: Vec<String>,
}

/// Response from `GET /api/soul/convergences`.
#[derive(Debug, Serialize)]
pub struct ConvergenceListResponse {
    /// Convergences, strongest first.
    pub convergences: Vec<ConvergenceSummary>,
    /// Total count reported by Neo4j for "showing N of M" displays.
    pub total: u64,
}

/// One `:LINKS_TO` edge as rendered by `Hero3D`: source + target vault paths.
///
/// Vault paths keep the response front-end-actionable (color derivation +
/// sibling placement live in the Svelte scene, not the server).
#[derive(Debug, Serialize)]
pub struct EdgeSummary {
    /// Source Step's `vault_path` (e.g. `"eva/entries/day-0122.md"`).
    pub source: String,
    /// Target Step's `vault_path` (e.g. `"eva/identity.md"`).
    pub target: String,
    /// First path segment of `source` — used as the source sibling color key.
    pub source_sibling: String,
    /// First path segment of `target` — used as the target sibling color key.
    pub target_sibling: String,
}

/// Response from `GET /api/soul/edges`.
#[derive(Debug, Serialize)]
pub struct EdgeListResponse {
    /// Edges (up to the requested limit, bounded server-side).
    pub edges: Vec<EdgeSummary>,
    /// Total edge count reported by Neo4j for the client to optionally
    /// show "showing 500/2804" indicators.
    pub total: u64,
}

// ── Response shapes ─────────────────────────────────────────────────────────

/// Response from `GET /api/soul/search`.
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    /// Matching entries, newest-first.
    pub results: Vec<EnrichedEntry>,
    /// Whether RRF (reciprocal-rank-fusion) ranking was applied. Currently
    /// false — Phase 9 is filesystem-grep only; Phase C bridges RRF.
    pub rrf_used: bool,
}

/// Response from `GET /api/soul/entries/*path`.
#[derive(Debug, Serialize)]
pub struct EntryResponse {
    /// Structured projection of the entry.
    pub entry: EnrichedEntry,
    /// Full raw markdown source (front-matter + body) for diff/export.
    pub raw_markdown: String,
}

/// Response from `GET /api/soul/memory/{hot,cold}`.
#[derive(Debug, Serialize)]
pub struct MemoryListResponse {
    /// Memo list, newest-first.
    pub memos: Vec<ContextMemo>,
}

// ── Auth helper ─────────────────────────────────────────────────────────────

/// Validate Bearer token, returning `Err(StatusCode::UNAUTHORIZED)` on failure.
///
/// Mirrors the inline pattern in `builds_handler.rs`. Extracted here because
/// four routes share the same check.
fn check_auth(headers: &axum::http::HeaderMap, token: &str) -> Result<(), StatusCode> {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if auth::validate_bearer(authz, token) {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

// ── Handlers ────────────────────────────────────────────────────────────────

/// `GET /api/soul/search` — regex-search helix entries.
///
/// Walks every sibling's `entries/*.md`, scans body + front-matter for the
/// pattern (case-insensitive substring). Future-ready for RRF — the response
/// includes `rrf_used: false` today and will flip to `true` once Phase C
/// wires the hybrid retriever over REST.
#[allow(clippy::missing_panics_doc)]
pub async fn search_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> impl IntoResponse {
    if let Err(status) = check_auth(&headers, &state.config.token) {
        return status.into_response();
    }

    let pattern = q.q.trim();
    if pattern.is_empty() {
        return Json(SearchResponse {
            results: Vec::new(),
            rrf_used: false,
        })
        .into_response();
    }

    let Some(helix_root) = lightarchitects::core::paths::helix_root() else {
        warn!("helix_root unavailable — /api/soul/search");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    let limit = q.limit.unwrap_or(20).min(100) as usize;

    // Candidate pool: all cold memos (capped at 500). Every mode filters or
    // re-ranks over the same pool so the retrieval strategy is the only
    // variable in differential tests.
    let candidates = cold::snapshot_cold(&helix_root, None, 500).await;

    let (ranked_paths, rrf_used) = match q.mode {
        SearchMode::Bm25 => (rank_bm25(&candidates, pattern, limit), false),
        SearchMode::Semantic => (
            rank_semantic(&state, &candidates, pattern, limit).await,
            false,
        ),
        SearchMode::Hybrid => {
            // Phase 20a — 4-signal RRF fusion. Every signal ranks over
            // the same candidate pool; RRF collapses the four rankings
            // into a single list. Signals that return empty (e.g. graph
            // when Neo4j is absent) are silently dropped from the fusion
            // so retrieval degrades gracefully without a full failure.
            let bm25_ranked = rank_bm25(&candidates, pattern, 50);
            let sem_ranked = rank_semantic(&state, &candidates, pattern, 50).await;
            let graph_ranked = rank_graph(&state, &candidates, 50).await;
            let recency_ranked = rank_recency(&candidates, 50);
            let fused = rrf_fuse_n(
                &[bm25_ranked, sem_ranked, graph_ranked, recency_ranked],
                limit,
            );
            // Phase 20a flips rrf_used permanently to true for hybrid —
            // the fusion path is the canonical hybrid contract now, not
            // a "was RRF actually applied" telemetry flag. Clients that
            // want "did the graph contribute" can query /api/soul/health.
            (fused, true)
        }
    };

    // Hydrate ranked paths back to full EnrichedEntry records.
    let mut matches: Vec<EnrichedEntry> = Vec::new();
    for path in ranked_paths {
        if let Some((entry, _raw)) = cold::read_entry(&helix_root, &path).await {
            matches.push(entry);
        }
    }

    Json(SearchResponse {
        results: matches,
        rrf_used,
    })
    .into_response()
}

/// Lexical ranking — substring match on content / sibling / strands. The
/// output order is snapshot order (newest-first), which preserves the
/// Phase 9 behaviour tested by existing vitests.
fn rank_bm25(candidates: &[ContextMemo], pattern: &str, limit: usize) -> Vec<String> {
    let needle = pattern.to_lowercase();
    candidates
        .iter()
        .filter(|memo| {
            memo.content.to_lowercase().contains(&needle)
                || memo.sibling.to_lowercase().contains(&needle)
                || memo
                    .strands
                    .iter()
                    .any(|s| s.to_lowercase().contains(&needle))
        })
        .filter_map(|memo| memo.source_path.clone())
        .take(limit)
        .collect()
}

/// Semantic ranking — Phase 17b.
///
/// Query flow:
///   1. Embed the query via [`AppState::embedding`] (`FastEmbed` real when
///      available, `MockEmbedding` fallback).
///   2. Fetch pre-computed doc vectors from Neo4j where possible — the
///      boot-time populator writes `Step.embedding` for every Step with a
///      `vault_path`. This lets us cosine-score 700+ docs in ~1ms without
///      re-embedding content every query.
///   3. When Neo4j is absent OR no pre-computed vectors exist for the
///      candidate pool, fall back to Phase-17a behaviour: embed each
///      candidate at query time. Slower with real `FastEmbed` (~1.5s per
///      500 docs) but correct.
async fn rank_semantic(
    state: &AppState,
    candidates: &[ContextMemo],
    pattern: &str,
    limit: usize,
) -> Vec<String> {
    let provider = state.embedding().await;
    let Ok(query_vecs) = provider.embed(&[pattern]).await else {
        return Vec::new();
    };
    let Some(query_vec) = query_vecs.first() else {
        return Vec::new();
    };

    // Fast path — pre-computed vectors from Neo4j.
    if let Some(scored) = rank_semantic_from_neo4j(state, candidates, query_vec, limit).await {
        if !scored.is_empty() {
            return scored;
        }
    }

    // Fallback — embed candidate bodies at query time.
    rank_semantic_embedding_at_query(provider.as_ref(), candidates, query_vec, limit).await
}

/// Try to score candidates using pre-computed `Step.embedding` values.
///
/// Returns `None` when Neo4j isn't attached. Returns an empty `Some(vec![])`
/// when Neo4j is up but no matching Steps have embeddings — the caller then
/// falls back to the at-query-time path.
async fn rank_semantic_from_neo4j(
    state: &AppState,
    candidates: &[ContextMemo],
    query_vec: &[f32],
    limit: usize,
) -> Option<Vec<String>> {
    let soul = state.soul_store.as_ref()?;
    let neo4j = soul.neo4j_arc().await?;
    let db = neo4j.helix_db();

    // Candidate paths — the ones we already walked on disk. We restrict
    // the Neo4j query to this pool so cosine ranking stays consistent
    // with what bm25 + the UI are seeing.
    let paths: Vec<String> = candidates
        .iter()
        .filter_map(|m| m.source_path.clone())
        .collect();
    if paths.is_empty() {
        return Some(Vec::new());
    }

    let mut params: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    params.insert("paths".into(), serde_json::json!(paths));

    let cypher = "MATCH (s:Step) \
        WHERE s.vault_path IN $paths AND s.embedding IS NOT NULL \
        RETURN s.vault_path AS path, s.embedding AS embedding";
    let records = db.execute_cypher_with_params(cypher, params).await.ok()?;

    let mut scored: Vec<(f32, String)> = Vec::with_capacity(records.len());
    for r in records {
        let Some(path) = r.get("path").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(arr) = r.get("embedding").and_then(|v| v.as_array()) else {
            continue;
        };
        let doc_vec: Vec<f32> = arr
            .iter()
            .filter_map(|v| {
                v.as_f64().map(|f| {
                    #[allow(clippy::cast_possible_truncation)]
                    let x = f as f32;
                    x
                })
            })
            .collect();
        if doc_vec.is_empty() {
            continue;
        }
        let score = cosine_similarity(query_vec, &doc_vec);
        scored.push((score, path.to_owned()));
    }

    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    Some(scored.into_iter().map(|(_, p)| p).take(limit).collect())
}

/// Phase-17a fallback — embed candidate bodies at query time.
///
/// Used when Neo4j is unavailable OR no Steps have pre-computed embeddings
/// yet (e.g. the populator is still running on first boot). Slower with
/// real `FastEmbed` but still produces content-aware rankings.
async fn rank_semantic_embedding_at_query(
    provider: &(dyn EmbeddingProvider + Send + Sync),
    candidates: &[ContextMemo],
    query_vec: &[f32],
    limit: usize,
) -> Vec<String> {
    let texts: Vec<&str> = candidates
        .iter()
        .filter(|m| m.source_path.is_some())
        .map(|m| m.content.as_str())
        .collect();
    if texts.is_empty() {
        return Vec::new();
    }
    let Ok(doc_vecs) = provider.embed(&texts).await else {
        return Vec::new();
    };

    let mut scored: Vec<(f32, String)> = candidates
        .iter()
        .filter_map(|m| m.source_path.clone().map(|p| (m, p)))
        .zip(doc_vecs.iter())
        .map(|((_memo, path), doc_vec)| (cosine_similarity(query_vec, doc_vec), path))
        .collect();

    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    scored.into_iter().map(|(_, p)| p).take(limit).collect()
}

/// Cosine similarity for two L2-normalised vectors reduces to a dot product.
/// `MockEmbeddingProvider` guarantees L2-normalisation; `FastEmbed` does as
/// well for E5/BGE family models, so this stays correct across Phase 17a/b.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    a.iter().zip(b.iter()).take(len).map(|(x, y)| x * y).sum()
}

/// N-way reciprocal-rank fusion — Phase 20a generalisation.
///
/// Formula: `score(path) = Σ 1 / (k + rank_i(path))` where `k = 60` is the
/// canonical constant (Cormack, Clarke, Büttcher 2009). Ties break by
/// descending score, then lexical path.
///
/// Accepts any number of ranked lists and fuses them with the same `k=60`
/// constant. Empty signals contribute nothing — they drop out of the
/// fusion silently. This is the retrieval contract for hybrid mode:
/// however many of the four signals (bm25 / semantic / graph / recency)
/// happen to return results, the response is still a coherent fused
/// ranking.
fn rrf_fuse_n(signals: &[Vec<String>], limit: usize) -> Vec<String> {
    const K: f32 = 60.0;
    let mut scores: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    for signal in signals {
        for (rank, path) in signal.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = (rank + 1) as f32;
            *scores.entry(path.clone()).or_insert(0.0) += 1.0 / (K + r);
        }
    }
    let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    ranked.into_iter().map(|(p, _)| p).take(limit).collect()
}

/// Phase 20a — graph-walk signal.
///
/// Ranks candidates by their structural centrality in the graph: outgoing
/// `:LINKS_TO`, incoming `:LINKS_TO`, and `:MATERIALIZED_FROM` all count
/// as edges. A Step that's referenced from many places (or references
/// many) is more retrieval-worthy even when lexical/semantic signals tie.
///
/// Returns the candidate `vault_path`s sorted by descending connectivity.
/// When Neo4j is unavailable, returns an empty vec — `rrf_fuse_n` handles
/// that cleanly by silently dropping the signal.
async fn rank_graph(state: &AppState, candidates: &[ContextMemo], limit: usize) -> Vec<String> {
    let Some(soul) = state.soul_store.as_ref() else {
        return Vec::new();
    };
    let Some(neo4j) = soul.neo4j_arc().await else {
        return Vec::new();
    };
    let db = neo4j.helix_db();

    let paths: Vec<String> = candidates
        .iter()
        .filter_map(|m| m.source_path.clone())
        .collect();
    if paths.is_empty() {
        return Vec::new();
    }

    let mut params: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    params.insert("paths".into(), serde_json::json!(paths));

    // Degree = links-out + links-in + materialized-from (all from/to this
    // Step). `coalesce(..., 0)` keeps isolated Steps in the result set at
    // score 0 rather than dropping them — lets the fused rank still pick
    // up a Step by its other signals.
    let cypher = "MATCH (s:Step) \
         WHERE s.vault_path IN $paths \
         OPTIONAL MATCH (s)-[out:LINKS_TO]->() \
         WITH s, count(out) AS outgoing \
         OPTIONAL MATCH (s)<-[inc:LINKS_TO]-() \
         WITH s, outgoing, count(inc) AS incoming \
         OPTIONAL MATCH (s)-[mat:MATERIALIZED_FROM]->() \
         WITH s, outgoing + incoming + count(mat) AS degree \
         RETURN s.vault_path AS path, degree \
         ORDER BY degree DESC, s.vault_path ASC";
    let records = match db.execute_cypher_with_params(cypher, params).await {
        Ok(r) => r,
        Err(e) => {
            warn!(target: "soul.search.graph", error = %e, "graph-walk query failed");
            return Vec::new();
        }
    };

    records
        .into_iter()
        .filter_map(|r| r.get("path").and_then(|v| v.as_str()).map(str::to_owned))
        .take(limit)
        .collect()
}

/// Phase 20a — recency signal.
///
/// Exponential decay on the candidate's `created_at` timestamp:
/// `score = exp(-age_days / RECENCY_TAU_DAYS)`. Newer Steps rank higher.
/// Pure Rust — no graph round-trip — so this signal is always available
/// whenever the cold walker returned anything.
///
/// `RECENCY_TAU_DAYS = 30` puts the half-life at ~21 days. An entry from
/// today scores ~1.0; a month-old entry scores ~0.37; a year-old one
/// scores ~5e-6 and rarely survives fusion against fresher candidates.
fn rank_recency(candidates: &[ContextMemo], limit: usize) -> Vec<String> {
    const RECENCY_TAU_DAYS: f64 = 30.0;
    let now = chrono::Utc::now();
    let mut scored: Vec<(f64, String)> = candidates
        .iter()
        .filter_map(|m| {
            let path = m.source_path.clone()?;
            let created = chrono::DateTime::parse_from_rfc3339(&m.created_at)
                .ok()?
                .with_timezone(&chrono::Utc);
            #[allow(clippy::cast_precision_loss)]
            let age_days = (now - created).num_seconds() as f64 / 86_400.0;
            let score = (-age_days.max(0.0) / RECENCY_TAU_DAYS).exp();
            Some((score, path))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    scored.into_iter().map(|(_, p)| p).take(limit).collect()
}

/// `GET /api/soul/entries/*path` — read one helix entry.
#[allow(clippy::missing_panics_doc)]
pub async fn entry_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    if let Err(status) = check_auth(&headers, &state.config.token) {
        return status.into_response();
    }
    let Some(helix_root) = lightarchitects::core::paths::helix_root() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match cold::read_entry(&helix_root, &path).await {
        Some((entry, raw_markdown)) => Json(EntryResponse {
            entry,
            raw_markdown,
        })
        .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Project a `Vec<`[`lightarchitects::helix::types::HotMemo`]`>` into
/// [`ContextMemo`] display structs for the `MemoryDrawer`.
///
/// Used by [`hot_memory_handler`] when reading from the Neo4j tier (Phase 18c).
fn hot_memos_to_context(
    hot_memos: Vec<lightarchitects::helix::types::HotMemo>,
) -> Vec<ContextMemo> {
    use crate::memory::types::MemoryTier;
    hot_memos
        .into_iter()
        .map(|m| ContextMemo {
            id: m.id,
            tier: MemoryTier::Hot,
            content: m.content,
            #[allow(clippy::cast_possible_truncation)]
            significance: m.significance as f32,
            sibling: m.sibling,
            strands: m.strands,
            created_at: m.created_at.to_rfc3339(),
            source_path: Some("neo4j:HotMemo".to_owned()),
            resonance: Vec::new(),
            themes: Vec::new(),
            self_defining: false,
            entry_type: None,
        })
        .collect()
}

/// `GET /api/soul/memory/hot` — Phase-18c Neo4j-only hot-tier handler.
///
/// Read order (Phase 18c Step 3 — NDJSON bridge burned):
///   1. If Neo4j is attached and has live `:HotMemo` nodes, return them.
///      TTL gate on `expires` ensures stale entries drop automatically.
///   2. If Neo4j is unavailable or returned no nodes, return an empty list
///      and emit a telemetry warning.  The NDJSON safety-net fallback was
///      removed in Phase 18c Step 3 after 7-day dual-write stability was
///      confirmed.
///
/// `:HotMemo` nodes carry an `expires` property (24-hour TTL) so
/// [`HelixDb::query_hot_memos`] TTL-gates without a compaction pass.
/// Phase 19 will replace the fixed TTL with a per-sibling decay curve.
#[allow(clippy::missing_panics_doc)]
pub async fn hot_memory_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    Query(q): Query<HotMemoryQuery>,
) -> impl IntoResponse {
    if let Err(status) = check_auth(&headers, &state.config.token) {
        return status.into_response();
    }
    let limit = q.limit.unwrap_or(50).min(200) as usize;

    // Phase 18c Step 3 — Neo4j only; NDJSON bridge removed.
    // Fallback is empty list + telemetry warn so operators know if Neo4j drops.
    let memos = if let Some(soul) = state.soul_store.as_ref() {
        if let Some(neo4j) = soul.neo4j_arc().await {
            let db = neo4j.helix_db();
            #[allow(clippy::cast_possible_truncation)]
            match db.query_hot_memos(None, limit as u32).await {
                Ok(hot_memos) if !hot_memos.is_empty() => hot_memos_to_context(hot_memos),
                Ok(_) => {
                    tracing::debug!(
                        target: "soul.hot_memory",
                        "Neo4j returned 0 HotMemo nodes — session may have no memos yet"
                    );
                    Vec::new()
                }
                Err(e) => {
                    tracing::warn!(
                        target: "soul.hot_memory",
                        error = %e,
                        "query_hot_memos failed — returning empty (NDJSON bridge removed)"
                    );
                    Vec::new()
                }
            }
        } else {
            tracing::warn!(
                target: "soul.hot_memory",
                "neo4j_arc unavailable — returning empty (NDJSON bridge removed)"
            );
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Phase 18c Step 3 — dual-write loop removed.
    // The read source is now Neo4j; MERGEing Neo4j results back into Neo4j would
    // be a pure no-op.  New :HotMemo nodes are written directly by the terminal
    // PTY session handler via create_hot_memo() at append time.

    Json(MemoryListResponse { memos }).into_response()
}

/// `GET /api/soul/health` — Phase 10.5 per-tier status + per-sibling counts.
///
/// Parity check for the `SOUL` `MCP` plugin: the `counts` map mirrors the
/// `soul health-check` CLI output. Tier flags show which storage layers the
/// webshell has live connections to.
#[allow(clippy::missing_panics_doc)]
pub async fn health_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err(status) = check_auth(&headers, &state.config.token) {
        return status.into_response();
    }
    let tiers = if let Some(s) = state.soul_store.as_ref() {
        s.tier_status().await
    } else {
        crate::memory::persistence::TierStatus {
            filesystem: false,
            sqlite: false,
            neo4j: false,
        }
    };

    // Per-sibling entry counts — authoritative source is the filesystem
    // because it's always present. SQLite counts could diverge briefly during
    // ingest, so we use the filesystem as ground truth for the UI.
    let mut counts = serde_json::Map::new();
    if let Some(helix_root) = lightarchitects::core::paths::helix_root() {
        if let Ok(mut rd) = tokio::fs::read_dir(&helix_root).await {
            while let Ok(Some(entry)) = rd.next_entry().await {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if name.starts_with('.') || name.starts_with('_') {
                    continue;
                }
                let entries_dir = path.join("entries");
                let count = count_md_files(&entries_dir).await;
                counts.insert(name.to_owned(), serde_json::json!(count));
            }
        }
    }

    // Wikilink health — the count of `:LINKS_TO` edges in the graph is a
    // proxy for "resolved wikilinks". `unresolved` would require re-parsing
    // all entries to detect broken refs; that's expensive so we expose it as
    // `null` and annotate the API-surface gap. Neo4j offline → both null.
    let wikilinks_resolved: Option<i64> = if tiers.neo4j {
        if let Some(s) = state.soul_store.as_ref() {
            if let Some(neo4j) = s.neo4j_arc().await {
                let db = neo4j.helix_db();
                let cypher = "MATCH ()-[r:LINKS_TO]->() RETURN count(r) AS n";
                let params = std::collections::BTreeMap::new();
                match db.execute_cypher_with_params(cypher, params).await {
                    Ok(rows) => rows
                        .first()
                        .and_then(|r| r.get("n"))
                        .and_then(serde_json::Value::as_i64),
                    Err(_) => None,
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    Json(serde_json::json!({
        "tiers": tiers,
        "counts": counts,
        "bolt_uri": std::env::var("WEBSHELL_NEO4J_URI").unwrap_or_default(),
        "wikilinks": {
            "resolved": wikilinks_resolved,
            "unresolved": serde_json::Value::Null,
            "note": "unresolved count requires re-ingest; resolved reflects current :LINKS_TO edge count"
        }
    }))
    .into_response()
}

/// `GET /api/soul/relationships/*entry_id` — Phase 11.4 graph walk.
///
/// Returns the 1-hop + 2-hop neighborhood of an entry via `HelixDb::traverse`.
/// Requires the `Neo4j` tier to be active — falls back to 200 with empty
/// `neighbors` + `tier: "none"` when `Neo4j` isn't connected so the UI can
/// render "no graph available" instead of an error.
#[allow(clippy::missing_panics_doc)]
pub async fn relationships_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    Path(entry_id): Path<String>,
) -> impl IntoResponse {
    if let Err(status) = check_auth(&headers, &state.config.token) {
        return status.into_response();
    }
    let Some(soul) = state.soul_store.as_ref() else {
        return Json(empty_graph_response(&entry_id)).into_response();
    };
    let Some(neo4j) = soul.neo4j_arc().await else {
        return Json(empty_graph_response(&entry_id)).into_response();
    };

    let db = neo4j.helix_db();
    // `find_backlinks` returns every Step that references this entry —
    // exactly the "related entries" semantic the MemoryDrawer detail pane
    // needs. Phase 11.4 MVP; Phase 13 will expand to typed relationship
    // edges (CITES / PROMOTED_FROM / ACTIVATES).
    match db.find_backlinks(&entry_id).await {
        Ok(steps) => {
            let neighbors: Vec<serde_json::Value> = steps
                .into_iter()
                .take(25)
                .map(|step| {
                    serde_json::json!({
                        "id": step.id,
                        "title": step.title,
                        "helix_id": step.helix_id,
                        "significance": step.significance,
                    })
                })
                .collect();
            Json(serde_json::json!({
                "entry_id": entry_id,
                "tier": "neo4j",
                "relation": "backlinks",
                "neighbors": neighbors,
            }))
            .into_response()
        }
        Err(e) => {
            warn!(target: "soul", error = %e, entry_id = %entry_id, "find_backlinks failed");
            Json(empty_graph_response(&entry_id)).into_response()
        }
    }
}

fn empty_graph_response(entry_id: &str) -> serde_json::Value {
    serde_json::json!({
        "entry_id": entry_id,
        "tier": "none",
        "relation": "backlinks",
        "neighbors": [],
    })
}

/// `GET /api/soul/edges` — Phase 12 bulk `:LINKS_TO` edges for `Hero3D`.
///
/// Returns up to `limit` (default 500, cap 5,000) vault-path pairs so the
/// Svelte scene can draw a persistent bloom-lit line between each pair of
/// sibling rings. When the `Neo4j` tier is absent the response is
/// 200 with an empty edge list + `total: 0` (same degrade-gracefully
/// pattern as `/api/soul/relationships`).
#[allow(clippy::missing_panics_doc)]
pub async fn edges_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    Query(params): Query<EdgeListQuery>,
) -> impl IntoResponse {
    if let Err(status) = check_auth(&headers, &state.config.token) {
        return status.into_response();
    }
    let limit = params.limit.unwrap_or(500).min(5_000);
    let Some(soul) = state.soul_store.as_ref() else {
        return Json(EdgeListResponse {
            edges: Vec::new(),
            total: 0,
        })
        .into_response();
    };
    let Some(neo4j) = soul.neo4j_arc().await else {
        return Json(EdgeListResponse {
            edges: Vec::new(),
            total: 0,
        })
        .into_response();
    };

    let db = neo4j.helix_db();

    // Pull edges + total in the same round-trip. Both Steps must have
    // `vault_path` populated; edges whose endpoints predate Phase 11.5 are
    // skipped (they can't be placed on a sibling ring without a path).
    let mut params_map: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    params_map.insert("limit".into(), serde_json::json!(i64::from(limit)));

    let edges_cypher = "MATCH (a:Step)-[:LINKS_TO]->(b:Step) \
         WHERE a.vault_path IS NOT NULL AND b.vault_path IS NOT NULL \
         RETURN a.vault_path AS source, b.vault_path AS target \
         LIMIT $limit";

    let edges_result = db
        .execute_cypher_with_params(edges_cypher, params_map.clone())
        .await;
    let total_result = db
        .execute_cypher_with_params(
            "MATCH ()-[r:LINKS_TO]->() RETURN count(r) AS n",
            std::collections::BTreeMap::new(),
        )
        .await;

    let edges: Vec<EdgeSummary> = match edges_result {
        Ok(records) => records
            .into_iter()
            .filter_map(|r| {
                let source = r.get("source").and_then(|v| v.as_str())?.to_owned();
                let target = r.get("target").and_then(|v| v.as_str())?.to_owned();
                let source_sibling = source.split('/').next().unwrap_or_default().to_owned();
                let target_sibling = target.split('/').next().unwrap_or_default().to_owned();
                Some(EdgeSummary {
                    source,
                    target,
                    source_sibling,
                    target_sibling,
                })
            })
            .collect(),
        Err(e) => {
            warn!(target: "soul", error = %e, "edges_handler: list edges failed");
            Vec::new()
        }
    };

    let total = total_result
        .ok()
        .and_then(|rows| rows.into_iter().next())
        .and_then(|r| r.get("n").and_then(serde_json::Value::as_u64))
        .unwrap_or(0);

    Json(EdgeListResponse { edges, total }).into_response()
}

/// `GET /api/soul/convergences` — Phase 13.3 cross-sibling convergence view.
///
/// Returns `SharedExperience` nodes ordered by weight with each participant
/// step + its sibling so the UI can render side-by-side columns keyed by
/// sibling. Empty list when `Neo4j` isn't attached or the consolidator
/// hasn't populated any convergences yet (the usual case on a fresh vault
/// — `GET /api/soul/convergences` + UI empty state "no convergences yet").
#[allow(clippy::missing_panics_doc)]
#[allow(clippy::too_many_lines)]
pub async fn convergences_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    Query(params): Query<ConvergenceListQuery>,
) -> impl IntoResponse {
    if let Err(status) = check_auth(&headers, &state.config.token) {
        return status.into_response();
    }
    let min_p = params.min_participants.unwrap_or(2).max(2);
    let limit = params.limit.unwrap_or(50).min(200);

    let Some(soul) = state.soul_store.as_ref() else {
        return Json(ConvergenceListResponse {
            convergences: Vec::new(),
            total: 0,
        })
        .into_response();
    };
    let Some(neo4j) = soul.neo4j_arc().await else {
        return Json(ConvergenceListResponse {
            convergences: Vec::new(),
            total: 0,
        })
        .into_response();
    };
    let db = neo4j.helix_db();

    // Aggregate participants per SharedExperience in a single round-trip.
    // Cross-sibling = we don't filter by helix_id; the UI keys off each
    // participant's `sibling` for side-by-side columns.
    let list_cypher = "MATCH (s:Step)-[:PARTICIPATES_IN]->(se:SharedExperience) \
         WHERE se.participant_count >= $min_p \
         WITH se, collect(DISTINCT { \
             step_id: s.id, \
             title: s.title, \
             vault_path: s.vault_path, \
             helix_id: s.helix_id \
         }) AS parts \
         RETURN se.id AS id, se.weight AS weight, \
                se.participant_count AS participant_count, \
                se.discovered_by AS discovered_by, \
                se.label AS label, \
                toString(se.created_at) AS created_at, \
                parts \
         ORDER BY se.weight DESC \
         LIMIT $limit";

    let mut list_params: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    list_params.insert("min_p".into(), serde_json::json!(i64::from(min_p)));
    list_params.insert("limit".into(), serde_json::json!(i64::from(limit)));

    let list_result = db
        .execute_cypher_with_params(list_cypher, list_params)
        .await;
    let total_result = db
        .execute_cypher_with_params(
            "MATCH (se:SharedExperience) RETURN count(se) AS n",
            std::collections::BTreeMap::new(),
        )
        .await;

    let convergences: Vec<ConvergenceSummary> = match list_result {
        Ok(records) => records
            .into_iter()
            .filter_map(|row| {
                let id = row.get("id")?.as_str()?.to_owned();
                let weight = row.get("weight")?.as_f64().unwrap_or(0.0);
                let participant_count = usize::try_from(
                    row.get("participant_count")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(0),
                )
                .unwrap_or(0);
                let discovered_by = row
                    .get("discovered_by")
                    .and_then(|v| v.as_str())
                    .unwrap_or("declared")
                    .to_owned();
                let label = row.get("label").and_then(|v| v.as_str()).map(str::to_owned);
                let created_at = row
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned();
                let parts_raw = row.get("parts")?.as_array()?.clone();
                let participants: Vec<ConvergenceParticipant> = parts_raw
                    .into_iter()
                    .filter_map(|p| {
                        let step_id = p.get("step_id")?.as_str()?.to_owned();
                        let title = p.get("title").and_then(|v| v.as_str()).map(str::to_owned);
                        let vault_path = p
                            .get("vault_path")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned);
                        // Sibling = first path segment of vault_path; fall
                        // back to first segment of helix_id for pre-11.5
                        // Steps that never got a vault_path.
                        let sibling = vault_path
                            .as_deref()
                            .and_then(|vp| vp.split('/').next())
                            .or_else(|| {
                                p.get("helix_id")
                                    .and_then(|v| v.as_str())
                                    .and_then(|h| h.split('/').next())
                            })
                            .unwrap_or("unknown")
                            .to_owned();
                        Some(ConvergenceParticipant {
                            step_id,
                            title,
                            vault_path,
                            sibling,
                        })
                    })
                    .collect();
                let mut siblings: Vec<String> =
                    participants.iter().map(|p| p.sibling.clone()).collect();
                siblings.sort();
                siblings.dedup();
                Some(ConvergenceSummary {
                    id,
                    weight,
                    participant_count,
                    discovered_by,
                    label,
                    created_at,
                    participants,
                    siblings,
                })
            })
            .collect(),
        Err(e) => {
            warn!(target: "soul", error = %e, "convergences_handler: list failed");
            Vec::new()
        }
    };

    let total = total_result
        .ok()
        .and_then(|rows| rows.into_iter().next())
        .and_then(|r| r.get("n").and_then(serde_json::Value::as_u64))
        .unwrap_or(0);

    Json(ConvergenceListResponse {
        convergences,
        total,
    })
    .into_response()
}

/// `POST /api/soul/reindex` — force a filesystem→`SQLite` backfill.
///
/// Phase 11.1. Returns a per-sibling count report. No-op when `SQLite` isn't
/// available — returns 503.
#[allow(clippy::missing_panics_doc)]
pub async fn reindex_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err(status) = check_auth(&headers, &state.config.token) {
        return status.into_response();
    }
    let Some(soul) = state.soul_store.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match soul.reindex().await {
        Some(report) => (StatusCode::OK, Json(report)).into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "sqlite_unavailable"})),
        )
            .into_response(),
    }
}

async fn count_md_files(dir: &std::path::Path) -> u32 {
    let Ok(mut rd) = tokio::fs::read_dir(dir).await else {
        return 0;
    };
    let mut count = 0u32;
    while let Ok(Some(entry)) = rd.next_entry().await {
        if entry.path().extension().and_then(|e| e.to_str()) == Some("md") {
            count = count.saturating_add(1);
        }
    }
    count
}

/// `GET /api/soul/memory/cold` — snapshot of promoted helix entries.
///
/// Phase 10.2 tier-preference: when the `SOUL` `SQLite` backend is live, queries
/// it first so entries ingested via the `SOUL` `MCP` plugin show up immediately.
/// Falls back to a filesystem walk when `SQLite` is unavailable.
#[allow(clippy::missing_panics_doc)]
pub async fn cold_memory_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    Query(q): Query<ColdMemoryQuery>,
) -> impl IntoResponse {
    if let Err(status) = check_auth(&headers, &state.config.token) {
        return status.into_response();
    }
    let limit = q.limit.unwrap_or(50).min(500) as usize;

    if let Some(soul) = state.soul_store.as_ref() {
        let memos = cold::snapshot_cold_via_soul(soul, q.sibling.as_deref(), limit).await;
        return Json(MemoryListResponse { memos }).into_response();
    }

    let Some(helix_root) = lightarchitects::core::paths::helix_root() else {
        return Json(MemoryListResponse { memos: Vec::new() }).into_response();
    };
    let memos = cold::snapshot_cold(&helix_root, q.sibling.as_deref(), limit).await;
    Json(MemoryListResponse { memos }).into_response()
}

/// Phase 16a — `POST /api/soul/compaction/preview`
///
/// Request body: a [`RetentionPolicy`] JSON payload (see
/// [`crate::memory::compaction::RetentionPolicy`] for the tagged-enum shape).
/// Response: a [`CompactionSummary`] listing candidates without touching
/// the filesystem. The permanent guard is applied unconditionally so
/// self-defining entries and significance ≥ 0.9 memos never appear in
/// the candidate list regardless of policy.
#[allow(clippy::missing_panics_doc)]
pub async fn compaction_preview_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    Json(policy): Json<crate::memory::compaction::RetentionPolicy>,
) -> impl IntoResponse {
    if let Err(status) = check_auth(&headers, &state.config.token) {
        return status.into_response();
    }

    // Reuse whichever cold source is active — SoulPersistence when the
    // SQLite tier is up (richer metadata, includes self_defining), else
    // filesystem walk as a fallback.
    // Compaction must scan the ENTIRE cold tier deterministically so
    // preview and apply agree on the candidate set. Route through the
    // fs walker (alphabetical sibling + file order) with a huge cap —
    // the SQLite path isn't deterministic back-to-back without ORDER BY.
    let Some(helix_root) = lightarchitects::core::paths::helix_root() else {
        return Json(crate::memory::compaction::CompactionSummary {
            total_scanned: 0,
            candidates: Vec::new(),
            permanent_skipped: 0,
            policy,
        })
        .into_response();
    };
    let memos: Vec<ContextMemo> =
        cold::snapshot_cold_capped(&helix_root, None, 10_000, 10_000).await;

    let summary = crate::memory::compaction::classify_for_compaction(&memos, &policy);
    Json(summary).into_response()
}

/// Phase 16b — `POST /api/soul/compaction/apply`
///
/// Destructive counterpart to [`compaction_preview_handler`]. Re-classifies
/// the current cold snapshot against `policy`, then moves each candidate
/// markdown file from its current path to
/// `{helix_root}/.compacted/{YYYY-MM-DD}/{original-relative-path}`.
///
/// Moves, not deletes — the .compacted/ directory is a recovery tier. A
/// future Phase 16c could add a restore endpoint or a periodic prune.
///
/// Returns a [`CompactionSummary`] describing what was moved. The
/// permanent guard is still applied — apply classifies freshly before
/// acting, so a protected entry can never slip through even if the
/// preview was stale.
#[allow(clippy::missing_panics_doc)]
pub async fn compaction_apply_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    Json(policy): Json<crate::memory::compaction::RetentionPolicy>,
) -> impl IntoResponse {
    if let Err(status) = check_auth(&headers, &state.config.token) {
        return status.into_response();
    }

    let Some(helix_root) = lightarchitects::core::paths::helix_root() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    // Re-classify at apply time — the Phase-16 invariant is that apply
    // consumes the SAME classify() output that preview did. This only
    // holds when the snapshot is deterministic; the fs walker
    // (`snapshot_cold_capped`) is alphabetical and therefore stable,
    // whereas the SQLite path isn't ordered without an explicit ORDER BY.
    // Both preview and apply route through the fs walker for this reason.
    let memos: Vec<ContextMemo> =
        cold::snapshot_cold_capped(&helix_root, None, 10_000, 10_000).await;

    let summary = crate::memory::compaction::classify_for_compaction(&memos, &policy);
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let compacted_root = helix_root.join(".compacted").join(&date);

    // Move each candidate file. Failures are per-entry: a file that can't
    // be moved stays in place and the summary returns to the caller with
    // whatever was successfully compacted so far. Nothing is silently
    // dropped — every move either completes or logs.
    let mut moved_paths = Vec::new();
    for candidate in &summary.candidates {
        let source = helix_root.join(&candidate.path);
        let dest = compacted_root.join(&candidate.path);
        if let Some(parent) = dest.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                warn!(
                    target: "soul.compaction",
                    error = %e,
                    path = %candidate.path,
                    "failed to create .compacted parent directory"
                );
                continue;
            }
        }
        match tokio::fs::rename(&source, &dest).await {
            Ok(()) => moved_paths.push(candidate.path.clone()),
            Err(e) => warn!(
                target: "soul.compaction",
                error = %e,
                from = %source.display(),
                to = %dest.display(),
                "compaction apply: rename failed — file left in place"
            ),
        }
    }

    // Echo a summary limited to what actually moved so the UI can render
    // an accurate "42 of 50 rolled up" report instead of assuming full
    // success.
    let moved_set: std::collections::HashSet<&str> =
        moved_paths.iter().map(String::as_str).collect();
    let moved_candidates: Vec<_> = summary
        .candidates
        .into_iter()
        .filter(|c| moved_set.contains(c.path.as_str()))
        .collect();
    let response = crate::memory::compaction::CompactionSummary {
        total_scanned: summary.total_scanned,
        candidates: moved_candidates,
        permanent_skipped: summary.permanent_skipped,
        policy: summary.policy,
    };

    Json(response).into_response()
}

// Silence unused-import warnings in stubs where serde imports are aspirational.
#[allow(dead_code)]
fn _frontmatter_linked() {
    let _ = frontmatter::parse;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use lightarchitects::soul::embedding::mock::MockEmbeddingProvider;

    #[test]
    fn search_query_default_limit_absent() {
        let q: SearchQuery = serde_json::from_str(r#"{"q":"hello"}"#).unwrap();
        assert_eq!(q.q, "hello");
        assert!(q.limit.is_none());
    }

    #[test]
    fn hot_query_parses_limit() {
        let q: HotMemoryQuery = serde_json::from_str(r#"{"limit":100}"#).unwrap();
        assert_eq!(q.limit, Some(100));
    }

    #[test]
    fn cold_query_parses_sibling_filter() {
        let q: ColdMemoryQuery = serde_json::from_str(r#"{"sibling":"eva","limit":50}"#).unwrap();
        assert_eq!(q.sibling.as_deref(), Some("eva"));
        assert_eq!(q.limit, Some(50));
    }

    // ── Phase 17a — search mode plumbing ────────────────────────────────────

    fn memo(path: &str, sibling: &str, content: &str) -> ContextMemo {
        ContextMemo {
            id: format!("m-{path}"),
            tier: crate::memory::types::MemoryTier::Cold,
            content: content.to_owned(),
            significance: 0.5,
            sibling: sibling.to_owned(),
            strands: vec![],
            created_at: "2026-04-19T00:00:00Z".to_owned(),
            source_path: Some(path.to_owned()),
            resonance: vec![],
            themes: vec![],
            self_defining: false,
            entry_type: None,
        }
    }

    #[test]
    fn search_mode_defaults_to_bm25() {
        let q: SearchQuery = serde_json::from_str(r#"{"q":"hello"}"#).unwrap();
        assert_eq!(q.mode, SearchMode::Bm25);
    }

    #[test]
    fn search_mode_parses_lowercase_variants() {
        let hybrid: SearchQuery = serde_json::from_str(r#"{"q":"x","mode":"hybrid"}"#).unwrap();
        let sem: SearchQuery = serde_json::from_str(r#"{"q":"x","mode":"semantic"}"#).unwrap();
        assert_eq!(hybrid.mode, SearchMode::Hybrid);
        assert_eq!(sem.mode, SearchMode::Semantic);
    }

    #[test]
    fn bm25_ranks_by_substring_hits_only() {
        let candidates = vec![
            memo("a.md", "eva", "the quick brown fox"),
            memo("b.md", "corso", "nothing matches"),
            memo("c.md", "eva", "fox trot"),
        ];
        let ranked = rank_bm25(&candidates, "fox", 10);
        assert_eq!(ranked, vec!["a.md".to_owned(), "c.md".to_owned()]);
    }

    #[tokio::test]
    async fn semantic_embedding_at_query_returns_top_k() {
        // Direct-test the Phase-17a fallback path (no AppState / Neo4j
        // needed). With a deterministic MockEmbeddingProvider, ranking is
        // stable across runs. Phase-17b's fast path is exercised
        // end-to-end in the headed Playwright gate instead.
        let candidates = vec![
            memo("a.md", "eva", "alpha beta gamma"),
            memo("b.md", "corso", "completely unrelated body"),
            memo("c.md", "eva", "alpha"),
        ];
        let provider = MockEmbeddingProvider::nomic();
        let query_vec = provider.embed(&["alpha"]).await.unwrap();
        let ranked =
            rank_semantic_embedding_at_query(&provider, &candidates, &query_vec[0], 2).await;
        assert_eq!(ranked.len(), 2, "limit honoured");
        for path in &ranked {
            assert!(["a.md", "b.md", "c.md"].contains(&path.as_str()));
        }
    }

    #[test]
    fn rrf_fuse_merges_ranks_from_both_sources() {
        let bm25 = vec!["a.md".to_owned(), "b.md".to_owned()];
        let sem = vec!["b.md".to_owned(), "c.md".to_owned()];
        let fused = rrf_fuse_n(&[bm25, sem], 10);
        // b.md appears in both rankings and should rank first after fusion.
        assert_eq!(fused[0], "b.md");
        assert_eq!(fused.len(), 3);
    }

    #[test]
    fn rrf_fuse_respects_limit() {
        let bm25 = vec!["a.md".into(), "b.md".into(), "c.md".into(), "d.md".into()];
        let sem = vec!["a.md".into(), "b.md".into()];
        let fused = rrf_fuse_n(&[bm25, sem], 2);
        assert_eq!(fused.len(), 2);
    }

    // ── Phase 20a — 4-signal RRF fusion ────────────────────────────────────

    #[test]
    fn rrf_fuse_n_four_signals_prefers_universally_ranked_paths() {
        // A path that appears in all 4 signals should outrank any path
        // that only appears in 1 — RRF's canonical contract.
        let bm25 = vec!["universal.md".into(), "a.md".into()];
        let sem = vec!["universal.md".into(), "b.md".into()];
        let graph = vec!["universal.md".into(), "c.md".into()];
        let recency = vec!["universal.md".into(), "d.md".into()];
        let fused = rrf_fuse_n(&[bm25, sem, graph, recency], 10);
        assert_eq!(
            fused[0], "universal.md",
            "path in all 4 signals ranks first"
        );
    }

    #[test]
    fn rrf_fuse_n_drops_empty_signals_silently() {
        // Graceful degradation: an empty signal (e.g. graph when Neo4j
        // is down) still lets the remaining signals fuse correctly.
        let bm25 = vec!["a.md".into(), "b.md".into()];
        let sem = vec!["b.md".into(), "c.md".into()];
        let graph: Vec<String> = Vec::new();
        let recency: Vec<String> = Vec::new();
        let fused = rrf_fuse_n(&[bm25, sem, graph, recency], 10);
        // Same result as the 2-signal case — empty signals contribute 0.
        assert_eq!(fused[0], "b.md");
        assert_eq!(fused.len(), 3);
    }

    #[test]
    fn rank_recency_orders_newer_first() {
        use chrono::{Duration, Utc};
        let now = Utc::now();
        let stamp_old = (now - Duration::days(200)).to_rfc3339();
        let stamp_mid = (now - Duration::days(10)).to_rfc3339();
        let stamp_new = (now - Duration::hours(1)).to_rfc3339();

        let candidates = vec![
            {
                let mut m = memo("old.md", "eva", "old");
                m.created_at = stamp_old;
                m
            },
            {
                let mut m = memo("mid.md", "eva", "mid");
                m.created_at = stamp_mid;
                m
            },
            {
                let mut m = memo("new.md", "eva", "new");
                m.created_at = stamp_new;
                m
            },
        ];
        let ranked = rank_recency(&candidates, 10);
        assert_eq!(
            ranked,
            vec![
                "new.md".to_owned(),
                "mid.md".to_owned(),
                "old.md".to_owned()
            ]
        );
    }

    #[test]
    fn rank_recency_skips_unparseable_timestamps() {
        let mut m = memo("bad.md", "eva", "bad");
        m.created_at = "not-a-date".to_owned();
        let ranked = rank_recency(&[m], 10);
        assert!(ranked.is_empty(), "unparseable dates drop out silently");
    }

    #[test]
    fn cosine_normalised_vectors_dot_product() {
        let a = vec![0.6, 0.8, 0.0];
        let b = vec![0.6, 0.8, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-5);
        let c = vec![0.8, -0.6, 0.0];
        assert!(cosine_similarity(&a, &c).abs() < 1e-5);
    }
}

// ── /api/debug/parity ─────────────────────────────────────────────────────────

/// Response body for `GET /api/debug/parity`.
#[derive(Debug, Serialize)]
pub struct ParityReport {
    /// Number of `Step` nodes in `Neo4j` (`None` when `Neo4j` tier absent).
    pub neo4j_count: Option<i64>,
    /// Number of entries in `SQLite` (`None` when `SQLite` tier absent).
    pub sqlite_count: Option<usize>,
    /// `|neo4j_count - sqlite_count|`, or `null` when either tier is absent.
    pub divergence: Option<i64>,
    /// Whether `SOUL_DISABLE_SQLITE_WRITES` is currently set.
    pub writes_disabled: bool,
}

/// `GET /api/debug/parity` — Phase 20b.3 pre-drop verification.
///
/// Compares the `Step` count in `Neo4j` against the entry count in `SQLite`.
/// A `divergence` of 0 confirms both tiers are in sync before dropping the
/// webshell `SQLite` write path.
///
/// Requires a valid Bearer token.
pub async fn parity_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err(status) = check_auth(&headers, &state.config.token) {
        return status.into_response();
    }

    let soul = state.soul_store.as_ref();

    // Query Neo4j step count.
    let neo4j_count = if let Some(soul) = soul {
        if let Some(neo4j) = soul.neo4j_arc().await {
            match neo4j
                .helix_db()
                .execute_cypher("MATCH (s:Step) RETURN count(s) AS cnt")
                .await
            {
                Ok(rows) => rows
                    .first()
                    .and_then(|r| r.fields.get("cnt"))
                    .and_then(serde_json::Value::as_i64),
                Err(e) => {
                    warn!(target: "parity", error = %e, "Neo4j count query failed");
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    // Query SQLite entry count.
    let sqlite_count = if let Some(soul) = soul {
        match soul
            .query_sqlite(&lightarchitects::soul::storage::EntryFilter::default())
            .await
        {
            Some(Ok(entries)) => Some(entries.len()),
            _ => None,
        }
    } else {
        None
    };

    #[allow(clippy::cast_possible_wrap)]
    let divergence = neo4j_count
        .zip(sqlite_count.map(|n| n as i64))
        .map(|(neo4j, sqlite)| (neo4j - sqlite).abs());

    Json(ParityReport {
        neo4j_count,
        sqlite_count,
        divergence,
        writes_disabled: crate::memory::persistence::SoulPersistence::sqlite_writes_disabled(),
    })
    .into_response()
}
