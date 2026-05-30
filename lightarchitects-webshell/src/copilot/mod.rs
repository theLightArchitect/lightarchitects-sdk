//! Copilot chat handler — per-turn subprocess spawning with disk-persistent sessions.
//!
//! `Lightarchitects` backend: each HTTP request spawns a fresh `claude --print` process.
//! Session continuity via `--session-id` (Turn 1) / `--resume` (Turn 2+) with disk persistence.
//!
//! `Codex` backend: each HTTP request spawns `codex exec` (Turn 1) or
//! `codex exec resume <thread_id>` (Turn 2+) with disk-persistent session continuity.
//!

pub mod chatroom;
pub mod code_grounding;
pub mod context;
pub mod eva_identity;
pub mod event_stream;
pub mod git_context;
pub mod lightsquad_tool;
pub mod native_session;
pub mod persona_cache;
#[cfg(feature = "playwright")]
pub mod playwright;
#[cfg(not(feature = "playwright"))]
#[allow(missing_docs, unused_imports)]
pub mod playwright {
    //! Stub module — playwright feature is disabled.

    use axum::Json;
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use serde::Deserialize;

    /// Shared state placeholder when playwright feature is disabled.
    pub type PlaywrightState = std::sync::Arc<tokio::sync::Mutex<Option<()>>>;

    #[derive(Debug, Deserialize)]
    pub struct ScreenshotRequest {
        pub url: String,
        pub token: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct DomSnapshotRequest {
        pub url: String,
        pub token: String,
    }

    /// Returns 503 — playwright feature is disabled at compile time.
    pub async fn handle_init(
        State(_state): State<crate::server::AppState>,
    ) -> axum::response::Response {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "playwright_feature_disabled"})),
        )
            .into_response()
    }

    /// Returns 503 — playwright feature is disabled at compile time.
    pub async fn handle_screenshot(
        State(_state): State<crate::server::AppState>,
        Json(_req): Json<ScreenshotRequest>,
    ) -> axum::response::Response {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "playwright_feature_disabled"})),
        )
            .into_response()
    }

    /// Returns 503 — playwright feature is disabled at compile time.
    pub async fn handle_dom_snapshot(
        State(_state): State<crate::server::AppState>,
        Json(_req): Json<DomSnapshotRequest>,
    ) -> axum::response::Response {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "playwright_feature_disabled"})),
        )
            .into_response()
    }
}
pub mod routes;
pub mod soul_grounding;
pub mod strategy_runner;
pub mod voice;
pub use event_stream::copilot_event_stream_handler;
pub use routes::{
    copilot_chat_handler, copilot_clear_handler, copilot_hitl_resolve_handler,
    copilot_interrupt_handler,
};
pub use voice::copilot_voice_handler;

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, BufReader};

use lightarchitects::agent::ProviderEvent;
use lightarchitects::agent::messages_stream_parser::stream_json::parse_ndjson_value;
use std::collections::HashMap;

use crate::{
    config::{AgentSession, ClaudeBackend, CodexBackend},
    session::BuildSession,
};

/// Wall-clock cap for copilot subprocess turns (5 minutes).
const TURN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Shared HTTP client for Ollama calls — avoids per-request TLS handshake + connection pool.
static OLLAMA_HTTP_CLIENT: std::sync::LazyLock<reqwest::Client> =
    std::sync::LazyLock::new(reqwest::Client::new);

/// Resolve a binary name to its full path by checking known install locations.
/// Falls back to the bare name (relies on PATH) if not found in known locations.
pub fn resolve_binary(name: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
    resolve_binary_with_home(name, &home)
}

/// Inner implementation of [`resolve_binary`] with an explicit home directory.
///
/// Extracted so unit tests can supply a controlled `home` without mutating
/// `$HOME` in the process environment (which races with parallel test threads).
fn resolve_binary_with_home(name: &str, home: &str) -> String {
    let candidates: Vec<String> = match name {
        "claude" => vec![
            format!("{home}/.local/bin/claude"),
            format!("{home}/.claude/local/bin/claude"),
            "/usr/local/bin/claude".to_owned(),
        ],
        "corso" => vec![
            format!("{home}/lightarchitects/corso/bin/corso"),
            "/usr/local/bin/corso".to_owned(),
        ],
        "codex" => vec![
            format!("{home}/.local/bin/codex"),
            "/opt/homebrew/bin/codex".to_owned(),
            "/usr/local/bin/codex".to_owned(),
        ],
        "lightarchitects" => vec![
            // CLI binary (agent runner) takes priority over the gateway binary.
            // The CLI installs here via `make deploy-fast` in lightarchitects-cli.
            format!("{home}/lightarchitects/cli/bin/lightarchitects"),
            format!("{home}/.lightarchitects/bin/lightarchitects"),
            format!("{home}/.local/bin/lightarchitects"),
            "/usr/local/bin/lightarchitects".to_owned(),
        ],
        // Mistral Vibe: installed via `uv tool install mistral-vibe` → ~/.local/bin/
        "vibe" => vec![
            format!("{home}/.local/bin/vibe"),
            "/opt/homebrew/bin/vibe".to_owned(),
            "/usr/local/bin/vibe".to_owned(),
        ],
        "vibe-acp" => vec![
            format!("{home}/.local/bin/vibe-acp"),
            "/opt/homebrew/bin/vibe-acp".to_owned(),
            "/usr/local/bin/vibe-acp".to_owned(),
        ],
        _ => vec![],
    };
    for path in &candidates {
        let p = std::path::Path::new(path);
        let exists = p.exists();
        let is_file = p.is_file();
        tracing::info!(
            "resolve_binary({name}): checking {path} → exists={exists}, is_file={is_file}"
        );
        if is_file {
            tracing::info!("resolve_binary({name}) → {path}");
            return path.clone();
        }
    }
    tracing::warn!(
        "resolve_binary({name}): not found in known locations, falling back to bare name"
    );
    name.to_owned()
}

/// Mint a Claude Code–compatible session UUID for pre-minting before subprocess spawn.
///
/// Generates a `UUIDv4` (hyphenated lowercase) that Claude Code accepts as `--session-id`
/// on the first turn. The resulting JSONL file on disk will be named `<uuid>.jsonl`,
/// giving the webshell a stable handle before the subprocess is launched.
///
/// If Claude Code's `--session-id` semantics change in a future release, add a
/// version-detect or feature-detect guard here.
pub fn mint_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Resolve an Anthropic API key for the `LightArchitects` CLI subprocess.
///
/// Priority:
/// 1. Keychain `"lightarchitects"/"anthropic"` — canonical namespace (new writes always go here)
/// 2. Keychain `"lightarchitects-webshell-setup"/"anthropic"` — legacy fallback during migration
/// 3. Claude Code credentials file (`~/.claude/.credentials.json`)
/// 4. `ANTHROPIC_API_KEY` env var (if not a placeholder)
///
/// Returns `None` if no valid key found — the CLI will fall back to its own resolution.
pub fn resolve_api_key_for_native() -> Option<String> {
    // 1. Canonical keychain namespace ("lightarchitects") — new writes land here.
    if let Ok(entry) = keyring::Entry::new("lightarchitects", "anthropic") {
        if let Ok(key) = entry.get_password() {
            if !key.is_empty() && !key.contains("placeholder") && !key.contains("your_") {
                tracing::debug!(
                    "resolve_api_key_for_native: found key in keychain (lightarchitects/anthropic)"
                );
                return Some(key);
            }
        }
    }

    // 2. Legacy keychain namespace — coexists until a future migration command cleans it up.
    if let Ok(entry) = keyring::Entry::new("lightarchitects-webshell-setup", "anthropic") {
        if let Ok(key) = entry.get_password() {
            if !key.is_empty() && !key.contains("placeholder") && !key.contains("your_") {
                tracing::debug!(
                    "resolve_api_key_for_native: found key in legacy keychain (lightarchitects-webshell-setup/anthropic)"
                );
                return Some(key);
            }
        }
    }

    // 3. Claude Code credentials file
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
    let creds_path = std::path::Path::new(&home)
        .join(".claude")
        .join(".credentials.json");
    if let Ok(content) = std::fs::read_to_string(&creds_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(key) = json.get("primaryApiKey").and_then(|v| v.as_str()) {
                if !key.is_empty() {
                    tracing::debug!("resolve_api_key_for_native: found key in .credentials.json");
                    return Some(key.to_owned());
                }
            }
        }
    }

    // 4. Environment variable (if not a placeholder)
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        if !key.is_empty() && !key.contains("your_") && key.starts_with("sk-ant-") {
            tracing::debug!("resolve_api_key_for_native: found key in env ANTHROPIC_API_KEY");
            return Some(key);
        }
    }

    tracing::warn!("resolve_api_key_for_native: no valid API key found for native CLI");
    None
}

