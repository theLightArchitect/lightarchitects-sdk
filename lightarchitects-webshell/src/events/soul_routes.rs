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
use lightarchitects::turnlog::StoreLayout;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    auth,
    memory::{
        cold, frontmatter, hot,
        types::{ContextMemo, EnrichedEntry},
    },
    server::AppState,
};
use lightarchitects::helix::HelixDb;

// ── Query parameter shapes ──────────────────────────────────────────────────

/// Query parameters for `GET /api/soul/search`.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Regex/substring pattern to match against entry body + front-matter.
    #[serde(default)]
    pub q: String,
    /// Maximum results to return. Server caps at 100.
    #[serde(default)]
    pub limit: Option<u8>,
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
    /// draw-call cost on the Hero3D scene. Default 500.
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

/// One `SharedExperience` projected for the MemoryDrawer convergence view.
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

/// One `:LINKS_TO` edge as rendered by Hero3D: source + target vault paths.
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
    let needle = pattern.to_lowercase();

    // Reuse cold-walker to get all memos, then filter by substring.
    let all = cold::snapshot_cold(&helix_root, None, 500).await;
    let mut matches: Vec<EnrichedEntry> = Vec::new();
    for memo in all.into_iter().take(500) {
        if memo.content.to_lowercase().contains(&needle)
            || memo.sibling.to_lowercase().contains(&needle)
            || memo.strands.iter().any(|s| s.contains(&needle))
        {
            let Some(path) = &memo.source_path else {
                continue;
            };
            if let Some((entry, _raw)) = cold::read_entry(&helix_root, path).await {
                matches.push(entry);
            }
            if matches.len() >= limit {
                break;
            }
        }
    }

    Json(SearchResponse {
        results: matches,
        rrf_used: false,
    })
    .into_response()
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

/// `GET /api/soul/memory/hot` — snapshot of active-session turnlog memos.
#[allow(clippy::missing_panics_doc)]
pub async fn hot_memory_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    Query(q): Query<HotMemoryQuery>,
) -> impl IntoResponse {
    if let Err(status) = check_auth(&headers, &state.config.token) {
        return status.into_response();
    }
    let Some(layout) = StoreLayout::default_for_user() else {
        return Json(MemoryListResponse { memos: Vec::new() }).into_response();
    };
    let limit = q.limit.unwrap_or(50).min(200) as usize;
    let memos = hot::snapshot_hot(&layout, limit).await;
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

    Json(serde_json::json!({
        "tiers": tiers,
        "counts": counts,
        "bolt_uri": std::env::var("WEBSHELL_NEO4J_URI").unwrap_or_default(),
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

/// `GET /api/soul/edges` — Phase 12 bulk `:LINKS_TO` edges for Hero3D.
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
                let source_sibling = source
                    .split('/')
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                let target_sibling = target
                    .split('/')
                    .next()
                    .unwrap_or_default()
                    .to_owned();
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
        .and_then(|r| r.get("n").and_then(|v| v.as_u64()))
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

    let list_result = db.execute_cypher_with_params(list_cypher, list_params).await;
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
                let label = row
                    .get("label")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
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
                        let title = p
                            .get("title")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned);
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
                let mut siblings: Vec<String> = participants
                    .iter()
                    .map(|p| p.sibling.clone())
                    .collect();
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
        .and_then(|r| r.get("n").and_then(|v| v.as_u64()))
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

// Silence unused-import warnings in stubs where serde imports are aspirational.
#[allow(dead_code)]
fn _frontmatter_linked() {
    let _ = frontmatter::parse;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
}
