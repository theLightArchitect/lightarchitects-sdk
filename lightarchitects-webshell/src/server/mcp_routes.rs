//! MCP host HTTP surface.
//!
//! - `GET  /api/mcp/servers` — list all managed servers + their live state.
//! - `GET  /api/mcp/tools`   — list all cached tools across ready servers.
//! - `POST /api/mcp/invoke`  — invoke a single tool (scope + schema gated).
//!
//! All routes require [`AuthGuard`]. If no MCP host is configured the routes
//! return 503 with `{"error":"mcp_host not configured"}`.

use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use lightarchitects_webshell_mcp_host::{HostManager, McpHostConfig};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{error, info, instrument};

use crate::{auth::AuthGuard, server::AppState};

// ── Type alias ────────────────────────────────────────────────────────────────

/// Shared handle to the optional MCP host.
pub type McpHostHandle = Arc<RwLock<Option<HostManager>>>;

// ── Startup helper ────────────────────────────────────────────────────────────

/// Try to load `~/.lightarchitects/webshell-mcp.json` and start the host.
/// Returns `None` if the file is absent (Phase 7 places it).
pub async fn try_init_host() -> Option<HostManager> {
    let config_path = std::env::var("HOME")
        .map(|h| {
            std::path::PathBuf::from(h)
                .join(".lightarchitects")
                .join("webshell-mcp.json")
        })
        .ok()?;

    if !config_path.exists() {
        return None;
    }

    let json = match std::fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => {
            error!(path = %config_path.display(), err = %e, "failed to read webshell-mcp.json");
            return None;
        }
    };

    let config = match McpHostConfig::from_json(&json) {
        Ok(c) => c,
        Err(e) => {
            error!(err = %e, "failed to parse webshell-mcp.json");
            return None;
        }
    };

    info!(servers = config.servers.len(), "initialising MCP host");
    Some(HostManager::from_config(config).await)
}

// ── Response types ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ServerEntry {
    name: String,
    state: String,
    tool_count: usize,
}

#[derive(Serialize)]
struct ToolEntry {
    server: String,
    name: String,
    description: String,
}

/// Request body for `POST /api/mcp/invoke`.
#[derive(Deserialize)]
pub struct InvokeRequest {
    /// Logical server name from the config.
    pub server: String,
    /// Tool name to invoke.
    pub tool: String,
    /// Tool input (must match the tool's JSON Schema).
    pub input: serde_json::Value,
}

#[derive(Serialize)]
struct InvokeResponse {
    output: serde_json::Value,
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

const NOT_CONFIGURED: (StatusCode, Json<ErrorBody>) = (
    StatusCode::SERVICE_UNAVAILABLE,
    Json(ErrorBody {
        error: "mcp_host not configured",
    }),
);

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /api/mcp/servers` — list all managed servers with live state.
#[instrument(skip_all)]
pub async fn list_servers_handler(
    State(state): State<AppState>,
    _: AuthGuard,
) -> impl IntoResponse {
    let guard = state.mcp_host.read().await;
    let Some(host) = guard.as_ref() else {
        return NOT_CONFIGURED.into_response();
    };
    let servers: Vec<ServerEntry> = host
        .list_servers()
        .await
        .into_iter()
        .map(|s| ServerEntry {
            name: s.name,
            state: s.state,
            tool_count: s.tool_count,
        })
        .collect();
    Json(servers).into_response()
}

/// `GET /api/mcp/tools` — list all cached tools across ready servers.
#[instrument(skip_all)]
pub async fn list_tools_handler(State(state): State<AppState>, _: AuthGuard) -> impl IntoResponse {
    let guard = state.mcp_host.read().await;
    let Some(host) = guard.as_ref() else {
        return NOT_CONFIGURED.into_response();
    };
    let tools: Vec<ToolEntry> = host
        .list_tools()
        .into_iter()
        .map(|(server, t)| ToolEntry {
            server,
            name: t.name,
            description: t.description,
        })
        .collect();
    Json(tools).into_response()
}

/// `POST /api/mcp/invoke` — invoke a tool through the scope + schema gate.
#[instrument(skip_all, fields(server = %req.server, tool = %req.tool))]
pub async fn invoke_handler(
    State(state): State<AppState>,
    _: AuthGuard,
    Json(req): Json<InvokeRequest>,
) -> impl IntoResponse {
    let guard = state.mcp_host.read().await;
    let Some(host) = guard.as_ref() else {
        return NOT_CONFIGURED.into_response();
    };
    match host.invoke_tool(&req.server, &req.tool, req.input).await {
        Ok(output) => Json(InvokeResponse { output }).into_response(),
        Err(e) => {
            let status = status_for(&e);
            (status, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

fn status_for(e: &lightarchitects_webshell_mcp_host::McpHostError) -> StatusCode {
    use lightarchitects_webshell_mcp_host::McpHostError;
    match e {
        McpHostError::NotFound { .. } => StatusCode::NOT_FOUND,
        McpHostError::NotReady { .. } => StatusCode::SERVICE_UNAVAILABLE,
        McpHostError::Scope { .. } => StatusCode::FORBIDDEN,
        _ => StatusCode::BAD_GATEWAY,
    }
}
