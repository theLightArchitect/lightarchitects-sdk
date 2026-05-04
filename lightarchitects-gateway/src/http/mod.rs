//! Platform HTTP mode — private REST API backed by local Neo4j.
//!
//! Activated by `lightarchitects platform [--port 8080]`.
//! Runs on a dedicated `TcpListener`; MCP stdio mode is unaffected when both
//! run concurrently (`--no-mcp` suppresses the stdio loop for HTTP-only mode).
//!
//! Route surface:
//! - `GET  /v1/platform/canon/:name`
//! - `GET  /v1/platform/agents/:sibling`
//! - `GET  /v1/platform/agents/:sibling/strands`
//! - `GET  /v1/platform/skills`
//! - `GET  /v1/platform/skills/:name`
//! - `GET  /v1/platform/standards/:name`
//! - `GET  /v1/platform/helix/query`
//! - `GET  /v1/platform/health`
//! - `GET  /v1/vault/info`
//! - `POST /v1/admin/canon/upload`

pub mod etag;
pub mod middleware;
pub mod neo4j;
pub mod routes;
pub mod state;

use axum::Router;
use axum::middleware as axum_mw;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::error::GatewayError;
use state::PlatformState;

/// Assemble the full platform HTTP router.
///
/// Middleware stack — request order (outer → inner):
/// `TraceLayer` → version header → `CorsLayer` → rate-limit → handlers.
///
/// Version is outermost (after TraceLayer) so every response — including 429s
/// from rate-limit and CORS preflight responses — carries `lightarchitects-version`.
pub fn build_http_router(state: Arc<PlatformState>) -> Router {
    let platform = routes::platform::platform_routes();
    let admin = routes::admin::admin_routes();

    Router::new()
        .merge(platform)
        .merge(admin)
        // innermost: read-auth (token validation + scope enforcement)
        .layer(axum_mw::from_fn_with_state(
            Arc::clone(&state),
            middleware::auth::read_auth_middleware,
        ))
        // rate-limit wraps auth — exhausted clients are rejected before reaching auth
        .layer(axum_mw::from_fn_with_state(
            Arc::clone(&state),
            middleware::rate_limit::rate_limit_middleware,
        ))
        // CORS wraps rate-limit — preflight OPTIONS never reaches rate-limit
        .layer(
            CorsLayer::new()
                .allow_origin([
                    axum::http::HeaderValue::from_static("http://127.0.0.1:5173"),
                    axum::http::HeaderValue::from_static("http://localhost:5173"),
                    axum::http::HeaderValue::from_static("http://127.0.0.1:8080"),
                ])
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::OPTIONS,
                ])
                .allow_headers([
                    axum::http::header::AUTHORIZATION,
                    axum::http::header::CONTENT_TYPE,
                    axum::http::HeaderName::from_static("lightarchitects-version"),
                    axum::http::HeaderName::from_static("lightarchitects-beta"),
                    axum::http::HeaderName::from_static("x-org-id"),
                    axum::http::HeaderName::from_static("x-admin-token"),
                ]),
        )
        // version wraps CORS — injected on all responses including 429s
        .layer(axum_mw::from_fn_with_state(
            Arc::clone(&state),
            middleware::version::version_header_middleware,
        ))
        // outermost: trace (sees every request and final response)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Bind and serve the platform HTTP server.
///
/// Returns when the server shuts down. Intended to be `tokio::spawn`'d so that
/// the MCP stdio loop can run concurrently on the main task.
///
/// # Errors
///
/// Returns [`GatewayError`] if binding the TCP listener fails.
pub async fn run_http_mode(addr: SocketAddr, state: Arc<PlatformState>) -> Result<(), GatewayError> {
    let router = build_http_router(state);
    let listener = TcpListener::bind(addr)
        .await
        .map_err(GatewayError::Io)?;

    tracing::info!(addr = %addr, "Platform HTTP server listening");

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(GatewayError::Io)
}
