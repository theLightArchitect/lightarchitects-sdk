//! Platform content read endpoints — `/v1/platform/*` + `/v1/vault/info`.

use axum::Router;
use axum::extract::{Extension, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use neo4rs::BoltType;
use futures_util::stream;
use serde::Deserialize;
use serde_json::{Value, json};
use std::convert::Infallible;
use std::sync::Arc;

use crate::http::etag::{compute_etag, etag_from_hash, is_not_modified};
use crate::http::middleware::identity_extractor::UserContext;
use crate::http::state::PlatformState;

/// Wire all platform read routes onto the router.
pub fn platform_routes() -> Router<Arc<PlatformState>> {
    Router::new()
        .route("/v1/platform/canon/{name}", get(canon_get))
        .route("/v1/platform/agents/{sibling}", get(agents_get))
        .route(
            "/v1/platform/agents/{sibling}/strands",
            get(agents_strands_get),
        )
        .route("/v1/platform/skills", get(skills_list))
        .route("/v1/platform/skills/{name}", get(skills_get))
        .route("/v1/platform/standards/{name}", get(standards_get))
        .route("/v1/platform/helix/query", get(helix_query))
        .route(
            "/v1/platform/helix/search",
            get(helix_search).post(helix_vector_search),
        )
        .route("/v1/platform/helix/stream", get(helix_stream))
        .route("/v1/platform/health", get(health))
        .route("/v1/identity", get(identity_get))
        .route("/v1/vault/info", get(vault_info))
}

// ── Override-aware handlers ────────────────────────────────────────────────────

/// `GET /v1/platform/canon/:name` — canonical content with optional org override.
///
/// Reads `X-Org-Id` request header. A single OPTIONAL MATCH Cypher fetches
/// both the base node and any per-org override in one round-trip. Results are
/// cached for 60 s keyed on `(name, org_id)`.
async fn canon_get(
    State(s): State<Arc<PlatformState>>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    if let Some(e) = validate_path_param(&name) {
        return Err(e);
    }
    let org_id = extract_org_id(&headers);
    let cache_key = (name.clone(), org_id.clone());

    let body = match s.canon_cache.get(&cache_key).await {
        Some(cached) => {
            tracing::debug!(path = %name, org_id = %org_id, "canon cache hit");
            cached.as_ref().clone()
        }
        None => {
            tracing::debug!(path = %name, org_id = %org_id, "canon cache miss — fetching neo4j");
            let body = fetch_canon_body(&s, &name, &org_id).await?;
            s.canon_cache
                .insert(cache_key, Arc::new(body.clone()))
                .await;
            body
        }
    };

    Ok(respond_with_body_etag(body, &headers))
}

/// `GET /v1/platform/agents/:sibling` — full agent identity with optional org override.
async fn agents_get(
    State(s): State<Arc<PlatformState>>,
    Path(sibling): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    if let Some(e) = validate_path_param(&sibling) {
        return Err(e);
    }
    let org_id = extract_org_id(&headers);
    let cache_key = (format!("agents/{sibling}"), org_id.clone());

    let body = match s.agent_cache.get(&cache_key).await {
        Some(cached) => {
            tracing::debug!(sibling = %sibling, org_id = %org_id, "agent cache hit");
            cached.as_ref().clone()
        }
        None => {
            tracing::debug!(sibling = %sibling, org_id = %org_id, "agent cache miss — fetching neo4j");
            let body = fetch_agent_body(&s, &sibling, &org_id).await?;
            s.agent_cache
                .insert(cache_key, Arc::new(body.clone()))
                .await;
            body
        }
    };

    Ok(respond_with_body_etag(body, &headers))
}

/// `GET /v1/platform/agents/:sibling/strands` — strands list only.
///
/// Reuses the agent cache (same full body) to avoid a redundant Cypher
/// round-trip when the full agent was recently fetched.
async fn agents_strands_get(
    State(s): State<Arc<PlatformState>>,
    Path(sibling): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    if let Some(e) = validate_path_param(&sibling) {
        return Err(e);
    }
    let org_id = extract_org_id(&headers);
    let cache_key = (format!("agents/{sibling}"), org_id.clone());

    let full_body = match s.agent_cache.get(&cache_key).await {
        Some(cached) => {
            tracing::debug!(sibling = %sibling, "strands cache hit");
            cached.as_ref().clone()
        }
        None => {
            tracing::debug!(sibling = %sibling, "strands cache miss — fetching neo4j");
            let body = fetch_agent_body(&s, &sibling, &org_id).await?;
            s.agent_cache
                .insert(cache_key, Arc::new(body.clone()))
                .await;
            body
        }
    };

    let strands_body = json!({
        "sibling": sibling,
        "strands": full_body.get("strands").cloned().unwrap_or(Value::Array(vec![])),
    });

    Ok(respond_with_body_etag(strands_body, &headers))
}

// ── Non-override handlers ──────────────────────────────────────────────────────

/// `GET /v1/platform/skills` — cursor-paginated published skills.
async fn skills_list(
    State(s): State<Arc<PlatformState>>,
    Query(q): Query<SkillsListQuery>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    let limit = q.limit.min(100);
    let rs_query = if let Some(after) = &q.after_id {
        neo4rs::query(
            "MATCH (s:Skill { published: true }) WHERE s.name > $after \
             RETURN s.name AS name, s.description AS description, s.version AS version, \
                    s.trigger_patterns AS trigger_patterns, \
                    s.content_hash AS content_hash, s.updated_at AS updated_at \
             ORDER BY s.name LIMIT $limit",
        )
        .param("after", after.clone())
        .param("limit", limit as i64)
    } else {
        neo4rs::query(
            "MATCH (s:Skill { published: true }) \
             RETURN s.name AS name, s.description AS description, s.version AS version, \
                    s.trigger_patterns AS trigger_patterns, \
                    s.content_hash AS content_hash, s.updated_at AS updated_at \
             ORDER BY s.name LIMIT $limit",
        )
        .param("limit", limit as i64)
    };

    let mut rs = s.graph.execute(rs_query).await.map_err(|e| db_error(&e))?;
    let mut skills: Vec<Value> = Vec::new();
    while let Ok(Some(row)) = rs.next().await {
        skills.push(json!({
            "name": row.get::<String>("name").unwrap_or_default(),
            "description": row.get::<String>("description").ok(),
            "version": row.get::<String>("version").unwrap_or_default(),
            "trigger_patterns": row.get::<Vec<String>>("trigger_patterns").unwrap_or_default(),
            "content_hash": row.get::<String>("content_hash").unwrap_or_default(),
            "updated_at": row.get::<String>("updated_at").unwrap_or_default(),
        }));
    }

    let next_cursor = if skills.len() == limit {
        skills
            .last()
            .and_then(|s| s.get("name"))
            .and_then(Value::as_str)
            .map(String::from)
    } else {
        None
    };

    let body = json!({ "skills": skills, "next_cursor": next_cursor });
    Ok(respond_with_body_etag(body, &headers))
}

/// `GET /v1/platform/skills/:name` — single skill by name.
async fn skills_get(
    State(s): State<Arc<PlatformState>>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    if let Some(e) = validate_path_param(&name) {
        return Err(e);
    }
    let q = neo4rs::query(
        "MATCH (s:Skill { name: $name }) \
         RETURN s.name AS name, s.description AS description, s.version AS version, \
                s.trigger_patterns AS trigger_patterns, s.published AS published, \
                s.content_hash AS content_hash, s.updated_at AS updated_at",
    )
    .param("name", name.clone());

    let mut rs = s.graph.execute(q).await.map_err(|e| db_error(&e))?;
    let row = rs
        .next()
        .await
        .map_err(|e| db_error(&e))?
        .ok_or_else(|| not_found(&name))?;

    let hash = row.get::<String>("content_hash").unwrap_or_default();
    let etag = etag_from_hash(&hash);
    if is_not_modified(
        headers
            .get(header::IF_NONE_MATCH)
            .and_then(|v| v.to_str().ok()),
        &etag,
    ) {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }

    let body = json!({
        "name": row.get::<String>("name").unwrap_or_default(),
        "description": row.get::<String>("description").ok(),
        "version": row.get::<String>("version").unwrap_or_default(),
        "trigger_patterns": row.get::<Vec<String>>("trigger_patterns").unwrap_or_default(),
        "published": row.get::<bool>("published").unwrap_or(false),
        "content_hash": hash,
        "updated_at": row.get::<String>("updated_at").unwrap_or_default(),
    });
    Ok(etag_response(StatusCode::OK, body, &etag))
}

/// `GET /v1/platform/standards/:name` — canonical standard document.
async fn standards_get(
    State(s): State<Arc<PlatformState>>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    if let Some(e) = validate_path_param(&name) {
        return Err(e);
    }
    let q = neo4rs::query(
        "MATCH (s:Standard { name: $name }) \
         RETURN s.name AS name, s.title AS title, \
                s.content_text AS content_text, \
                s.content_hash AS content_hash, s.updated_at AS updated_at",
    )
    .param("name", name.clone());

    let mut rs = s.graph.execute(q).await.map_err(|e| db_error(&e))?;
    let row = rs
        .next()
        .await
        .map_err(|e| db_error(&e))?
        .ok_or_else(|| not_found(&name))?;

    let hash = row.get::<String>("content_hash").unwrap_or_default();
    let etag = etag_from_hash(&hash);
    if is_not_modified(
        headers
            .get(header::IF_NONE_MATCH)
            .and_then(|v| v.to_str().ok()),
        &etag,
    ) {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }

    let body = json!({
        "name": row.get::<String>("name").unwrap_or_default(),
        "title": row.get::<String>("title").ok(),
        "content_text": row.get::<String>("content_text").ok(),
        "content_hash": hash,
        "updated_at": row.get::<String>("updated_at").unwrap_or_default(),
    });
    Ok(etag_response(StatusCode::OK, body, &etag))
}

/// Max byte lengths for `helix_query` filter parameters.
const HQ_MAX_SEARCH_LEN: usize = 512;
const HQ_MAX_PARAM_LEN: usize = 128;
/// Expected dimension for `step-embeddings` HNSW index (corrected in migration v10).
const HELIX_VECTOR_DIM: usize = 384;

/// Validate `HelixQueryParams` — returns a 400 response on the first violation.
fn validate_helix_params(q: &HelixQueryParams) -> Option<Response> {
    let too_long = q.search.as_deref().is_some_and(|s| s.len() > HQ_MAX_SEARCH_LEN)
        || q.sibling.as_deref().is_some_and(|s| s.len() > HQ_MAX_PARAM_LEN)
        || q.tag.as_deref().is_some_and(|s| s.len() > HQ_MAX_PARAM_LEN)
        || q.after_id.as_deref().is_some_and(|s| s.len() > HQ_MAX_PARAM_LEN);
    if too_long {
        return Some(
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": { "code": "param_too_long", "status": 400 } })),
            )
                .into_response(),
        );
    }
    if let Some(sig) = q.min_sig {
        if sig.is_nan() || !(0.0..=10.0).contains(&sig) {
            return Some(
                (
                    StatusCode::BAD_REQUEST,
                    Json(
                        json!({ "error": { "code": "invalid_param", "message": "min_sig must be in [0.0, 10.0]", "status": 400 } }),
                    ),
                )
                    .into_response(),
            );
        }
    }
    None
}

