//! Admin endpoints — `/v1/admin/canon/*`.
//!
//! All admin routes require localhost-origin requests (enforced by the CORS
//! allowlist in `http/mod.rs`). Audit log is written to
//! `~/.lightarchitects/audit/admin-canon.jsonl` on every upload — outside Neo4j
//! to survive Cypher tamper attacks (per F-SERAPH-CRIT).

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use std::io::Write as _;
use std::sync::Arc;

use crate::http::etag::sha256_hex;
use crate::http::routes::platform::validate_path_param;
use crate::http::state::PlatformState;
use crate::security::hmac::ct_eq_bytes;

/// Permitted sibling names for the agent upload endpoint.
///
/// LÆX promoted from implicit-layer to canonical routed sibling 2026-05-08;
/// vestigial `"claude"` entry removed in the same swap (no SDK backing). See
/// `helix/corso/builds/laex-sibling-promotion/manifest.yaml` for the build
/// trail and the migration script at `scripts/migrate-claude-to-laex-sibling-identity.sh`
/// for handling any extant `:SiblingIdentity {sibling: 'claude'}` records.
const ALLOWED_SIBLINGS: &[&str] = &["corso", "eva", "soul", "quantum", "seraph", "ayin", "laex"];

/// Wire admin routes onto the router.
pub fn admin_routes() -> Router<Arc<PlatformState>> {
    Router::new()
        .route("/v1/admin/canon/upload", axum::routing::post(upload_canon))
        .route("/v1/admin/agents/upload", axum::routing::post(upload_agent))
        .route(
            "/v1/admin/standards/upload",
            axum::routing::post(upload_standard),
        )
        .route(
            "/v1/admin/overrides",
            axum::routing::post(upsert_override),
        )
        .route(
            "/v1/admin/overrides/{org_id}/{*target_path}",
            axum::routing::delete(delete_override),
        )
        .route(
            "/v1/admin/operator/resolve-assertion",
            axum::routing::post(resolve_assertion),
        )
}

/// Maximum byte length for `content_text` and serialized `content_json`.
const MAX_CONTENT_BYTES: usize = 512 * 1024; // 512 KB

/// Permitted values for the `kind` field.
const ALLOWED_KINDS: &[&str] = &["canon", "standard", "template", "skill"];