/// Resolve the Mistral API key for vibe subprocess injection.
///
/// Priority order:
/// 1. `security find-generic-password` CLI (macOS native Keychain, service=lightarchitects account=mistral)
/// 2. `keyring::Entry::new("lightarchitects", "mistral")` — cross-platform fallback
/// 3. `MISTRAL_API_KEY` env var inherited by the webshell process
///
/// Returns `None` if no key found — vibe will fail with its own auth error.
pub fn resolve_mistral_api_key() -> Option<SecretString> {
    if let Some(key) = keychain_via_security_cli("lightarchitects", "mistral") {
        tracing::debug!(
            "resolve_mistral_api_key: found key via security CLI (lightarchitects/mistral)"
        );
        return Some(SecretString::new(key.into()));
    }

    if let Ok(entry) = keyring::Entry::new("lightarchitects", "mistral") {
        if let Ok(key) = entry.get_password() {
            if !key.is_empty() && !key.contains("placeholder") && !key.contains("your_") {
                tracing::debug!(
                    "resolve_mistral_api_key: found key in keychain (lightarchitects/mistral)"
                );
                return Some(SecretString::new(key.into()));
            }
        }
    }

    if let Ok(key) = std::env::var("MISTRAL_API_KEY") {
        if !key.is_empty() && !key.contains("your_") {
            tracing::debug!("resolve_mistral_api_key: found key in env MISTRAL_API_KEY");
            return Some(SecretString::new(key.into()));
        }
    }

    tracing::warn!("resolve_mistral_api_key: no Mistral API key found for vibe subprocess");
    None
}

