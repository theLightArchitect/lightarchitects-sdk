//! HTTP route handler for `POST /api/builds/:id/copilot`.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use axum::body::Bytes;
use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt as _;
use lightarchitects::agent::conversation::{
    ConversationEvent, SseTransport, Transport, event::TerminationReason,
};
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use tracing::Instrument as _;
use uuid::Uuid;

use crate::{
    auth,
    config::{AgentSession, ClaudeBackend},
    events::{WebEventV2, types::TraceSpanSummary},
    server::AppState,
};

use lightarchitects::chat::mode::Mode;

use super::native_session::{CliSubprocessHandle, NativeSessionPool};
use super::strategy_runner::dispatch_strategy_initial;
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
#[allow(clippy::too_many_lines)]
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

    let Some(session) = state.builds.get(id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "build_not_found" })),
        )
            .into_response();
    };
    // Emit the user.message span FIRST so that grounding spans and tool spans
    // can be parented to the turn.  Use session_span_id (from a prior turn) as
    // the parent of the user message, creating a proper session → turn hierarchy.
    // If session_span_id is None (first turn), the user message becomes a root —
    // still correct because the turn IS the root of that interaction.
    let session_span_id: Option<String> = {
        let guard = session.copilot_proc.lock().await;
        guard.as_ref().and_then(|p| p.session_span_id.clone())
    };
    let turn_span_id = emit_message_span(
        &state,
        "user",
        &body.message,
        session_span_id.as_deref(),
        Some(id),
    );

    // Grounding spans are parented to the turn (not the session root) so they
    // appear as children of the user's message in the Lineage Circuit.
    let (prelude, soul_block, git_ctx) = gather_grounding(
        &state,
        id,
        &identity_text,
        &body.message,
        &body.recent_events,
        body.ui_context.as_ref(),
        Some(id),
        Some(turn_span_id.clone()),
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

    let grounding_hdrs = grounding_headers(&identity_text, &soul_block, git_ctx.as_ref());

    // ── Strategy pre-emption ──
    // Slash commands (/BUILD, /SECURE, /ENRICH, /SCRUM) route directly to the
    // strategy engine instead of the LLM.  This prevents the LLM from generating
    // a conversational explanation instead of taking action.
    let mode = Mode::classify(
        &body.message,
        &lightarchitects::chat::roster::ActiveRoster::new(),
    );
    if let Some(strategy_id) = mode.strategy_id() {
        if let Some(strategy) =
            lightarchitects::agent::loops::registry::StrategyRegistry::lookup(strategy_id)
        {
            tracing::info!(strategy_id, build_id = %id, "strategy pre-emption: routing slash command to strategy engine");
            return dispatch_strategy_initial(
                strategy,
                id,
                &body.message,
                turn_span_id,
                session.event_tx.clone(),
                Arc::clone(&state.resume_registry),
            )
            .await;
        }
        // Fall through to LLM if strategy not found (unknown future mode).
        tracing::warn!(
            strategy_id,
            "strategy pre-emption: unknown strategy ID, falling through to LLM"
        );
    }

    // ── Context window rotation ──
    // After `max_context_prompts` turns, the cumulative model context can exhaust
    // the Ollama context window, producing empty responses.  Auto-clear the session
    // to reset the context.  The HelixSessionMemory will still reload recent turns
    // from disk, but the cumulative token load is shed.
    let turn_count = session.turn_count.load(std::sync::atomic::Ordering::SeqCst);
    if turn_count >= state.config.max_context_prompts {
        tracing::warn!(
            build_id = %id,
            turns = turn_count,
            max = state.config.max_context_prompts,
            "auto-clearing copilot session to prevent context window exhaustion"
        );
        // Kill any in-progress copilot session and wipe the helix memory file.
        // The next turn starts with a fresh context window while HelixSessionMemory
        // reloads recent turns from disk (bounded by the `open(&cwd, 40)` limit).
        {
            let mut guard = session.copilot_proc.lock().await;
            guard.take();
        }
        let path = lightarchitects::agent::conversation::helix_memory::session_path(&session.cwd);
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!(path = %path.display(), error = %e, "auto-clear: failed to delete session file");
            }
        }
        session
            .turn_count
            .store(0, std::sync::atomic::Ordering::SeqCst);
    }
    session
        .turn_count
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    // Native path: forward the raw user message to the persistent `lightarchitects`
    // subprocess, which handles its own grounding via HelixSessionMemory.
    // The prelude is not injected into stdin — the CLI reads stdin line-by-line
    // and a multi-line prelude would be split into multiple turns.
    if matches!(session.agent, AgentSession::LightarchitectsNative(_)) {
        return drive_native_sse(
            id,
            &body.message,
            session.cwd.clone(),
            grounding_hdrs,
            Arc::clone(&session.native_interrupt_flag),
            state.la_native_api_key.clone(),
            state.config.token.clone(),
            turn_span_id,
            Arc::clone(&state.native_session_pool),
        );
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
            ClaudeBackend::Anthropic | ClaudeBackend::OllamaLaunch(_) | ClaudeBackend::LiteLlm(_),
        )
        | AgentSession::Codex(_)
        | AgentSession::MistralVibe(_) => {
            call_subprocess(&grounded_message, &session.copilot_proc, &session).await
        }
        // Guarded above by the `if matches!(…LightarchitectsNative…)` early return.
        AgentSession::LightarchitectsNative(_) => unreachable!("native session not intercepted"),
    };

    match result {
        Ok(text) => {
            emit_message_span(&state, "assistant", &text, Some(&turn_span_id), Some(id));
            // Persist Q&A to SOUL vault so future turns can retrieve this exchange
            // via 4-signal RRF.  Fire-and-forget — client already has the response.
            if let Some(client) = state.soul_client.get() {
                super::soul_grounding::spawn_write_turn(
                    std::sync::Arc::clone(client),
                    id,
                    body.message.clone(),
                    text.clone(),
                );
            }
            (
                StatusCode::OK,
                grounding_hdrs,
                Json(json!({ "response": text })),
            )
                .into_response()
        }
        Err(reason) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "provider_error", "reason": reason })),
        )
            .into_response(),
    }
}