/// `POST /v1/admin/canon/upload` — upsert a [`PlatformEntry`] node.
///
/// Body (JSON):
/// ```json
/// {
///   "path": "canon/builders-cookbook",
///   "kind": "canon",
///   "content_text": "...",
///   "content_json": { ... },
///   "version": "1.0.0"
/// }
/// ```
async fn upload_canon(
    State(s): State<Arc<PlatformState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    // Scope guard is handled by read_auth_middleware for all /v1/admin/* paths.
    if let Err(r) = require_admin_token(&s, &headers) {
        return r;
    }

    let path = match body.get("path").and_then(Value::as_str) {
        Some(p) if !p.is_empty() => p.to_owned(),
        _ => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(
                    json!({ "error": { "code": "missing_field", "field": "path", "status": 422 } }),
                ),
            )
                .into_response();
        }
    };

    if let Some(e) = validate_path_param(&path) {
        return e;
    }
    let kind = body
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("canon")
        .to_owned();

    if !ALLOWED_KINDS.contains(&kind.as_str()) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": {
                "code": "invalid_kind",
                "allowed": ALLOWED_KINDS,
                "status": 422
            } })),
        )
            .into_response();
    }

    let content_text = body
        .get("content_text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let content_json_str = body
        .get("content_json")
        .map(|v| v.to_string())
        .unwrap_or_default();
    let version = body
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("1.0.0")
        .to_owned();
    // Optional: fields to lock on this entry after writing.
    let locked_fields: Vec<String> = body
        .get("locked_fields")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();

    if !is_valid_semver(&version) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": {
                "code": "invalid_version",
                "detail": "version must be MAJOR.MINOR.PATCH (e.g. 1.0.0)",
                "status": 422
            } })),
        )
            .into_response();
    }

    // Guard against arbitrarily large payloads reaching Neo4j.
    let content_bytes = content_text.len().max(content_json_str.len());
    if content_bytes > MAX_CONTENT_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({ "error": { "code": "content_too_large",
                "max_bytes": MAX_CONTENT_BYTES, "status": 413 } })),
        )
            .into_response();
    }

    // Locked-field guard: if the existing node has locked_fields, reject any modification.
    // Propagate DB errors — silently skipping the check on error would allow overwrites.
    let lock_check = neo4rs::query(
        "MATCH (p:PlatformEntry { path: $path }) RETURN p.locked_fields AS locked_fields",
    )
    .param("path", path.clone());

    let mut lock_stream = match s.graph.execute(lock_check).await {
        Ok(rs) => rs,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": { "code": "database_error", "message": format!("{e}"), "status": 500 } })),
            )
                .into_response();
        }
    };

    if let Ok(Some(row)) = lock_stream.next().await {
        let existing_locked: Vec<String> =
            row.get::<Vec<String>>("locked_fields").unwrap_or_default();
        if !existing_locked.is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": {
                    "code": "LockedFieldViolation",
                    "locked_fields": existing_locked,
                    "detail": "This entry has locked fields and cannot be modified.",
                    "status": 400
                } })),
            )
                .into_response();
        }
    }

    let hash_src = if content_text.is_empty() {
        content_json_str.as_bytes()
    } else {
        content_text.as_bytes()
    };
    let content_hash = sha256_hex(hash_src);
    let updated_at = chrono::Utc::now().to_rfc3339();

    let q = neo4rs::query(
        "MERGE (p:PlatformEntry { path: $path }) \
         SET p.kind = $kind, p.content_text = $content_text, \
             p.content_json = $content_json, p.version = $version, \
             p.content_hash = $content_hash, p.updated_at = $updated_at, \
             p.locked_fields = $locked_fields",
    )
    .param("path", path.clone())
    .param("kind", kind.clone())
    .param("content_text", content_text)
    .param("content_json", content_json_str)
    .param("version", version.clone())
    .param("content_hash", content_hash.clone())
    .param("updated_at", updated_at.clone())
    .param("locked_fields", locked_fields);

    if let Err(e) = s.graph.run(q).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "database_error", "message": format!("{e}"), "status": 500 } })),
        )
            .into_response();
    }

    // Evict any org-scoped cache entries for this path so the next read sees the new content.
    let evict_path = path.clone();
    let _ = s
        .canon_cache
        .invalidate_entries_if(move |k, _v| k.0 == evict_path);

    write_audit_log("upload_canon", &path, &kind, &version, "admin", &updated_at);

    (
        StatusCode::CREATED,
        Json(json!({ "path": path, "content_hash": content_hash, "updated_at": updated_at })),
    )
        .into_response()
}

