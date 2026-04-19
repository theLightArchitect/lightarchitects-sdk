//! `Arena` route handlers for REST and MCP transports.

use std::sync::Arc;

use axum::Json;
use axum::extract::{FromRequest, FromRequestParts, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use secrecy::ExposeSecret;
use serde_json::Value;

use super::compat::{ApiError, ErrorCode};
use super::compat::{JsonRpcRequestExt, JsonRpcResponseExt};
use lightarchitects::core::jsonrpc::JsonRpcRequest;

use super::AppState;
use super::arena_config::VALID_SIBLINGS;
use super::auth::AuthContext;
use super::mcp_pool;

/// Standard REST error response type.
type RestResponse = (StatusCode, Json<Value>);

/// Build an error response with the standard envelope.
fn rest_error(status: StatusCode, code: &str, message: &str) -> RestResponse {
    (
        status,
        Json(serde_json::json!({
            "error": { "code": code, "message": message, "status": status.as_u16() }
        })),
    )
}

/// Validate REST input: sibling name, action format.
fn validate_rest_input(sibling: &str, action: &str) -> Option<RestResponse> {
    if !VALID_SIBLINGS.contains(&sibling) {
        let msg = format!(
            "Unknown sibling: '{sibling}'. Valid: {}",
            VALID_SIBLINGS.join(", ")
        );
        return Some(rest_error(StatusCode::BAD_REQUEST, "invalid_sibling", &msg));
    }
    if action.is_empty() || !action.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Some(rest_error(
            StatusCode::BAD_REQUEST,
            "invalid_action",
            "Action must be non-empty alphanumeric with underscores",
        ));
    }
    None
}

/// REST transport handler: `POST /v1/{sibling}/{action}`
///
/// Defense-in-depth: extracts and verifies `AuthContext` even though the auth
/// middleware already runs. Fail-closed — missing context returns 401, scope
/// mismatch returns 403.
pub async fn rest_action(
    State(state): State<Arc<AppState>>,
    request: axum::http::Request<axum::body::Body>,
) -> impl IntoResponse {
    // Defense-in-depth: extract AuthContext inserted by auth middleware.
    // Fail-closed — if the middleware didn't insert it, deny the request.
    let auth_ctx = request.extensions().get::<AuthContext>().cloned();

    // Split request into parts (headers, URI, extensions) and body.
    // Path comes from parts; JSON body is parsed separately.
    let (mut parts, body) = request.into_parts();

    let Path((sibling, action)): Path<(String, String)> =
        match Path::from_request_parts(&mut parts, &()).await {
            Ok(path) => path,
            Err(_) => {
                return rest_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_path",
                    "Expected /v1/{sibling}/{action}",
                );
            }
        };

    if let Some(err) = validate_rest_input(&sibling, &action) {
        return err;
    }

    // Defense-in-depth scope check — mirrors mcp_tools_call behavior.
    let Some(ctx) = auth_ctx else {
        return rest_error(
            StatusCode::UNAUTHORIZED,
            "auth_required",
            "Authentication required",
        );
    };
    if !ctx.has_scope(&sibling) {
        tracing::warn!(
            key_prefix = %ctx.key_prefix, sibling = %sibling, action = %action,
            "REST scope denied"
        );
        return rest_error(
            StatusCode::FORBIDDEN,
            "scope_denied",
            &format!("Scope denied for '{sibling}'"),
        );
    }

    // Read body bytes first. DefaultBodyLimit fires here with PAYLOAD_TOO_LARGE when
    // the body exceeds MAX_BODY_SIZE — inspecting the rejection status lets us return
    // 413 instead of letting the JSON extractor swallow it as a generic 400.
    let reassembled = axum::http::Request::from_parts(parts, body);
    let raw_bytes: axum::body::Bytes = match axum::body::Bytes::from_request(reassembled, &()).await
    {
        Ok(b) => b,
        Err(e) => {
            return if e.into_response().status() == StatusCode::PAYLOAD_TOO_LARGE {
                rest_error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "body_too_large",
                    "Request body exceeds 1 MB limit",
                )
            } else {
                rest_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_body",
                    "Failed to read request body",
                )
            };
        }
    };
    let json_body: Value = match serde_json::from_slice::<Value>(&raw_bytes) {
        Ok(v) => v,
        Err(_) => {
            return rest_error(
                StatusCode::BAD_REQUEST,
                "invalid_body",
                "Request body must be valid JSON",
            );
        }
    };

    let request_id = state
        .request_counter
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let params = json_body
        .get("params")
        .cloned()
        .unwrap_or(Value::Object(serde_json::Map::default()));

    let (target_sibling, rpc_request) =
        match mcp_pool::rest_to_jsonrpc(&sibling, &action, params, request_id) {
            Ok(v) => v,
            Err(msg) => return rest_error(StatusCode::BAD_REQUEST, "invalid_request", &msg),
        };

    dispatch_mcp_call(&state, &target_sibling, &action, &rpc_request, request_id).await
}

