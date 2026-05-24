//! HTTP route handler for `POST /api/builds/:id/copilot`.

use std::sync::Arc;

use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use lightarchitects::agent::{
    ChainContext, ClaudeCliProvider, OllamaCliProvider,
    conversation::{
        ConversationEvent, ConversationSession, SessionConfig, SseTransport, Transport,
        helix_memory::HelixSessionMemory,
    },
};
use serde_json::json;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::{
    auth,
    config::{AgentSession, ClaudeBackend},
    events::{WebEventV2, types::TraceSpanSummary},
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
    if let Err(e) = context::validate(&body.recent_events, body.ui_context.as_ref()) {
        return e.into_response();
    }

    let identity_text = state.eva_identity.read().await.text().to_owned();
    let (prelude, soul_block, git_ctx) = gather_grounding(
        &state,
        id,
        &identity_text,
        &body.message,
        &body.recent_events,
        body.ui_context.as_ref(),
    )
    .await;

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

    let grounding_hdrs = grounding_headers(&identity_text, &soul_block, git_ctx.as_ref());

    // Native path: drive ConversationSession with SseTransport, stream back to caller.
    //
    // We pass the UN-grounded user message — the OllamaCliProvider sanitizer
    // enforces an 8,192-byte cap on user_prompt and the full grounding prelude
    // (EVA identity + SOUL + git + recent events) routinely exceeds that.
    // Grounding for the LA-native path is a follow-on (prelude trimming or
    // system-prompt placement instead of inline injection).
    if matches!(session.agent, AgentSession::LightarchitectsNative(_)) {
        return drive_native_sse(&body.message, session.cwd.clone(), grounding_hdrs);
    }

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
        | AgentSession::MistralVibe(_)
        | AgentSession::LightarchitectsNative(_) => {
            call_subprocess(&grounded_message, &session.copilot_proc, &session).await
        }
    };

    match result {
        Ok(text) => (
            StatusCode::OK,
            grounding_hdrs,
            Json(json!({ "response": text })),
        )
            .into_response(),
        Err(reason) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "provider_error", "reason": reason })),
        )
            .into_response(),
    }
}

/// Stream a single turn via [`ConversationSession`] + [`SseTransport`].
///
/// Creates a `tokio::io::duplex` pipe: one end feeds [`SseTransport`] (written by the
/// spawned task), the other becomes the HTTP response body. The caller receives raw
/// SSE frames (`event: …\ndata: …\n\n`) as they are emitted by the strategy engine.
fn drive_native_sse(
    grounded_message: &str,
    cwd: std::path::PathBuf,
    extra_headers: HeaderMap,
) -> Response {
    let (write_half, read_half) = tokio::io::duplex(64 * 1024);
    let msg = grounded_message.to_owned();

    // Provider selection: prefer Ollama Cloud when OLLAMA_API_KEY is set
    // (matches `agent_stream::run_ndjson` provider-build logic in the gateway),
    // otherwise fall back to ClaudeCliProvider for legacy compatibility.
    // ConversationSession is generic over a concrete provider type, so the
    // branches construct independent sessions rather than sharing a trait object.
    let use_ollama = std::env::var("OLLAMA_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .is_some();
    let model = std::env::var("LA_MODEL")
        .ok()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "nemotron-3-super:cloud".to_owned());
    let ollama_provider = if use_ollama {
        OllamaCliProvider::new(&model).ok()
    } else {
        None
    };

    let provider_name = if ollama_provider.is_some() {
        "ollama-cli"
    } else {
        "claude-cli"
    };
    tracing::info!(provider = provider_name, model = %model, "drive_native_sse spawning turn");
    tokio::spawn(async move {
        // Load up to 40 prior turns from the helix session file for this project.
        // Falls back to ephemeral in-memory if the helix path is absent.
        let memory = HelixSessionMemory::open(&cwd, 40);
        let restored = memory.restored_turn_count();
        tracing::debug!(restored_turns = restored, "helix session memory loaded");

        let config = SessionConfig {
            cwd,
            ..SessionConfig::default()
        };
        let mut transport = SseTransport::new(write_half);
        let ctx = ChainContext::default();
        let result = if let Some(provider) = ollama_provider {
            let mut session =
                ConversationSession::new(config, Arc::new(provider)).with_memory(Box::new(memory));
            session.run_turn(&msg, &mut transport, &ctx).await
        } else {
            let mut session =
                ConversationSession::new(config, Arc::new(ClaudeCliProvider::default()))
                    .with_memory(Box::new(memory));
            session.run_turn(&msg, &mut transport, &ctx).await
        };
        if let Err(e) = result {
            tracing::error!(provider = provider_name, error = %e, "drive_native_sse run_turn failed");
            // Surface the error to the SSE stream so the operator can see it.
            let _ = transport
                .emit(&ConversationEvent::Error {
                    message: e.to_string(),
                    recoverable: Some(false),
                })
                .await;
        } else {
            tracing::info!(
                provider = provider_name,
                "drive_native_sse run_turn completed"
            );
        }
        // transport drop closes write_half → EOF on read_half
    });

    let stream = ReaderStream::new(read_half);
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| Response::new(Body::empty()));

    for (k, v) in &extra_headers {
        response.headers_mut().insert(k.clone(), v.clone());
    }
    response
}