/// `GET /v1/platform/helix/query` — filtered helix journal entry listing.
///
/// Queries `Step` nodes (2800+ journal entries), NOT `Helix` root nodes.
/// Supports sibling, significance, tag, and fulltext filters with pivot-based
/// cursor pagination via `after_id` / `next_cursor`.
///
/// Auth: governed by `read_auth_middleware` — freely accessible when no read token
/// is configured (localhost trust model). If the gateway binds to a non-loopback
/// address, configure a read token to protect journal entry content.
///
/// Note: `?q=` fulltext search uses `CONTAINS` — O(n) full scan on `Step.content`.
/// At ~3000 entries this is tolerable. Add a Neo4j full-text index before scaling.
async fn helix_query(
    State(s): State<Arc<PlatformState>>,
    Query(q): Query<HelixQueryParams>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    if let Some(err) = validate_helix_params(&q) {
        return Err(err);
    }
    // clamp: 1 minimum prevents a semantically empty page; 100 maximum caps DB load.
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    // Fetch one extra row to detect whether a next page exists without a separate COUNT query.
    let rs_query = build_helix_query(&q, limit + 1);

    let mut rs = s.graph.execute(rs_query).await.map_err(|e| db_error(&e))?;
    let mut entries: Vec<Value> = Vec::new();
    while let Ok(Some(row)) = rs.next().await {
        // helix_id is null for Steps promoted from turnlog — treat as Option.
        let helix_id: Option<String> = row.get::<String>("helix_id").ok();
        // helix_id format: "<sibling>/<sibling>" — extract the first segment.
        let sibling: Option<String> = helix_id.as_deref().map(|h| {
            h.split_once('/').map_or(h, |(a, _)| a).to_owned()
        });
        entries.push(json!({
            "id": row.get::<String>("id").unwrap_or_default(),
            "content": row.get::<String>("content").ok(),
            "content_hash": row.get::<String>("content_hash").ok(),
            "significance": row.get::<f64>("significance").ok(),
            "vault_path": row.get::<String>("vault_path").ok(),
            "created_at": row.get::<String>("created_at").ok(),
            "helix_id": helix_id,
            "sibling": sibling,
        }));
    }

    // The extra sentinel row tells us whether a next page exists.
    let has_more = entries.len() > limit;
    if has_more {
        entries.truncate(limit);
    }
    let next_cursor: Option<String> = if has_more {
        entries.last().and_then(|e| e["id"].as_str()).map(String::from)
    } else {
        None
    };

    let body = json!({
        "entries": entries,
        "count": entries.len(),
        "next_cursor": next_cursor,
    });
    Ok(respond_with_body_etag(body, &headers))
}

