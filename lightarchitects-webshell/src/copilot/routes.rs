//! HTTP route handler for `POST /api/builds/:id/copilot`.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
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
    let headers = grounding_headers(&identity_text, &soul_block, git_ctx.as_ref());

    match result {
        Ok(text) => (StatusCode::OK, headers, Json(json!({ "response": text }))).into_response(),
        Err(reason) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "provider_error", "reason": reason })),
        )
            .into_response(),
    }
}

/// Build the `X-LA-Grounding` response header for the `CopilotContextTray` (Phase 4).
///
/// Format: `eva=<0|1>,soul=<N>,git=<N>`
fn grounding_headers(
    identity: &str,
    soul_block: &str,
    git: Option<&super::git_context::GitContext>,
) -> HeaderMap {
    let soul_count = soul_block.lines().filter(|l| l.starts_with("- ")).count();
    let git_count = git.map_or(0, |g| g.commits.len());
    let value = format!(
        "eva={},soul={},git={}",
        i32::from(!identity.is_empty()),
        soul_count,
        git_count,
    );
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&value) {
        headers.insert("x-la-grounding", v);
    }
    headers
}

/// Phase 5 — integration tests: grounding pipeline assembly + graceful degradation.
///
/// These tests verify that `assemble_prompt_prelude` + `grounding_headers` compose correctly
/// under nominal and failure-mode conditions, without invoking the AI backend.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod integration_tests {
    use super::{context, grounding_headers};
    use crate::copilot::{git_context, soul_grounding, soul_grounding::GroundingEntry};
    use std::path::Path;

    /// All 3 grounding sources present — prelude contains all 4 blocks; header is non-zero.
    #[tokio::test]
    async fn grounding_e2e() {
        let identity = "EVA identity: analytical, precision-first.";
        let entries = vec![
            GroundingEntry {
                title: "QUAL gate failure causes".to_owned(),
                excerpt: "Clippy -D warnings blocks the commit if any warning is emitted."
                    .to_owned(),
            },
            GroundingEntry {
                title: "cargo test configuration".to_owned(),
                excerpt: "Run `cargo test --all-features` before every merge.".to_owned(),
            },
            GroundingEntry {
                title: "CORS policy".to_owned(),
                excerpt: "AllowOrigin::exact restricted to webshell origin.".to_owned(),
            },
        ];
        let nonce = soul_grounding::vault_nonce();
        let soul_block = soul_grounding::format_block(&nonce, &entries);

        // Use the worktree itself — known git repo with commits.
        let sdk_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or(Path::new("/tmp"));
        let git_ctx = git_context::gather(sdk_root).await;

        let prelude =
            context::assemble_prompt_prelude(identity, &soul_block, git_ctx.as_ref(), &[], None);

        // All four blocks present
        assert!(
            prelude.contains("[Identity]"),
            "prelude missing [Identity] block"
        );
        assert!(
            prelude.contains("[Knowledge]"),
            "prelude missing [Knowledge] block"
        );
        assert!(prelude.contains("[Git:"), "prelude missing [Git] block");

        // Grounding header reflects counts
        let headers = grounding_headers(identity, &soul_block, git_ctx.as_ref());
        let hdr = headers.get("x-la-grounding").unwrap().to_str().unwrap();
        assert!(
            hdr.starts_with("eva=1,"),
            "header should show eva=1, got: {hdr}"
        );
        assert!(
            hdr.contains(",soul=3,"),
            "header should show soul=3, got: {hdr}"
        );
        // git count > 0 when run inside a git repo
        let git_count: usize = hdr.rsplit("git=").next().unwrap().parse().unwrap_or(0);
        assert!(git_count > 0, "header should show git>0, got: {hdr}");
    }

    /// SOUL timeout path: empty soul_block → prelude omits [Knowledge]; header shows soul=0.
    #[test]
    fn grounding_e2e_soul_timeout() {
        let identity = "EVA identity string.";
        // Simulate timeout result: empty block (what timeout returns on Err)
        let soul_block = String::new();
        let git_ctx: Option<git_context::GitContext> = None;

        let prelude =
            context::assemble_prompt_prelude(identity, &soul_block, git_ctx.as_ref(), &[], None);

        assert!(
            !prelude.contains("[Knowledge]"),
            "timed-out soul should omit [Knowledge]"
        );
        assert!(
            prelude.contains("[Identity]"),
            "identity should still be present"
        );

        let headers = grounding_headers(identity, &soul_block, None);
        let hdr = headers.get("x-la-grounding").unwrap().to_str().unwrap();
        assert!(
            hdr.contains("soul=0"),
            "soul=0 expected on timeout, got: {hdr}"
        );
    }

    /// Identity absent: empty string → prelude omits [Identity]; header shows eva=0.
    #[test]
    fn grounding_e2e_identity_absent() {
        let identity = "";
        let entries = vec![GroundingEntry {
            title: "entry".to_owned(),
            excerpt: "excerpt".to_owned(),
        }];
        let nonce = soul_grounding::vault_nonce();
        let soul_block = soul_grounding::format_block(&nonce, &entries);
        let git_ctx: Option<git_context::GitContext> = None;

        let prelude =
            context::assemble_prompt_prelude(identity, &soul_block, git_ctx.as_ref(), &[], None);

        assert!(
            !prelude.contains("[Identity]"),
            "absent identity should omit [Identity]"
        );
        assert!(
            prelude.contains("[Knowledge]"),
            "vault entries should still appear"
        );

        let headers = grounding_headers(identity, &soul_block, None);
        let hdr = headers.get("x-la-grounding").unwrap().to_str().unwrap();
        assert!(
            hdr.starts_with("eva=0,"),
            "eva=0 expected when identity empty, got: {hdr}"
        );
    }

    /// Git non-repo path: cwd outside any git repo → gather() returns None;
    /// prelude omits [Git]; header shows git=0.
    #[tokio::test]
    async fn grounding_e2e_git_non_repo() {
        let identity = "EVA identity string.";
        let soul_block = String::new();
        // /tmp is never a git repo
        let git_ctx = git_context::gather(Path::new("/tmp")).await;

        assert!(git_ctx.is_none(), "expected None for /tmp, got Some");

        let prelude =
            context::assemble_prompt_prelude(identity, &soul_block, git_ctx.as_ref(), &[], None);

        assert!(
            !prelude.contains("[Git:"),
            "non-repo should omit [Git] block"
        );

        let headers = grounding_headers(identity, &soul_block, None);
        let hdr = headers.get("x-la-grounding").unwrap().to_str().unwrap();
        assert!(
            hdr.ends_with("git=0"),
            "git=0 expected for non-repo, got: {hdr}"
        );
    }
}
