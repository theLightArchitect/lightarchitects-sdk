//! Admin endpoints — `/v1/admin/canon/*`.
//!
//! All admin routes require localhost-origin requests (enforced by the CORS
//! allowlist in `http/mod.rs`). Audit log is written to
//! `~/.lightarchitects/audit/admin-canon.jsonl` on every upload — outside Neo4j
//! to survive Cypher tamper attacks (per F-SERAPH-CRIT).

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use std::io::Write as _;
use std::sync::Arc;
use subtle::ConstantTimeEq;

use crate::http::etag::sha256_hex;
use crate::http::state::PlatformState;

/// Wire admin routes onto the router.
pub fn admin_routes() -> Router<Arc<PlatformState>> {
    Router::new().route(
        "/v1/admin/canon/upload",
        axum::routing::post(upload_canon),
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
    // Scope guard: a read bearer on an admin endpoint → 403, not 401.
    // The auth middleware already validated the token; we check scope here.
    if let Some(read_tok) = &s.read_token {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .unwrap_or("");
        if !bearer.is_empty() {
            let is_read: bool = read_tok
                .expose_secret()
                .as_str()
                .as_bytes()
                .ct_eq(bearer.as_bytes())
                .into();
            if is_read {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": {
                        "code": "insufficient_scope",
                        "message": "Read token cannot access admin endpoints. Use x-admin-token.",
                        "status": 403
                    } })),
                )
                    .into_response();
            }
        }
    }

    // Authenticate before reading any body fields.
    match &s.admin_token {
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": {
                    "code": "admin_disabled",
                    "detail": "Admin token is not configured. Set soul-neo4j-local/admin-token in keychain.",
                    "status": 503
                } })),
            )
                .into_response();
        }
        Some(stored) => {
            let provided = headers
                .get("x-admin-token")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            let ok: bool = stored
                .expose_secret()
                .as_str()
                .as_bytes()
                .ct_eq(provided.as_bytes())
                .into();
            if !ok {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": { "code": "invalid_token", "status": 401 } })),
                )
                    .into_response();
            }
        }
    }

    let path = match body.get("path").and_then(Value::as_str) {
        Some(p) if !p.is_empty() => p.to_owned(),
        _ => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": { "code": "missing_field", "field": "path", "status": 422 } })),
            )
                .into_response();
        }
    };
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
        .map(|arr| arr.iter().filter_map(Value::as_str).map(str::to_owned).collect())
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
    let lock_check = neo4rs::query(
        "MATCH (p:PlatformEntry { path: $path }) RETURN p.locked_fields AS locked_fields",
    )
    .param("path", path.clone());

    let mut lock_rs = s.graph.execute(lock_check).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "database_error", "message": format!("{e}"), "status": 500 } })),
        )
            .into_response()
    });

    if let Ok(ref mut lock_stream) = lock_rs {
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
    }

    let hash_src = if content_text.is_empty() { content_json_str.as_bytes() } else { content_text.as_bytes() };
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
    let _ = s.canon_cache.invalidate_entries_if(move |k, _v| k.0 == evict_path);

    write_audit_log(&path, &kind, &version, &updated_at);

    (
        StatusCode::CREATED,
        Json(json!({ "path": path, "content_hash": content_hash, "updated_at": updated_at })),
    )
        .into_response()
}

/// Return `true` if `v` is exactly three dot-separated non-negative integers (e.g. `"1.2.3"`).
fn is_valid_semver(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Append one JSONL line to the admin audit log. Errors are swallowed.
fn write_audit_log(path: &str, kind: &str, version: &str, ts: &str) {
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
            "action": "upload_canon",
            "path": path,
            "kind": kind,
            "version": version,
        });
        let _ = writeln!(file, "{entry}");
    }
}