/// Gather all three grounding vectors (SOUL + git) concurrently, assemble the prelude,
/// and emit AYIN latency spans. Returns `(prelude, soul_block, git_ctx)`.
async fn gather_grounding(
    state: &AppState,
    id: Uuid,
    identity: &str,
    message: &str,
    recent_events: &[super::context::RecentEventEntry],
    ui_context: Option<&super::UiContext>,
) -> (String, String, Option<super::git_context::GitContext>) {
    // SOUL vault: top-5 BM25, 400 ms hard timeout (Phase 2).
    let soul_t0 = std::time::Instant::now();
    let (soul_block, soul_result_count, soul_timed_out) =
        if let Some(soul) = state.soul_store.as_deref() {
            let msg_prefix: String = message.chars().take(150).collect();
            let fts5_expr = format!("{id} {msg_prefix}");
            match tokio::time::timeout(
                std::time::Duration::from_millis(400),
                super::soul_grounding::search(soul, &fts5_expr),
            )
            .await
            {
                Ok(entries) => {
                    let count = entries.len();
                    let nonce = super::soul_grounding::vault_nonce();
                    (
                        super::soul_grounding::format_block(&nonce, &entries),
                        count,
                        false,
                    )
                }
                Err(_) => (String::new(), 0, true),
            }
        } else {
            (String::new(), 0, false)
        };
    let soul_ms = u64::try_from(soul_t0.elapsed().as_millis()).unwrap_or(u64::MAX);

    // Git context: branch + commits + status, 800 ms hard timeout (Phase 3).
    let git_t0 = std::time::Instant::now();
    let (git_ctx, git_timed_out) = match tokio::time::timeout(
        std::time::Duration::from_millis(800),
        super::git_context::gather(&state.config.cwd),
    )
    .await
    {
        Ok(ctx) => (ctx, false),
        Err(_) => (None, true),
    };
    let git_ms = u64::try_from(git_t0.elapsed().as_millis()).unwrap_or(u64::MAX);

    let prelude = context::assemble_prompt_prelude(
        identity,
        &soul_block,
        git_ctx.as_ref(),
        recent_events,
        ui_context,
    );

    emit_grounding_spans(
        state,
        soul_ms,
        soul_result_count,
        soul_timed_out,
        git_ms,
        git_ctx.as_ref(),
        git_timed_out,
        prelude.len(),
    );

    (prelude, soul_block, git_ctx)
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

/// Emit three AYIN spans for the grounding pipeline (Phase 6 — `copilot-eva-ambient`).
///
/// Spans are broadcast on the global SSE channel so the AYIN dashboard surfaces
/// `copilot.eva_ambient.*` latency without requiring a live build session.
///
/// Span names: `copilot.eva_ambient.soul_search_ms`, `copilot.eva_ambient.git_gather_ms`,
/// `copilot.eva_ambient.prelude_bytes`.
#[allow(clippy::too_many_arguments)]
fn emit_grounding_spans(
    state: &AppState,
    soul_ms: u64,
    soul_result_count: usize,
    soul_timed_out: bool,
    git_ms: u64,
    git: Option<&super::git_context::GitContext>,
    git_timed_out: bool,
    prelude_bytes: usize,
) {
    let ts = chrono::Utc::now().to_rfc3339();
    let _ = state.event_tx.send(WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(TraceSpanSummary {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: None,
            actor: "webshell".to_owned(),
            action: "copilot.eva_ambient.soul_search_ms".to_owned(),
            timestamp: ts.clone(),
            duration_ms: soul_ms,
            outcome: serde_json::json!(if soul_timed_out { "timeout" } else { "ok" }),
            metadata: serde_json::json!({
                "result_count": soul_result_count,
                "timed_out": soul_timed_out,
            }),
            strand_activations: Vec::new(),
        }),
        None,
    ));
    let _ = state.event_tx.send(WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(TraceSpanSummary {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: None,
            actor: "webshell".to_owned(),
            action: "copilot.eva_ambient.git_gather_ms".to_owned(),
            timestamp: ts.clone(),
            duration_ms: git_ms,
            outcome: serde_json::json!(if git_timed_out { "timeout" } else { "ok" }),
            metadata: serde_json::json!({
                "branch": git.map_or("", |g| g.branch.as_str()),
                "commit_count": git.map_or(0, |g| g.commits.len()),
                "timed_out": git_timed_out,
            }),
            strand_activations: Vec::new(),
        }),
        None,
    ));
    let _ = state.event_tx.send(WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(TraceSpanSummary {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: None,
            actor: "webshell".to_owned(),
            action: "copilot.eva_ambient.prelude_bytes".to_owned(),
            timestamp: ts,
            duration_ms: 0,
            outcome: serde_json::json!("ok"),
            metadata: serde_json::json!({ "prelude_bytes": prelude_bytes }),
            strand_activations: Vec::new(),
        }),
        None,
    ));
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
