//! Architecture intelligence routes — `/v1/platform/arch/*`.
//!
//! Exposes the `lightarchitects-arch` crate over HTTP with:
//! - Auth inherited from the global `read_auth_middleware` stack.
//! - 5-minute moka response cache keyed on `(project_root, "")`.
//! - M6 per-sibling capability check: `X-Sibling-Id` header + home-dir allowlist.
//! - AYIN metrics emitted for extract / verify / blocking_count.
//!
//! Route surface:
//! - `POST /v1/platform/arch/extract` — extract `ArchModel` from a project root.
//! - `POST /v1/platform/arch/verify`  — diff planned vs current, return findings.
//! - `POST /v1/platform/arch/render`  — render `ArchModel` to a diagram format.
//! - `POST /v1/platform/arch/emit`    — emit full package (all formats + HTML).
//! - `GET  /v1/platform/arch/health`  — smoke check (always 200 if server is up).

use axum::Router;
use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use lightarchitects_arch::{
    ArchModel, Severity,
    emitter::{emit_d2, emit_html, emit_likec4, emit_markdown, emit_mermaid},
    extractor::{ExtractorConfig, walk_and_extract},
    security::path::canonicalize_and_check,
    verifier,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::instrument;

use crate::http::state::PlatformState;

/// Register all arch routes.
pub fn arch_routes() -> Router<Arc<PlatformState>> {
    Router::new()
        .route("/v1/platform/arch/extract", post(arch_extract))
        .route("/v1/platform/arch/verify", post(arch_verify))
        .route("/v1/platform/arch/render", post(arch_render))
        .route("/v1/platform/arch/emit", post(arch_emit))
        .route("/v1/platform/arch/health", get(arch_health))
}

// ── Request / response shapes ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExtractRequest {
    /// Absolute path to the project root to analyse.
    pub project_root: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    /// JSON-serialised planned `ArchModel` (baseline).
    pub planned: ArchModel,
    /// Absolute path to the project root (current model extracted live).
    pub project_root: String,
    /// Severity threshold above which `has_blocking` is set. Default: "high".
    #[serde(default = "default_threshold")]
    pub blocking_threshold: String,
}

fn default_threshold() -> String {
    "high".into()
}

#[derive(Debug, Deserialize)]
pub struct RenderRequest {
    /// The `ArchModel` to render.
    pub model: ArchModel,
    /// Output format: "mermaid", "d2", "likec4", "markdown", "html".
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct EmitRequest {
    /// Absolute path to the project root.
    pub project_root: String,
}

#[derive(Debug, Serialize)]
struct VerifyResponse {
    findings: Vec<lightarchitects_arch::ArchFinding>,
    duplicates_dropped: u32,
    capped_dropped: u32,
    has_blocking: bool,
}

// ── M6 capability check ───────────────────────────────────────────────────────

/// Returns the sibling identity from the `X-Sibling-Id` header, if present.
fn sibling_id(headers: &HeaderMap) -> &str {
    headers
        .get("X-Sibling-Id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("operator")
}

/// Validates the project_root path against the operator home directory.
///
/// Rejects anything outside `$HOME` (M6 cross-sibling exfil guard). Fails
/// closed (HTTP 500) if `$HOME` is not set. Phase 7 adds a per-project
/// Neo4j-backed allowlist.
#[allow(clippy::result_large_err)] // axum Response is inherently large in HTTP helpers.
fn validate_root(root: &str) -> Result<PathBuf, Response> {
    let home = std::env::var("HOME").map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "$HOME not set — M6 allowlist cannot be constructed",
        )
            .into_response()
    })?;
    let allowed = [PathBuf::from(&home)];
    canonicalize_and_check(std::path::Path::new(root), &allowed).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            format!("path rejected (M6 allowlist): {e}"),
        )
            .into_response()
    })
}