/// `GET /v1/platform/helix/search` — BM25 fulltext search over Step journal entries.
///
/// Uses the `step-fulltext` Lucene index (English analyzer, eventually consistent).
/// Supports Lucene query syntax in `?q=` (e.g., `canon AND significance`, `"builders cookbook"`).
/// Results are ordered by BM25 relevance score descending.
///
/// For HNSW vector search (cosine similarity over 384-dim embeddings), use
/// `POST /v1/platform/helix/search` with a JSON body containing `"vector": [...]`.
///
/// Auth: same as `helix_query` — governed by `read_auth_middleware`.
/// Rate limit: 20 req/min per IP (helix tier).
async fn helix_search(
    State(s): State<Arc<PlatformState>>,
    Query(q): Query<HelixSearchParams>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    // Validate inputs.
    if q.q.is_empty() || q.q.len() > HQ_MAX_SEARCH_LEN {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": { "code": "invalid_param", "message": "q must be 1–512 chars", "status": 400 } })),
        )
            .into_response());
    }
    if q.sibling.as_deref().is_some_and(|s| s.len() > HQ_MAX_PARAM_LEN) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": { "code": "param_too_long", "status": 400 } })),
        )
            .into_response());
    }
    let min_score = q.min_score.unwrap_or(0.0);
    if min_score.is_nan() || min_score < 0.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": { "code": "invalid_param", "message": "min_score must be >= 0.0", "status": 400 } })),
        )
            .into_response());
    }
    let k = q.k.unwrap_or(10).clamp(1, 50);

    // Build Cypher — sibling filter applied as a WHERE predicate after YIELD.
    // The fulltext procedure returns results sorted by score DESC; we re-sort + LIMIT
    // after the WHERE so the sibling filter doesn't cut into our page size.
    let cypher_str = if q.sibling.is_some() {
        "CALL db.index.fulltext.queryNodes('step-fulltext', $q) YIELD node AS s, score \
         WHERE score >= $min_score AND s.helix_id STARTS WITH $sibling_prefix \
         RETURN s.id AS id, s.content AS content, s.content_hash AS content_hash, \
                s.significance AS significance, s.vault_path AS vault_path, \
                toString(s.created_at) AS created_at, s.helix_id AS helix_id, score \
         ORDER BY score DESC LIMIT $k"
    } else {
        "CALL db.index.fulltext.queryNodes('step-fulltext', $q) YIELD node AS s, score \
         WHERE score >= $min_score \
         RETURN s.id AS id, s.content AS content, s.content_hash AS content_hash, \
                s.significance AS significance, s.vault_path AS vault_path, \
                toString(s.created_at) AS created_at, s.helix_id AS helix_id, score \
         ORDER BY score DESC LIMIT $k"
    };

    let mut neo_q = neo4rs::query(cypher_str)
        .param("q", q.q.clone())
        .param("min_score", min_score)
        .param("k", k as i64);

    if let Some(sibling) = &q.sibling {
        // helix_id format: "<sibling>/<sibling>" — STARTS WITH filters to the correct helix.
        neo_q = neo_q.param("sibling_prefix", format!("{sibling}/"));
    }

    let mut rs = s.graph.execute(neo_q).await.map_err(|e| db_error(&e))?;
    let mut results: Vec<Value> = Vec::new();
    while let Ok(Some(row)) = rs.next().await {
        let helix_id: Option<String> = row.get::<String>("helix_id").ok();
        let sibling_out: Option<String> = helix_id.as_deref().map(|h| {
            h.split_once('/').map_or(h, |(a, _)| a).to_owned()
        });
        results.push(json!({
            "id": row.get::<String>("id").unwrap_or_default(),
            "content": row.get::<String>("content").ok(),
            "content_hash": row.get::<String>("content_hash").ok(),
            "significance": row.get::<f64>("significance").ok(),
            "vault_path": row.get::<String>("vault_path").ok(),
            "created_at": row.get::<String>("created_at").ok(),
            "helix_id": helix_id,
            "sibling": sibling_out,
            "score": row.get::<f64>("score").unwrap_or(0.0),
        }));
    }

    let body = json!({
        "results": results,
        "count": results.len(),
        "query": q.q,
        "search_type": "fulltext",
    });
    Ok(respond_with_body_etag(body, &headers))
}

