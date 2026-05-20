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

use super::{CopilotRequest, call_ollama, call_subprocess, context};

/// Maximum prompt size accepted by the copilot endpoint (§3.4 — 8 KiB).
const MAX_PROMPT_BYTES: usize = 8192;

/// Maximum total size of the grounded message (prelude + user message).
///
/// The prelude from `recent_events` is unbounded by `MAX_PROMPT_BYTES`, so a
/// separate ceiling is required. Set to 256 KiB — comfortably below macOS
/// `ARG_MAX` (262 144 B) which subprocess backends hit when the message is
/// passed as a CLI argument.
const MAX_GROUNDED_MESSAGE_BYTES: usize = 256 * 1024;

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

    // Validate context fields before session lookup (cheap, no allocation on happy path).
    // Includes source/timestamp injection guards and UiContext field limits.
    if let Err(e) = context::validate(&body.recent_events, body.ui_context.as_ref()) {
        return e.into_response();
    }

    // Read EVA identity under a brief read lock — no file I/O on hot path (Phase 1).
    let identity_text = state.eva_identity.read().await.text().to_owned();

    // SOUL vault grounding: top-5 BM25 entries, 400 ms hard timeout (Phase 2).
    // Query = "{route_tail} {message[:150]}" — route_tail boosts build-specific entries.
    // Skipped when soul_store is None (no SQLite backend) or on timeout.
    let soul_block = if let Some(soul) = state.soul_store.as_deref() {
        // route_tail = build UUID → boosts vault entries tagged to this build in FTS5
        let route_tail = id.to_string();
        let msg_prefix: String = body.message.chars().take(150).collect();
        let fts5_expr = format!("{route_tail} {msg_prefix}");
        let entries = tokio::time::timeout(
            std::time::Duration::from_millis(400),
            super::soul_grounding::search(soul, &fts5_expr),
        )
        .await
        .unwrap_or_default();
        let nonce = super::soul_grounding::vault_nonce();
        super::soul_grounding::format_block(&nonce, &entries)
    } else {
        String::new()
    };

    // Git context grounding: branch + 10 commits + status, 800 ms hard timeout (Phase 3).
    // Skipped silently when cwd is not a git repo or on timeout.
    let git_ctx = tokio::time::timeout(
        std::time::Duration::from_millis(800),
        super::git_context::gather(&state.config.cwd),
    )
    .await
    .unwrap_or(None);

    // Assemble the grounded prompt: context prelude prepended to the user message.
    // Passes event payloads verbatim — no silent truncation (§P check 2; northstar.md:491).
    let prelude = context::assemble_prompt_prelude(
        &identity_text,
        &soul_block,
        git_ctx.as_ref(),
        &body.recent_events,
        body.ui_context.as_ref(),
    );
    let grounded_message: std::borrow::Cow<str> = if prelude.is_empty() {
        std::borrow::Cow::Borrowed(&body.message)
    } else {
        std::borrow::Cow::Owned(format!("{prelude}\n{}", body.message))
    };

    if grounded_message.len() > MAX_GROUNDED_MESSAGE_BYTES {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "grounded_message_too_large",
                "max_bytes": MAX_GROUNDED_MESSAGE_BYTES
            })),
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
            call_ollama(
                &cfg.base_url,
                &cfg.model,
                &cfg.auth_token,
                &grounded_message,
            )
            .await
        }
        AgentSession::Lightarchitects(
            ClaudeBackend::Anthropic | ClaudeBackend::OllamaLaunch(_),
        )
        | AgentSession::Codex(_)
        | AgentSession::LightarchitectsNative(_)
        | AgentSession::MistralVibe(_) => {
            call_subprocess(&grounded_message, &session.copilot_proc, &session).await
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