// Stream a single turn via [`ConversationSession`] + [`SseTransport`].
//
// Creates a `tokio::io::duplex` pipe: one end feeds [`SseTransport`] (written by the
// spawned task), the other becomes the HTTP response body. The caller receives raw
// SSE frames (`event: …\ndata: …\n\n`) as they are emitted by the strategy engine.

/// Abort guard: calls `AbortHandle::abort()` when dropped.
///
/// Stored inside the SSE body stream's `map` closure so the in-flight
/// `native_turn_task` is cancelled when the HTTP client disconnects and the
/// response `Body` is dropped (W7.3).
struct AbortOnDrop(tokio::task::AbortHandle);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Body of the spawned turn task — separated from [`drive_native_sse`] to keep
/// that function within the 100-line clippy limit.
//
// Subprocess model: one persistent `lightarchitects --output-format stream-json`
// process per build UUID, kept alive in `session_pool` across HTTP turns.
// Prompts are written to stdin one line at a time; NDJSON events are read from
// stdout until the terminal `{"type":"result",…}` line arrives.
// On EOF, error, or interrupt the subprocess is evicted from the pool so the
// next turn cold-starts a fresh process.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn native_turn_task(
    cwd: std::path::PathBuf,
    write_half: tokio::io::DuplexStream,
    msg: String,
    interrupt_flag: Arc<AtomicBool>,
    turn_span_id: String,
    build_id: Uuid,
    model: String,
    session_pool: NativeSessionPool,
    binary: String,
) {
    let mut transport = SseTransport::new(write_half);
    let start = std::time::Instant::now();

    // ── Acquire or spawn the persistent subprocess ────────────────────────
    let session_arc = match session_pool.entry(build_id) {
        dashmap::mapref::entry::Entry::Occupied(o) => o.get().clone(),
        dashmap::mapref::entry::Entry::Vacant(v) => {
            match CliSubprocessHandle::try_spawn(
                &cwd,
                build_id,
                &binary,
                Arc::clone(&interrupt_flag),
            ) {
                Ok(handle) => {
                    tracing::info!(build_id = %build_id, binary = %binary, "native_turn: cold-start subprocess");
                    let arc = Arc::new(tokio::sync::Mutex::new(handle));
                    let cloned = Arc::clone(&arc);
                    v.insert(arc);
                    cloned
                }
                Err(e) => {
                    tracing::error!(build_id = %build_id, error = %e, "native_turn: subprocess spawn failed");
                    let _ = transport
                        .emit(&ConversationEvent::Error {
                            message: format!("subprocess spawn failed: {e}"),
                            recoverable: Some(false),
                        })
                        .await;
                    return;
                }
            }
        }
    };

    // ── Lock the session for this turn (serialises concurrent turns) ─────
    let mut handle = session_arc.lock().await;

    // Write the prompt line; subprocess reads one line = one turn.
    let prompt_line = format!("{msg}\n");
    if let Err(e) = handle.stdin.write_all(prompt_line.as_bytes()).await {
        tracing::error!(build_id = %build_id, error = %e, "native_turn: stdin write failed");
        session_pool.remove(&build_id);
        let _ = transport
            .emit(&ConversationEvent::Error {
                message: format!("stdin write failed: {e}"),
                recoverable: Some(false),
            })
            .await;
        return;
    }
    if let Err(e) = handle.stdin.flush().await {
        tracing::error!(build_id = %build_id, error = %e, "native_turn: stdin flush failed");
        session_pool.remove(&build_id);
        let _ = transport
            .emit(&ConversationEvent::Error {
                message: format!("stdin flush failed: {e}"),
                recoverable: Some(false),
            })
            .await;
        return;
    }

    // ── NDJSON → SSE translation loop ────────────────────────────────────
    //
    // CLI event types (from `run_stream_json_loop`):
    //   thinking   → ConversationEvent::Thinking
    //   tool_use   → ConversationEvent::ToolStart   (synthetic id = "tool-N")
    //   tool_result→ ConversationEvent::ToolComplete (same id)
    //   context    → ConversationEvent::TokenUsage  (used tokens as `input`)
    //   result     → ConversationEvent::Text + Complete (terminal line; break)
    //   error / strategy_halt → ConversationEvent::Error (evict subprocess)
    let mut tool_counter: u64 = 0;
    loop {
        if handle.interrupt_flag.load(Ordering::SeqCst) {
            tracing::info!(build_id = %build_id, "native_turn: interrupted");
            session_pool.remove(&build_id);
            let _ = transport
                .emit(&ConversationEvent::Error {
                    message: "interrupted".to_owned(),
                    recoverable: Some(false),
                })
                .await;
            return;
        }
        match handle.stdout.next_line().await {
            Ok(Some(line)) if !line.is_empty() => {
                let val: serde_json::Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                match val["type"].as_str().unwrap_or("") {
                    "thinking" => {
                        let content = val["text"].as_str().unwrap_or("").to_owned();
                        let _ = transport
                            .emit(&ConversationEvent::Thinking { content })
                            .await;
                    }
                    "tool_use" => {
                        tool_counter += 1;
                        let name = val["tool"].as_str().unwrap_or("unknown").to_owned();
                        let summary = val["input_summary"].as_str().unwrap_or("").to_owned();
                        let _ = transport
                            .emit(&ConversationEvent::ToolStart {
                                name,
                                id: format!("tool-{tool_counter}"),
                                input: serde_json::json!({ "summary": summary }),
                            })
                            .await;
                    }
                    "tool_result" => {
                        let success = val["success"].as_bool().unwrap_or(true);
                        let duration_ms = val["duration_ms"].as_u64().unwrap_or(0);
                        let result = val["preview"].as_str().map(ToOwned::to_owned);
                        let _ = transport
                            .emit(&ConversationEvent::ToolComplete {
                                id: format!("tool-{tool_counter}"),
                                success,
                                duration_ms,
                                result,
                            })
                            .await;
                    }
                    "context" => {
                        let used = val["used"].as_u64().unwrap_or(0);
                        let _ = transport
                            .emit(&ConversationEvent::TokenUsage {
                                input: used,
                                output: 0,
                            })
                            .await;
                    }
                    "result" => {
                        // CLI emits {"type":"result","subtype":"success","result":"..."}
                        // or {"type":"result","subtype":"error","error":"..."}.
                        // Fall back to "text" for forward compatibility.
                        if val["subtype"] == "error" {
                            let message = val["error"].as_str().unwrap_or("agent error").to_owned();
                            tracing::warn!(build_id = %build_id, message = %message, "native_turn: result error");
                            session_pool.remove(&build_id);
                            let _ = transport
                                .emit(&ConversationEvent::Error {
                                    message,
                                    recoverable: Some(false),
                                })
                                .await;
                            return;
                        }
                        let text = val["result"]
                            .as_str()
                            .or_else(|| val["text"].as_str())
                            .unwrap_or("")
                            .to_owned();
                        if !text.is_empty() {
                            let _ = transport
                                .emit(&ConversationEvent::Text { chunk: text })
                                .await;
                        }
                        let _ = transport
                            .emit(&ConversationEvent::Complete {
                                reason: TerminationReason::Complete,
                            })
                            .await;
                        break;
                    }
                    "strategy_halt" | "error" => {
                        let message = val["message"]
                            .as_str()
                            .or_else(|| val["text"].as_str())
                            .or_else(|| val["error"].as_str())
                            .unwrap_or("agent halted")
                            .to_owned();
                        tracing::warn!(build_id = %build_id, message = %message, "native_turn: subprocess halt");
                        session_pool.remove(&build_id);
                        let _ = transport
                            .emit(&ConversationEvent::Error {
                                message,
                                recoverable: Some(false),
                            })
                            .await;
                        return;
                    }
                    _ => {}
                }
            }
            Ok(Some(_)) => {} // empty line — skip
            Ok(None) => {
                // EOF — subprocess exited unexpectedly
                tracing::warn!(build_id = %build_id, "native_turn: subprocess stdout EOF");
                session_pool.remove(&build_id);
                let _ = transport
                    .emit(&ConversationEvent::Error {
                        message: "subprocess exited unexpectedly".to_owned(),
                        recoverable: Some(false),
                    })
                    .await;
                return;
            }
            Err(e) => {
                tracing::error!(build_id = %build_id, error = %e, "native_turn: stdout read error");
                session_pool.remove(&build_id);
                let _ = transport
                    .emit(&ConversationEvent::Error {
                        message: format!("subprocess read error: {e}"),
                        recoverable: Some(false),
                    })
                    .await;
                return;
            }
        }
    }

    drop(handle); // release mutex before span emit

    let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    tracing::info!(build_id = %build_id, model = %model, duration_ms, "native_turn: completed");
    emit_disk_span(
        "lightarchitects-cli",
        "assistant.response",
        serde_json::json!({
            "build_id": build_id.to_string(),
            "provider": "lightarchitects",
            "model": model,
            "duration_ms": duration_ms,
        }),
        lightarchitects::ayin::TraceOutcome::Continue,
        turn_span_id.parse::<uuid::Uuid>().ok(),
        Some(build_id),
    );
    // transport drop closes write_half → EOF on read_half
}

