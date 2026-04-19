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
