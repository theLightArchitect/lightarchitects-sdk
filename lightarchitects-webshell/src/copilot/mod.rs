//! Copilot chat handler — per-turn subprocess spawning with disk-persistent sessions.
//!
//! `Lightarchitects` backend: each HTTP request spawns a fresh `claude --print` process.
//! Session continuity via `--session-id` (Turn 1) / `--resume` (Turn 2+) with disk persistence.
//!
//! `Codex` backend: each HTTP request spawns `codex exec` (Turn 1) or
//! `codex exec resume <thread_id>` (Turn 2+) with disk-persistent session continuity.
//!
//! `LightarchitectsNative` backend: persistent subprocess with piped I/O.

pub mod context;
pub mod eva_identity;
pub mod git_context;
pub mod routes;
pub mod soul_grounding;
pub mod voice;
pub use routes::copilot_chat_handler;
pub use voice::copilot_voice_handler;

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter, Lines},
    process::{Child, ChildStdin, ChildStdout},
};

use crate::{
    config::{AgentSession, ClaudeBackend, CodexBackend},
    events::WebEventV2,
    session::BuildSession,
};

/// Parsed `content_block_delta` inner delta from Claude's `stream-json` output.
///
/// Uses `#[serde(rename = "type")]` because `delta.type` is a Rust reserved
/// keyword — never access as `delta.type` (SA-20).
#[derive(Debug, Deserialize)]
struct ContentBlockDelta {
    /// The delta type — `"text_delta"`, `"thinking_delta"`, `"input_json_delta"`, etc.
    #[serde(rename = "type")]
    delta_type: String,
    /// Present when `delta_type == "text_delta"`.
    #[serde(default)]
    text: String,
}

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
        "lightarchitects-cli" => vec![
            format!("{home}/.local/bin/lightarchitects-cli"),
            format!("{home}/.lightarchitects/cli/bin/lightarchitects-cli"),
            "/usr/local/bin/lightarchitects-cli".to_owned(),
        ],
        "lightarchitects" => vec![
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
/// **`Lightarchitects`**, **`Codex`**: only `session_id` is populated; stdin/stdout/child are `None`.
/// Per-turn processes are short-lived and not stored here.
///
/// **`LightarchitectsNative`**: all fields populated; child is killed on drop via
/// `kill_on_drop(true)` (RAII cleanup).
pub struct CopilotProcess {
    /// Session ID for conversation continuity: passed as `--resume` on the next turn
    /// (`Lightarchitects`) or extracted from stdout (`Codex`/`LightarchitectsNative`).
    pub session_id: Option<String>,
    /// Persistent stdin (`Codex`, `LightarchitectsNative` only).
    stdin: Option<BufWriter<ChildStdin>>,
    /// Persistent stdout reader (`Codex`, `LightarchitectsNative` only).
    stdout: Option<Lines<BufReader<ChildStdout>>>,
    /// Subprocess handle — `kill_on_drop(true)` sends SIGKILL on drop.
    _child: Option<Child>,
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
            stdin: None,
            stdout: None,
            _child: None,
        }
    }
}

/// Dispatch a single NDJSON event line from the CLI stream to the webshell event bus.
///
/// Returns `Some(text)` when `line` is the `{"type":"result","text":"..."}` event.
/// Returns `None` for any other event type (caller should continue reading).
fn dispatch_ndjson_line(line: &str, session: &BuildSession, build_id: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.starts_with('{') {
        return None;
    }
    let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return None;
    };
    match val["type"].as_str() {
        Some("result") => {
            return val["text"].as_str().map(ToOwned::to_owned);
        }
        Some("context") => {
            #[allow(clippy::cast_possible_truncation)]
            let usage_pct = val["usage_pct"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0) as f32;
            let _ = session.event_tx.send(WebEventV2::from_event(
                crate::events::WebEvent::ContextStatus(crate::events::types::ContextStatusEvent {
                    usage_pct,
                    level: val["level"].as_str().map(ToOwned::to_owned),
                    budget: val["budget"].as_u64().unwrap_or(0),
                    used: val["used"].as_u64().unwrap_or(0),
                }),
                Some(session.build_id),
            ));
        }
        Some(kind @ ("tool_call" | "thinking")) => {
            let summary = if kind == "tool_call" {
                val["tool_name"].as_str().map(ToOwned::to_owned)
            } else {
                val["text"]
                    .as_str()
                    .map(|t| t.chars().take(120).collect::<String>())
            };
            let _ = session.event_tx.send(crate::events::WebEventV2::from_event(
                crate::events::WebEvent::CopilotActivity(
                    crate::events::types::CopilotActivityEvent {
                        build_id: build_id.to_owned(),
                        kind: kind.to_owned(),
                        summary,
                        raw: val,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    },
                ),
                Some(session.build_id),
            ));
        }
        _ => {}
    }
    None
}