/// Dispatch a JSON-RPC call to an MCP binary and normalize the response.
#[tracing::instrument(skip(state, request), fields(sibling, action, request_id))]
async fn dispatch_mcp_call(
    state: &AppState,
    sibling: &str,
    action: &str,
    request: &JsonRpcRequest,
    request_id: u64,
) -> RestResponse {
    match state.pool.call(sibling, request).await {
        Ok(response) if response.is_error() => {
            let error = response.error.as_ref().map(|e| ApiError {
                code: ErrorCode::ActionFailed,
                message: sanitize_error_message(&e.message),
                status: Some(500),
                details: None,
            });
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": error,
                    "trace": { "trace_id": format!("ic-{request_id}") }
                })),
            )
        }
        Ok(response) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "result": response.result,
                "trace": { "trace_id": format!("ic-{request_id}") }
            })),
        ),
        Err(e) => {
            tracing::error!(sibling = %sibling, action = %action, error = %e, "MCP call failed");
            rest_error(
                StatusCode::BAD_GATEWAY,
                "sibling_unavailable",
                &format!("Sibling '{sibling}' is unavailable"),
            )
        }
    }
}

/// MCP Streamable HTTP handler: `POST /mcp`
///
/// Routes JSON-RPC by tool name with scope enforcement.
pub async fn mcp_post(
    State(state): State<Arc<AppState>>,
    request: axum::http::Request<axum::body::Body>,
) -> impl IntoResponse {
    // Extract auth context for scope checking
    let auth_ctx = request.extensions().get::<AuthContext>().cloned();

    // Deserialize to Value first, then convert — SDK's JsonRpcRequest uses
    // &'static str for jsonrpc which can't deserialize from request body.
    let Json(raw): Json<serde_json::Value> = match axum::Json::from_request(request, &()).await {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"}}),
                ),
            );
        }
    };
    let rpc_request = JsonRpcRequest {
        jsonrpc: "2.0",
        id: raw
            .get("id")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        method: raw
            .get("method")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned(),
        params: raw.get("params").cloned(),
    };

    if rpc_request.method == "tools/list" {
        return mcp_tools_list(&rpc_request);
    }

    if rpc_request.method == "tools/call" {
        return mcp_tools_call(&state, &rpc_request, auth_ctx.as_ref()).await;
    }

    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": rpc_request.id,
            "error": {"code": -32601, "message": format!("Method not supported: {}", sanitize_user_input(&rpc_request.method))}
        })),
    )
}

/// Handle `tools/list` — aggregate tool listing from all siblings.
fn mcp_tools_list(request: &JsonRpcRequest) -> RestResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "result": {
                "tools": [
                    {"name": "corsoTools", "description": "CORSO orchestrator (26 actions)"},
                    {"name": "speak", "description": "EVA conversation"},
                    {"name": "visualize", "description": "EVA image generation"},
                    {"name": "ideate", "description": "EVA creative workflow"},
                    {"name": "memory", "description": "EVA memory operations"},
                    {"name": "build", "description": "EVA code assistance"},
                    {"name": "bible", "description": "EVA scripture search"},
                    {"name": "research", "description": "EVA knowledge retrieval"},
                    {"name": "secure", "description": "EVA security analysis"},
                    {"name": "teach", "description": "EVA education"},
                    {"name": "soulTools", "description": "SOUL orchestrator (23 actions)"},
                    {"name": "qsTools", "description": "QUANTUM orchestrator (13 actions)"},
                    {"name": "penTools", "description": "SERAPH orchestrator (18 actions, scope-gated)"}
                ]
            }
        })),
    )
}