/// `POST /v1/platform/helix/search` — HNSW cosine vector search over Step embeddings.
///
/// Requires a caller-supplied 384-dim float vector; the platform has no embedded model.
/// Uses the `step-embeddings` HNSW index (cosine similarity, 384-dim — corrected in
/// helix migration v10). Results are ordered by cosine similarity descending.
///
/// Request body (JSON):
/// ```json
/// { "vector": [0.12, -0.03, ...], "k": 10, "min_score": 0.0, "sibling": "ayin" }
/// ```
///
/// - `vector` (required): exactly 384 floats — the caller is responsible for embedding the query.
/// - `k` (optional, 1–50, default 10): number of nearest neighbours to return.
/// - `min_score` (optional, ≥ 0.0, default 0.0): minimum cosine similarity threshold.
/// - `sibling` (optional): restrict results to a specific sibling helix (e.g. `"ayin"`).
///
/// Note: `db.index.vector.queryNodes` retrieves the global k nearest before applying
/// the sibling filter, so a narrow `sibling` filter with a large `k` may return fewer
/// than `k` results. Callers should over-request `k` when filtering by sibling.
///
/// Auth: governed by `read_auth_middleware`. Rate limit: 20 req/min (helix tier).
async fn helix_vector_search(
    State(s): State<Arc<PlatformState>>,
    headers: HeaderMap,
    Json(body): Json<HelixVectorSearchRequest>,
) -> Result<Response, Response> {
    let (k, min_score) = validate_vector_search_params(&body)?;

    let cypher_str = if body.sibling.is_some() {
        "CALL db.index.vector.queryNodes('step-embeddings', $k, $vector) YIELD node AS s, score \
         WHERE score >= $min_score AND s.helix_id STARTS WITH $sibling_prefix \
         RETURN s.id AS id, s.content AS content, s.content_hash AS content_hash, \
                s.significance AS significance, s.vault_path AS vault_path, \
                toString(s.created_at) AS created_at, s.helix_id AS helix_id, score \
         ORDER BY score DESC"
    } else {
        "CALL db.index.vector.queryNodes('step-embeddings', $k, $vector) YIELD node AS s, score \
         WHERE score >= $min_score \
         RETURN s.id AS id, s.content AS content, s.content_hash AS content_hash, \
                s.significance AS significance, s.vault_path AS vault_path, \
                toString(s.created_at) AS created_at, s.helix_id AS helix_id, score \
         ORDER BY score DESC"
    };

    // Convert Vec<f64> → BoltType::List via the From<&[A]> impl (each f64 → BoltFloat).
    let vector_bolt = BoltType::from(body.vector.as_slice());
    let mut neo_q = neo4rs::query(cypher_str)
        .param("vector", vector_bolt)
        .param("min_score", min_score)
        .param("k", k as i64);

    if let Some(sibling) = &body.sibling {
        neo_q = neo_q.param("sibling_prefix", format!("{sibling}/"));
    }

    let mut rs = s.graph.execute(neo_q).await.map_err(|e| db_error(&e))?;
    let mut results: Vec<Value> = Vec::new();
    while let Ok(Some(row)) = rs.next().await {
        results.push(vector_row_to_json(&row));
    }

    Ok(respond_with_body_etag(
        json!({
            "results": results,
            "count": results.len(),
            "search_type": "vector",
            "dimensions": HELIX_VECTOR_DIM,
        }),
        &headers,
    ))
}

/// Validate `HelixVectorSearchRequest` fields; return `(k, min_score)` on success.
// Response is a large type by design — boxing it would require changing all handler return types.
#[allow(clippy::result_large_err)]
fn validate_vector_search_params(
    body: &HelixVectorSearchRequest,
) -> Result<(usize, f64), Response> {
    if body.vector.len() != HELIX_VECTOR_DIM {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {
                    "code": "invalid_param",
                    "message": format!(
                        "vector must be exactly {HELIX_VECTOR_DIM} dimensions, got {}",
                        body.vector.len()
                    ),
                    "status": 400,
                }
            })),
        )
            .into_response());
    }
    if body
        .sibling
        .as_deref()
        .is_some_and(|si| si.is_empty() || si.len() > HQ_MAX_PARAM_LEN)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": { "code": "param_too_long", "status": 400 } })),
        )
            .into_response());
    }
    let min_score = body.min_score.unwrap_or(0.0);
    if min_score.is_nan() || min_score < 0.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {
                    "code": "invalid_param",
                    "message": "min_score must be >= 0.0",
                    "status": 400,
                }
            })),
        )
            .into_response());
    }
    Ok((body.k.unwrap_or(10).clamp(1, 50), min_score))
}

