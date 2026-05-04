//! Platform content read endpoints — `/v1/platform/*` + `/v1/vault/info`.

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::http::etag::{compute_etag, etag_from_hash, is_not_modified};
use crate::http::state::PlatformState;

/// Wire all platform read routes onto the router.
pub fn platform_routes() -> Router<Arc<PlatformState>> {
    Router::new()
        .route("/v1/platform/canon/{name}", get(canon_get))
        .route("/v1/platform/agents/{sibling}", get(agents_get))
        .route("/v1/platform/agents/{sibling}/strands", get(agents_strands_get))
        .route("/v1/platform/skills", get(skills_list))
        .route("/v1/platform/skills/{name}", get(skills_get))
        .route("/v1/platform/standards/{name}", get(standards_get))
        .route("/v1/platform/helix/query", get(helix_query))
        .route("/v1/platform/health", get(health))
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
    if let Some(e) = validate_path_param(&name) { return Err(e); }
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
            s.canon_cache.insert(cache_key, Arc::new(body.clone())).await;
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
    if let Some(e) = validate_path_param(&sibling) { return Err(e); }
    let org_id = extract_org_id(&headers);
    let cache_key = (sibling.clone(), org_id.clone());

    let body = match s.agent_cache.get(&cache_key).await {
        Some(cached) => {
            tracing::debug!(sibling = %sibling, org_id = %org_id, "agent cache hit");
            cached.as_ref().clone()
        }
        None => {
            tracing::debug!(sibling = %sibling, org_id = %org_id, "agent cache miss — fetching neo4j");
            let body = fetch_agent_body(&s, &sibling, &org_id).await?;
            s.agent_cache.insert(cache_key, Arc::new(body.clone())).await;
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
    if let Some(e) = validate_path_param(&sibling) { return Err(e); }
    let org_id = extract_org_id(&headers);
    let cache_key = (sibling.clone(), org_id.clone());

    let full_body = match s.agent_cache.get(&cache_key).await {
        Some(cached) => {
            tracing::debug!(sibling = %sibling, "strands cache hit");
            cached.as_ref().clone()
        }
        None => {
            tracing::debug!(sibling = %sibling, "strands cache miss — fetching neo4j");
            let body = fetch_agent_body(&s, &sibling, &org_id).await?;
            s.agent_cache.insert(cache_key, Arc::new(body.clone())).await;
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
        skills.last().and_then(|s| s.get("name")).and_then(Value::as_str).map(String::from)
    } else {
        None
    };

    let body = json!({ "skills": skills, "next_cursor": next_cursor });
    let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
    let etag = compute_etag(&body_bytes);
    Ok(etag_response(StatusCode::OK, body, &etag))
}

/// `GET /v1/platform/skills/:name` — single skill by name.
async fn skills_get(
    State(s): State<Arc<PlatformState>>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    if let Some(e) = validate_path_param(&name) { return Err(e); }
    let q = neo4rs::query(
        "MATCH (s:Skill { name: $name }) \
         RETURN s.name AS name, s.description AS description, s.version AS version, \
                s.trigger_patterns AS trigger_patterns, s.published AS published, \
                s.content_hash AS content_hash, s.updated_at AS updated_at",
    )
    .param("name", name.clone());

    let mut rs = s.graph.execute(q).await.map_err(|e| db_error(&e))?;
    let row = rs.next().await.map_err(|e| db_error(&e))?.ok_or_else(|| not_found(&name))?;

    let hash = row.get::<String>("content_hash").unwrap_or_default();
    let etag = etag_from_hash(&hash);
    if is_not_modified(headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()), &etag) {
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
    if let Some(e) = validate_path_param(&name) { return Err(e); }
    let q = neo4rs::query(
        "MATCH (s:Standard { name: $name }) \
         RETURN s.name AS name, s.title AS title, \
                s.content_text AS content_text, \
                s.content_hash AS content_hash, s.updated_at AS updated_at",
    )
    .param("name", name.clone());

    let mut rs = s.graph.execute(q).await.map_err(|e| db_error(&e))?;
    let row = rs.next().await.map_err(|e| db_error(&e))?.ok_or_else(|| not_found(&name))?;

    let hash = row.get::<String>("content_hash").unwrap_or_default();
    let etag = etag_from_hash(&hash);
    if is_not_modified(headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()), &etag) {
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

/// `GET /v1/platform/helix/query` — filtered helix entry listing.
async fn helix_query(
    State(s): State<Arc<PlatformState>>,
    Query(q): Query<HelixQueryParams>,
) -> Result<Response, Response> {
    let limit = q.limit.unwrap_or(20).min(100);
    let rs_query = build_helix_query(&q, limit);

    let mut rs = s.graph.execute(rs_query).await.map_err(|e| db_error(&e))?;
    let mut entries: Vec<Value> = Vec::new();
    while let Ok(Some(row)) = rs.next().await {
        entries.push(json!({
            "id": row.get::<String>("id").unwrap_or_default(),
            "kind": row.get::<String>("kind").ok(),
            "content": row.get::<String>("content").ok(),
            "significance": row.get::<f64>("significance").ok(),
            "tags": row.get::<Vec<String>>("tags").unwrap_or_default(),
            "created_at": row.get::<String>("created_at").ok(),
        }));
    }

    let body = json!({ "entries": entries, "count": entries.len() });
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    Ok(etag_response(StatusCode::OK, body, &compute_etag(&bytes)))
}

/// `GET /v1/platform/health` — liveness probe; no auth, no Neo4j.
pub async fn health(State(_): State<Arc<PlatformState>>) -> impl IntoResponse {
    Json(json!({ "status": "healthy", "service": "platform-api-v1" }))
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

    let mut tier_counts: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();
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
async fn fetch_canon_body(
    s: &PlatformState,
    name: &str,
    org_id: &str,
) -> Result<Value, Response> {
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
    let row = rs.next().await.map_err(|e| db_error(&e))?.ok_or_else(|| not_found(name))?;

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
    let q = neo4rs::query(
        "MATCH (s:SiblingIdentity { sibling: $sibling }) \
         OPTIONAL MATCH (o:OrgOverride { org_id: $org_id, target_path: $sibling }) \
         RETURN s.role AS role, s.voice AS voice, s.strands AS strands, \
                s.content_hash AS content_hash, s.updated_at AS updated_at, \
                o.override_value AS override_value",
    )
    .param("sibling", sibling.to_owned())
    .param("org_id", org_id.to_owned());

    let mut rs = s.graph.execute(q).await.map_err(|e| db_error(&e))?;
    let row = rs.next().await.map_err(|e| db_error(&e))?.ok_or_else(|| not_found(sibling))?;

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
fn validate_path_param(val: &str) -> Option<Response> {
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
    let inm = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok());
    if is_not_modified(inm, &etag) {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    etag_response(StatusCode::OK, body, &etag)
}

/// Build a Cypher query for the helix endpoint, filtering by optional params.
fn build_helix_query(q: &HelixQueryParams, limit: usize) -> neo4rs::Query {
    match (&q.kind, &q.tier) {
        (Some(k), Some(t)) => neo4rs::query(
            "MATCH (h:Helix) WHERE h.kind = $kind AND h.tier = $tier \
             RETURN h.id AS id, h.kind AS kind, h.content AS content, \
                    h.significance AS significance, h.tags AS tags, h.created_at AS created_at \
             ORDER BY h.created_at DESC LIMIT $limit",
        )
        .param("kind", k.clone())
        .param("tier", t.clone())
        .param("limit", limit as i64),

        (Some(k), None) => neo4rs::query(
            "MATCH (h:Helix) WHERE h.kind = $kind \
             RETURN h.id AS id, h.kind AS kind, h.content AS content, \
                    h.significance AS significance, h.tags AS tags, h.created_at AS created_at \
             ORDER BY h.created_at DESC LIMIT $limit",
        )
        .param("kind", k.clone())
        .param("limit", limit as i64),

        (None, Some(t)) => neo4rs::query(
            "MATCH (h:Helix) WHERE h.tier = $tier \
             RETURN h.id AS id, h.kind AS kind, h.content AS content, \
                    h.significance AS significance, h.tags AS tags, h.created_at AS created_at \
             ORDER BY h.created_at DESC LIMIT $limit",
        )
        .param("tier", t.clone())
        .param("limit", limit as i64),

        (None, None) => neo4rs::query(
            "MATCH (h:Helix) \
             RETURN h.id AS id, h.kind AS kind, h.content AS content, \
                    h.significance AS significance, h.tags AS tags, h.created_at AS created_at \
             ORDER BY h.created_at DESC LIMIT $limit",
        )
        .param("limit", limit as i64),
    }
}

/// Build a response with the given status, JSON body, ETag, and `Cache-Control` headers.
fn etag_response(status: StatusCode, body: Value, etag: &str) -> Response {
    let mut resp = (status, Json(body)).into_response();
    let h = resp.headers_mut();
    if let Ok(v) = HeaderValue::from_str(etag) {
        h.insert(header::ETAG, v);
    }
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("max-age=2592000"),
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
    kind: Option<String>,
    tier: Option<String>,
    limit: Option<usize>,
}
