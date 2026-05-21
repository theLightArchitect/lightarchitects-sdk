//! Architecture Intelligence proxy — `/api/arch/*`.
//!
//! Forwards requests to the gateway's `/v1/platform/arch/*` surface (Phase 5).
//! No direct dependency on `lightarchitects-arch` (M17 fold).
//!
//! Gateway URL: `GATEWAY_PLATFORM_URL` env var (default `http://127.0.0.1:8080`).
//!
//! Routes:
//! - `POST /api/arch/extract` → `POST /v1/platform/arch/extract`
//! - `POST /api/arch/verify`  → `POST /v1/platform/arch/verify`
//! - `POST /api/arch/render`  → `POST /v1/platform/arch/render`
//! - `POST /api/arch/emit`    → `POST /v1/platform/arch/emit`
//! - `POST /api/arch/kroki`   → `POST /v1/platform/arch/kroki`
//! - `GET  /api/arch/health`  → `GET  /v1/platform/arch/health`

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use reqwest::Client;
use serde_json::Value;
use tracing::instrument;

use crate::{auth::AuthGuard, server::AppState};

/// Returns the configured gateway base URL.
fn gateway_url() -> String {
    std::env::var("GATEWAY_PLATFORM_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned())
}

/// Forwards a JSON body to the gateway and returns the response.
///
/// Passes through the caller's `Authorization` header so the gateway's
/// own `AuthGuard` (Bearer token) validates the request.
async fn proxy_post(
    op: &str,
    body: Value,
    auth_header: Option<&str>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let url = format!("{}/v1/platform/arch/{op}", gateway_url());

    let client = Client::new();
    let mut req = client.post(&url).json(&body);
    if let Some(token) = auth_header {
        req = req.header("Authorization", token);
    }

    match req.send().await {
        Ok(resp) => {
            let status = StatusCode::from_u16(resp.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let json: Value = resp.json().await.unwrap_or(Value::Null);
            if status.is_success() {
                Ok((status, Json(json)))
            } else {
                Err((status, Json(json)))
            }
        }
        Err(e) => {
            tracing::warn!(op, error = %e, "arch gateway unreachable");
            Err((
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": "gateway unreachable",
                    "detail": e.to_string()
                })),
            ))
        }
    }
}

/// Extract bearer token from incoming `Authorization` header.
fn bearer_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /api/arch/extract` — proxy to gateway `arch_extract`.
#[instrument(skip_all)]
pub async fn extract_handler(
    _: AuthGuard,
    headers: HeaderMap,
    State(_state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    match proxy_post("extract", body, bearer_from_headers(&headers).as_deref()).await {
        Ok(r) => r.into_response(),
        Err((s, j)) => (s, j).into_response(),
    }
}

/// `POST /api/arch/verify` — proxy to gateway `arch_verify`.
#[instrument(skip_all)]
pub async fn verify_handler(
    _: AuthGuard,
    headers: HeaderMap,
    State(_state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    match proxy_post("verify", body, bearer_from_headers(&headers).as_deref()).await {
        Ok(r) => r.into_response(),
        Err((s, j)) => (s, j).into_response(),
    }
}

/// `POST /api/arch/render` — proxy to gateway `arch_render`.
#[instrument(skip_all)]
pub async fn render_handler(
    _: AuthGuard,
    headers: HeaderMap,
    State(_state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    match proxy_post("render", body, bearer_from_headers(&headers).as_deref()).await {
        Ok(r) => r.into_response(),
        Err((s, j)) => (s, j).into_response(),
    }
}

/// `POST /api/arch/emit` — proxy to gateway `arch_emit`.
#[instrument(skip_all)]
pub async fn emit_handler(
    _: AuthGuard,
    headers: HeaderMap,
    State(_state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    match proxy_post("emit", body, bearer_from_headers(&headers).as_deref()).await {
        Ok(r) => r.into_response(),
        Err((s, j)) => (s, j).into_response(),
    }
}

/// `POST /api/arch/kroki` — proxy to gateway `arch_kroki`.
///
/// Body: `{"diagram_type": "mermaid"|"d2"|"plantuml"|..., "source": "..."}`.
/// Response: `{"svg": "<svg>...</svg>", "diagram_type": "..."}`.
///
/// Used by the `DiagramLibrary` screen to render catalogue examples server-side
/// (avoids leaking the operator's browser to `kroki.io` and lets the deployment
/// override `KROKI_URL` to a self-hosted Kroki).
#[instrument(skip_all)]
pub async fn kroki_handler(
    _: AuthGuard,
    headers: HeaderMap,
    State(_state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    match proxy_post("kroki", body, bearer_from_headers(&headers).as_deref()).await {
        Ok(r) => r.into_response(),
        Err((s, j)) => (s, j).into_response(),
    }
}

/// `GET /api/arch/health` — proxy to gateway arch health probe.
///
/// Returns 200 when the gateway's arch surface is reachable, 502 otherwise.
#[instrument(skip_all)]
pub async fn health_handler(
    _: AuthGuard,
    headers: HeaderMap,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    let url = format!("{}/v1/platform/arch/health", gateway_url());
    let client = Client::new();
    let mut req = client.get(&url);
    if let Some(token) = bearer_from_headers(&headers) {
        req = req.header("Authorization", token);
    }
    match req.send().await {
        Ok(r) if r.status().is_success() => {
            (StatusCode::OK, Json(serde_json::json!({"status": "ok"}))).into_response()
        }
        Ok(r) => {
            let status =
                StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            (status, Json(serde_json::json!({"status": "error"}))).into_response()
        }
        Err(e) => {
            tracing::warn!(error = %e, "arch health: gateway unreachable");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"status": "unreachable", "detail": e.to_string()})),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::gateway_url;

    #[test]
    fn default_gateway_url() {
        // GATEWAY_PLATFORM_URL not set in tests — must return default
        if std::env::var("GATEWAY_PLATFORM_URL").is_err() {
            assert_eq!(gateway_url(), "http://127.0.0.1:8080");
        }
    }

    #[test]
    fn gateway_url_format_is_http() {
        // gateway_url() always returns a valid HTTP URL string.
        let url = gateway_url();
        assert!(url.starts_with("http://"), "unexpected scheme: {url}");
    }
}