/// Handle `tools/call` — route to sibling with scope enforcement.
async fn mcp_tools_call(
    state: &AppState,
    request: &JsonRpcRequest,
    auth_ctx: Option<&AuthContext>,
) -> RestResponse {
    let tool_name = request
        .params
        .as_ref()
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str());

    let Some(tool_name) = tool_name else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "jsonrpc": "2.0", "id": request.id,
                "error": {"code": -32602, "message": "Missing params.name"}
            })),
        );
    };

    let Some(sibling) = mcp_pool::McpPool::resolve_sibling(tool_name) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "jsonrpc": "2.0", "id": request.id,
                "error": {"code": -32602, "message": format!("Unknown tool: {}", sanitize_user_input(tool_name))}
            })),
        );
    };

    // Scope check on MCP transport — fail-closed if no auth context
    let Some(ctx) = auth_ctx else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "jsonrpc": "2.0", "id": request.id,
                "error": {"code": -32603, "message": "Authentication required"}
            })),
        );
    };
    if !ctx.has_scope(sibling) {
        tracing::warn!(
            key_prefix = %ctx.key_prefix, tool = %tool_name, sibling = %sibling,
            "MCP scope denied"
        );
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "jsonrpc": "2.0", "id": request.id,
                "error": {"code": -32603, "message": format!("Scope denied for '{sibling}'")}
            })),
        );
    }

    match state.pool.call(sibling, request).await {
        Ok(response) => {
            let sanitized = sanitize_jsonrpc_response(response);
            (
                StatusCode::OK,
                Json(serde_json::to_value(sanitized).unwrap_or_default()),
            )
        }
        Err(e) => {
            tracing::error!(sibling = %sibling, tool = %tool_name, error = %e, "MCP call failed");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "jsonrpc": "2.0", "id": request.id,
                    "error": {"code": -32603, "message": "Sibling unavailable"}
                })),
            )
        }
    }
}

/// Extract key-creation parameters from a request body JSON value.
/// Defaults: scopes `["all"]`, `rate_limit` 60.
fn parse_key_body(body: &serde_json::Value) -> (Vec<String>, u32) {
    let scopes: Vec<String> = body.get("scopes").and_then(|v| v.as_array()).map_or_else(
        || vec!["all".to_owned()],
        |arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                .collect()
        },
    );
    let rate_limit: u32 = body
        .get("rate_limit")
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(60);
    (scopes, rate_limit)
}

/// Key management endpoint: `POST /v1/keys` — bootstrap or admin-only creation.
///
/// **Bootstrap mode**: if no active keys exist, unauthenticated creation is allowed
/// (Gitea/Grafana pattern — first-run key provisioning without out-of-band setup).
/// **Admin mode**: once any active key exists, requires a valid "all"-scoped key.
///
/// `/v1/keys` is in `EXEMPT_PATHS` so the auth middleware never runs for this route.
/// Auth is verified manually here when the store is not in bootstrap state.
pub async fn key_create(
    State(state): State<Arc<AppState>>,
    request: axum::http::Request<axum::body::Body>,
) -> impl IntoResponse {
    let Some(auth_store) = &state.auth_store else {
        return rest_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "auth_unavailable",
            "Authentication service not configured",
        );
    };

    let is_bootstrap = auth_store.is_empty().await;

    // Clone the auth header value before `request` is consumed by body parsing.
    let auth_header: Option<String> = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned);

    if !is_bootstrap {
        // Verify the Bearer token manually — middleware exempt path means no AuthContext.
        let token = auth_header
            .as_deref()
            .and_then(|h| h.strip_prefix("Bearer "))
            .filter(|t| t.starts_with("lak_"));

        let Some(token) = token else {
            return rest_error(
                StatusCode::UNAUTHORIZED,
                "missing_auth",
                "Authorization required — bootstrap complete, 'all'-scoped key needed",
            );
        };

        let Some(ctx) = auth_store.verify(token).await else {
            return rest_error(
                StatusCode::UNAUTHORIZED,
                "invalid_key",
                "Invalid or inactive API key",
            );
        };

        if !ctx.has_scope("all") {
            return rest_error(
                StatusCode::FORBIDDEN,
                "scope_denied",
                "Key management requires 'all' scope",
            );
        }
    }

    // Parse body — optional (defaults apply for empty bootstrap requests).
    let body: serde_json::Value = axum::Json::from_request(request, &())
        .await
        .map(|axum::Json(v)| v)
        .unwrap_or(serde_json::Value::Object(serde_json::Map::default()));

    let (scopes, rate_limit) = parse_key_body(&body);

    // Generate key: lak_live_{uuid-no-dashes} — UUID v4 gives 128 bits of entropy.
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let raw_key = format!("lak_live_{suffix}");
    let key_prefix = raw_key[..12].to_owned();

    // Bootstrap uses atomic check+insert to prevent TOCTOU race.
    // Admin (non-bootstrap) uses regular add_key — auth was already verified above.
    let add_result = if is_bootstrap {
        match auth_store
            .bootstrap_key(&raw_key, &scopes, rate_limit)
            .await
        {
            Ok(Some(hash)) => Ok(hash),
            Ok(None) => {
                // Race lost — another request bootstrapped between our is_empty() check and now.
                return rest_error(
                    StatusCode::UNAUTHORIZED,
                    "bootstrap_complete",
                    "Bootstrap already completed — provide an 'all'-scoped API key",
                );
            }
            Err(e) => Err(e),
        }
    } else {
        auth_store.add_key(&raw_key, &scopes, rate_limit).await
    };

    match add_result {
        Ok(_) => {
            tracing::info!(
                key_prefix = %key_prefix,
                is_bootstrap = %is_bootstrap,
                scopes = ?scopes,
                "API key created"
            );
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "key": raw_key,
                    "prefix": key_prefix,
                    "scopes": scopes,
                    "rate_limit": rate_limit,
                    "bootstrap": is_bootstrap,
                })),
            )
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to create API key");
            rest_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "key_creation_failed",
                "Failed to create API key",
            )
        }
    }
}

