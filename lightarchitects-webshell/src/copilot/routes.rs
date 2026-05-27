//! HTTP route handler for `POST /api/builds/:id/copilot`.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::lightsquad_tool::{LightsquadToolExecutor, build_lightsquad_executor};
use axum::body::Bytes;
use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt as _;
use lightarchitects::agent::{
    ChainContext, ClaudeCliProvider, OllamaCliProvider,
    conversation::{
        ConversationEvent, ConversationSession, SessionConfig, SseTransport, Transport,
        helix_memory::HelixSessionMemory,
    },
};
use serde_json::json;
use tokio_util::io::ReaderStream;
use tracing::Instrument as _;
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
    // Peek the session_span_id from the prior turn so grounding spans can be
    // parented to the session root.  On the first turn the field is None and
    // grounding spans remain orphaned — acceptable since the root doesn't exist yet.
    let grounding_parent_id: Option<String> = {
        let guard = session.copilot_proc.lock().await;
        guard.as_ref().and_then(|p| p.session_span_id.clone())
    };
    // Clone before grounding_parent_id is moved into gather_grounding so that
    // message spans (user + assistant) can be parented to the same session root.
    let session_parent_id = grounding_parent_id.clone();
    emit_message_span(
        &state,
        "user",
        &body.message,
        session_parent_id.as_deref(),
        Some(id),
    );

    let (prelude, soul_block, git_ctx) = gather_grounding(
        &state,
        id,
        &identity_text,
        &body.message,
        &body.recent_events,
        body.ui_context.as_ref(),
        Some(id),
        grounding_parent_id,
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

    // Native path: stream via ConversationSession + SseTransport.
    // The full grounding prelude (EVA identity + SOUL + git + recent events) is
    // placed in SessionConfig.system_prompt rather than inline in the user message,
    // removing the previous 8 KiB wedge limitation.  The model's 131 072-token
    // context window handles multi-KB system prompts without issue.
    if matches!(session.agent, AgentSession::LightarchitectsNative(_)) {
        let system = if prelude.is_empty() {
            None
        } else {
            Some(prelude)
        };
        return drive_native_sse(
            id,
            &body.message,
            session.cwd.clone(),
            grounding_hdrs,
            system,
            Arc::clone(&session.native_interrupt_flag),
            state.la_native_api_key.clone(),
            state.config.token.clone(),
            build_lightsquad_executor(&state),
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
            ClaudeBackend::Anthropic | ClaudeBackend::OllamaLaunch(_),
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
            emit_message_span(
                &state,
                "assistant",
                &text,
                session_parent_id.as_deref(),
                Some(id),
            );
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

/// Stream a single turn via [`ConversationSession`] + [`SseTransport`].
///
/// Creates a `tokio::io::duplex` pipe: one end feeds [`SseTransport`] (written by the
/// spawned task), the other becomes the HTTP response body. The caller receives raw
/// SSE frames (`event: …\ndata: …\n\n`) as they are emitted by the strategy engine.
/// Characters-per-token estimate for context-window overflow warning (W4.3).
/// Rough proxy: English text averages ~4 bytes/token for sub-word tokenisers.
const CHARS_PER_TOKEN: usize = 4;
/// Emit a `tracing::warn` when the system prelude exceeds this fraction of the
/// model context window (`num_ctx = 131_072` tokens, warn at 50%).
const SYSTEM_PROMPT_WARN_CHARS: usize = 131_072 * CHARS_PER_TOKEN / 2; // ~262 144

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
async fn native_turn_task(
    cwd: std::path::PathBuf,
    system_prompt: Option<String>,
    write_half: tokio::io::DuplexStream,
    msg: String,
    ollama_provider: Option<OllamaCliProvider>,
    interrupt_flag: Arc<AtomicBool>,
    tool_executor: Arc<LightsquadToolExecutor>,
) {
    let memory = HelixSessionMemory::open(&cwd, 40);
    let restored = memory.restored_turn_count();
    tracing::debug!(restored_turns = restored, "helix session memory loaded");

    let config = SessionConfig {
        cwd,
        system_prompt,
        ..SessionConfig::default()
    };
    let mut transport = SseTransport::new(write_half);
    let ctx = ChainContext::default();
    let result = if let Some(provider) = ollama_provider {
        let mut session = ConversationSession::new(config, Arc::new(provider))
            .with_memory(Box::new(memory))
            .with_interrupt_flag(Arc::clone(&interrupt_flag))
            .with_tool_executor(tool_executor);
        session.run_turn(&msg, &mut transport, &ctx).await
    } else {
        let mut session = ConversationSession::new(config, Arc::new(ClaudeCliProvider::default()))
            .with_memory(Box::new(memory))
            .with_interrupt_flag(Arc::clone(&interrupt_flag))
            .with_tool_executor(tool_executor);
        session.run_turn(&msg, &mut transport, &ctx).await
    };
    if let Err(e) = result {
        tracing::error!(error = %e, "run_turn failed");
        let _ = transport
            .emit(&ConversationEvent::Error {
                message: e.to_string(),
                recoverable: Some(false),
            })
            .await;
    } else {
        tracing::info!("run_turn completed");
    }
    // transport drop closes write_half → EOF on read_half
}

// All 8 arguments are distinct, named, and load-bearing for the single call
// site; bundling them into a struct would obscure the wiring without any
// material benefit.  Phase-10 GAP-3 added la_native_api_key + session_token
// so the per-chunk redact wrapper can scrub both secrets — both are
// per-request data, not part of any natural sub-struct.
#[allow(clippy::too_many_arguments)]
fn drive_native_sse(
    build_id: Uuid,
    grounded_message: &str,
    cwd: std::path::PathBuf,
    extra_headers: HeaderMap,
    system_prompt: Option<String>,
    interrupt_flag: Arc<AtomicBool>,
    la_native_api_key: Option<secrecy::SecretString>,
    session_token: String,
    tool_executor: Arc<LightsquadToolExecutor>,
) -> Response {
    // Reset any prior interrupt before starting a new turn so the flag does
    // not carry over from a previous cancelled request.
    interrupt_flag.store(false, Ordering::SeqCst);
    let (write_half, read_half) = tokio::io::duplex(64 * 1024);
    let msg = grounded_message.to_owned();

    if let Some(ref sp) = system_prompt {
        if sp.len() > SYSTEM_PROMPT_WARN_CHARS {
            tracing::warn!(
                system_prompt_bytes = sp.len(),
                warn_threshold_bytes = SYSTEM_PROMPT_WARN_CHARS,
                "system prelude exceeds 50% of model context window — consider trimming"
            );
        }
    }

    // Provider selection: prefer Ollama Cloud when the AppState-resolved auth
    // token is present (read once at startup via `AppState::new` — no
    // per-request `std::env::var` read, closing the TOCTOU window per Phase-10
    // hardening), otherwise fall back to ClaudeCliProvider for legacy
    // compatibility. ConversationSession is generic over a concrete provider
    // type, so the branches construct independent sessions rather than sharing
    // a trait object.
    let use_ollama = la_native_api_key.is_some();
    let model = std::env::var("LA_MODEL")
        .ok()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "nemotron-3-super:cloud".to_owned());
    let ollama_provider = if use_ollama {
        match OllamaCliProvider::new(&model, la_native_api_key.clone()) {
            Ok(p) => Some(p),
            Err(e) => {
                // W8.2: surface provider-construction failure so the operator
                // can see why we fell back to ClaudeCliProvider rather than
                // silently degrading without explanation.
                tracing::warn!(
                    error = %e,
                    model = %model,
                    "OllamaCliProvider construction failed — falling back to ClaudeCliProvider"
                );
                None
            }
        }
    } else {
        None
    };

    let provider_name = if ollama_provider.is_some() {
        "ollama-cli"
    } else {
        "claude-cli"
    };

    // Span carries build_id so every tracing event inside run_turn is correlated
    // in AYIN's dashboard under the same trace root (W8.4).
    let span = tracing::info_span!(
        "native_turn",
        build_id = %build_id,
        provider = provider_name,
        model = %model,
    );
    tracing::info!(parent: &span, "drive_native_sse spawning turn");

    let handle = tokio::spawn(
        native_turn_task(
            cwd,
            system_prompt,
            write_half,
            msg,
            ollama_provider,
            interrupt_flag,
            tool_executor,
        )
        .instrument(span),
    );

    // W7.3: AbortOnDrop (module-level struct) lives inside the stream's map
    // closure. When the response Body is dropped on client disconnect, the
    // closure drops, firing abort_handle.abort() on the in-flight task.
    //
    // Phase-10 (GAP-3): every chunk is passed through `redact_secrets()`
    // against the session bearer token and (when present) the
    // OLLAMA_API_KEY before the bytes leave the process. This closes the
    // native-SSE bypass of `sse_handler::redact()` documented in the
    // webshell-la-native-backend merge gate.
    //
    // Performance: each chunk is UTF-8 decoded with `String::from_utf8_lossy`
    // and re-encoded only when a secret is present in the chunk (the helper
    // returns the input unchanged otherwise — branch-free hot path).
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
                    // Hot path: no secret found, return original bytes verbatim.
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
            // Dropping `stream` here also drops `abort_guard`, cancelling the task.
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

    // SOUL future: top-5 BM25, 400 ms hard timeout.
    let soul_fut = async {
        let soul_t0 = std::time::Instant::now();
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

    // Parallel execution — wall-clock is max of the two timeouts, not sum.
    let (
        (soul_block, soul_result_count, soul_timed_out, soul_ms),
        (git_ctx, git_timed_out, git_ms),
    ) = tokio::join!(soul_fut, git_fut);
    let grounding_wall_ms = u64::try_from(wall_t0.elapsed().as_millis()).unwrap_or(u64::MAX);

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
fn emit_disk_span(
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
fn emit_message_span(
    state: &AppState,
    kind: &str,
    content: &str,
    parent_id: Option<&str>,
    build_id: Option<Uuid>,
) {
    let _ = state.event_tx.send(WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(TraceSpanSummary {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: parent_id.map(ToOwned::to_owned),
            actor: kind.to_owned(),
            action: format!("copilot.message.{kind}"),
            timestamp: chrono::Utc::now().to_rfc3339(),
            duration_ms: 0,
            outcome: serde_json::json!("ok"),
            metadata: serde_json::json!({
                "kind": kind,
                "preview": &content[..content.len().min(200)],
            }),
            strand_activations: Vec::new(),
            decision_points: Vec::new(),
        }),
        build_id,
    ));
    emit_disk_span(
        kind,
        &format!("copilot.message.{kind}"),
        json!({
            "kind": kind,
            "preview": &content[..content.len().min(200)],
        }),
        lightarchitects::ayin::TraceOutcome::Continue,
        parent_id.and_then(|s| s.parse::<Uuid>().ok()),
        build_id,
    );
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
    use crate::copilot::strategy_runner::ResumeRegistry;

    let registry: &ResumeRegistry = &state.resume_registry;

    match registry.take(&body.request_id, &body.session_id) {
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "unknown, expired, or already-consumed request_id"
            })),
        )
            .into_response(),
        Some((loop_state, strategy_id, options_count)) => {
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
            tracing::info!(
                request_id = %body.request_id,
                strategy_id = %strategy_id,
                choice = body.choice,
                "hitl_resolve: operator resolved strategy pause"
            );
            (
                StatusCode::OK,
                Json(json!({
                    "strategy_id": strategy_id,
                    "loop_state_phase": loop_state.phase,
                    "choice": body.choice,
                    "status": "accepted"
                })),
            )
                .into_response()
        }
    }
}
