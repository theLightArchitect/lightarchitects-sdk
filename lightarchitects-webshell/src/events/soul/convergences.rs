//! `GET /api/soul/convergences` — `SharedExperience` projection for the Memory drawer.
//!
//! Split out from `soul_routes.rs` (task #51, partial). The route registration
//! in `server/mod.rs` resolves via `events::soul_routes::convergences_handler`
//! re-export, so this split is transparent to upstream callers.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tracing::warn;

use lightarchitects::helix::HelixDb;

use crate::server::AppState;

// ─────────────────────────────────────────────────────────────────────────────
// Auth helper — duplicated in each split file until we migrate to a shared
// `events::soul::auth` module. Kept identical to soul_routes.rs::check_auth so
// behavior is bit-identical during the partial-split window.
// ─────────────────────────────────────────────────────────────────────────────

fn check_auth(headers: &axum::http::HeaderMap, token: &str) -> Result<(), axum::http::StatusCode> {
    let bearer = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    match bearer {
        Some(t) if t == token => Ok(()),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// Handler
// ─────────────────────────────────────────────────────────────────────────────

/// `GET /api/soul/convergences` — list `SharedExperience` clusters for the
/// Memory drawer's Convergences tab.
#[allow(clippy::missing_panics_doc, clippy::too_many_lines)]
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