// la_native_api_key is retained for secret-redaction only (OA-3: scrub tokens
// before bytes leave the process); provider selection is handled entirely by
// the subprocess binary, which reads its own credentials at startup.
#[allow(clippy::too_many_arguments)]
fn drive_native_sse(
    build_id: Uuid,
    msg: &str,
    cwd: std::path::PathBuf,
    extra_headers: HeaderMap,
    interrupt_flag: Arc<AtomicBool>,
    la_native_api_key: Option<secrecy::SecretString>,
    session_token: String,
    turn_span_id: String,
    session_pool: NativeSessionPool,
) -> Response {
    // Reset any prior interrupt before starting a new turn.
    interrupt_flag.store(false, Ordering::SeqCst);
    let (write_half, read_half) = tokio::io::duplex(64 * 1024);
    let msg = msg.to_owned();

    let binary = super::resolve_binary("lightarchitects");
    let model = std::env::var("LA_MODEL")
        .ok()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "nemotron-3-super:cloud".to_owned());

    // Span carries build_id so AYIN correlates all tracing events for this turn.
    let span = tracing::info_span!(
        "native_turn",
        build_id = %build_id,
        provider = "lightarchitects",
        model = %model,
    );
    tracing::info!(parent: &span, "drive_native_sse spawning turn");

    let handle = tokio::spawn(
        native_turn_task(
            cwd,
            write_half,
            msg,
            interrupt_flag,
            turn_span_id,
            build_id,
            model,
            session_pool,
            binary,
        )
        .instrument(span),
    );

    // W7.3: AbortOnDrop cancels the in-flight task on client disconnect.
    // GAP-3: every chunk is scrubbed for the session token and OLLAMA_API_KEY
    // before the bytes leave the process (branch-free hot path when no secret present).
    let abort_guard = AbortOnDrop(handle.abort_handle());
    let api_key_for_redact: Option<String> = la_native_api_key
        .as_ref()
        .map(|s| secrecy::ExposeSecret::expose_secret(s).to_owned());
    let stream = ReaderStream::new(read_half).map(move |chunk_result| {
        let _ = &abort_guard;
        match chunk_result {
            Ok(bytes) => {
                let s = String::from_utf8_lossy(&bytes);
                let key_ref: &str = api_key_for_redact.as_deref().unwrap_or("");
                let redacted = crate::events::sse_handler::redact_secrets(
                    s.as_ref(),
                    &[session_token.as_str(), key_ref],
                );
                if redacted == s.as_ref() {
                    Ok(bytes)
                } else {
                    Ok(Bytes::from(redacted.into_bytes()))
                }
            }
            Err(e) => Err(e),
        }
    });
    let response_result = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from_stream(stream));

    let mut response = match response_result {
        Ok(r) => r,
        Err(e) => {
            // Static header names/values cannot produce an error in practice;
            // this branch exists to satisfy the no-unwrap/no-expect policy.
            tracing::error!(error = %e, "BUG: failed to construct SSE response with static headers");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "sse_response_construction_failed" })),
            )
                .into_response();
        }
    };

    for (k, v) in &extra_headers {
        response.headers_mut().insert(k.clone(), v.clone());
    }
    response
}