/// `POST /v1/admin/agents/upload` — upsert a [`SiblingIdentity`] node.
///
/// Body (JSON):
/// ```json
/// {
///   "sibling": "corso",
///   "role": "AppSec engineer and build cycle orchestrator",
///   "voice": "SAS precision with Birmingham dialect",
///   "strands": ["tactical", "security", "vigilance"],
///   "version": "1.0.0"
/// }
/// ```
async fn upload_agent(
    State(s): State<Arc<PlatformState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(r) = require_admin_token(&s, &headers) {
        return r;
    }

    let sibling = match body.get("sibling").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_owned(),
        _ => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": { "code": "missing_field", "field": "sibling", "status": 422 } })),
            )
                .into_response();
        }
    };

    if !ALLOWED_SIBLINGS.contains(&sibling.as_str()) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": {
                "code": "invalid_sibling",
                "allowed": ALLOWED_SIBLINGS,
                "status": 422
            } })),
        )
            .into_response();
    }

    let role = body
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let voice = body
        .get("voice")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let strands: Vec<String> = body
        .get("strands")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let version = body
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("1.0.0")
        .to_owned();

    if !is_valid_semver(&version) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": {
                "code": "invalid_version",
                "detail": "version must be MAJOR.MINOR.PATCH",
                "status": 422
            } })),
        )
            .into_response();
    }

    let hash_src = format!("{sibling}:{role}:{voice}:{}", strands.join(","));
    let content_hash = sha256_hex(hash_src.as_bytes());
    let updated_at = chrono::Utc::now().to_rfc3339();

    let q = neo4rs::query(
        "MERGE (s:SiblingIdentity { sibling: $sibling }) \
         SET s.role = $role, s.voice = $voice, s.strands = $strands, \
             s.content_hash = $content_hash, s.updated_at = $updated_at, \
             s.version = $version",
    )
    .param("sibling", sibling.clone())
    .param("role", role)
    .param("voice", voice)
    .param("strands", strands)
    .param("content_hash", content_hash.clone())
    .param("updated_at", updated_at.clone())
    .param("version", version.clone());

    if let Err(e) = s.graph.run(q).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "database_error", "message": format!("{e}"), "status": 500 } })),
        )
            .into_response();
    }

    // Invalidate agent cache for this sibling (key format: "agents/{sibling}").
    let evict = format!("agents/{sibling}");
    let _ = s
        .agent_cache
        .invalidate_entries_if(move |k, _v| k.0 == evict);

    write_audit_log("upload_agent", &sibling, "agent_identity", &version, "admin", &updated_at);

    (
        StatusCode::CREATED,
        Json(json!({ "sibling": sibling, "content_hash": content_hash, "updated_at": updated_at })),
    )
        .into_response()
}

/// `POST /v1/admin/standards/upload` — upsert a [`Standard`] node.
///
/// Body (JSON):
/// ```json
/// {
///   "name": "builders-cookbook",
///   "title": "Builders Cookbook",
///   "content_text": "...",
///   "version": "1.0.0"
/// }
/// ```
async fn upload_standard(
    State(s): State<Arc<PlatformState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(r) = require_admin_token(&s, &headers) {
        return r;
    }

    let name = match body.get("name").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_owned(),
        _ => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": { "code": "missing_field", "field": "name", "status": 422 } })),
            )
                .into_response();
        }
    };

    if let Some(e) = validate_path_param(&name) {
        return e;
    }

    let title = body
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(&name)
        .to_owned();
    let content_text = body
        .get("content_text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let version = body
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("1.0.0")
        .to_owned();

    if content_text.len() > MAX_CONTENT_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({ "error": { "code": "content_too_large",
                "max_bytes": MAX_CONTENT_BYTES, "status": 413 } })),
        )
            .into_response();
    }

    if !is_valid_semver(&version) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": {
                "code": "invalid_version",
                "detail": "version must be MAJOR.MINOR.PATCH",
                "status": 422
            } })),
        )
            .into_response();
    }

    let content_hash = sha256_hex(content_text.as_bytes());
    let updated_at = chrono::Utc::now().to_rfc3339();

    let q = neo4rs::query(
        "MERGE (s:Standard { name: $name }) \
         SET s.title = $title, s.content_text = $content_text, \
             s.content_hash = $content_hash, s.updated_at = $updated_at, \
             s.version = $version",
    )
    .param("name", name.clone())
    .param("title", title)
    .param("content_text", content_text)
    .param("content_hash", content_hash.clone())
    .param("updated_at", updated_at.clone())
    .param("version", version.clone());

    if let Err(e) = s.graph.run(q).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "database_error", "message": format!("{e}"), "status": 500 } })),
        )
            .into_response();
    }

    write_audit_log("upload_standard", &name, "standard", &version, "admin", &updated_at);

    (
        StatusCode::CREATED,
        Json(json!({ "name": name, "content_hash": content_hash, "updated_at": updated_at })),
    )
        .into_response()
}

