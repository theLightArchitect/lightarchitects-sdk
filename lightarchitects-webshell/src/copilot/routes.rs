//! HTTP route handler for `POST /api/builds/:id/copilot`.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::json;
use uuid::Uuid;

use crate::{
    auth,
    config::{AgentSession, ClaudeBackend},
    server::AppState,
};

use super::{CopilotRequest, call_ollama, call_subprocess};

/// Maximum prompt size accepted by the copilot endpoint (§3.4 — 8 KiB).
const MAX_PROMPT_BYTES: usize = 8192;

/// `POST /api/builds/:id/copilot` — dispatch to subprocess or HTTP backend.
pub async fn copilot_chat_handler(
    _: auth::AuthGuard,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<CopilotRequest>,
) -> impl IntoResponse {
    if body.message.len() > MAX_PROMPT_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({ "error": "prompt_too_large", "max_bytes": MAX_PROMPT_BYTES })),
        )
            .into_response();
    }

    let Some(session) = state.builds.get(id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "build_not_found" })),
        )
            .into_response();
    };
    let result = match &session.agent {
        AgentSession::Lightarchitects(ClaudeBackend::Ollama(cfg)) => {
            call_ollama(&cfg.base_url, &cfg.model, &cfg.auth_token, &body.message).await
        }
        AgentSession::Lightarchitects(
            ClaudeBackend::Anthropic | ClaudeBackend::OllamaLaunch(_),
        )
        | AgentSession::Codex(_)
        | AgentSession::LightarchitectsNative(_) => {
            call_subprocess(&body.message, &session.copilot_proc, &session).await
        }
    };
    match result {
        Ok(text) => (StatusCode::OK, Json(json!({ "response": text }))).into_response(),
        Err(reason) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "provider_error", "reason": reason })),
        )
            .into_response(),
    }
}