/// Gather all grounding vectors (SOUL + git) **in parallel**, assemble the
/// prelude, and emit AYIN latency spans.  Returns `(prelude, soul_block, git_ctx)`.
///
/// Phase-10 (Phase 3): SOUL and git futures run concurrently via
/// `tokio::join!`, dropping worst-case wall-clock from `soul_timeout +
/// git_timeout` (1200 ms sequential) to `max(soul_timeout, git_timeout)`
/// (800 ms parallel).  Each future retains its own independent
/// `tokio::time::timeout`, so a hang in one source does not block the other.
#[allow(clippy::too_many_arguments)]
async fn gather_grounding(
    state: &AppState,
    id: Uuid,
    identity: &str,
    message: &str,
    recent_events: &[super::context::RecentEventEntry],
    ui_context: Option<&super::UiContext>,
    build_id: Option<Uuid>,
    parent_span_id: Option<String>,
) -> (String, String, Option<super::git_context::GitContext>) {
    let wall_t0 = std::time::Instant::now();

    // SOUL future: prefer 4-signal RRF via MCP client; fall back to BM25.
    let soul_fut = async {
        let soul_t0 = std::time::Instant::now();

        // Primary: 4-signal RRF (BM25 + semantic + graph + structural).
        if let Some(client) = state.soul_client.get() {
            let (block, count, timed_out, ms) =
                super::soul_grounding::query_rrf(client, id, message).await;
            if !block.is_empty() || timed_out {
                return (block, count, timed_out, ms);
            }
        }

        // Fallback: BM25 SQLite when the MCP client is unavailable or empty.
        let (block, count, timed_out) = if let Some(soul) = state.soul_store.as_deref() {
            let msg_prefix: String = message.chars().take(150).collect();
            let fts5_expr = format!("{id} {msg_prefix}");
            match tokio::time::timeout(
                std::time::Duration::from_millis(400),
                super::soul_grounding::search(soul, &fts5_expr),
            )
            .await
            {
                Ok(entries) => {
                    let n = entries.len();
                    let nonce = super::soul_grounding::vault_nonce();
                    (
                        super::soul_grounding::format_block(&nonce, &entries),
                        n,
                        false,
                    )
                }
                Err(_) => (String::new(), 0, true),
            }
        } else {
            (String::new(), 0, false)
        };
        let ms = u64::try_from(soul_t0.elapsed().as_millis()).unwrap_or(u64::MAX);
        (block, count, timed_out, ms)
    };

    // Git future: branch + commits + status, 800 ms hard timeout.
    let git_fut = async {
        let git_t0 = std::time::Instant::now();
        let (ctx, timed_out) = match tokio::time::timeout(
            std::time::Duration::from_millis(800),
            super::git_context::gather(&state.config.cwd),
        )
        .await
        {
            Ok(ctx) => (ctx, false),
            Err(_) => (None, true),
        };
        let ms = u64::try_from(git_t0.elapsed().as_millis()).unwrap_or(u64::MAX);
        (ctx, timed_out, ms)
    };

    // Code-grounding future: grep the source tree for symbols in the message.
    // `search_code` handles its own 500 ms internal timeout; we just join it.
    let code_root = state.config.cwd.clone();
    let code_fut = super::code_grounding::search_code(&code_root, message);

    // Parallel execution — wall-clock is max of the three timeouts, not sum.
    let (
        (soul_block, soul_result_count, soul_timed_out, soul_ms),
        (git_ctx, git_timed_out, git_ms),
        code_block,
    ) = tokio::join!(soul_fut, git_fut, code_fut);
    let code_block = code_block.unwrap_or_default();
    let grounding_wall_ms = u64::try_from(wall_t0.elapsed().as_millis()).unwrap_or(u64::MAX);

    let prelude = context::assemble_prompt_prelude(
        identity,
        &soul_block,
        &code_block,
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
        grounding_wall_ms,
        build_id,
        parent_span_id.as_deref(),
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

/// Emit a webshell AYIN span to disk (fire-and-forget, alongside SSE).
///
/// Persists spans so the AYIN Lineage Circuit dashboard at `:3742` can build
/// the session tree from disk files, not only from the live SSE stream.
pub(crate) fn emit_disk_span(
    actor: &str,
    action: &str,
    metadata: serde_json::Value,
    outcome: lightarchitects::ayin::TraceOutcome,
    parent_id: Option<Uuid>,
    session_id: Option<Uuid>,
) {
    use lightarchitects::ayin::{
        emit_span_background,
        span::{Actor, TraceContext},
    };
    let mut ctx = TraceContext::new(Actor::new(actor), action)
        .outcome(outcome)
        .metadata(metadata);
    if let Some(pid) = parent_id {
        ctx = ctx.parent(pid);
    }
    let sid_str = session_id.map(|u| u.to_string());
    if let Some(ref sid) = sid_str {
        ctx = ctx.session_id(sid);
    }
    emit_span_background(ctx);
}

/// Emit a user or assistant message span parented to the session root.
///
/// `kind` is `"user"` or `"assistant"`. Makes conversation turns visible as
/// first-class nodes in the AYIN Lineage Circuit dashboard.
/// Emit an AYIN turn boundary span and return the new span ID.
///
/// `kind = "user"` → `action: "user.message"`, `actor: "user"` (gold in Lineage Circuit).
/// `kind = "assistant"` → `action: "assistant.response"`, `actor: "claude"` (gold leaf).
fn emit_message_span(
    state: &AppState,
    kind: &str,
    content: &str,
    parent_id: Option<&str>,
    build_id: Option<Uuid>,
) -> String {
    let span_id = uuid::Uuid::new_v4().to_string();
    let (actor, action) = if kind == "user" {
        ("user", "user.message")
    } else {
        ("claude", "assistant.response")
    };
    let _ = state.event_tx.send(WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(TraceSpanSummary {
            id: span_id.clone(),
            parent_id: parent_id.map(ToOwned::to_owned),
            actor: actor.to_owned(),
            action: action.to_owned(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            duration_ms: 0,
            outcome: serde_json::json!("Continue"),
            metadata: serde_json::json!({
                "kind": kind,
                "preview": &content[..content.len().min(200)],
            }),
            strand_activations: Vec::new(),
            session_id: build_id.map(|id| id.to_string()),
            decision_points: Vec::new(),
        }),
        build_id,
    ));
    emit_disk_span(
        actor,
        action,
        json!({
            "kind": kind,
            "preview": &content[..content.len().min(200)],
        }),
        lightarchitects::ayin::TraceOutcome::Continue,
        parent_id.and_then(|s| s.parse::<Uuid>().ok()),
        build_id,
    );
    span_id
}

/// Emit AYIN spans for the grounding pipeline (Phase 6 — `copilot-eva-ambient`).
///
/// Spans are broadcast on the global SSE channel so the AYIN dashboard surfaces
/// `copilot.eva_ambient.*` latency without requiring a live build session.
///
/// Span names:
/// - `copilot.eva_ambient.soul_search_ms` — individual SOUL latency
/// - `copilot.eva_ambient.git_gather_ms` — individual git latency
/// - `copilot.eva_ambient.grounding_wall_ms` — parallel wall-clock max
///   (Phase-10 Phase 3: introduced when `gather_grounding` was parallelised
///   via `tokio::join!`).  Always `< soul_ms + git_ms`; expected ≈
///   `max(soul_ms, git_ms)` + small scheduling overhead.
/// - `copilot.eva_ambient.prelude_bytes` — prelude payload size
// Duration values are bounded by 400 ms timeout; f64 has enough precision.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]
fn emit_grounding_spans(
    state: &AppState,
    soul_ms: u64,
    soul_result_count: usize,
    soul_timed_out: bool,
    git_ms: u64,
    git: Option<&super::git_context::GitContext>,
    git_timed_out: bool,
    prelude_bytes: usize,
    grounding_wall_ms: u64,
    build_id: Option<Uuid>,
    parent_id: Option<&str>,
) {
    let ts = chrono::Utc::now().to_rfc3339();
    let parent = parent_id.map(ToOwned::to_owned);
    let span_pid = parent_id.and_then(|s| s.parse::<Uuid>().ok());
    let degraded = soul_timed_out || git_timed_out;
    let parallel_efficiency: f64 = if degraded {
        0.0
    } else {
        let total = soul_ms + git_ms;
        if total == 0 {
            1.0
        } else {
            let speedup = total.saturating_sub(grounding_wall_ms);
            (speedup as f64 / total as f64).min(0.99)
        }
    };
    let _ = state.event_tx.send(WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(TraceSpanSummary {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: parent.clone(),
            session_id: build_id.map(|id| id.to_string()),
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
            decision_points: vec![serde_json::json!({
                "name": "grounding_source",
                "input": "soul_fts5",
                "decision": if soul_timed_out { "timed_out_fallback" } else { "retrieved" },
                "confidence": if soul_timed_out { 0.0_f64 } else { 0.85_f64 },
                "duration_ms": soul_ms,
            })],
        }),
        build_id,
    ));
    emit_disk_span(
        "webshell",
        "copilot.eva_ambient.soul_search_ms",
        json!({ "result_count": soul_result_count, "timed_out": soul_timed_out }),
        if soul_timed_out {
            lightarchitects::ayin::TraceOutcome::Block
        } else {
            lightarchitects::ayin::TraceOutcome::Continue
        },
        span_pid,
        build_id,
    );
    let _ = state.event_tx.send(WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(TraceSpanSummary {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: parent.clone(),
            session_id: build_id.map(|id| id.to_string()),
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
            decision_points: vec![serde_json::json!({
                "name": "git_available",
                "input": "git_context",
                "decision": if git_timed_out { "timed_out" } else if git.is_some() { "available" } else { "absent" },
                "confidence": if git_timed_out { 0.0_f64 } else if git.is_some() { 0.95_f64 } else { 0.80_f64 },
                "duration_ms": git_ms,
            })],
        }),
        build_id,
    ));
    emit_disk_span(
        "webshell",
        "copilot.eva_ambient.git_gather_ms",
        json!({
            "branch": git.map_or("", |g| g.branch.as_str()),
            "commit_count": git.map_or(0, |g| g.commits.len()),
            "timed_out": git_timed_out,
        }),
        if git_timed_out {
            lightarchitects::ayin::TraceOutcome::Block
        } else {
            lightarchitects::ayin::TraceOutcome::Continue
        },
        span_pid,
        build_id,
    );
    let _ = state.event_tx.send(WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(TraceSpanSummary {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: parent.clone(),
            session_id: build_id.map(|id| id.to_string()),
            actor: "webshell".to_owned(),
            action: "copilot.eva_ambient.grounding_wall_ms".to_owned(),
            timestamp: ts.clone(),
            duration_ms: grounding_wall_ms,
            outcome: serde_json::json!("ok"),
            metadata: serde_json::json!({
                "soul_ms": soul_ms,
                "git_ms": git_ms,
                "parallel_speedup_ms": (soul_ms + git_ms).saturating_sub(grounding_wall_ms),
                "pivot": degraded,
            }),
            strand_activations: Vec::new(),
            decision_points: vec![serde_json::json!({
                "name": "parallel_efficiency",
                "input": "grounding",
                "decision": if degraded { "degraded" } else { "optimal" },
                "confidence": parallel_efficiency,
                "duration_ms": grounding_wall_ms,
            })],
        }),
        build_id,
    ));
    emit_disk_span(
        "webshell",
        "copilot.eva_ambient.grounding_wall_ms",
        json!({
            "soul_ms": soul_ms,
            "git_ms": git_ms,
            "parallel_speedup_ms": (soul_ms + git_ms).saturating_sub(grounding_wall_ms),
            "pivot": degraded,
        }),
        lightarchitects::ayin::TraceOutcome::Continue,
        span_pid,
        build_id,
    );
    let _ = state.event_tx.send(WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(TraceSpanSummary {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: parent,
            actor: "webshell".to_owned(),
            action: "copilot.eva_ambient.prelude_bytes".to_owned(),
            timestamp: ts,
            duration_ms: 0,
            outcome: serde_json::json!("ok"),
            metadata: serde_json::json!({ "prelude_bytes": prelude_bytes }),
            strand_activations: Vec::new(),
            session_id: build_id.map(|id| id.to_string()),
            decision_points: Vec::new(),
        }),
        build_id,
    ));
    emit_disk_span(
        "webshell",
        "copilot.eva_ambient.prelude_bytes",
        json!({ "prelude_bytes": prelude_bytes }),
        lightarchitects::ayin::TraceOutcome::Continue,
        span_pid,
        build_id,
    );
}