/// Map a neo4rs row from the vector search Cypher query to a JSON value.
fn vector_row_to_json(row: &neo4rs::Row) -> Value {
    let helix_id: Option<String> = row.get::<String>("helix_id").ok();
    let sibling_out: Option<String> = helix_id
        .as_deref()
        .map(|h| h.split_once('/').map_or(h, |(a, _)| a).to_owned());
    json!({
        "id": row.get::<String>("id").unwrap_or_default(),
        "content": row.get::<String>("content").ok(),
        "content_hash": row.get::<String>("content_hash").ok(),
        "significance": row.get::<f64>("significance").ok(),
        "vault_path": row.get::<String>("vault_path").ok(),
        "created_at": row.get::<String>("created_at").ok(),
        "helix_id": helix_id,
        "sibling": sibling_out,
        "score": row.get::<f64>("score").unwrap_or(0.0),
    })
}

/// `GET /v1/platform/helix/stream` — progressive SSE stream of Step journal entries.
///
/// Shares filter params with `helix_query` (`sibling`, `min_sig`, `tag`, `q`, `after_id`).
/// Each matched `Step` is emitted as an SSE `step` event with the Step's UUID as the event id.
/// A terminal `done` event carries the total row count after the stream is exhausted.
///
/// **Resumption**: Pass `Last-Event-ID: <uuid>` to resume from the last received entry.
/// This takes precedence over `?after_id=` if both are present.
///
/// **Limit**: 1–500, default 100. Higher than `helix_query` because rows arrive progressively.
///
/// Rate limit: 20 req/min (helix tier). Auth: inherited from `read_auth_middleware`.
///
/// Note: once the 200 OK + SSE headers are sent, row-level Neo4j errors terminate the stream
/// silently — the `done` event's `error` field will be true in that case.
async fn helix_stream(
    State(s): State<Arc<PlatformState>>,
    Query(mut q): Query<HelixQueryParams>,
    headers: HeaderMap,
) -> Result<Sse<impl stream::Stream<Item = Result<Event, Infallible>> + Send>, Response> {
    if let Some(err) = validate_helix_params(&q) {
        return Err(err);
    }
    // `Last-Event-ID` takes precedence for SSE resumption — overrides ?after_id= if set.
    if let Some(last_id) = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
    {
        q.after_id = Some(last_id.to_owned());
    }
    // Streaming allows a larger limit — client consumes rows progressively.
    let limit = q.limit.unwrap_or(100).clamp(1, 500);

    // Execute the query here — failures before the stream starts can return HTTP errors.
    let rs = s
        .graph
        .execute(build_helix_query(&q, limit))
        .await
        .map_err(|e| db_error(&e))?;

    // Wrap the DetachedRowStream in an unfold so each row becomes one SSE event.
    // State: (row_stream, rows_emitted, limit, stream_done).
    // `stream_done` lets us emit the terminal `done` event before closing.
    let event_stream = stream::unfold(
        (rs, 0_usize, limit, false),
        |(mut rs, count, limit, done_sent)| async move {
            if done_sent {
                return None;
            }
            // If we've emitted `limit` rows, skip to the terminal event.
            if count < limit {
                match rs.next().await {
                    Ok(Some(row)) => {
                        let helix_id: Option<String> = row.get::<String>("helix_id").ok();
                        let sibling: Option<String> = helix_id.as_deref().map(|h| {
                            h.split_once('/').map_or(h, |(a, _)| a).to_owned()
                        });
                        let id = row.get::<String>("id").unwrap_or_default();
                        let payload = json!({
                            "id": id,
                            "content": row.get::<String>("content").ok(),
                            "content_hash": row.get::<String>("content_hash").ok(),
                            "significance": row.get::<f64>("significance").ok(),
                            "vault_path": row.get::<String>("vault_path").ok(),
                            "created_at": row.get::<String>("created_at").ok(),
                            "helix_id": helix_id,
                            "sibling": sibling,
                        });
                        // id() enables Last-Event-ID resumption.
                        let event = Event::default()
                            .event("step")
                            .id(id)
                            .json_data(payload)
                            .ok()?;
                        return Some((Ok(event), (rs, count + 1, limit, false)));
                    }
                    Ok(None) => {
                        // Stream exhausted — fall through to emit `done`.
                    }
                    Err(_) => {
                        // DB error mid-stream — emit `done` with error flag then stop.
                        let ev = Event::default()
                            .event("done")
                            .json_data(json!({ "count": count, "error": true }))
                            .ok()?;
                        return Some((Ok(ev), (rs, count, limit, true)));
                    }
                }
            }
            // Normal end-of-stream — emit `done` so clients know the stream closed cleanly.
            let ev = Event::default()
                .event("done")
                .json_data(json!({ "count": count, "error": false }))
                .ok()?;
            Some((Ok(ev), (rs, count, limit, true)))
        },
    );

    Ok(Sse::new(event_stream).keep_alive(KeepAlive::default()))
}

