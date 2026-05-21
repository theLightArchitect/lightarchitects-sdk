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
    emitter::{emit_d2, emit_html, emit_likec4, emit_markdown, emit_mermaid, kroki},
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

/// Default Kroki endpoint when `KROKI_URL` env var is unset.
const DEFAULT_KROKI_URL: &str = "https://kroki.io";

/// Maximum bytes accepted in a Kroki source body — guards against oversize
/// inputs that would either hit upstream rate limits or produce unrenderable
/// payloads. 64 KiB covers the entire LASDLC v1 schema rendered as Mermaid.
const KROKI_MAX_SOURCE_BYTES: usize = 64 * 1024;

/// Register all arch routes.
pub fn arch_routes() -> Router<Arc<PlatformState>> {
    Router::new()
        .route("/v1/platform/arch/extract", post(arch_extract))
        .route("/v1/platform/arch/verify", post(arch_verify))
        .route("/v1/platform/arch/render", post(arch_render))
        .route("/v1/platform/arch/emit", post(arch_emit))
        .route("/v1/platform/arch/kroki", post(arch_kroki))
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
pub struct KrokiRequest {
    /// Kroki diagram type (e.g. "mermaid", "d2", "plantuml", "structurizr").
    ///
    /// Validated against [`lightarchitects_arch::emitter::kroki::SUPPORTED_TYPES`].
    pub diagram_type: String,
    /// Diagram DSL source. Forwarded verbatim to Kroki as `text/plain`.
    ///
    /// Size-capped at `KROKI_MAX_SOURCE_BYTES`.
    pub source: String,
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
            "unknown format '{other}'; valid: mermaid|d2|likec4|markdown|html|kroki-svg"
        )),
    }
}

/// Returns the configured Kroki endpoint (`KROKI_URL` env var, default
/// `https://kroki.io`). Trailing slash stripped.
fn kroki_base_url() -> String {
    normalize_kroki_url(std::env::var("KROKI_URL").ok().as_deref())
}

/// Pure helper for [`kroki_base_url`] — easier to unit-test than the env-reading
/// path because the crate forbids `unsafe` and therefore can't mutate process
/// env from tests.
fn normalize_kroki_url(raw: Option<&str>) -> String {
    raw.unwrap_or(DEFAULT_KROKI_URL)
        .trim_end_matches('/')
        .to_owned()
}

/// Renders a diagram DSL to SVG via Kroki HTTP POST.
///
/// Sends the source as `text/plain` to `{base}/{type}/svg`. Caller is
/// responsible for type and size validation; this function trusts both.
///
/// # Errors
///
/// Returns an `(HTTP status code, error message)` tuple on network failure
/// (`502`), Kroki rejection (`422`), or any non-2xx upstream response
/// (forwards Kroki status code).
async fn kroki_render(diagram_type: &str, source: &str) -> Result<String, (StatusCode, String)> {
    let base = kroki_base_url();
    let url = format!("{base}/{diagram_type}/svg");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("kroki client init failed: {e}"),
            )
        })?;

    let response = client
        .post(&url)
        .header("Content-Type", "text/plain")
        .body(source.to_owned())
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("kroki unreachable: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(200).collect();
        let mapped = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        return Err((mapped, format!("kroki error ({status}): {snippet}")));
    }

    response.text().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("kroki body read failed: {e}"),
        )
    })
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
///
/// Supports two render paths:
/// - Pure emitters (`mermaid` / `d2` / `likec4` / `markdown` / `html`) — synchronous
///   text output from `lightarchitects-arch` emitters.
/// - External SVG rendering (`kroki-svg`) — first emits Mermaid from the model,
///   then POSTs to Kroki and returns the SVG document as `output`.
#[instrument(skip(_state, headers))]
async fn arch_render(
    State(_state): State<Arc<PlatformState>>,
    headers: HeaderMap,
    Json(req): Json<RenderRequest>,
) -> Response {
    tracing::info!(sibling_id = sibling_id(&headers), format = %req.format, "arch_render");

    if req.format == "kroki-svg" {
        let source = match emit_mermaid(&req.model) {
            Ok(s) => s,
            Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        };
        if source.len() > KROKI_MAX_SOURCE_BYTES {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                format!(
                    "rendered Mermaid is {} bytes; Kroki limit is {KROKI_MAX_SOURCE_BYTES}",
                    source.len()
                ),
            )
                .into_response();
        }
        return match kroki_render("mermaid", &source).await {
            Ok(svg) => (
                StatusCode::OK,
                axum::Json(json!({"output": svg, "format": "kroki-svg"})),
            )
                .into_response(),
            Err((status, msg)) => (status, msg).into_response(),
        };
    }

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