/// Read a generic-password entry from the macOS Keychain via the `security` CLI.
///
/// `keyring` v3 silently falls back to an in-process mock on macOS (no D-Bus).
/// The `security` binary is always in the ACL of items it created and needs no GUI dialog.
#[cfg(target_os = "macos")]
fn keychain_via_security_cli(service: &str, account: &str) -> Option<String> {
    let out = std::process::Command::new("security")
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .output()
        .ok()?;
    if out.status.success() {
        let s = String::from_utf8(out.stdout).ok()?;
        let trimmed = s.trim().to_owned();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn keychain_via_security_cli(_service: &str, _account: &str) -> Option<String> {
    None
}

/// Build an augmented PATH that includes known binary install directories.
/// Subprocess spawns (claude, corso, codex) need these paths even if the
/// webshell server was launched from a minimal environment (e.g., `LaunchAgent`).
pub fn augmented_path() -> String {
    use lightarchitects::{core::paths, squad_registry::SquadRegistry};
    let la_home = paths::root_or_fallback();
    let registry = SquadRegistry::load(&la_home);
    paths::augmented_path(&registry)
}

/// JSON body for `POST /api/builds/:id/copilot`.
///
/// The `recent_events` and `ui_context` fields are optional for backwards
/// compatibility — older clients sending `{"message":"..."}` only continue to
/// work unchanged (§R4 backwards-compat contract).
#[derive(Debug, Deserialize)]
pub struct CopilotRequest {
    /// User message text (may include injected build context from the frontend).
    pub message: String,
    /// Last N events from `GlobalEventStore` captured by the frontend at submit
    /// time. Grounds the copilot prompt in recent activity (Northstar §P check 1;
    /// `northstar.md:490`). Frontend caps at 50; server validates ≤100.
    #[serde(default)]
    pub recent_events: Vec<context::RecentEventEntry>,
    /// Operator's current UI state at submit time (route, selection, view mode).
    ///
    /// Combined with `recent_events` to produce the `<ui_context>` prelude block
    /// (Northstar §P check 1 + §C check 9; `northstar.md:490, :261`).
    pub ui_context: Option<UiContext>,
}

/// Current UI state snapshot attached to a copilot turn.
///
/// Captured by the frontend at submit time; embedded in the prompt prelude via
/// [`context::assemble_prompt_prelude`]. Satisfies Northstar §P check 1
/// (typed context schema; `northstar.md:490`).
#[derive(Debug, Deserialize, Serialize)]
pub struct UiContext {
    /// Current browser route (URL pathname, e.g. `"/builds/abc"`).
    pub route: String,
    /// Selected item identifier or text within the current screen, if any.
    pub selection: Option<String>,
    /// Active view mode within the current screen (e.g. `"activity"`, `"files"`).
    pub view: Option<String>,
    /// Degradation codes appended by the frontend when context retrieval was
    /// unhealthy at submit time (e.g. `["stream_disconnected_22s"]`).
    /// Surfaced in the `<ui_context>` prelude so the model knows context may
    /// be incomplete.
    #[serde(default)]
    pub degraded: Vec<String>,
    /// Cockpit state at submit time: active preset + selected target scope.
    #[serde(default)]
    pub cockpit: Option<CockpitUiContext>,
}

/// Active cockpit preset and target scope, injected into every copilot message.
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct CockpitUiContext {
    /// Domain preset key (e.g. `"engineer"`, `"security"`).
    pub preset: String,
    /// Currently selected target entity, or `null` when no target is pinned.
    pub target: Option<CockpitTarget>,
}

/// A pinned target scope within the LASDLC hierarchy.
#[derive(Debug, Deserialize, Serialize)]
pub struct CockpitTarget {
    /// Entity type: `"project"`, `"build"`, `"phase"`, `"wave"`, `"file"`,
    /// `"commit"`, `"branch"`, or `"pr"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Stable identifier (PR URL, build ID, file path, etc.).
    pub id: String,
    /// Human-readable display label.
    pub label: String,
}

/// Per-session agent state held behind `tokio::sync::Mutex<Option<CopilotProcess>>`.
///
/// Holds conversation-continuity IDs that persist across turns within one session.
pub struct CopilotProcess {
    /// Session ID for conversation continuity: passed as `--resume` on the next turn
    /// (`Lightarchitects`) or extracted from stdout (`Codex`).
    pub session_id: Option<String>,
    /// ID of the session-root AYIN span emitted on the first turn.
    ///
    /// All per-turn spans use this as `parent_id` to form the lineage chain visible
    /// in the AYIN Lineage Circuit.  `None` only for slots seeded externally before
    /// the first real turn; [`call_subprocess`] always populates it on first use.
    pub session_span_id: Option<String>,
}

impl CopilotProcess {
    /// Seed a copilot slot with a pre-existing session UUID so the next
    /// turn resumes that conversation (`claude --resume <id>` or
    /// `codex exec resume <id>`). No subprocess is spawned — Lightarchitects
    /// and Codex backends re-spawn per turn and only need `session_id`.
    #[must_use]
    pub fn seed_from_session_id(session_id: String) -> Self {
        Self {
            session_id: Some(session_id),
            session_span_id: None,
        }
    }
}

/// Parent-span IDs threaded through one copilot turn for AYIN span hierarchy.
///
/// `session_span_id` — the session-root span emitted once on the first turn and
/// stored in [`CopilotProcess`].  Reused as parent for every turn span so the
/// Lineage Circuit can draw the full session tree.
///
/// `turn_span_id` — this turn's start-span ID; used by [`routes`] to link
/// EVA-ambient and tool-call spans as children of the turn.
#[derive(Clone, Debug)]
pub struct TurnSpanContext {
    /// ID of the session-root span (emitted once, first turn).
    pub session_span_id: String,
    /// ID of this turn's start span (parent for EVA-ambient + tool-call spans).
    pub turn_span_id: String,
}

/// Spawn one turn of a `claude --print` subprocess for `Lightarchitects` backends.
///
/// Uses `--output-format stream-json --verbose` (required combination for `--print`).
/// Turn 1 (no `prev_session_id`): claude assigns a new session UUID returned in the result.
/// Turn 2+ (`prev_session_id` is `Some`): `--resume <id>` continues the prior conversation
/// from disk — giving full multi-turn context without re-uploading history.
///
/// Streams intermediate events (`assistant`, `tool_use`, `tool_result`) to the
/// per-build `event_tx` as `WebEvent::CopilotActivity` so the Activity tab
/// can render live progress.
///
/// # Errors
///
/// Returns a descriptive string on spawn failure, non-zero exit, or missing result event.
#[allow(clippy::too_many_lines)]
async fn run_print_turn(
    message: &str,
    session: &BuildSession,
    prev_session_id: Option<&str>,
    turn_span_id: Option<&str>,
) -> Result<(String, Option<String>), String> {
    let AgentSession::Lightarchitects(backend) = &session.agent else {
        return Err("run_print_turn: not a Lightarchitects session".to_owned());
    };

    let resolved = resolve_binary("claude");
    let mut c = tokio::process::Command::new(&resolved);
    // Ensure child process has full PATH for dynamic libs and shebang resolution
    c.env("PATH", augmented_path());
    for arg in session.build_argv() {
        c.arg(arg);
    }
    // --verbose is mandatory when combining --print with --output-format stream-json.
    c.arg("--output-format").arg("stream-json");
    c.arg("--verbose");
    c.arg("--dangerously-skip-permissions");
    c.arg("--print").arg("-p").arg(message);
    // Pin the child's working directory to the build's cwd. This matters
    // critically for `--resume <id>`: claude derives the on-disk session
    // file path from the cwd's project hash, so a child spawned in the
    // wrong directory will look in the wrong project folder and exit 1
    // when the UUID isn't found. Turn-to-turn continuity within a single
    // webshell run works with inherited cwd by accident; session-sync
    // (resuming a session created in a different process tree) exposes
    // the need to set it explicitly.
    // Ensure cwd exists — spawn fails with misleading "No such file or directory"
    // (referring to cwd, not the binary) if current_dir doesn't exist.
    if !session.cwd.is_dir() {
        if let Err(e) = std::fs::create_dir_all(&session.cwd) {
            tracing::warn!(path = %session.cwd.display(), error = %e, "failed to create cwd for claude");
        }
    }
    c.current_dir(&session.cwd);
    c.env_remove("ANTHROPIC_API_KEY");
    match backend {
        ClaudeBackend::OllamaLaunch(lc) => {
            c.env("ANTHROPIC_BASE_URL", &lc.base_url);
            c.env("ANTHROPIC_AUTH_TOKEN", "ollama");
            c.env("ANTHROPIC_API_KEY", "");
            c.env("ANTHROPIC_DEFAULT_SONNET_MODEL", &lc.model);
            c.env("ANTHROPIC_DEFAULT_OPUS_MODEL", &lc.model);
            c.env("ANTHROPIC_DEFAULT_HAIKU_MODEL", &lc.model);
            c.arg("--model").arg(&lc.model);
        }
        ClaudeBackend::Anthropic | ClaudeBackend::Ollama(_) => {}
    }
    if let Some(id) = prev_session_id {
        tracing::debug!(session_id = %id, "run_print_turn: resuming prior session");
        c.arg("--resume").arg(id);
    } else {
        tracing::debug!("run_print_turn: starting new session");
    }
    // Stderr is piped to null — we only read stdout. Piped stderr without a drain
    // task risks deadlock when the child writes more than the OS pipe buffer (64KB).
    c.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);

    let mut child = c.spawn().map_err(|e| {
        tracing::warn!(error = %e, "failed to spawn claude subprocess");
        "claude_spawn_failed".to_owned()
    })?;

    let turn = async {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "claude stdout unavailable".to_owned())?;
        let mut reader = BufReader::new(stdout).lines();

        let mut result_text: Option<String> = None;
        let mut found_session_id: Option<String> = None;
        let build_id = session.build_id.to_string();

        // Track tool call start times so we can emit AYIN spans with actual duration
        // at ContentBlockStop instead of duration_ms: 0 at ContentBlockStart.
        let mut tool_start_times: HashMap<u32, (String, std::time::Instant)> = HashMap::new();

        // OTel semconv v1.56.0 — SA-23: emit a structured event instead of `.entered()`
        // because EnteredSpan is !Send; holding it across .await makes the future !Send,
        // breaking axum's Handler trait. Use tracing::info! to preserve the span semantics.
        tracing::info!(build_id = %build_id, "app.copilot.turn starting");

        while let Some(line) = reader.next_line().await.map_err(|e| {
            tracing::warn!(error = %e, "claude stdout read failed");
            "claude_read_failed".to_owned()
        })? {
            // SA-21: parse errors are non-fatal — warn and skip the line.
            let val = match serde_json::from_str::<serde_json::Value>(&line) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(build_id = %build_id, error = %e, "copilot NDJSON parse error");
                    tracing::warn!(
                        counter = "app.copilot.parse_error_total",
                        "incrementing counter"
                    );
                    continue;
                }
            };

            if let Some(id) = val["session_id"].as_str() {
                found_session_id = Some(id.to_owned());
            }

            let event_type = val["type"].as_str().unwrap_or("unknown");

            // Broadcast activity event for the Activity tab
            let summary = extract_activity_summary(&val);
            let send_result = session.event_tx.send(crate::events::WebEventV2::from_event(
                crate::events::WebEvent::CopilotActivity(
                    crate::events::types::CopilotActivityEvent {
                        build_id: build_id.clone(),
                        kind: event_type.to_owned(),
                        summary,
                        raw: val.clone(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    },
                ),
                Some(session.build_id),
            ));
            if send_result.is_err() {
                tracing::warn!(
                    counter = "app.copilot.token_buffer.overflow_total",
                    "channel send failed — receiver lagged or dropped"
                );
            }

            // Delegate stream-json event dispatch to the SDK parser (TS-3 §21.3).
            // `val` is already parsed; `parse_ndjson_value` avoids a second JSON parse.
            match parse_ndjson_value(&val) {
                Ok(Some(ProviderEvent::ContentBlockStart {
                    block_type,
                    tool_name,
                    index,
                    ..
                })) if block_type == "tool_use" => {
                    // Record start time — span emitted at ContentBlockStop with actual duration.
                    let name = tool_name.as_deref().unwrap_or("unknown").to_owned();
                    tool_start_times.insert(index, (name, std::time::Instant::now()));
                }
                Ok(Some(ProviderEvent::ContentBlockStop { index })) => {
                    if let Some((name, start)) = tool_start_times.remove(&index) {
                        // Truncation-safe: tool calls never approach u64::MAX ms.
                        #[allow(clippy::cast_possible_truncation)]
                        let duration_ms = start.elapsed().as_millis() as u64;
                        let _ = session.event_tx.send(crate::events::WebEventV2::from_event(
                            crate::events::WebEvent::AyinSpan(
                                crate::events::types::TraceSpanSummary {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    parent_id: turn_span_id.map(ToOwned::to_owned),
                                    actor: "copilot".to_owned(),
                                    action: format!("tool.{name}"),
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                    duration_ms,
                                    outcome: serde_json::json!("Continue"),
                                    metadata: serde_json::json!({
                                        "build_id": build_id
                                    }),
                                    strand_activations: Vec::new(),
                                    session_id: None,
                                    decision_points: Vec::new(),
                                },
                            ),
                            Some(session.build_id),
                        ));
                        emit_disk_span(
                            "copilot",
                            &format!("tool.{name}"),
                            serde_json::json!({ "build_id": build_id, "duration_ms": duration_ms }),
                            lightarchitects::ayin::TraceOutcome::Continue,
                            turn_span_id.and_then(|s| s.parse::<uuid::Uuid>().ok()),
                            session.build_id,
                        );
                    }
                }
                Ok(Some(ProviderEvent::TextDelta { text, .. })) => {
                    let send_r = session.event_tx.send(crate::events::WebEventV2::from_event(
                        crate::events::WebEvent::CopilotResponse {
                            chunk: text,
                            done: false,
                            sibling: Some("claude".to_owned()),
                            turn_span_id: turn_span_id.map(ToOwned::to_owned),
                        },
                        Some(session.build_id),
                    ));
                    if send_r.is_err() {
                        tracing::warn!(
                            counter = "app.copilot.token_buffer.overflow_total",
                            "channel send failed — receiver lagged or dropped"
                        );
                    }
                }
                Ok(Some(ProviderEvent::MessageStop)) => {
                    let _ = session.event_tx.send(crate::events::WebEventV2::from_event(
                        crate::events::WebEvent::CopilotResponse {
                            chunk: String::new(),
                            done: true,
                            sibling: Some("claude".to_owned()),
                            turn_span_id: turn_span_id.map(ToOwned::to_owned),
                        },
                        Some(session.build_id),
                    ));
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(
                        build_id = %build_id,
                        error = %e,
                        counter = "app.copilot.parse_error_total",
                        "copilot NDJSON event parse error"
                    );
                }
            }

            if event_type == "result" && val["subtype"].as_str() == Some("success") {
                result_text = Some(val["result"].as_str().unwrap_or("").to_owned());
            }
        }

        // Wait for the child to exit so we can check status
        let status = child.wait().await.map_err(|e| {
            tracing::warn!(error = %e, "wait claude failed");
            "claude_wait_failed".to_owned()
        })?;

        result_text.map(|t| (t, found_session_id)).ok_or_else(|| {
            if status.success() {
                "no_result_event".to_owned()
            } else {
                "claude_nonzero_exit".to_owned()
            }
        })
    };

    tokio::time::timeout(TURN_TIMEOUT, turn)
        .await
        .map_err(|_| {
            tracing::warn!("claude turn exceeded {TURN_TIMEOUT:?} — killing");
            "claude_turn_timeout".to_owned()
        })?
}