/// `POST /v1/admin/overrides` — upsert a per-org JSON-patch override.
///
/// Sets or replaces an [`OrgOverride`] node for `(org_id, target_path)`. The
/// `override_value` is a JSON object whose keys shallow-merge over the base
/// content on every read. Both `canon_cache` and `agent_cache` entries for
/// `(target_path, org_id)` are evicted so the next read sees the new patch.
///
/// Body (JSON):
/// ```json
/// {
///   "org_id": "acme",
///   "target_path": "canon/builders-cookbook",
///   "override_value": { "title": "Acme Cookbook" },
///   "updated_by": "kft"
/// }
/// ```
async fn upsert_override(
    State(s): State<Arc<PlatformState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(r) = require_admin_token(&s, &headers) {
        return r;
    }

    let org_id = match body.get("org_id").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_owned(),
        _ => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": { "code": "missing_field", "field": "org_id", "status": 422 } })),
            )
                .into_response();
        }
    };

    let target_path = match body.get("target_path").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_owned(),
        _ => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": { "code": "missing_field", "field": "target_path", "status": 422 } })),
            )
                .into_response();
        }
    };

    if let Some(e) = validate_path_param(&org_id) {
        return e;
    }
    if let Some(e) = validate_target_path(&target_path) {
        return e;
    }

    let override_value = match body.get("override_value") {
        Some(Value::Object(_)) => {
            let v = body["override_value"].to_string();
            if v.len() > MAX_CONTENT_BYTES {
                return (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    Json(json!({ "error": {
                        "code": "override_value_too_large",
                        "detail": format!("override_value serialized size exceeds {} bytes", MAX_CONTENT_BYTES),
                        "status": 413
                    } })),
                )
                    .into_response();
            }
            v
        }
        Some(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": {
                    "code": "invalid_override_value",
                    "detail": "override_value must be a JSON object",
                    "status": 422
                } })),
            )
                .into_response();
        }
        None => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": { "code": "missing_field", "field": "override_value", "status": 422 } })),
            )
                .into_response();
        }
    };

    let updated_by = body
        .get("updated_by")
        .and_then(Value::as_str)
        .unwrap_or("admin")
        .to_owned();
    let now = chrono::Utc::now().to_rfc3339();

    let q = neo4rs::query(
        "MERGE (o:OrgOverride { org_id: $org_id, target_path: $target_path }) \
         SET o.override_value = $override_value, \
             o.updated_by = $updated_by, \
             o.created_at = coalesce(o.created_at, $now), \
             o.updated_at = $now",
    )
    .param("org_id", org_id.clone())
    .param("target_path", target_path.clone())
    .param("override_value", override_value)
    .param("updated_by", updated_by.clone())
    .param("now", now.clone());

    if let Err(e) = s.graph.run(q).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "database_error", "message": format!("{e}"), "status": 500 } })),
        )
            .into_response();
    }

    // Evict (target_path, org_id) from both caches immediately — override applies to both
    // PlatformEntry and SiblingIdentity read paths, so we must evict regardless of node type.
    // Use `.invalidate(&key).await` (not `invalidate_entries_if`) for synchronous eviction.
    let evict_key = (target_path.clone(), org_id.clone());
    s.canon_cache.invalidate(&evict_key).await;
    s.agent_cache.invalidate(&evict_key).await;

    write_audit_log("upsert_override", &target_path, "org_override", "1.0.0", &updated_by, &now);

    (
        StatusCode::CREATED,
        Json(json!({ "org_id": org_id, "target_path": target_path, "updated_at": now })),
    )
        .into_response()
}

/// `DELETE /v1/admin/overrides/{org_id}/{*target_path}` — remove a per-org override.
///
/// Returns 204 when the node is deleted, 404 when it does not exist.
/// Evicts both caches for the `(target_path, org_id)` key.
async fn delete_override(
    State(s): State<Arc<PlatformState>>,
    headers: HeaderMap,
    Path((org_id, target_path)): Path<(String, String)>,
) -> Response {
    if let Err(r) = require_admin_token(&s, &headers) {
        return r;
    }

    if let Some(e) = validate_path_param(&org_id) {
        return e;
    }
    if let Some(e) = validate_target_path(&target_path) {
        return e;
    }

    // Atomic delete — MATCH + DELETE + RETURN count(*) in a single round-trip to Neo4j.
    // If count(*) == 0 the node did not exist; return 404. This eliminates the TOCTOU
    // window between a separate existence check and the delete.
    let del = neo4rs::query(
        "MATCH (o:OrgOverride { org_id: $org_id, target_path: $target_path }) \
         DELETE o \
         RETURN count(*) AS deleted",
    )
    .param("org_id", org_id.clone())
    .param("target_path", target_path.clone());

    let deleted = match s.graph.execute(del).await {
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": { "code": "database_error", "message": format!("{e}"), "status": 500 } })),
            )
                .into_response();
        }
        Ok(mut rs) => rs
            .next()
            .await
            .ok()
            .flatten()
            .and_then(|row| row.get::<i64>("deleted").ok())
            .unwrap_or(0),
    };

    if deleted == 0 {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": { "code": "not_found", "status": 404 } })),
        )
            .into_response();
    }

    let evict_key = (target_path.clone(), org_id.clone());
    s.canon_cache.invalidate(&evict_key).await;
    s.agent_cache.invalidate(&evict_key).await;

    let now = chrono::Utc::now().to_rfc3339();
    write_audit_log("delete_override", &target_path, "org_override", "1.0.0", "admin", &now);

    StatusCode::NO_CONTENT.into_response()
}