/// Health check: `GET /health` — minimal info, no PIDs, no auth status.
pub async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let shutting_down = *state.shutdown_tx.borrow();
    if shutting_down {
        return Json(serde_json::json!({
            "status": "shutting_down",
            "gateway": { "version": env!("CARGO_PKG_VERSION") },
        }));
    }

    let mut siblings = state.pool.health().await;

    // Add placeholder siblings not in the MCP pool
    if state.config.siblings.laex.is_none() {
        siblings
            .entry("laex".to_owned())
            .or_insert(crate::arena::mcp_pool::SiblingHealth {
                status: "not_configured",
            });
    }

    let all_connected = siblings
        .values()
        .all(|s| s.status == "connected" || s.status == "not_configured");

    Json(serde_json::json!({
        "status": if all_connected { "healthy" } else { "degraded" },
        "gateway": { "version": env!("CARGO_PKG_VERSION") },
        "siblings": siblings,
    }))
}

/// Light Architect Genesis chat proxy: `POST /v1/larc/chat`
///
/// Forwards OpenAI-compatible chat completion requests to the `HuggingFace`
/// Inference Endpoint running Light Architect Genesis (llama.cpp server).
#[tracing::instrument(skip(state, body))]
pub async fn larc_chat(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let (Some(url), Some(token)) = (&state.config.laex_endpoint_url, &state.config.laex_hf_token)
    else {
        return rest_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "larc_not_configured",
            "Laex endpoint not configured (set LAEX_ENDPOINT_URL and LAEX_HF_TOKEN)",
        );
    };

    let chat_url = format!("{url}/v1/chat/completions");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .unwrap_or_default();

    match client
        .post(&chat_url)
        .header("Authorization", format!("Bearer {}", token.expose_secret()))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let result: Value = resp.json().await.unwrap_or(Value::Null);
            (StatusCode::OK, Json(result))
        }
        Ok(resp) => {
            let status = resp.status();
            tracing::warn!(status = %status, "L-ARC proxy error");
            rest_error(
                StatusCode::BAD_GATEWAY,
                "larc_error",
                &format!("L-ARC returned {status}"),
            )
        }
        Err(e) => {
            tracing::error!(error = %e, "L-ARC proxy connection failed");
            rest_error(
                StatusCode::BAD_GATEWAY,
                "larc_unavailable",
                "L-ARC endpoint is unavailable",
            )
        }
    }
}

/// Sanitize user-supplied values before including in error responses.
/// Prevents XSS if error messages are rendered in HTML by downstream consumers.
fn sanitize_user_input(input: &str) -> String {
    input
        .chars()
        .take(50)
        .filter(|c| c.is_alphanumeric() || *c == '/' || *c == '_' || *c == '.' || *c == '-')
        .collect()
}

/// Sanitize error messages — allowlist approach, never forwards raw sibling output.
fn sanitize_error_message(msg: &str) -> String {
    // Take only the first line, cap at 200 chars, strip any path-like content
    let first_line = msg.lines().next().unwrap_or("Internal error");
    let sanitized: String = first_line.chars().take(200).collect();

    // If it contains anything that looks like a file path, replace entirely
    if sanitized.contains('/')
        && (sanitized.contains("/Users")
            || sanitized.contains("/home")
            || sanitized.contains("/opt")
            || sanitized.contains("/tmp")
            || sanitized.contains("/srv"))
    {
        return "Action failed — check trace ID for details".into();
    }

    if sanitized.is_empty() {
        "Internal error".into()
    } else {
        sanitized
    }
}

/// Sanitize a JSON-RPC response — strip internal details from error messages.
fn sanitize_jsonrpc_response(
    mut response: lightarchitects::core::jsonrpc::JsonRpcResponse,
) -> lightarchitects::core::jsonrpc::JsonRpcResponse {
    if let Some(ref mut error) = response.error {
        error.message = sanitize_error_message(&error.message);
        // Note: SDK JsonRpcError has no `data` field (stripped during migration).
        // la_sdk_core's version had `data: Option<Value>` for internal details.
    }
    response
}