/// `POST /api/builds/:id/copilot/interrupt` — signal a running native turn to stop.
///
/// Sets the shared `native_interrupt_flag` on the build session. The running
/// `ConversationSession` polls the flag after each chunk and returns early when
/// it is set. Idempotent: safe to call when no turn is in flight.
pub async fn copilot_interrupt_handler(
    _: auth::AuthGuard,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let Some(session) = state.builds.get(id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "build_not_found" })),
        )
            .into_response();
    };
    session.native_interrupt_flag.store(true, Ordering::SeqCst);
    StatusCode::NO_CONTENT.into_response()
}

/// `POST /api/builds/:id/copilot/clear` — wipe the in-progress conversation memory.
///
/// Deletes today's helix session file for the build's `cwd`. The next turn
/// will start with a blank context window. No-op if no file exists yet.
pub async fn copilot_clear_handler(
    _: auth::AuthGuard,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let Some(session) = state.builds.get(id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "build_not_found" })),
        )
            .into_response();
    };
    let path = lightarchitects::agent::conversation::helix_memory::session_path(&session.cwd);
    if path.exists() {
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!(path = %path.display(), error = %e, "copilot_clear: failed to delete session file");
        }
    }
    tracing::info!(build_id = %id, path = %path.display(), "copilot_clear: session memory wiped");
    StatusCode::NO_CONTENT.into_response()
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

        let prelude = context::assemble_prompt_prelude(
            identity,
            &soul_block,
            "",
            git_ctx.as_ref(),
            &[],
            None,
        );

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

        let prelude = context::assemble_prompt_prelude(
            identity,
            &soul_block,
            "",
            git_ctx.as_ref(),
            &[],
            None,
        );

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

        let prelude = context::assemble_prompt_prelude(
            identity,
            &soul_block,
            "",
            git_ctx.as_ref(),
            &[],
            None,
        );

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

        let prelude = context::assemble_prompt_prelude(
            identity,
            &soul_block,
            "",
            git_ctx.as_ref(),
            &[],
            None,
        );

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