/// `GET /v1/platform/health` — deep liveness probe with Neo4j check.
///
/// Runs `MATCH (m:Migration) RETURN count(m) AS n` inside a 5-second deadline.
/// Returns 200 `healthy` when Neo4j is reachable, 503 `degraded` otherwise.
/// Records CB success on a healthy probe; does NOT record failure — health is
/// auth-exempt and unauthenticated callers could otherwise use it to trip the
/// circuit breaker as a denial-of-service vector.
/// Auth-exempt; rate-limit-exempt.
pub async fn health(State(s): State<Arc<PlatformState>>) -> Response {
    let start = std::time::Instant::now();

    // Execute the Neo4j probe inside a 5-second timeout.
    let probe = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        async {
            let mut rs = s
                .graph
                .execute(neo4rs::query("MATCH (m:Migration) RETURN count(m) AS n"))
                .await
                .map_err(|e| e.to_string())?;
            let count = if let Ok(Some(row)) = rs.next().await {
                row.get::<i64>("n").unwrap_or(0)
            } else {
                0
            };
            Ok::<i64, String>(count)
        },
    )
    .await;

    let latency_ms = start.elapsed().as_millis() as u64;

    match probe {
        Ok(Ok(migrations_applied)) => {
            s.circuit_breaker.lock().await.record_success();
            Json(json!({
                "status": "healthy",
                "neo4j": "ok",
                "migrations_applied": migrations_applied,
                "latency_ms": latency_ms,
            }))
            .into_response()
        }
        Ok(Err(e)) => {
            tracing::warn!(error = %e, latency_ms, "health probe: Neo4j error");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "status": "degraded",
                    "neo4j": "error",
                    "migrations_applied": null,
                    "latency_ms": latency_ms,
                })),
            )
                .into_response()
        }
        Err(_timeout) => {
            tracing::warn!(latency_ms, "health probe: Neo4j timeout (>5s)");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "status": "degraded",
                    "neo4j": "timeout",
                    "migrations_applied": null,
                    "latency_ms": latency_ms,
                })),
            )
                .into_response()
        }
    }
}

/// `GET /v1/identity` — current user identity and scope policy.
///
/// Returns the resolved `user_id` from the request's [`UserContext`] (injected by
/// `identity_extractor_middleware`), not the platform's static config default.
async fn identity_get(
    State(s): State<Arc<PlatformState>>,
    Extension(ctx): Extension<UserContext>,
) -> Result<Response, Response> {
    let body = json!({
        "user_id": ctx.user_id,
        "identity_scope_policy": s.config.identity_scope_policy.to_string(),
    });
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    Ok(etag_response(StatusCode::OK, body, &compute_etag(&bytes)))
}

/// `GET /v1/vault/info` — node-count summary + platform metadata for vault monitoring.
async fn vault_info(State(s): State<Arc<PlatformState>>) -> Result<Response, Response> {
    let mut rs = s
        .graph
        .execute(neo4rs::query(
            "MATCH (n) RETURN labels(n)[0] AS label, count(n) AS cnt \
             ORDER BY cnt DESC LIMIT 20",
        ))
        .await
        .map_err(|e| db_error(&e))?;

    let mut tier_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    while let Ok(Some(row)) = rs.next().await {
        let label = row.get::<String>("label").unwrap_or_default();
        let cnt = row.get::<i64>("cnt").unwrap_or(0);
        tier_counts.insert(label, cnt);
    }

    let body = json!({
        "user_id": s.config.user_id,
        "api_version": s.config.api_version,
        "tier_counts": tier_counts,
    });
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    Ok(etag_response(StatusCode::OK, body, &compute_etag(&bytes)))
}

// ── Fetch helpers (Neo4j + override) ──────────────────────────────────────────

/// Fetch and compose a `PlatformEntry` body, applying any `OrgOverride` for `org_id`.
async fn fetch_canon_body(s: &PlatformState, name: &str, org_id: &str) -> Result<Value, Response> {
    tracing::debug!(path = %name, has_org_override = !org_id.is_empty(), "neo4j: fetch canon");
    let q = neo4rs::query(
        "MATCH (p:PlatformEntry { path: $path }) \
         OPTIONAL MATCH (o:OrgOverride { org_id: $org_id, target_path: $path }) \
         RETURN p.kind AS kind, p.content_json AS content_json, \
                p.content_text AS content_text, p.version AS version, \
                p.updated_at AS updated_at, p.content_hash AS content_hash, \
                o.override_value AS override_value",
    )
    .param("path", name.to_owned())
    .param("org_id", org_id.to_owned());

    let mut rs = s.graph.execute(q).await.map_err(|e| db_error(&e))?;
    let row = rs
        .next()
        .await
        .map_err(|e| db_error(&e))?
        .ok_or_else(|| not_found(name))?;

    let content_json = row
        .get::<String>("content_json")
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok());

    let mut body = json!({
        "path": name,
        "kind": row.get::<String>("kind").unwrap_or_default(),
        "content_json": content_json,
        "content_text": row.get::<String>("content_text").ok(),
        "version": row.get::<String>("version").unwrap_or_default(),
        "updated_at": row.get::<String>("updated_at").unwrap_or_default(),
        "content_hash": row.get::<String>("content_hash").unwrap_or_default(),
    });

    if let Ok(ov) = row.get::<String>("override_value") {
        merge_override(&mut body, &ov);
    }

    Ok(body)
}