/// Spawn one turn of a `claude --print` subprocess for `Lightarchitects` backends.
///
/// Uses `--output-format stream-json --verbose` (required combination for `--print`).
/// Turn 1 (no `prev_session_id`): claude assigns a new session UUID returned in the result.
/// Turn 2+ (`prev_session_id` is `Some`): `--resume <id>` continues the prior conversation
/// from disk — giving full multi-turn context without a persistent subprocess.
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
        let _ = std::fs::create_dir_all(&session.cwd);
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
        c.arg("--resume").arg(id);
    }
    c.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = c.spawn().map_err(|e| format!("spawn claude: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "claude stdout unavailable".to_owned())?;
    let mut reader = BufReader::new(stdout).lines();

    let mut result_text: Option<String> = None;
    let mut found_session_id: Option<String> = None;
    let build_id = session.build_id.to_string();

    // OTel semconv v1.56.0 — SA-23: emit a structured event instead of `.entered()`
    // because EnteredSpan is !Send; holding it across .await makes the future !Send,
    // breaking axum's Handler trait. Use tracing::info! to preserve the span semantics.
    tracing::info!(build_id = %build_id, "app.copilot.turn starting");

    while let Some(line) = reader
        .next_line()
        .await
        .map_err(|e| format!("read stdout: {e}"))?
    {
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
            crate::events::WebEvent::CopilotActivity(crate::events::types::CopilotActivityEvent {
                build_id: build_id.clone(),
                kind: event_type.to_owned(),
                summary,
                raw: val.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            }),
            Some(session.build_id),
        ));
        if send_result.is_err() {
            tracing::warn!(
                counter = "app.copilot.token_buffer.overflow_total",
                "channel send failed — receiver lagged or dropped"
            );
        }

        // Emit AYIN span for tool calls so they appear in the AYIN SPANS column
        if event_type == "content_block_start"
            && val["content_block"]["type"].as_str() == Some("tool_use")
        {
            let tool_name = val["content_block"]["name"].as_str().unwrap_or("unknown");
            let _ = session.event_tx.send(crate::events::WebEventV2::from_event(
                crate::events::WebEvent::AyinSpan(crate::events::types::TraceSpanSummary {
                    id: uuid::Uuid::new_v4().to_string(),
                    parent_id: None,
                    actor: "eva".to_owned(),
                    action: format!("tool.{tool_name}"),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    duration_ms: 0,
                    outcome: serde_json::json!("started"),
                    metadata: serde_json::json!({ "build_id": build_id }),
                    strand_activations: Vec::new(),
                }),
                Some(session.build_id),
            ));
        }

        // SA-20: extract text_delta — field named `delta_type` not `delta.type`
        // (reserved keyword). SA-21: parse errors warn-and-continue, never `?`.
        if event_type == "content_block_delta" {
            if let Some(delta_val) = val.get("delta") {
                match serde_json::from_value::<ContentBlockDelta>(delta_val.clone()) {
                    Ok(delta) => {
                        if delta.delta_type == "text_delta" {
                            let send_r =
                                session.event_tx.send(crate::events::WebEventV2::from_event(
                                    crate::events::WebEvent::CopilotResponse {
                                        chunk: delta.text.clone(),
                                        done: false,
                                        sibling: Some("claude".to_owned()),
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
                    }
                    Err(e) => {
                        tracing::warn!(
                            build_id = %build_id,
                            error = %e,
                            "copilot delta parse error"
                        );
                        tracing::warn!(
                            counter = "app.copilot.parse_error_total",
                            "incrementing counter"
                        );
                    }
                }
            }
        }

        if event_type == "result" && val["subtype"].as_str() == Some("success") {
            result_text = Some(val["result"].as_str().unwrap_or("").to_owned());
        }

        // Emit done:true on message_stop to signal end-of-turn to frontend.
        if event_type == "message_stop" {
            let _ = session.event_tx.send(crate::events::WebEventV2::from_event(
                crate::events::WebEvent::CopilotResponse {
                    chunk: String::new(),
                    done: true,
                    sibling: Some("claude".to_owned()),
                },
                Some(session.build_id),
            ));
        }
    }

    // Wait for the child to exit so we can check status
    let status = child
        .wait()
        .await
        .map_err(|e| format!("wait claude: {e}"))?;

    result_text.map(|t| (t, found_session_id)).ok_or_else(|| {
        if status.success() {
            "no result event in claude output".to_owned()
        } else {
            format!("claude exited with status {status}")
        }
    })
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
        let _ = std::fs::create_dir_all(&session.cwd);
    }
    c.current_dir(&session.cwd);
    c.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = c.spawn().map_err(|e| format!("spawn codex: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "codex stdout unavailable".to_owned())?;
    let mut reader = BufReader::new(stdout).lines();

    let mut thread_id: Option<String> = None;
    let mut text = String::new();
    let mut turn_done = false;
    let mut turn_error: Option<String> = None;
    let build_id = session.build_id.to_string();

    while let Some(line) = reader
        .next_line()
        .await
        .map_err(|e| format!("read stdout: {e}"))?
    {
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

        let event_type = val["type"].as_str().unwrap_or("unknown");

        // Broadcast activity event for the Activity tab
        let summary = extract_codex_activity_summary(&val);
        let _ = session.event_tx.send(crate::events::WebEventV2::from_event(
            crate::events::WebEvent::CopilotActivity(crate::events::types::CopilotActivityEvent {
                build_id: build_id.clone(),
                kind: event_type.to_owned(),
                summary,
                raw: val.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            }),
            Some(session.build_id),
        ));

        if event_type == "thread.started" {
            if let Some(id) = val["thread_id"].as_str() {
                thread_id = Some(id.to_owned());
            }
        }
        if event_type == "item.completed" && val["item"]["type"].as_str() == Some("agent_message") {
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
    let status = child.wait().await.map_err(|e| format!("wait codex: {e}"))?;

    if let Some(err) = turn_error {
        return Err(format!("codex turn failed: {err}"));
    }
    if turn_done {
        Ok((text, thread_id))
    } else {
        Err(format!("no turn.completed in codex output (exit {status})"))
    }
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
        let _ = std::fs::create_dir_all(&session.cwd);
    }
    c.current_dir(&session.cwd);
    c.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = c.output().await.map_err(|e| {
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
        return Err("vibe returned empty response".to_owned());
    }
    Ok(text)
}

/// Spawn a persistent agent subprocess for the `LightarchitectsNative` backend.
///
/// | Session | Binary | Extra env |
/// |---------|--------|-----------|
/// | `LightarchitectsNative` | `<cfg.binary>` | none |
///
/// # Errors
///
/// Returns a descriptive string if the subprocess cannot be spawned or if
/// stdin/stdout handles are unavailable.
fn spawn_copilot(session: &BuildSession) -> Result<CopilotProcess, String> {
    let mut cmd = match &session.agent {
        // LightArchitects CLI (formerly lÆx0) — single-shot `run <prompt>` mode.
        // The webshell acts as auth broker: resolves credentials from OS keychain
        // (stored by /api/setup/save) and injects them into the subprocess env.
        AgentSession::LightarchitectsNative(cfg) => {
            let resolved = resolve_binary(&cfg.binary);
            let mut c = tokio::process::Command::new(&resolved);
            c.env("PATH", augmented_path());

            // ── Auth broker: inject credentials from webshell keychain ──
            // Priority: keychain entry from setup/save → SDK credential registry → env fallback
            if let Some(key) = resolve_api_key_for_native() {
                c.env("ANTHROPIC_API_KEY", &key);
            }

            c.arg("run").arg("--yes").arg("--no-splash");
            if session.cwd.is_dir() {
                c.arg("--cwd").arg(&session.cwd);
            }
            c
        }
        _ => return Err("spawn_copilot called for non-persistent-subprocess backend".to_owned()),
    };

    cmd.kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn agent: {e}"))?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "agent stdin unavailable".to_owned())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "agent stdout unavailable".to_owned())?;

    Ok(CopilotProcess {
        session_id: None,
        stdin: Some(BufWriter::new(stdin)),
        stdout: Some(BufReader::new(stdout).lines()),
        _child: Some(child),
    })
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
/// `LightarchitectsNative`: writes to a persistent subprocess stdin and reads
/// until the EOT marker.  Spawns lazily on first call or after a crash.
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
    let (span_id, start, start_ts) = emit_turn_start_span(session, actor, message);

    // Per-turn path for Lightarchitects (claude --print + disk-persistent sessions).
    if matches!(&session.agent, AgentSession::Lightarchitects(_)) {
        let prev_session_id = guard
            .as_ref()
            .and_then(|p| p.session_id.as_deref())
            .map(ToOwned::to_owned);

        let (text, new_session_id) =
            run_print_turn(message, session, prev_session_id.as_deref()).await?;

        if let Some(ref mut proc) = *guard {
            proc.session_id = new_session_id;
        } else {
            *guard = Some(CopilotProcess {
                session_id: new_session_id,
                stdin: None,
                stdout: None,
                _child: None,
            });
        }

        // Emit turn-complete AYIN span
        emit_turn_complete_span(
            session,
            &span_id,
            actor,
            &start_ts,
            start.elapsed(),
            "success",
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
            },
            Some(session.build_id),
        ));

        emit_turn_complete_span(
            session,
            &span_id,
            actor,
            &start_ts,
            start.elapsed(),
            "success",
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
        } else {
            *guard = Some(CopilotProcess {
                session_id: new_session_id,
                stdin: None,
                stdout: None,
                _child: None,
            });
        }

        // Emit turn-complete AYIN span
        emit_turn_complete_span(
            session,
            &span_id,
            actor,
            &start_ts,
            start.elapsed(),
            "success",
        );

        return Ok(text);
    }

    // Persistent subprocess path — `LightarchitectsNative` CLI with NDJSON streaming.
    if guard.is_none() {
        *guard = Some(spawn_copilot(session)?);
    }

    let proc = guard
        .as_mut()
        .ok_or_else(|| "copilot process unavailable".to_owned())?;

    // Wrap prompt in a structured envelope (§2.1 LLM01 — structural isolation).
    // The CLI's parse_stdin_prompt helper strips this envelope before forwarding
    // to the LLM, ensuring operator text never lands verbatim in the system-prompt
    // namespace.
    let envelope = json!({"type": "prompt", "text": message});
    let envelope_str = envelope.to_string();
    let msg_bytes = [envelope_str.as_bytes(), b"\n"].concat();
    {
        let stdin = proc
            .stdin
            .as_mut()
            .ok_or_else(|| "no stdin for persistent subprocess".to_owned())?;
        stdin
            .write_all(&msg_bytes)
            .await
            .map_err(|e| format!("stdin write: {e}"))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("stdin flush: {e}"))?;
    }

    let build_id = session.build_id.to_string();
    let result_text: Option<String> = loop {
        // Borrow proc.stdout only within this inner block to allow accessing
        // proc.session_id (a different field) in the match arms below.
        let next_line = if let Some(stdout) = proc.stdout.as_mut() {
            stdout.next_line().await
        } else {
            *guard = None;
            return Err("no stdout for persistent subprocess".to_owned());
        };
        match next_line {
            Ok(Some(line)) if !line.is_empty() => {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                    if let Some(id) = val["session_id"].as_str() {
                        proc.session_id = Some(id.to_owned());
                    }
                }
                if let Some(text) = dispatch_ndjson_line(&line, session, &build_id) {
                    break Some(text);
                }
            }
            Ok(None) => {
                *guard = None;
                return Err("agent process exited unexpectedly".to_owned());
            }
            Ok(Some(_)) => {}
            Err(e) => {
                *guard = None;
                return Err(format!("stdout read: {e}"));
            }
        }
    };

    let text = result_text.ok_or_else(|| "no result in agent stream output".to_owned())?;
    emit_turn_complete_span(
        session,
        &span_id,
        actor,
        &start_ts,
        start.elapsed(),
        "success",
    );
    Ok(text)
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
    let mut builder = reqwest::Client::new()
        .post(format!("{base_url}/v1/chat/completions"))
        .json(&json!({
            "model": model,
            "messages": [{ "role": "user", "content": message }],
        }));
    if auth_token != "ollama" {
        builder = builder.header("authorization", format!("Bearer {auth_token}"));
    }
    let res = builder.send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        let code = res.status().as_u16();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Ollama {code}: {body}"));
    }
    let val: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    val["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "unexpected Ollama response shape".to_owned())
}

/// Emit a turn-start AYIN span and return `(span_id, Instant, timestamp)` for
/// the caller to pass to [`emit_turn_complete_span`] when the turn finishes.
fn emit_turn_start_span(
    session: &BuildSession,
    actor: &str,
    message: &str,
) -> (String, std::time::Instant, String) {
    let span_id = uuid::Uuid::new_v4().to_string();
    let start = std::time::Instant::now();
    let start_ts = chrono::Utc::now().to_rfc3339();
    let _ = session.event_tx.send(crate::events::WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(crate::events::types::TraceSpanSummary {
            id: span_id.clone(),
            parent_id: None,
            actor: actor.to_owned(),
            action: "copilot.turn.started".to_owned(),
            timestamp: start_ts.clone(),
            duration_ms: 0,
            outcome: serde_json::json!("pending"),
            metadata: serde_json::json!({
                "message_preview": &message[..message.len().min(200)],
                "build_id": session.build_id.to_string(),
            }),
            strand_activations: Vec::new(),
        }),
        Some(session.build_id),
    ));
    (span_id, start, start_ts)
}

/// Emit a turn-complete AYIN span with real duration measurement.
fn emit_turn_complete_span(
    session: &BuildSession,
    parent_span_id: &str,
    actor: &str,
    start_ts: &str,
    elapsed: std::time::Duration,
    outcome: &str,
) {
    let _ = session.event_tx.send(crate::events::WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(crate::events::types::TraceSpanSummary {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: Some(parent_span_id.to_owned()),
            actor: actor.to_owned(),
            action: "copilot.turn.completed".to_owned(),
            timestamp: start_ts.to_owned(),
            duration_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
            outcome: serde_json::json!(outcome),
            metadata: serde_json::json!({
                "build_id": session.build_id.to_string(),
                "duration_s": format!("{:.1}", elapsed.as_secs_f64()),
            }),
            strand_activations: Vec::new(),
        }),
        Some(session.build_id),
    ));
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
        .stderr(std::process::Stdio::null());

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

    // ── Test helpers ─────────────────────────────────────────────────────────

    fn make_test_session() -> crate::session::BuildSession {
        crate::session::BuildSession::new(
            std::path::PathBuf::from("/tmp"),
            crate::config::AgentSession::LightarchitectsNative(
                crate::config::LightarchitectsNativeConfig::default(),
            ),
        )
    }

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

    // ── dispatch_ndjson_line — unit suite ─────────────────────────────────────

    #[test]
    fn dispatch_result_line_returns_text() {
        let session = make_test_session();
        let result = dispatch_ndjson_line(
            r#"{"type":"result","text":"Hello, world!"}"#,
            &session,
            "build-1",
        );
        assert_eq!(result.as_deref(), Some("Hello, world!"));
    }

    #[test]
    fn dispatch_result_with_no_text_returns_none() {
        let session = make_test_session();
        let result = dispatch_ndjson_line(r#"{"type":"result"}"#, &session, "b");
        assert!(result.is_none());
    }

    // ── dispatch_ndjson_line — integration suite (crosses event-bus boundary) ─

    #[test]
    fn dispatch_tool_call_sends_activity_event() {
        let session = make_test_session();
        let mut rx = session.event_tx.subscribe();
        let result =
            dispatch_ndjson_line(r#"{"type":"tool_call","tool_name":"Read"}"#, &session, "b1");
        assert!(result.is_none(), "tool_call must not return text");
        let event = rx
            .try_recv()
            .expect("expected CopilotActivity event on event_tx");
        assert!(
            matches!(
                event,
                crate::events::WebEventV2 { inner: crate::events::types::WebEvent::CopilotActivity(ref e), .. } if e.kind == "tool_call"
            ),
            "unexpected event variant: {event:?}"
        );
    }

    #[test]
    fn dispatch_thinking_sends_activity_event() {
        let session = make_test_session();
        let mut rx = session.event_tx.subscribe();
        let result = dispatch_ndjson_line(
            r#"{"type":"thinking","text":"Planning step."}"#,
            &session,
            "b2",
        );
        assert!(result.is_none());
        let event = rx
            .try_recv()
            .expect("expected CopilotActivity event on event_tx");
        assert!(
            matches!(
                event,
                crate::events::WebEventV2 { inner: crate::events::types::WebEvent::CopilotActivity(ref e), .. } if e.kind == "thinking"
            ),
            "unexpected event variant: {event:?}"
        );
    }

    #[test]
    fn dispatch_context_sends_context_status_event() {
        let session = make_test_session();
        let mut rx = session.event_tx.subscribe();
        let result = dispatch_ndjson_line(
            r#"{"type":"context","usage_pct":0.42,"level":null,"budget":200000,"used":84000}"#,
            &session,
            "b3",
        );
        assert!(result.is_none());
        let event = rx
            .try_recv()
            .expect("expected ContextStatus event on event_tx");
        assert!(
            matches!(
                event,
                crate::events::WebEventV2 { inner: crate::events::types::WebEvent::ContextStatus(ref e), .. } if (e.usage_pct - 0.42).abs() < 0.01
            ),
            "unexpected event variant: {event:?}"
        );
    }

    // ── dispatch_ndjson_line — smoke suite (happy-path gate) ─────────────────

    #[test]
    fn dispatch_smoke_result_roundtrip() {
        let session = make_test_session();
        assert_eq!(
            dispatch_ndjson_line(r#"{"type":"result","text":"done"}"#, &session, "smoke")
                .as_deref(),
            Some("done")
        );
    }

    // ── dispatch_ndjson_line — regression suite ───────────────────────────────

    #[test]
    fn dispatch_empty_line_returns_none_no_panic() {
        let session = make_test_session();
        assert!(dispatch_ndjson_line("", &session, "b").is_none());
        assert!(dispatch_ndjson_line("   ", &session, "b").is_none());
    }

    #[test]
    fn dispatch_non_json_returns_none_no_panic() {
        let session = make_test_session();
        assert!(dispatch_ndjson_line("tracing INFO span", &session, "b").is_none());
        assert!(dispatch_ndjson_line("plain text output", &session, "b").is_none());
    }

    #[test]
    fn dispatch_unknown_type_returns_none_no_event() {
        let session = make_test_session();
        let mut rx = session.event_tx.subscribe();
        let result = dispatch_ndjson_line(r#"{"type":"unknown_future_type","x":1}"#, &session, "b");
        assert!(result.is_none());
        assert!(
            rx.try_recv().is_err(),
            "no event expected for unrecognised type"
        );
    }

    #[test]
    fn dispatch_malformed_json_does_not_panic() {
        let session = make_test_session();
        // Truncated JSON that starts with '{' — must not panic.
        dispatch_ndjson_line(r#"{"type":"result","text":"#, &session, "b");
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