/// `POST /v1/platform/arch/kroki` — render arbitrary DSL source via Kroki.
///
/// Accepts any `lightarchitects_arch::emitter::kroki::SUPPORTED_TYPES` value
/// plus raw source text. Returns `{"svg": "<svg>...</svg>", "diagram_type": "..."}`.
///
/// Source is size-capped at `KROKI_MAX_SOURCE_BYTES` (64 KiB). Diagram type is
/// validated server-side against the supported-types registry — invalid types
/// short-circuit with `400 Bad Request` before any network call is issued.
#[instrument(skip(_state, headers), fields(diagram_type = %req.diagram_type))]
async fn arch_kroki(
    State(_state): State<Arc<PlatformState>>,
    headers: HeaderMap,
    Json(req): Json<KrokiRequest>,
) -> Response {
    tracing::info!(
        sibling_id = sibling_id(&headers),
        diagram_type = %req.diagram_type,
        source_bytes = req.source.len(),
        "arch_kroki",
    );

    if !kroki::is_supported_type(&req.diagram_type) {
        return (
            StatusCode::BAD_REQUEST,
            format!(
                "unsupported diagram_type '{}'; see lightarchitects_arch::emitter::kroki::SUPPORTED_TYPES",
                req.diagram_type
            ),
        )
            .into_response();
    }

    if req.source.is_empty() {
        return (StatusCode::BAD_REQUEST, "source is empty").into_response();
    }
    if req.source.len() > KROKI_MAX_SOURCE_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "source is {} bytes; Kroki limit is {KROKI_MAX_SOURCE_BYTES}",
                req.source.len()
            ),
        )
            .into_response();
    }

    match kroki_render(&req.diagram_type, &req.source).await {
        Ok(svg) => (
            StatusCode::OK,
            axum::Json(json!({"svg": svg, "diagram_type": req.diagram_type})),
        )
            .into_response(),
        Err((status, msg)) => (status, msg).into_response(),
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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

    #[test]
    fn dispatch_render_unknown_format_message_lists_kroki_svg() {
        // Regression guard — UI surface advertises kroki-svg as a render format,
        // so the error message must enumerate it alongside the legacy formats.
        let model = ArchModel::new("test");
        let Err(err) = dispatch_render(&model, "powerpoint") else {
            panic!("expected Err for unknown format");
        };
        assert!(
            err.contains("kroki-svg"),
            "expected kroki-svg in error: {err}"
        );
    }

    #[test]
    fn normalize_kroki_url_defaults_when_unset() {
        assert_eq!(normalize_kroki_url(None), DEFAULT_KROKI_URL);
    }

    #[test]
    fn normalize_kroki_url_strips_trailing_slash() {
        assert_eq!(
            normalize_kroki_url(Some("http://localhost:8000/")),
            "http://localhost:8000"
        );
        assert_eq!(
            normalize_kroki_url(Some("http://localhost:8000///")),
            "http://localhost:8000"
        );
    }

    #[test]
    fn normalize_kroki_url_preserves_paths() {
        // KROKI_URL may include a path prefix when self-hosted behind a reverse proxy.
        assert_eq!(
            normalize_kroki_url(Some("https://k.lightarchitects.io/kroki")),
            "https://k.lightarchitects.io/kroki"
        );
    }
}