/// Fetch and compose a `SiblingIdentity` body, applying any `OrgOverride` for `org_id`.
async fn fetch_agent_body(
    s: &PlatformState,
    sibling: &str,
    org_id: &str,
) -> Result<Value, Response> {
    tracing::debug!(sibling = %sibling, has_org_override = !org_id.is_empty(), "neo4j: fetch agent");
    let target_path = format!("agents/{sibling}");
    let q = neo4rs::query(
        "MATCH (s:SiblingIdentity { sibling: $sibling }) \
         OPTIONAL MATCH (o:OrgOverride { org_id: $org_id, target_path: $target_path }) \
         RETURN s.role AS role, s.voice AS voice, s.strands AS strands, \
                s.content_hash AS content_hash, s.updated_at AS updated_at, \
                o.override_value AS override_value",
    )
    .param("sibling", sibling.to_owned())
    .param("org_id", org_id.to_owned())
    .param("target_path", target_path);

    let mut rs = s.graph.execute(q).await.map_err(|e| db_error(&e))?;
    let row = rs
        .next()
        .await
        .map_err(|e| db_error(&e))?
        .ok_or_else(|| not_found(sibling))?;

    let mut body = json!({
        "sibling": sibling,
        "role": row.get::<String>("role").ok(),
        "voice": row.get::<String>("voice").ok(),
        "strands": row.get::<Vec<String>>("strands").unwrap_or_default(),
        "content_hash": row.get::<String>("content_hash").unwrap_or_default(),
        "updated_at": row.get::<String>("updated_at").unwrap_or_default(),
    });

    if let Ok(ov) = row.get::<String>("override_value") {
        merge_override(&mut body, &ov);
    }

    Ok(body)
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Reject path parameters that contain traversal sequences or non-printable bytes.
///
/// Returns `Some(error_response)` when the value is invalid; `None` when clean.
/// Allowed: printable ASCII (0x20–0x7E) excluding `/` and `\`.
/// Rejected: empty string, `..`, `/`, `\`, any byte outside 0x20–0x7E.
///
/// Pub-crate so admin handlers can reuse the same validation discipline.
pub(crate) fn validate_path_param(val: &str) -> Option<Response> {
    let invalid = val.is_empty()
        || val.contains("..")
        || val.contains('/')
        || val.contains('\\')
        || val.bytes().any(|b| !(0x20..=0x7E).contains(&b));
    if invalid {
        return Some(
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": { "code": "invalid_path", "status": 400 } })),
            )
                .into_response(),
        );
    }
    None
}

/// Extract the `X-Org-Id` header value, returning `""` when absent.
fn extract_org_id(headers: &HeaderMap) -> String {
    headers
        .get("x-org-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned()
}

/// Shallow-merge a JSON-object override patch into `base`.
///
/// Unknown keys in the patch are added; existing keys are replaced. Non-object
/// patches are silently ignored — the base is returned unmodified.
fn merge_override(base: &mut Value, override_json: &str) {
    if let Ok(Value::Object(patch)) = serde_json::from_str::<Value>(override_json) {
        if let Value::Object(base_map) = base {
            for (k, v) in patch {
                base_map.insert(k, v);
            }
        }
    }
}

/// Serialize `body`, compute a SHA-256 ETag, check `If-None-Match`, return 304 or 200.
fn respond_with_body_etag(body: Value, headers: &HeaderMap) -> Response {
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    let etag = compute_etag(&bytes);
    let inm = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok());
    if is_not_modified(inm, &etag) {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    etag_response(StatusCode::OK, body, &etag)
}

/// Build a parameterized Cypher query over `Step` journal entries with optional filters.
///
/// **Cursor invariant (DESC sort)**: `ORDER BY created_at DESC, id DESC`. The cursor
/// clause `created_at < pivot.created_at OR (created_at = pivot.created_at AND id < pivot.id)`
/// continues from the pivot row. The tie-break `id < pivot.id` is correct for DESC: smaller
/// id strings appear later in the result set. Step IDs are UUID v4 (random, not time-ordered),
/// so the tie-break order is stable but not time-semantic — acceptable since nanosecond-precision
/// `created_at` timestamps make same-timestamp ties vanishingly rare in practice.
/// If sort direction ever changes to ASC, flip the tie-break to `id > pivot.id`.
///
/// When `after_id` is `None` the cursor block is omitted entirely, avoiding reliance on
/// the sentinel convention `""` → no-match → full scan.
fn build_helix_query(q: &HelixQueryParams, limit: usize) -> neo4rs::Query {
    let has_cursor = q.after_id.as_deref().is_some_and(|s| !s.is_empty());

    let mut cypher = String::new();
    if has_cursor {
        cypher.push_str("OPTIONAL MATCH (pivot:Step {id: $after_id}) WITH pivot ");
    }

    if q.sibling.is_some() {
        cypher.push_str("MATCH (h:Helix {name: $sibling})-[:HAS_STEP]->(s:Step) ");
    } else {
        cypher.push_str("MATCH (s:Step) ");
    }

    if q.tag.is_some() {
        cypher.push_str("MATCH (s)-[:MEMBER_OF]->(str:Strand {name: $tag}) ");
    }

    let mut where_parts: Vec<String> = Vec::new();
    if has_cursor {
        where_parts.push(
            "(pivot IS NULL \
              OR s.created_at < pivot.created_at \
              OR (s.created_at = pivot.created_at AND s.id < pivot.id))"
                .to_string(),
        );
    }
    if q.min_sig.is_some() {
        where_parts.push("s.significance >= $min_sig".to_string());
    }
    if q.search.is_some() {
        // O(n) full scan — add a Neo4j full-text index before scaling past ~10k Steps.
        where_parts.push("toLower(s.content) CONTAINS toLower($q_text)".to_string());
    }

    if !where_parts.is_empty() {
        cypher.push_str("WHERE ");
        cypher.push_str(&where_parts.join(" AND "));
        cypher.push(' ');
    }

    cypher.push_str(
        "RETURN s.id AS id, s.content AS content, s.content_hash AS content_hash, \
         s.significance AS significance, s.vault_path AS vault_path, \
         toString(s.created_at) AS created_at, s.helix_id AS helix_id \
         ORDER BY s.created_at DESC, s.id DESC LIMIT $limit",
    );

    let mut neo_q = neo4rs::query(&cypher).param("limit", limit as i64);

    if has_cursor {
        if let Some(id) = &q.after_id {
            neo_q = neo_q.param("after_id", id.clone());
        }
    }
    if let Some(sibling) = &q.sibling {
        neo_q = neo_q.param("sibling", sibling.clone());
    }
    if let Some(tag) = &q.tag {
        neo_q = neo_q.param("tag", tag.clone());
    }
    if let Some(min_sig) = q.min_sig {
        neo_q = neo_q.param("min_sig", min_sig);
    }
    if let Some(q_text) = &q.search {
        neo_q = neo_q.param("q_text", q_text.clone());
    }

    neo_q
}

/// Build a response with the given status, JSON body, ETag, and `Cache-Control` headers.
fn etag_response(status: StatusCode, body: Value, etag: &str) -> Response {
    let mut resp = (status, Json(body)).into_response();
    let h = resp.headers_mut();
    if let Ok(v) = HeaderValue::from_str(etag) {
        h.insert(header::ETAG, v);
    }
    // no-cache: always revalidate — content is mutable via admin upload endpoints.
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    );
    resp
}