// ── HITL resolve ─────────────────────────────────────────────────────────────

/// Body for `POST /api/copilot/hitl/resolve`.
#[derive(serde::Deserialize)]
pub struct HitlResolveBody {
    /// The 16-char hex nonce returned by the strategy pause route.
    pub request_id: String,
    /// Session token that was active when the strategy was dispatched.
    ///
    /// Must match the session stored in [`ResumeRegistry`] — prevents
    /// cross-session confused-deputy attacks.
    pub session_id: String,
    /// Index into [`HitlRequest::options`] selected by the operator.
    pub choice: usize,
    /// S1-F4: when `true`, the operator dismissed without selecting an option.
    ///
    /// Consumes the nonce (preventing replay) but skips choice bounds validation
    /// and strategy execution. The strategy loop is abandoned at this pause point.
    #[serde(default)]
    pub dismissed: bool,
}

/// `POST /api/copilot/hitl/resolve` — operator resolves a paused strategy.
///
/// # Security model
///
/// - **`AuthGuard`**: Bearer token required — unauthenticated callers rejected 401.
/// - **Single-use nonce**: `ResumeRegistry::take` removes the entry on first
///   successful retrieval; replay returns 404.
/// - **Session binding**: `session_id` must match the one stored at park time;
///   mismatch returns 403 (entry is preserved for the legitimate session).
/// - **Choice bounds**: `choice` must be < `hitl.options.len()` — out-of-range
///   returns 422 to prevent silent index-out-of-bounds on the strategy side.
pub async fn copilot_hitl_resolve_handler(
    _: auth::AuthGuard,
    State(state): State<AppState>,
    Json(body): Json<HitlResolveBody>,
) -> impl IntoResponse {
    let registry = &state.resume_registry;

    let Some((loop_state, strategy_id, options_count)) =
        registry.take(&body.request_id, &body.session_id)
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "unknown, expired, or already-consumed request_id" })),
        )
            .into_response();
    };

    // S1-F4: dismiss consumes the nonce but skips strategy execution.
    if body.dismissed {
        tracing::info!(
            request_id = %body.request_id,
            strategy_id = %strategy_id,
            "hitl_resolve: operator dismissed strategy pause"
        );
        return (StatusCode::OK, Json(json!({ "status": "dismissed" }))).into_response();
    }

    if body.choice >= options_count {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "error": "choice index out of range",
                "choice": body.choice,
                "options_count": options_count
            })),
        )
            .into_response();
    }

    let Some(strategy) = lightarchitects::agent::loops::StrategyRegistry::lookup(&strategy_id)
    else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "strategy not in registry", "strategy_id": strategy_id })),
        )
            .into_response();
    };

    tracing::info!(
        request_id = %body.request_id,
        strategy_id = %strategy_id,
        choice = body.choice,
        "hitl_resolve: resuming strategy from operator choice"
    );

    let mut resumed_state = loop_state;
    resumed_state
        .meta
        .insert("hitl_choice".to_owned(), body.choice.to_string());

    spawn_hitl_continuation(
        strategy,
        resumed_state,
        body.session_id.clone(),
        strategy_id.clone(),
        state.event_tx.clone(),
        std::sync::Arc::clone(&state.resume_registry),
    );

    (
        StatusCode::OK,
        Json(json!({
            "strategy_id": strategy_id,
            "choice": body.choice,
            "status": "resuming"
        })),
    )
        .into_response()
}

