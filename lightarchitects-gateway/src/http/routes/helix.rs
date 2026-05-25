//! Helix cached-retrieval endpoints — `POST /v1/platform/helix/retrieve` and
//! `GET /v1/platform/helix/cache/stats`.
//!
//! Both endpoints sit under `/v1/platform/helix*` and therefore share the
//! `helix_limiter` (20 req/min per IP) via `rate_limit_middleware`.
//! Auth is enforced by `read_auth_middleware` before this handler is reached.

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use lightarchitects::agent::conversation::helix_memory::SsmState;
use lightarchitects::helix::{
    CachedRetriever, HybridRetriever, HybridRetrieverConfig, RetrievalMode, SearchOptions,
};

use crate::http::state::PlatformState;

/// Wire helix retrieve + cache-stats routes.
pub fn helix_routes() -> Router<Arc<PlatformState>> {
    Router::new()
        .route("/v1/platform/helix/retrieve", post(helix_retrieve))
        .route("/v1/platform/helix/cache/stats", get(helix_cache_stats))
}

// ── Constants ──────────────────────────────────────────────────────────────────

/// Maximum query length in bytes (F6 — CVSS 7.5 DoS prevention, OWASP API4).
const MAX_QUERY_BYTES: usize = 2048;

/// Valid `mode_override` string values (F1 — OWASP API3:2023).
const ALLOWED_MODES: &[&str] = &["keyword_dominated", "balanced", "graph_weighted"];

// ── Request body ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetrieveRequest {
    /// Query string (max 2 KiB).
    query: String,
    /// Optional helix-id filter (`<sibling>/<sibling>` prefix).
    helix_id: Option<String>,
    /// Maximum results after RRF fusion (1–100, default 20).
    top_k: Option<u32>,
    /// Override retrieval mode.  Must be one of `ALLOWED_MODES` or absent.
    mode_override: Option<String>,
}

// ── Handlers ───────────────────────────────────────────────────────────────────