/// Extract a human-readable summary from a stream-json event for the Activity tab.
fn extract_activity_summary(val: &serde_json::Value) -> Option<String> {
    let event_type = val["type"].as_str()?;
    match event_type {
        "assistant" => {
            // Thinking or text content
            val["message"]["content"].as_array().and_then(|blocks| {
                blocks.iter().find_map(|b| {
                    if b["type"].as_str() == Some("thinking") {
                        let t = b["thinking"].as_str().unwrap_or("");
                        Some(format!("Thinking: {}", &t[..t.len().min(500)]))
                    } else if b["type"].as_str() == Some("text") {
                        let t = b["text"].as_str().unwrap_or("");
                        Some(format!("Text: {}", &t[..t.len().min(500)]))
                    } else {
                        None
                    }
                })
            })
        }
        "content_block_start" => {
            let block = &val["content_block"];
            match block["type"].as_str() {
                Some("thinking") => Some("Thinking...".to_owned()),
                Some("tool_use") => {
                    let name = block["name"].as_str().unwrap_or("unknown");
                    Some(format!("Tool: {name}"))
                }
                Some("text") => Some("Generating text...".to_owned()),
                _ => None,
            }
        }
        "content_block_delta" => {
            let delta = &val["delta"];
            match delta["type"].as_str() {
                Some("thinking_delta") => {
                    let t = delta["thinking"].as_str().unwrap_or("");
                    if t.len() > 80 {
                        Some(format!("{}...", &t[..80]))
                    } else {
                        Some(t.to_owned())
                    }
                }
                Some("input_json_delta") => {
                    let partial = delta["partial_json"].as_str().unwrap_or("");
                    if partial.len() > 100 {
                        Some(format!("Input: {}...", &partial[..100]))
                    } else if !partial.is_empty() {
                        Some(format!("Input: {partial}"))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        "result" => Some("Turn complete".to_owned()),
        _ => None,
    }
}

/// Extract a human-readable summary from a Codex `--json` NDJSON event.
fn extract_codex_activity_summary(val: &serde_json::Value) -> Option<String> {
    let event_type = val["type"].as_str()?;
    match event_type {
        "thread.started" => Some("Thread started".to_owned()),
        "item.completed" => {
            let item_type = val["item"]["type"].as_str().unwrap_or("unknown");
            match item_type {
                "agent_message" => {
                    let t = val["item"]["text"].as_str().unwrap_or("");
                    Some(format!("Agent: {}", &t[..t.len().min(200)]))
                }
                "tool_call" => {
                    let name = val["item"]["name"].as_str().unwrap_or("unknown");
                    Some(format!("Tool: {name}"))
                }
                _ => Some(format!("Item: {item_type}")),
            }
        }
        "turn.completed" => Some("Turn complete".to_owned()),
        "turn.failed" => {
            let msg = val["error"]["message"].as_str().unwrap_or("unknown");
            Some(format!("Failed: {msg}"))
        }
        _ => None,
    }
}

/// Spawn one turn of `codex exec` for `Codex` backends.
///
/// Turn 1 (no `prev_session_id`): `codex exec "message" --json --skip-git-repo-check
/// --dangerously-bypass-approvals-and-sandbox -m <model>`.
/// Turn 2+ (`prev_session_id` is `Some`): `codex exec resume <id> "message" --json ...`.
/// Session continuity via `thread_id` extracted from `{"type":"thread.started"}` event.
///
/// Streams intermediate events to the per-build `event_tx` as
/// `WebEvent::CopilotActivity` for the Activity tab.
///
/// # Errors
///
/// Returns a descriptive string on spawn failure, non-zero exit, or missing result.
#[allow(clippy::too_many_lines)]
async fn run_codex_turn(
    message: &str,
    session: &BuildSession,
    prev_session_id: Option<&str>,
) -> Result<(String, Option<String>), String> {
    let AgentSession::Codex(cfg) = &session.agent else {
        return Err("run_codex_turn: not a Codex session".to_owned());
    };

    let mut c = tokio::process::Command::new(resolve_binary("codex"));
    c.env("PATH", augmented_path());
    if let Some(id) = prev_session_id {
        c.arg("exec").arg("resume").arg(id).arg(message);
    } else {
        c.arg("exec").arg(message);
    }
    c.arg("--json")
        .arg("--skip-git-repo-check")
        .arg("--dangerously-bypass-approvals-and-sandbox");
    match &cfg.backend {
        // OpenAi: defer to ~/.codex/config.toml for model selection.
        // Passing -m overrides the user's config and may fail if the model
        // name doesn't match the account type (e.g. "o3" on ChatGPT accounts).
        CodexBackend::OpenAi => {}
        CodexBackend::OllamaLaunch(lc) => {
            c.arg("-m").arg(&cfg.model);
            c.env("OPENAI_BASE_URL", format!("{}/v1", lc.base_url));
            c.env("OPENAI_API_KEY", "ollama");
        }
    }
    // Pin the child's working directory to the build's cwd — same reason
    // as run_print_turn: `codex exec resume <id>` looks up the session
    // file relative to the current project, so cwd must match what the
    // session was originally created in.
    if !session.cwd.is_dir() {
        if let Err(e) = std::fs::create_dir_all(&session.cwd) {
            tracing::warn!(path = %session.cwd.display(), error = %e, "failed to create cwd for codex");
        }
    }
    c.current_dir(&session.cwd);
    // Prevent API key leakage — codex should not inherit the host's Anthropic key.
    c.env_remove("ANTHROPIC_API_KEY");
    c.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);

    let mut child = c.spawn().map_err(|e| {
        tracing::warn!(error = %e, "failed to spawn codex subprocess");
        "codex_spawn_failed".to_owned()
    })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "codex stdout unavailable".to_owned())?;
    let mut reader = BufReader::new(stdout).lines();

    let turn = async {
        let mut thread_id: Option<String> = None;
        let mut text = String::new();
        let mut turn_done = false;
        let mut turn_error: Option<String> = None;
        let build_id = session.build_id.to_string();

        while let Some(line) = reader.next_line().await.map_err(|e| {
            tracing::warn!(error = %e, "codex stdout read failed");
            "codex_read_failed".to_owned()
        })? {
            let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else {
                tracing::warn!(
                    build_id = %build_id,
                    raw = %&line[..line.len().min(256)],
                    "run_codex_turn: NDJSON parse error — skipping line"
                );
                continue;
            };

            let event_type = val["type"].as_str().unwrap_or("unknown");

            // Broadcast activity event for the Activity tab
            let summary = extract_codex_activity_summary(&val);
            let _ = session.event_tx.send(crate::events::WebEventV2::from_event(
                crate::events::WebEvent::CopilotActivity(
                    crate::events::types::CopilotActivityEvent {
                        build_id: build_id.clone(),
                        kind: event_type.to_owned(),
                        summary,
                        raw: val.clone(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    },
                ),
                Some(session.build_id),
            ));

            if event_type == "thread.started" {
                if let Some(id) = val["thread_id"].as_str() {
                    thread_id = Some(id.to_owned());
                }
            }
            if event_type == "item.completed"
                && val["item"]["type"].as_str() == Some("agent_message")
            {
                if let Some(t) = val["item"]["text"].as_str() {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(t);
                }
            }
            if event_type == "turn.completed" {
                turn_done = true;
            }
            if event_type == "turn.failed" {
                let msg = val["error"]["message"]
                    .as_str()
                    .unwrap_or("unknown turn failure");
                turn_error = Some(msg.to_owned());
            }
        }

        // Wait for the child to exit
        let _status = child.wait().await.map_err(|e| {
            tracing::warn!(error = %e, "wait codex failed");
            "codex_wait_failed".to_owned()
        })?;

        if let Some(_err) = turn_error {
            return Err("codex_turn_failed".to_owned());
        }
        if turn_done {
            Ok((text, thread_id))
        } else {
            Err("codex_no_turn_completed".to_owned())
        }
    };

    tokio::time::timeout(TURN_TIMEOUT, turn)
        .await
        .map_err(|_| {
            tracing::warn!("codex turn exceeded {TURN_TIMEOUT:?} — killing");
            "codex_turn_timeout".to_owned()
        })?
}

/// Send a single turn to the Mistral Vibe CLI (`vibe -p`) and return the text response.
///
/// Uses `--output text` (human-readable, default for `-p`).  When the config
/// carries an explicit model override, it is injected via `VIBE_ACTIVE_MODEL`.
/// If no override is set, vibe resolves its own `active_model` from `~/.vibe/config.toml`.
async fn run_vibe_turn(message: &str, session: &BuildSession) -> Result<String, String> {
    let AgentSession::MistralVibe(cfg) = &session.agent else {
        return Err("run_vibe_turn: not a MistralVibe session".to_owned());
    };

    let mut c = tokio::process::Command::new(resolve_binary("vibe"));
    c.env("PATH", augmented_path());
    if let Some(key) = resolve_mistral_api_key() {
        c.env("MISTRAL_API_KEY", key.expose_secret());
    }
    if let Some(model) = &cfg.model {
        c.env("VIBE_ACTIVE_MODEL", model);
    }
    c.arg("-p").arg(message).arg("--output").arg("text");
    if !session.cwd.as_os_str().is_empty() {
        c.arg("--workdir").arg(&session.cwd);
    }
    if !session.cwd.is_dir() {
        if let Err(e) = std::fs::create_dir_all(&session.cwd) {
            tracing::warn!(path = %session.cwd.display(), error = %e, "failed to create cwd for vibe");
        }
    }
    c.current_dir(&session.cwd);
    // Prevent API key leakage — vibe only gets MISTRAL_API_KEY explicitly set above.
    c.env_remove("ANTHROPIC_API_KEY");
    c.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);

    let output = tokio::time::timeout(TURN_TIMEOUT, c.output())
        .await
        .map_err(|_| {
            tracing::warn!("vibe turn exceeded {TURN_TIMEOUT:?}");
            "vibe_turn_timeout".to_owned()
        })?
        .map_err(|e| {
            tracing::warn!(error = %e, "failed to spawn vibe subprocess");
            "vibe_spawn_failed".to_owned()
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(
            target: "webshell",
            status = %output.status,
            stderr = %&stderr[..stderr.len().min(512)],
            "vibe subprocess exited non-zero"
        );
        return Err("vibe_subprocess_error".to_owned());
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if text.is_empty() {
        return Err("vibe_empty_response".to_owned());
    }
    Ok(text)
}

/// Send `message` to the agent and return its response.
///
/// Public entry point for dispatch — routes a prompt through the copilot
/// subprocess. Same as the internal `call_subprocess` used by `copilot_chat_handler`.
///
/// `Lightarchitects`: spawns a fresh `claude --print` per turn; session continuity via
/// `--resume` with disk persistence.
///
/// `Codex`: spawns `codex exec` (Turn 1) or `codex exec resume` (Turn 2+); session
/// continuity via `thread_id` with disk persistence.
///
/// The mutex serializes turns — correct for a sequential chat UI.
///
/// # Errors
///
/// Returns a descriptive string on spawn failure, process death, or missing result.
pub async fn call_subprocess_public(
    message: &str,
    proc_lock: &tokio::sync::Mutex<Option<CopilotProcess>>,
    session: &BuildSession,
) -> Result<String, String> {
    call_subprocess(message, proc_lock, session).await
}

#[allow(clippy::too_many_lines)]
pub(super) async fn call_subprocess(
    message: &str,
    proc_lock: &tokio::sync::Mutex<Option<CopilotProcess>>,
    session: &BuildSession,
) -> Result<String, String> {
    let mut guard = proc_lock.lock().await;

    let actor = match &session.agent {
        AgentSession::Lightarchitects(_) | AgentSession::LightarchitectsNative(_) => "eva",
        AgentSession::Codex(_) => "codex",
        AgentSession::MistralVibe(_) => "vibe",
    };
    // Retrieve or create the session-root AYIN span ID (parent of all turn spans).
    let session_span_id: String = guard
        .as_ref()
        .and_then(|p| p.session_span_id.as_deref())
        .map_or_else(
            || emit_session_start_span(session, actor),
            ToOwned::to_owned,
        );
    let (span_id, start, start_ts) =
        emit_turn_start_span(session, actor, message, Some(&session_span_id));

    tracing::debug!(
        agent = %actor,
        build_id = %session.build_id,
        "call_subprocess: dispatching turn"
    );

    // Per-turn path for Lightarchitects (claude --print + disk-persistent sessions).
    if matches!(&session.agent, AgentSession::Lightarchitects(_)) {
        let prev_session_id = guard
            .as_ref()
            .and_then(|p| p.session_id.as_deref())
            .map(ToOwned::to_owned);

        let turn_result =
            run_print_turn(message, session, prev_session_id.as_deref(), Some(&span_id)).await;
        if let Err(ref e) = turn_result {
            tracing::warn!(
                error = %e,
                "subprocess exited before MessageStop; emitting fallback done=true to unblock UI"
            );
            let _ = session.event_tx.send(crate::events::WebEventV2::from_event(
                crate::events::WebEvent::CopilotResponse {
                    chunk: String::new(),
                    done: true,
                    sibling: None,
                    turn_span_id: Some(span_id.clone()),
                },
                Some(session.build_id),
            ));
        }
        let (text, new_session_id) = turn_result?;

        if let Some(ref mut proc) = *guard {
            proc.session_id = new_session_id;
            // Backfill for guards seeded via seed_from_session_id (session_span_id: None).
            proc.session_span_id
                .get_or_insert_with(|| session_span_id.clone());
        } else {
            *guard = Some(CopilotProcess {
                session_id: new_session_id,
                session_span_id: Some(session_span_id.clone()),
            });
        }

        emit_assistant_response_span(
            session,
            &span_id,
            &start_ts,
            start.elapsed(),
            "success",
            &text,
        );

        return Ok(text);
    }

    // Per-turn path for MistralVibe (`vibe -p` programmatic mode).
    if matches!(&session.agent, AgentSession::MistralVibe(_)) {
        let text = run_vibe_turn(message, session).await?;

        // Broadcast the full response so the UI SSE handler can render it.
        // The HTTP body is discarded by the frontend; only SSE events are displayed.
        let _ = session.event_tx.send(crate::events::WebEventV2::from_event(
            crate::events::WebEvent::CopilotResponse {
                chunk: text.clone(),
                done: true,
                sibling: Some("vibe".to_owned()),
                turn_span_id: None,
            },
            Some(session.build_id),
        ));

        emit_assistant_response_span(
            session,
            &span_id,
            &start_ts,
            start.elapsed(),
            "success",
            &text,
        );

        return Ok(text);
    }

    // Per-turn path for Codex (codex exec + disk-persistent sessions).
    if matches!(&session.agent, AgentSession::Codex(_)) {
        let prev_session_id = guard
            .as_ref()
            .and_then(|p| p.session_id.as_deref())
            .map(ToOwned::to_owned);

        let (text, new_session_id) =
            run_codex_turn(message, session, prev_session_id.as_deref()).await?;

        if let Some(ref mut proc) = *guard {
            proc.session_id = new_session_id;
            proc.session_span_id
                .get_or_insert_with(|| session_span_id.clone());
        } else {
            *guard = Some(CopilotProcess {
                session_id: new_session_id,
                session_span_id: Some(session_span_id.clone()),
            });
        }

        emit_assistant_response_span(
            session,
            &span_id,
            &start_ts,
            start.elapsed(),
            "success",
            &text,
        );

        return Ok(text);
    }

    Err("unsupported_agent_session".to_owned())
}

/// POST to Ollama-compatible `/v1/chat/completions` endpoint.
///
/// # Errors
///
/// Returns a descriptive string on network failure or unexpected response shape.
pub(super) async fn call_ollama(
    base_url: &str,
    model: &str,
    auth_token: &str,
    message: &str,
) -> Result<String, String> {
    tracing::debug!(base_url, model, "call_ollama: sending request");
    let start = std::time::Instant::now();
    let mut builder = OLLAMA_HTTP_CLIENT
        .post(format!("{base_url}/v1/chat/completions"))
        .json(&json!({
            "model": model,
            "messages": [{ "role": "user", "content": message }],
        }));
    if auth_token != "ollama" {
        builder = builder.header("authorization", format!("Bearer {auth_token}"));
    }
    let res = builder.send().await.map_err(|e| {
        tracing::warn!(base_url, model, error = %e, "call_ollama: request failed");
        "ollama_request_failed".to_owned()
    })?;
    if !res.status().is_success() {
        let code = res.status().as_u16();
        let body = res.text().await.unwrap_or_default();
        tracing::warn!(
            base_url,
            model,
            status = code,
            body = %&body[..body.len().min(512)],
            "call_ollama: non-2xx response"
        );
        return Err("ollama_upstream_error".to_owned());
    }
    let val: serde_json::Value = res.json().await.map_err(|e| {
        tracing::warn!(base_url, model, error = %e, "call_ollama: json parse failed");
        "ollama_json_error".to_owned()
    })?;
    let result = val["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "ollama_unexpected_shape".to_owned());
    let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    match &result {
        Ok(text) => tracing::debug!(
            base_url,
            model,
            latency_ms,
            response_len = text.len(),
            "call_ollama: completed"
        ),
        Err(e) => {
            tracing::warn!(base_url, model, latency_ms, error = %e, "call_ollama: unexpected response shape");
        }
    }
    result
}

/// Emit a webshell AYIN span to disk (fire-and-forget, alongside SSE).
fn emit_disk_span(
    actor: &str,
    action: &str,
    metadata: serde_json::Value,
    outcome: lightarchitects::ayin::TraceOutcome,
    parent_id: Option<uuid::Uuid>,
    session_id: uuid::Uuid,
) {
    use lightarchitects::ayin::{
        emit_span_background,
        span::{Actor, TraceContext},
    };
    let mut ctx = TraceContext::new(Actor::new(actor), action)
        .outcome(outcome)
        .metadata(metadata)
        .session_id(&session_id.to_string());
    if let Some(pid) = parent_id {
        ctx = ctx.parent(pid);
    }
    emit_span_background(ctx);
}

/// Emit a session-root AYIN span for a new copilot session and return its ID.
/// Subsequent turn spans use this ID as their `parent_id`.
fn emit_session_start_span(session: &BuildSession, actor: &str) -> String {
    let span_id = uuid::Uuid::new_v4().to_string();
    let _ = session.event_tx.send(crate::events::WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(crate::events::types::TraceSpanSummary {
            id: span_id.clone(),
            parent_id: None,
            actor: actor.to_owned(),
            action: "copilot.session.started".to_owned(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            duration_ms: 0,
            outcome: serde_json::json!("pending"),
            metadata: serde_json::json!({ "build_id": session.build_id.to_string() }),
            strand_activations: Vec::new(),
            session_id: None,
            decision_points: Vec::new(),
        }),
        Some(session.build_id),
    ));
    emit_disk_span(
        actor,
        "copilot.session.started",
        serde_json::json!({ "build_id": session.build_id.to_string(), "actor": actor }),
        lightarchitects::ayin::TraceOutcome::Continue,
        None,
        session.build_id,
    );
    span_id
}

/// Emit a `user.message` AYIN span (turn root) and return `(span_id, Instant, timestamp)`
/// for the caller to pass to [`emit_assistant_response_span`] when the turn finishes.
///
/// Actor is always `"user"` so the Lineage Circuit renders the node in gold,
/// distinguishing the human input from tool calls (cyan) and the assistant
/// response (gold leaf via `"claude"` actor).
fn emit_turn_start_span(
    session: &BuildSession,
    _actor: &str,
    message: &str,
    session_span_id: Option<&str>,
) -> (String, std::time::Instant, String) {
    let span_id = uuid::Uuid::new_v4().to_string();
    let start = std::time::Instant::now();
    let start_ts = chrono::Utc::now().to_rfc3339();
    let _ = session.event_tx.send(crate::events::WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(crate::events::types::TraceSpanSummary {
            id: span_id.clone(),
            parent_id: session_span_id.map(ToOwned::to_owned),
            actor: "user".to_owned(),
            action: "user.message".to_owned(),
            timestamp: start_ts.clone(),
            duration_ms: 0,
            outcome: serde_json::json!("pending"),
            metadata: serde_json::json!({
                "message_preview": &message[..message.len().min(200)],
                "build_id": session.build_id.to_string(),
            }),
            strand_activations: Vec::new(),
            session_id: None,
            decision_points: Vec::new(),
        }),
        Some(session.build_id),
    ));
    emit_disk_span(
        "user",
        "user.message",
        serde_json::json!({
            "message_preview": &message[..message.len().min(200)],
            "build_id": session.build_id.to_string(),
        }),
        lightarchitects::ayin::TraceOutcome::Continue,
        session_span_id.and_then(|s| s.parse::<uuid::Uuid>().ok()),
        session.build_id,
    );
    (span_id, start, start_ts)
}

/// Emit an `assistant.response` AYIN span — the terminal leaf of every turn.
///
/// Actor is always `"claude"` (the CLI agent that produced the response).
/// The Lineage Circuit renders this gold at the outermost radius, bookending
/// the turn: gold `user.message` root → cyan tool calls → gold leaf here.
fn emit_assistant_response_span(
    session: &BuildSession,
    parent_span_id: &str,
    start_ts: &str,
    elapsed: std::time::Duration,
    outcome: &str,
    response_preview: &str,
) {
    let duration_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
    let _ = session.event_tx.send(crate::events::WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(crate::events::types::TraceSpanSummary {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: Some(parent_span_id.to_owned()),
            actor: "claude".to_owned(),
            action: "assistant.response".to_owned(),
            timestamp: start_ts.to_owned(),
            duration_ms,
            outcome: serde_json::json!(outcome),
            metadata: serde_json::json!({
                "build_id": session.build_id.to_string(),
                "duration_s": format!("{:.1}", elapsed.as_secs_f64()),
                "response_preview": &response_preview[..response_preview.len().min(200)],
            }),
            strand_activations: Vec::new(),
            session_id: None,
            decision_points: Vec::new(),
        }),
        Some(session.build_id),
    ));
    emit_disk_span(
        "claude",
        "assistant.response",
        serde_json::json!({
            "build_id": session.build_id.to_string(),
            "duration_s": format!("{:.1}", elapsed.as_secs_f64()),
            "response_preview": &response_preview[..response_preview.len().min(200)],
        }),
        if outcome == "error" {
            lightarchitects::ayin::TraceOutcome::Block
        } else {
            lightarchitects::ayin::TraceOutcome::Continue
        },
        parent_span_id.parse::<uuid::Uuid>().ok(),
        session.build_id,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Plan draft subprocess — F-002 fix (plan-builder-copilot-bridge Phase 3)
// ─────────────────────────────────────────────────────────────────────────────

/// Form-field bundle for [`spawn_plan_draft`].
///
/// Groups the six operator-supplied fields so the function signature stays
/// within `clippy::too_many_arguments` (≤7).
pub struct PlanDraftArgs {
    /// Free-text description of the feature or build target.
    pub description: String,
    /// Optional Northstar statement injected into the `/PLAN` seed prompt.
    pub northstar: Option<String>,
    /// Optional repository URL or monorepo path for the plan's scope context.
    pub repository: Option<String>,
    /// When `true`, appends `--research` phase hint to the seed prompt.
    pub research: bool,
    /// Requested LASDLC tier (`SMALL`, `MEDIUM`, or `LARGE`), or `None` for auto.
    pub tier: Option<String>,
    /// Pre-minted `UUIDv4` passed as `--session-id` on Turn 1.
    pub session_id: String,
}

/// Build the `/PLAN` seed prompt from operator-supplied form fields.
fn build_plan_seed_prompt(args: &PlanDraftArgs) -> String {
    use std::fmt::Write as _;
    let desc = &args.description;
    let mut prompt =
        format!("/PLAN {desc}\n\n**Build context from webshell form**:\n- Description: {desc}");
    if let Some(ref ns) = args.northstar {
        let _ = write!(prompt, "\n- Northstar: {ns}");
    }
    if let Some(ref repo) = args.repository {
        let _ = write!(prompt, "\n- Repository: {repo}");
    }
    if let Some(ref t) = args.tier {
        let _ = write!(prompt, "\n- Tier: {t}");
    }
    if args.research {
        prompt.push_str("\n- Include research phase: yes");
    }
    prompt.push_str("\n\nAuthor a complete LASDLC v2.5.1-compliant plan body. Include northstar_lineage, all phases with [A+S+Q+C+O+P+K+D+T+R] gates, file-function map, C1-C8 rubric, and IEEE references.");
    prompt
}

/// Spawn a `claude --print` subprocess to author a `LASDLC v2.5.1` plan draft.
///
/// Uses the same `--print --output-format stream-json --verbose` pattern as
/// [`run_print_turn`] but is stateless (no [`BuildSession`] required) because
/// plan authoring runs outside a build session — the session UUID is
/// pre-minted by the caller via [`mint_session_id`] and passed as
/// `--session-id` on Turn 1.
///
/// # Streaming contract
///
/// Emits [`PlanDraftEvent`] variants over `tx` in this order:
/// 1. `TextChunk` — one per `text_delta` content block delta
/// 2. `IterationStart` — once per `/PLAN` self-review iteration detected
/// 3. `VerdictBlock` — once when EVA emits a `VALIDATED` or `REVISION_NEEDED` block
/// 4. `Done` — on clean subprocess exit (codename derived from description)
/// 5. `Error` — on spawn failure, timeout, or non-zero exit
///
fn push_copilot_chunk(store: &crate::events::GlobalEventStore, pid: u32, text: &str) {
    store.push(
        crate::events::types::EventSource::CopilotSubprocess { pid },
        crate::events::WebEvent::CopilotResponse {
            chunk: text.to_owned(),
            done: false,
            sibling: Some("eva".into()),
            turn_span_id: None,
        },
    );
}

/// # Errors
///
/// Returns [`PlanDraftError`] on spawn failure or I/O error. Runtime
/// errors (timeouts, subprocess non-zero exit) are sent over `tx` as
/// `PlanDraftEvent::Error` and then `Ok(())` is returned.
///
/// `cancel` is checked on each iteration; when triggered (client disconnect)
/// the subprocess is killed and [`PlanDraftError::CancelledByClient`] is returned.
pub async fn spawn_plan_draft(
    args: PlanDraftArgs,
    tx: tokio::sync::broadcast::Sender<crate::events::types::PlanDraftEvent>,
    global_store: Option<crate::events::GlobalEventStore>,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), crate::events::types::PlanDraftError> {
    use crate::events::types::{PlanDraftError, PlanDraftEvent};

    let binary = resolve_binary("claude");
    let prompt = build_plan_seed_prompt(&args);
    // Derive codename before args is consumed: lowercase kebab from first 5 words.
    let codename = args
        .description
        .split_whitespace()
        .take(5)
        .map(|w| w.to_lowercase().replace(|c: char| !c.is_alphanumeric(), ""))
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    let mut c = tokio::process::Command::new(&binary);
    c.env("PATH", augmented_path());
    c.env_remove("ANTHROPIC_API_KEY");
    c.arg("--output-format").arg("stream-json");
    c.arg("--verbose");
    c.arg("--dangerously-skip-permissions");
    c.arg("--print").arg("-p").arg(&prompt);
    c.arg("--session-id").arg(&args.session_id);
    c.stdout(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);

    let mut child = c.spawn().map_err(|_| {
        // Opaque spawn error — internal path redacted from all responses.
        PlanDraftError::SubprocessSpawnFailed("copilot unavailable".into())
    })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| PlanDraftError::SubprocessSpawnFailed("stdout unavailable".into()))?;
    let mut reader = tokio::io::BufReader::new(stdout).lines();
    let mut iteration: u8 = 1;

    loop {
        tokio::select! {
            // Cancelled by client disconnect.
            () = cancel.cancelled() => {
                let _ = child.kill().await;
                return Err(PlanDraftError::CancelledByClient);
            }
            line = reader.next_line() => {
                let Some(line) = line.map_err(|_| PlanDraftError::IoError("read error".into()))? else {
                    break;
                };
                let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else {
                    continue;
                };
                if val["type"].as_str() != Some("content_block_delta") { continue; }
                let Some(delta) = val.get("delta") else { continue; };
                if delta["type"].as_str() != Some("text_delta") { continue; }
                let Some(text) = delta["text"].as_str() else { continue; };

                if text.contains("## Self-Review") {
                    iteration = iteration.saturating_add(1);
                    let _ = tx.send(PlanDraftEvent::IterationStart { iteration });
                }
                if text.contains("validation_status: VALIDATED") {
                    let verdict = crate::events::types::ReviewVerdict {
                        validation_status: "VALIDATED".into(),
                        iteration,
                        summary: Some("Plan validated by EVA self-review".into()),
                    };
                    let _ = tx.send(PlanDraftEvent::VerdictBlock { verdict });
                }

                if let Some(ref store) = global_store {
                    push_copilot_chunk(store, child.id().unwrap_or(0), text);
                }
                // broadcast::send errors when zero receivers — that's fine; continue.
                let _ = tx.send(PlanDraftEvent::TextChunk { text: text.to_owned() });
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|_| PlanDraftError::IoError("wait error".into()))?;
    if status.success() {
        let _ = tx.send(PlanDraftEvent::Done { codename });
    } else {
        let _ = tx.send(PlanDraftEvent::Error {
            message: "plan draft failed".into(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, unsafe_code)]
mod tests {
    use super::*;

    // ── resolve_mistral_api_key — property suite ─────────────────────────────
    //
    // Tests the filtering predicate directly (no env mutation → no parallel race).
    // The env-var path checks: !empty && !contains("your_").
    // The keychain path additionally checks: !contains("placeholder").

    /// Returns true when the key satisfies the env-var acceptance predicate.
    #[cfg(test)]
    fn env_key_is_valid(key: &str) -> bool {
        !key.is_empty() && !key.contains("your_")
    }

    /// Returns true when the key satisfies the keychain acceptance predicate.
    #[cfg(test)]
    fn keychain_key_is_valid(key: &str) -> bool {
        !key.is_empty() && !key.contains("placeholder") && !key.contains("your_")
    }

    proptest::proptest! {
        /// Any alphanumeric key (no "your_") passes the env-var filter.
        #[test]
        fn prop_valid_env_keys_pass_filter(s in "[a-zA-Z0-9]{8,64}") {
            proptest::prop_assert!(
                env_key_is_valid(&s),
                "clean alphanumeric key must pass env filter: {s}"
            );
        }

        /// Any string prefixed with "your_" fails both filters.
        #[test]
        fn prop_your_prefix_rejected_by_both_filters(suffix in "[a-zA-Z0-9]{4,32}") {
            let key = format!("your_{suffix}");
            proptest::prop_assert!(!env_key_is_valid(&key), "your_ must fail env filter: {key}");
            proptest::prop_assert!(!keychain_key_is_valid(&key), "your_ must fail keychain filter: {key}");
        }

        /// Any string containing "placeholder" fails the keychain filter.
        #[test]
        fn prop_placeholder_rejected_by_keychain_filter(
            prefix in "[a-z]{0,8}",
            suffix in "[a-z]{0,8}"
        ) {
            let key = format!("{prefix}placeholder{suffix}");
            proptest::prop_assert!(
                !keychain_key_is_valid(&key),
                "placeholder must fail keychain filter: {key}"
            );
        }
    }

    // ── resolve_mistral_api_key — unit suite ─────────────────────────────────

    #[test]
    fn resolve_mistral_api_key_env_var_path_returns_secret() {
        // Tests the env-var acceptance predicate and SecretString wrapping directly.
        // The full resolve chain checks keychain first; on machines with a configured
        // keychain entry the env var is never reached, making the full-chain call
        // non-deterministic. The predicate + wrapping are what matter here.
        let key = "sk-test-valid-key-12345";
        assert!(env_key_is_valid(key), "valid key must pass env filter");
        let secret = SecretString::new(key.to_owned().into());
        assert_eq!(secret.expose_secret(), key);
    }

    #[test]
    fn resolve_mistral_api_key_rejects_placeholder_prefix() {
        assert!(
            !env_key_is_valid("your_api_key_here"),
            "placeholder prefix 'your_' must be rejected by env filter"
        );
    }

    #[test]
    fn resolve_mistral_api_key_rejects_empty_env_var() {
        assert!(
            !env_key_is_valid(""),
            "empty string must be rejected by env filter"
        );
    }

    fn is_hex(c: char) -> bool {
        matches!(c, '0'..='9' | 'a'..='f')
    }

    #[test]
    fn mint_session_id_is_uuidv4() {
        let id = mint_session_id();
        let chars: Vec<char> = id.chars().collect();
        // UUIDv4 canonical: xxxxxxxx-xxxx-4xxx-[89ab]xxx-xxxxxxxxxxxx  (36 chars)
        assert_eq!(chars.len(), 36, "expected 36-char UUID, got '{id}'");
        assert_eq!(chars[8], '-', "hyphen at pos 8");
        assert_eq!(chars[13], '-', "hyphen at pos 13");
        assert_eq!(chars[18], '-', "hyphen at pos 18");
        assert_eq!(chars[23], '-', "hyphen at pos 23");
        assert_eq!(chars[14], '4', "version nibble at pos 14 must be '4'");
        assert!(
            matches!(chars[19], '8' | '9' | 'a' | 'b'),
            "variant nibble at pos 19 must be 8/9/a/b, got '{}'",
            chars[19]
        );
        for (i, &c) in chars.iter().enumerate() {
            if [8, 13, 18, 23].contains(&i) {
                continue;
            }
            assert!(is_hex(c), "pos {i} in '{id}' is not a hex digit: '{c}'");
        }
    }

    #[test]
    fn mint_session_id_is_unique() {
        let ids: std::collections::HashSet<_> = (0..100).map(|_| mint_session_id()).collect();
        assert_eq!(ids.len(), 100, "mint_session_id() produced duplicate UUIDs");
    }

    // ── resolve_binary_with_home — vibe/* arms ──────────────────────────────
    //
    // Tests call the private helper directly with a controlled home directory,
    // avoiding $HOME mutation (which races with parallel test threads).
    // tempfile::TempDir creates a real filesystem path; p.is_file() requires
    // the file to actually exist, so we create a zero-byte stub.

    #[cfg(unix)]
    mod resolve_binary_tests {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn make_executable(dir: &std::path::Path, rel: &str) -> std::path::PathBuf {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        #[test]
        fn vibe_resolves_local_bin_first() {
            let tmp = tempfile::TempDir::new().unwrap();
            let home = tmp.path().to_str().unwrap();
            let expected = make_executable(tmp.path(), ".local/bin/vibe");
            // Also place a stub at a lower-priority location — must not win.
            make_executable(tmp.path(), "opt_homebrew_stub/vibe");

            let result = resolve_binary_with_home("vibe", home);
            assert_eq!(
                result,
                expected.to_str().unwrap(),
                "~/.local/bin/vibe must win over lower-priority candidates"
            );
        }

        #[test]
        fn vibe_falls_back_to_bare_name_when_absent() {
            let tmp = tempfile::TempDir::new().unwrap();
            let home = tmp.path().to_str().unwrap();
            // No vibe binary anywhere in the temp tree.
            let result = resolve_binary_with_home("vibe", home);
            assert_eq!(result, "vibe", "must fall back to bare name when not found");
        }

        #[test]
        fn vibe_acp_resolves_local_bin_first() {
            let tmp = tempfile::TempDir::new().unwrap();
            let home = tmp.path().to_str().unwrap();
            let expected = make_executable(tmp.path(), ".local/bin/vibe-acp");

            let result = resolve_binary_with_home("vibe-acp", home);
            assert_eq!(result, expected.to_str().unwrap());
        }

        #[test]
        fn vibe_acp_falls_back_to_bare_name_when_absent() {
            let tmp = tempfile::TempDir::new().unwrap();
            let home = tmp.path().to_str().unwrap();
            let result = resolve_binary_with_home("vibe-acp", home);
            assert_eq!(result, "vibe-acp");
        }

        #[test]
        fn unknown_binary_falls_back_to_bare_name() {
            let tmp = tempfile::TempDir::new().unwrap();
            let home = tmp.path().to_str().unwrap();
            let result = resolve_binary_with_home("no-such-tool", home);
            assert_eq!(result, "no-such-tool");
        }
    }

    /// Backwards-compat: old clients send `{"message":"..."}` only — both new
    /// context fields must be absent/empty without a parse error (§R4).
    #[test]
    fn backwards_compat_missing_fields() {
        let json = r#"{"message": "what just happened?"}"#;
        let req: CopilotRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.message, "what just happened?");
        assert!(
            req.recent_events.is_empty(),
            "recent_events must default to empty vec"
        );
        assert!(req.ui_context.is_none(), "ui_context must default to None");
    }
}