/// Dispatch a render to the correct per-format emitter.
fn dispatch_render(model: &ArchModel, format: &str) -> Result<String, String> {
    match format {
        "mermaid" => emit_mermaid(model).map_err(|e| e.to_string()),
        "d2" => emit_d2(model).map_err(|e| e.to_string()),
        "likec4" => emit_likec4(model).map_err(|e| e.to_string()),
        "markdown" => emit_markdown(model, None).map_err(|e| e.to_string()),
        "html" => emit_html(model, None, false).map_err(|e| e.to_string()),
        other => Err(format!(
            "unknown format '{other}'; valid: mermaid|d2|likec4|markdown|html"
        )),
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /v1/platform/arch/extract` — extract `ArchModel` from a project root.
///
/// Responses are cached for 5 min keyed on `(project_root, "")`.
#[instrument(skip(state, headers))]
async fn arch_extract(
    State(state): State<Arc<PlatformState>>,
    headers: HeaderMap,
    Json(req): Json<ExtractRequest>,
) -> Response {
    tracing::info!(sibling_id = sibling_id(&headers), project_root = %req.project_root, "arch_extract");

    let root = match validate_root(&req.project_root) {
        Ok(p) => p,
        Err(r) => return r,
    };

    let cache_key = (root.display().to_string(), String::new());
    if let Some(cached) = state.arch_cache.get(&cache_key).await {
        return (StatusCode::OK, axum::Json((*cached).clone())).into_response();
    }

    let facts = match walk_and_extract(&root, &ExtractorConfig::default()) {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("extract error: {e}"),
            )
                .into_response();
        }
    };

    let mut model = ArchModel::new(root.display().to_string());
    model.nodes = facts.nodes;
    model.relations = facts.relations;

    let payload = Arc::new(json!({
        "model": model,
        "warnings": facts.warnings,
        "cached": false,
    }));
    state
        .arch_cache
        .insert(cache_key, Arc::clone(&payload))
        .await;

    (StatusCode::OK, axum::Json((*payload).clone())).into_response()
}

/// `POST /v1/platform/arch/verify` — diff planned vs current, return findings.
#[instrument(skip(state, headers), fields(threshold = %req.blocking_threshold))]
async fn arch_verify(
    State(state): State<Arc<PlatformState>>,
    headers: HeaderMap,
    Json(req): Json<VerifyRequest>,
) -> Response {
    tracing::info!(sibling_id = sibling_id(&headers), "arch_verify");

    let root = match validate_root(&req.project_root) {
        Ok(p) => p,
        Err(r) => return r,
    };

    let threshold = match req.blocking_threshold.as_str() {
        "info" => Severity::Info,
        "low" => Severity::Low,
        "medium" => Severity::Medium,
        "critical" => Severity::Critical,
        _ => Severity::High,
    };

    let facts = match walk_and_extract(&root, &ExtractorConfig::default()) {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("extract error: {e}"),
            )
                .into_response();
        }
    };
    let mut current = ArchModel::new(root.display().to_string());
    current.nodes = facts.nodes;
    current.relations = facts.relations;

    let result = verifier::run(&req.planned, &current, threshold);

    tracing::info!(
        blocking_count = result
            .findings
            .iter()
            .filter(|f| f.severity >= threshold)
            .count(),
        "arch_verify_complete"
    );

    // Cache invalidation: a verify run means source changed; evict stale extract cache.
    let _ = state
        .arch_cache
        .invalidate(&(root.display().to_string(), String::new()))
        .await;

    let resp = VerifyResponse {
        findings: result.findings,
        duplicates_dropped: result.duplicates_dropped,
        capped_dropped: result.capped_dropped,
        has_blocking: result.has_blocking,
    };
    (StatusCode::OK, axum::Json(json!(resp))).into_response()
}

/// `POST /v1/platform/arch/render` — render an `ArchModel` to a diagram format.
#[instrument(skip(_state, headers))]
async fn arch_render(
    State(_state): State<Arc<PlatformState>>,
    headers: HeaderMap,
    Json(req): Json<RenderRequest>,
) -> Response {
    tracing::info!(sibling_id = sibling_id(&headers), format = %req.format, "arch_render");

    match dispatch_render(&req.model, &req.format) {
        Ok(output) => (
            StatusCode::OK,
            axum::Json(json!({"output": output, "format": req.format})),
        )
            .into_response(),
        Err(e) if e.starts_with("unknown format") => (StatusCode::BAD_REQUEST, e).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

/// `POST /v1/platform/arch/emit` — emit full package (all formats).
#[instrument(skip(state, headers))]
async fn arch_emit(
    State(state): State<Arc<PlatformState>>,
    headers: HeaderMap,
    Json(req): Json<EmitRequest>,
) -> Response {
    tracing::info!(sibling_id = sibling_id(&headers), project_root = %req.project_root, "arch_emit");

    let root = match validate_root(&req.project_root) {
        Ok(p) => p,
        Err(r) => return r,
    };

    let facts = match walk_and_extract(&root, &ExtractorConfig::default()) {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("extract error: {e}"),
            )
                .into_response();
        }
    };
    let mut model = ArchModel::new(root.display().to_string());
    model.nodes = facts.nodes;
    model.relations = facts.relations;

    // Invalidate stale extract cache.
    state
        .arch_cache
        .invalidate(&(root.display().to_string(), String::new()))
        .await;

    let formats = ["mermaid", "d2", "likec4", "markdown", "html"];
    let mut outputs: serde_json::Map<String, Value> = serde_json::Map::new();
    for fmt in &formats {
        let text = dispatch_render(&model, fmt).unwrap_or_else(|e| format!("ERROR: {e}"));
        outputs.insert((*fmt).to_string(), Value::String(text));
    }

    (
        StatusCode::OK,
        axum::Json(json!({
            "project_root": root.display().to_string(),
            "node_count": model.nodes.len(),
            "relation_count": model.relations.len(),
            "outputs": outputs,
        })),
    )
        .into_response()
}

/// `GET /v1/platform/arch/health` — always 200 when the server is up.
async fn arch_health() -> impl IntoResponse {
    (
        StatusCode::OK,
        axum::Json(json!({"status": "ok", "surface": "arch"})),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_root_rejects_etc() {
        let result = validate_root("/etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn validate_root_accepts_home() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        // HOME itself always exists and is within allowlist.
        let result = validate_root(&home);
        assert!(result.is_ok(), "home dir must be allowed: {result:?}");
    }

    #[test]
    fn dispatch_render_rejects_unknown_format() {
        let model = ArchModel::new("test");
        let result = dispatch_render(&model, "graphviz");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown format"));
    }

    #[test]
    fn dispatch_render_mermaid_produces_output() {
        let model = ArchModel::new("test");
        let result = dispatch_render(&model, "mermaid");
        assert!(result.is_ok());
    }
}