/// Spawns the mpsc-bridge and continuation tasks for a resumed HITL strategy.
fn spawn_hitl_continuation(
    strategy: lightarchitects::agent::loops::RegisteredStrategy,
    resumed_state: lightarchitects::agent::loops::LoopState,
    session_id: String,
    strategy_id: String,
    event_tx: tokio::sync::broadcast::Sender<crate::events::WebEventV2>,
    registry: std::sync::Arc<crate::copilot::strategy_runner::ResumeRegistry>,
) {
    use crate::copilot::strategy_runner::{DispatchResult, StrategyDispatcher};

    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<String>(64);
    let event_tx_bridge = event_tx.clone();
    tokio::spawn(async move {
        while let Some(msg) = progress_rx.recv().await {
            let _ = event_tx_bridge.send(crate::events::WebEventV2::from_event(
                crate::events::WebEvent::CopilotResponse {
                    chunk: msg,
                    done: false,
                    sibling: None,
                    turn_span_id: None,
                },
                None,
            ));
        }
    });

    let sid_label = strategy_id.clone();
    tokio::spawn(async move {
        let dispatcher = StrategyDispatcher::new(registry);
        let result = dispatcher
            .dispatch(strategy, resumed_state, session_id, progress_tx)
            .await;
        let summary = match &result {
            DispatchResult::Halted { phases_run } => {
                format!("[{sid_label}] complete ({phases_run} phases)")
            }
            DispatchResult::Error(e) => format!("[{sid_label}] error: {e}"),
            DispatchResult::Paused { hitl, .. } => {
                format!("[{sid_label}] paused: {}", hitl.question)
            }
        };
        let _ = event_tx.send(crate::events::WebEventV2::from_event(
            crate::events::WebEvent::CopilotResponse {
                chunk: summary,
                done: true,
                sibling: None,
                turn_span_id: None,
            },
            None,
        ));
    });
}