/// `POST /v1/platform/helix/retrieve` — cached hybrid 4-signal retrieval.
///
/// # Errors
///
/// - 400 if the body is malformed JSON (axum extraction).
/// - 422 if `query` exceeds 2 KiB or `mode_override` is not in the allowlist.
/// - 500 if the underlying `HelixDb` or `CachedRetriever` returns an error.
async fn helix_retrieve(
    State(s): State<Arc<PlatformState>>,
    headers: HeaderMap,
    Json(body): Json<RetrieveRequest>,
) -> Result<Response, Response> {
    // F6 — 2 KiB query cap before cache-key construction (CVSS 7.5 DoS prevention).
    if body.query.len() > MAX_QUERY_BYTES {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "error": {
                    "code": "query_too_long",
                    "message": format!(
                        "query exceeds maximum length of {} bytes",
                        MAX_QUERY_BYTES
                    )
                }
            })),
        )
            .into_response());
    }

    // F1 — mode_override allowlist (OWASP API3:2023).
    let mode_override: Option<RetrievalMode> = match body.mode_override.as_deref() {
        None => None,
        Some(m) if ALLOWED_MODES.contains(&m) => {
            // The three allowed strings map 1:1 to RetrievalMode Display impls.
            match m {
                "keyword_dominated" => Some(RetrievalMode::KeywordDominated),
                "balanced" => Some(RetrievalMode::Balanced),
                "graph_weighted" => Some(RetrievalMode::GraphWeighted),
                _ => unreachable!("covered by ALLOWED_MODES check above"),
            }
        }
        Some(invalid) => {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "error": {
                        "code": "invalid_mode_override",
                        "message": format!(
                            "mode_override '{}' is not valid; must be one of {:?}",
                            invalid, ALLOWED_MODES
                        )
                    }
                })),
            )
                .into_response());
        }
    };

    // F8 — helix_id character-set + length validation (Security Guardrails §3.4).
    if let Some(ref hid) = body.helix_id {
        if hid.len() > 128
            || !hid
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '/')
        {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "error": {
                        "code": "invalid_helix_id",
                        "message": "helix_id must be ≤128 bytes, alphanumeric/hyphen/slash only"
                    }
                })),
            )
                .into_response());
        }
    }

    let top_k = body.top_k.unwrap_or(20).clamp(1, 100);

    // F9 — session key is SHA-256(Authorization header bytes), never the raw token.
    let session_key = headers
        .get(header::AUTHORIZATION)
        .map(|v| format!("{:x}", Sha256::digest(v.as_bytes())))
        .unwrap_or_default();

    // Get or create the bounded SSM state for this session (10 K cap, 1-hour idle TTL).
    let ssm_entry = s
        .session_ssm_store
        .get_with(session_key, async {
            std::sync::Arc::new(tokio::sync::Mutex::new(SsmState::new()))
        })
        .await;

    // Read accumulated context bias before search; lock dropped immediately (not Send across await).
    let ssm_bias: Option<Vec<f32>> = {
        let ssm = ssm_entry.lock().await;
        if ssm.turn_count > 0 {
            Some(ssm.query_vec())
        } else {
            None
        }
    };

    let opts = {
        let mut o = SearchOptions::default().with_limit(top_k);
        if let Some(ref hid) = body.helix_id {
            o = o.with_helix(hid.clone());
        }
        o
    };

    let config = HybridRetrieverConfig {
        mode_override,
        top_k,
        embedding: lightarchitects::helix::EmbeddingConfig {
            backend: s.config.embedding_backend.clone(),
            model: s.config.embedding_model.clone(),
            dim: s.config.embedding_dim,
        },
        ssm_query_bias: ssm_bias,
        ..HybridRetrieverConfig::default()
    };

    // Semantic slot: general embedding backend (768-dim).
    // Structural slot: GraphSAGE provider (128-dim, queries step-sage-embeddings HNSW index).
    let retriever = HybridRetriever::new(
        Arc::clone(&s.embedding_provider),
        Arc::clone(&s.sage_provider),
    );
    let cached = CachedRetriever::new(s.helix_cache.clone(), retriever);

    let result = cached
        .search(s.helix_db.as_ref(), &body.query, &opts, &config)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "helix_retrieve: db error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": {"code": "db_error", "message": "retrieval failed"}})),
            )
                .into_response()
        })?;

    // Update SSM with a semantic embedding of the query (P1: replaces byte-position sampler).
    // The embed call is intentionally outside the lock so no await crosses the critical section.
    // Falls back to the byte-sampler when the embedding provider is temporarily unavailable.
    let ssm_input = s
        .embedding_provider
        .embed(&[body.query.as_str()])
        .await
        .ok()
        .and_then(|mut v| v.pop())
        .unwrap_or_else(|| SsmState::input_vec_for_query(&body.query));
    {
        let mut ssm = ssm_entry.lock().await;
        ssm.update(&ssm_input);
    }

    // ETag: SHA-256 of cache key so query text is not exposed in response headers (OWASP LLM02).
    let etag_value = format!("\"{:x}\"", Sha256::digest(result.cache_key.as_bytes()));
    if let Some(inm) = headers.get(header::IF_NONE_MATCH) {
        if inm.as_bytes() == etag_value.as_bytes() {
            return Ok((StatusCode::NOT_MODIFIED, [("etag", etag_value)]).into_response());
        }
    }

    let response_body = json!({
        "results": result.result.results.iter().map(|r| json!({
            "step_id": r.step_id,
            "score": r.score,
        })).collect::<Vec<_>>(),
        "mode": result.mode.to_string(),
        "cache_hit_ratio": result.cache_hit_ratio,
        "count": result.result.results.len(),
    });

    Ok((
        StatusCode::OK,
        [
            ("etag", etag_value),
            ("cache-control", "private, max-age=300".to_owned()),
        ],
        Json(response_body),
    )
        .into_response())
}

/// `GET /v1/platform/helix/cache/stats` — moka cache telemetry (admin-only).
///
/// Requires `x-admin-token` header (Security Guardrails §3.5 RBAC — operational
/// telemetry is not user-facing). Forces `run_pending_tasks()` before reading
/// counters so that in-flight evictions are reflected in the response.
async fn helix_cache_stats(
    State(s): State<Arc<PlatformState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, Response> {
    super::admin::require_admin_token(&s, &headers)?;
    s.helix_cache.run_pending_tasks().await;
    Ok(Json(json!({
        "entry_count": s.helix_cache.entry_count(),
        "weighted_size_bytes": s.helix_cache.weighted_size(),
    })))
}