/// Verify `x-admin-token` header. Returns `Err(response)` on failure.
// Response is a large type by design — boxing it would require changing all handler return types.
#[allow(clippy::result_large_err)]
fn require_admin_token(s: &PlatformState, headers: &HeaderMap) -> Result<(), Response> {
    match &s.admin_token {
        None => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": {
                "code": "admin_disabled",
                "detail": "Admin token is not configured.",
                "status": 503
            } })),
        )
            .into_response()),
        Some(stored) => {
            let provided = headers
                .get("x-admin-token")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if ct_eq_bytes(stored.expose_secret().as_bytes(), provided.as_bytes()) {
                Ok(())
            } else {
                Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": { "code": "invalid_token", "status": 401 } })),
                )
                    .into_response())
            }
        }
    }
}

/// Validate a multi-segment path like `"agents/corso"` or `"canon/builders-cookbook"`.
///
/// Allows `/` as a segment separator but still blocks directory traversal (`..`),
/// backslashes, non-printable / non-ASCII bytes, and empty strings.
fn validate_target_path(val: &str) -> Option<Response> {
    let invalid = val.is_empty()
        || val.contains("..")
        || val.contains('\\')
        || val.starts_with('/')
        || val.ends_with('/')
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

/// Return `true` if `v` is exactly three dot-separated non-negative integers (e.g. `"1.2.3"`).
fn is_valid_semver(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Append one JSONL line to the admin audit log. Errors are swallowed.
fn write_audit_log(action: &str, path: &str, kind: &str, version: &str, actor: &str, ts: &str) {
    let log_path = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join(".lightarchitects/audit/admin-canon.jsonl");

    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let entry = json!({
            "timestamp": ts,
            "action": action,
            "path": path,
            "kind": kind,
            "version": version,
            "actor": actor,
        });
        let _ = writeln!(file, "{entry}");
    }
}

// ── Operator resolve-assertion (Wave 3.2) ─────────────────────────────────────

/// `POST /v1/admin/operator/resolve-assertion` — submit an operator decision for
/// a blocked assertion gate.
///
/// # Request body
///
/// ```json
/// {
///   "assertion_id": "assert-001",
///   "action_type": "provide_citation",
///   "operator_id": "op-kft",
///   "timestamp_iso8601": "2026-05-04T10:00:00Z",
///   "build_id": "luminous-confidence-portal",
///   "signature": "<hmac-sha256-hex>",
///   "citation": { ... }
/// }
/// ```
///
/// The `signature` field must be a valid HMAC-SHA256 hex over the canonical
/// `HookPayload` JSON (excluding `signature` and `citation`).
async fn resolve_assertion(
    State(s): State<Arc<PlatformState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(r) = require_admin_token(&s, &headers) {
        return r;
    }

    // Step 2: extract required fields.
    let (assertion_id, action_type, operator_id, timestamp, build_id, signature) =
        match extract_resolve_fields(&body) {
            Ok(f) => f,
            Err(msg) => {
                return (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response();
            }
        };

    // Step 3: HMAC verification.
    if let Err(reason) = verify_resolve_hmac(
        &assertion_id,
        &action_type,
        &operator_id,
        &timestamp,
        &signature,
    ) {
        crate::governance::emit_hook_span(
            "PostToolUse:OperatorResolve_HMACVerify",
            "security",
            true,
        );
        return (StatusCode::FORBIDDEN, Json(json!({ "error": reason }))).into_response();
    }

    // Step 4: ScopeGovernor 5-gate.
    let ctx = crate::governance::ScopeGovernorContext {
        operator_id: operator_id.clone(),
        build_id: build_id.clone(),
        tool: "resolve-assertion".into(),
        timestamp_iso8601: timestamp.clone(),
        authorized_builds: Vec::new(), // empty = all builds (loopback trust model)
        allowed_tools: Vec::new(),     // empty = unrestricted
        concurrent_count: 0,
        concurrent_limit: 5,
    };
    if let Err(e) = crate::governance::enforce_operator_action(&ctx) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    // Step 5: forward to webshell squad-comms.
    let resolve_params = json!({
        "request_id": uuid::Uuid::new_v4().to_string(),
        "assertion_id": assertion_id,
        "build_id": build_id,
        "operator_id": operator_id,
        "action_type": action_type,
        "citation": body.get("citation"),
    });
    match crate::squad_comms::resolve_assertion_gate(
        resolve_params,
        &crate::config::GatewayConfig::default(),
    )
    .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

fn extract_resolve_fields(
    body: &Value,
) -> Result<(String, String, String, String, String, String), String> {
    macro_rules! req_str {
        ($field:expr) => {
            body[$field]
                .as_str()
                .ok_or_else(|| format!("missing required field: {}", $field))?
                .to_owned()
        };
    }
    Ok((
        req_str!("assertion_id"),
        req_str!("action_type"),
        req_str!("operator_id"),
        req_str!("timestamp_iso8601"),
        req_str!("build_id"),
        req_str!("signature"),
    ))
}

fn verify_resolve_hmac(
    assertion_id: &str,
    action_type: &str,
    operator_id: &str,
    timestamp: &str,
    signature: &str,
) -> Result<(), String> {
    use crate::security::hmac::{
        HookPayload, load_secret, replay_window_check, verify_hook_payload,
    };

    let secret = load_secret().map_err(|e| e.to_string())?;

    let ts_ok = replay_window_check(timestamp).map_err(|e| e.to_string())?;
    if !ts_ok {
        return Err("replay window expired".into());
    }

    let payload = HookPayload {
        assertion_id: assertion_id.to_owned(),
        action_type: action_type.to_owned(),
        operator_id: operator_id.to_owned(),
        timestamp_iso8601: timestamp.to_owned(),
    };
    let ok = verify_hook_payload(&payload, &secret, signature).map_err(|e| e.to_string())?;
    if !ok {
        return Err("HMAC signature verification failed".into());
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    // ── ALLOWED_SIBLINGS const tests (W4 — laex-sibling-promotion) ───────────────

    #[test]
    fn allowed_siblings_includes_laex() {
        assert!(
            ALLOWED_SIBLINGS.contains(&"laex"),
            "LÆX must be in ALLOWED_SIBLINGS post laex-sibling-promotion ship"
        );
    }

    #[test]
    fn allowed_siblings_excludes_vestigial_claude() {
        assert!(
            !ALLOWED_SIBLINGS.contains(&"claude"),
            "vestigial \"claude\" entry must be removed in laex-sibling-promotion W4 swap"
        );
    }

    #[test]
    fn allowed_siblings_length_unchanged() {
        // 7 entries before swap (incl. vestigial "claude"), 7 entries after (incl. "laex").
        assert_eq!(ALLOWED_SIBLINGS.len(), 7);
    }

    #[test]
    fn allowed_siblings_canonical_seven_routed_sibs() {
        // Verifies the full 7-sibling slate is represented post-promotion.
        for expected in ["corso", "eva", "soul", "quantum", "seraph", "ayin", "laex"] {
            assert!(
                ALLOWED_SIBLINGS.contains(&expected),
                "{expected} should be in ALLOWED_SIBLINGS"
            );
        }
    }
}