/// HTTP 500 — database error.
fn db_error(e: &dyn std::fmt::Display) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": { "code": "database_error", "message": format!("{e}"), "status": 500 }
        })),
    )
        .into_response()
}

/// HTTP 404 — resource not found.
fn not_found(name: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": { "code": "not_found", "message": format!("{name} not found"), "status": 404 }
        })),
    )
        .into_response()
}

// ── Query parameter types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SkillsListQuery {
    after_id: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

#[derive(Debug, Deserialize)]
struct HelixQueryParams {
    /// Filter by sibling name (e.g. `ayin`, `corso`).
    sibling: Option<String>,
    /// Minimum significance score (inclusive).
    min_sig: Option<f64>,
    /// Strand/tag name — only entries tagged with this strand are returned.
    tag: Option<String>,
    /// Fulltext substring match on `content` (case-insensitive).
    #[serde(rename = "q")]
    search: Option<String>,
    /// Opaque cursor for forward pagination — pass the `next_cursor` from the previous response.
    after_id: Option<String>,
    /// Maximum entries to return (1–100, default 20).
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct HelixSearchParams {
    /// BM25 fulltext query (required). Supports Lucene query syntax.
    q: String,
    /// Maximum results to return (1–50, default 10).
    k: Option<usize>,
    /// Minimum BM25 relevance score (≥ 0.0, default 0.0 — include all matches).
    min_score: Option<f64>,
    /// Restrict search to a specific sibling (e.g. `ayin`).
    sibling: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HelixVectorSearchRequest {
    /// Query embedding — exactly 384 floats (all-MiniLM-L6-v2 / BAAI/bge-small-en-v1.5).
    ///
    /// The platform has no embedded model; the caller is responsible for generating this vector.
    vector: Vec<f64>,
    /// Number of nearest neighbours to retrieve (1–50, default 10).
    k: Option<usize>,
    /// Minimum cosine similarity score (≥ 0.0, default 0.0 — include all matches).
    min_score: Option<f64>,
    /// Restrict results to a specific sibling helix (e.g. `"ayin"`).
    sibling: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_with_dim(dim: usize) -> HelixVectorSearchRequest {
        HelixVectorSearchRequest {
            vector: vec![0.0_f64; dim],
            k: None,
            min_score: None,
            sibling: None,
        }
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn validate_correct_dim_passes() {
        let body = body_with_dim(HELIX_VECTOR_DIM);
        let (k, min_score) = validate_vector_search_params(&body).unwrap();
        assert_eq!(k, 10);
        assert_eq!(min_score, 0.0);
    }

    #[test]
    fn validate_wrong_dim_rejected() {
        let body = body_with_dim(768);
        assert!(validate_vector_search_params(&body).is_err());
    }

    #[test]
    fn validate_empty_sibling_rejected() {
        let mut body = body_with_dim(HELIX_VECTOR_DIM);
        body.sibling = Some(String::new());
        assert!(validate_vector_search_params(&body).is_err());
    }

    #[test]
    fn validate_sibling_too_long_rejected() {
        let mut body = body_with_dim(HELIX_VECTOR_DIM);
        body.sibling = Some("x".repeat(HQ_MAX_PARAM_LEN + 1));
        assert!(validate_vector_search_params(&body).is_err());
    }

    #[test]
    fn validate_min_score_nan_rejected() {
        let mut body = body_with_dim(HELIX_VECTOR_DIM);
        body.min_score = Some(f64::NAN);
        assert!(validate_vector_search_params(&body).is_err());
    }

    #[test]
    fn validate_min_score_negative_rejected() {
        let mut body = body_with_dim(HELIX_VECTOR_DIM);
        body.min_score = Some(-0.1);
        assert!(validate_vector_search_params(&body).is_err());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn validate_k_clamps_to_max() {
        let mut body = body_with_dim(HELIX_VECTOR_DIM);
        body.k = Some(999);
        let (k, _) = validate_vector_search_params(&body).unwrap();
        assert_eq!(k, 50);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn validate_k_clamps_to_min() {
        let mut body = body_with_dim(HELIX_VECTOR_DIM);
        body.k = Some(0);
        let (k, _) = validate_vector_search_params(&body).unwrap();
        assert_eq!(k, 1);
    }
}
