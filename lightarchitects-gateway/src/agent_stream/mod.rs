//! Interactive coding agent — NDJSON streaming mode for webshell bridge.
//!
//! All agent I/O is handled by [`ConversationSession`] from the SDK.
//!
//! SDK re-exports:
//!
//! - [`ConversationSession`] — primary session type
//! - [`SessionConfig`] — frozen session configuration
//! - [`ConversationEvent`] — event enum
//! - [`Transport`] — outbound event sink trait
//! - [`NdjsonTransport`] / [`TtyTransport`] / [`SseTransport`] — concrete transports
//!
//! Entry points:
//! - [`run_ndjson`] — NDJSON stdin→stdout loop (webshell bridge)
//! - [`run_interactive`] — TTY REPL (human-facing)
//!
//! ## Tool surface
//!
//! Reuses gateway `core_tools`:
//! - `bash`   — shell commands (blocked-list protected)
//! - `read`   — file contents with line ranges
//! - `write`  — create / overwrite files atomically
//! - `edit`   — string replacement
//! - `search` — ripgrep file search
//! - `glob`   — file pattern matching
//!
//! ## LLM backend
//!
//! Uses the same `LlmClient` as Arena — Ollama, OpenAI-compatible, or
//! Anthropic (added in this module's companion change to `arena::llm`).

use std::path::Path;
use std::sync::Arc;

use lightarchitects::agent::ChainContext;
use tokio::io::AsyncBufReadExt as _;

use crate::config::GatewayConfig;

pub mod endpoint_policy;
pub mod protocol;
pub mod session_memory;
pub mod strategy;

// ── SDK re-exports (loops-core) ───────────────────────────────────────────────

pub use lightarchitects::agent::conversation::{
    ConversationEvent, ConversationSession, NdjsonTransport, SessionConfig, SessionError,
    SessionState, SseTransport, TerminationReason, Transport, TtyTransport,
};

// ── CapturingTransport ────────────────────────────────────────────────────────

/// A [`Transport`] wrapper that forwards all events to an inner transport while
/// accumulating [`ConversationEvent::Text`] chunks into a buffer.
///
/// After [`ConversationSession::run_turn`] returns, call [`take_buffer`] to
/// retrieve (and clear) the full assistant response text for post-turn analysis.
///
/// [`take_buffer`]: CapturingTransport::take_buffer
struct CapturingTransport<T: Transport> {
    inner: T,
    buffer: String,
}

impl<T: Transport> CapturingTransport<T> {
    fn new(inner: T) -> Self {
        Self {
            inner,
            buffer: String::new(),
        }
    }

    /// Return accumulated text since the last call (or since construction) and
    /// reset the internal buffer.
    fn take_buffer(&mut self) -> String {
        std::mem::take(&mut self.buffer)
    }
}

#[async_trait::async_trait]
impl<T: Transport> Transport for CapturingTransport<T> {
    async fn emit(&mut self, event: &ConversationEvent) -> std::io::Result<()> {
        if let ConversationEvent::Text { chunk } = event {
            self.buffer.push_str(chunk);
        }
        self.inner.emit(event).await
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush().await
    }
}

// ── Skill invocation extraction ───────────────────────────────────────────────

/// Scan the last non-empty line of an LLM response for a skill slash command.
///
/// The LLM is instructed (via system prompt) to emit the command as its final
/// line when it decides a skill should run. Checking only the last line avoids
/// false positives from skill names quoted inline in prose.
fn extract_skill_invocation(text: &str) -> Option<(String, Vec<String>)> {
    let last = text.lines().rev().find(|l| !l.trim().is_empty())?;
    crate::cli::skills::parse_skill_slash_command(last.trim())
}

/// Maximum byte length for a caller-supplied system prompt.
/// Prevents token-flood amplification when an untrusted caller controls the prompt.
pub const SYSTEM_PROMPT_MAX_BYTES: usize = 8 * 1024;

/// Validate a caller-supplied system prompt at the input boundary.
///
/// Returns an error string if the prompt violates the security constraints:
/// - Must not exceed [`SYSTEM_PROMPT_MAX_BYTES`].
/// - Must not contain NUL bytes (prevents C-string truncation in downstream tools).
///
/// # Errors
///
/// Returns a static error message string on violation.
pub fn validate_system_prompt(prompt: &str) -> Result<(), &'static str> {
    if prompt.len() > SYSTEM_PROMPT_MAX_BYTES {
        return Err("system_prompt exceeds 8 KiB limit");
    }
    if prompt.contains('\0') {
        return Err("system_prompt contains NUL byte");
    }
    Ok(())
}

/// Run the agent in NDJSON streaming mode.
///
/// - Reads `ControlMessage` lines from stdin.
/// - Emits `AgentEvent` lines to stdout.
/// - Blocking; returns when stdin closes or an unrecoverable error occurs.
///
/// `system_prompt` overrides the default "You are a helpful coding assistant." preamble.
/// It is validated at this boundary (length + NUL check); pass `None` for the default.
///
/// # Errors
///
/// Returns an error if the LLM client cannot be initialised from environment, or if
/// `system_prompt` fails validation.
pub async fn run_ndjson(
    cwd: &Path,
    system_prompt: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(ref sp) = system_prompt {
        validate_system_prompt(sp).map_err(Box::<dyn std::error::Error>::from)?;
    }
    if let Some(key) = std::env::var("LA_INHERITED_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
    {
        let backend = std::env::var("LA_INHERITED_BACKEND")
            .ok()
            .and_then(|b| match b.to_lowercase().as_str() {
                "anthropic" | "claude" => Some("ANTHROPIC_API_KEY"),
                "openai" | "codex" => Some("OPENAI_API_KEY"),
                "ollama" => Some("OLLAMA_API_KEY"),
                _ => None,
            })
            .unwrap_or("ANTHROPIC_API_KEY");
        persist_inherited_key(&key, backend);
    }
    let config = SessionConfig {
        cwd: cwd.to_path_buf(),
        system_prompt,
        ..SessionConfig::default()
    };
    let provider =
        crate::cli::skills::build_provider().map_err(Box::<dyn std::error::Error>::from)?;
    let mut session = ConversationSession::new(config, Arc::new(provider));
    let mut transport = NdjsonTransport::new(tokio::io::stdout());
    session.run_ndjson_loop(&mut transport).await;
    Ok(())
}

/// Persist an inherited API key to `~/.lightarchitects/keys.toml`.
fn persist_inherited_key(key: &str, key_name: &str) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let path = std::path::PathBuf::from(home)
        .join(".lightarchitects")
        .join("keys.toml");
    let mut keys: std::collections::HashMap<String, String> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| toml::from_str(&c).ok())
        .unwrap_or_default();
    keys.insert(key_name.to_owned(), key.to_owned());
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(serialized) = toml::to_string_pretty(&keys) {
        let _ = std::fs::write(&path, serialized);
    }
}

/// Run the agent in NDJSON streaming mode with strategy loop interception.
///
/// Handles `{"action":"run_strategy",...}` lines by dispatching directly to
/// the configured sibling strategy runners. All other NDJSON control messages
/// (`send_message`, `interrupt`, `set_system_prompt`, `ping`) are forwarded
/// to the [`ConversationSession`] via single-turn dispatch.
///
/// # Errors
///
/// Returns an error if `system_prompt` fails validation.
#[allow(clippy::too_many_lines)]
pub async fn run_ndjson_with_strategies(
    cwd: &Path,
    system_prompt: Option<String>,
    config: &GatewayConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(ref sp) = system_prompt {
        validate_system_prompt(sp).map_err(Box::<dyn std::error::Error>::from)?;
    }
    if let Some(key) = std::env::var("LA_INHERITED_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
    {
        let backend = std::env::var("LA_INHERITED_BACKEND")
            .ok()
            .and_then(|b| match b.to_lowercase().as_str() {
                "anthropic" | "claude" => Some("ANTHROPIC_API_KEY"),
                "openai" | "codex" => Some("OPENAI_API_KEY"),
                "ollama" => Some("OLLAMA_API_KEY"),
                _ => None,
            })
            .unwrap_or("ANTHROPIC_API_KEY");
        persist_inherited_key(&key, backend);
    }

    let session_config = SessionConfig {
        cwd: cwd.to_path_buf(),
        system_prompt,
        ..SessionConfig::default()
    };
    let provider =
        crate::cli::skills::build_provider().map_err(Box::<dyn std::error::Error>::from)?;
    let memory = session_memory::HelixSessionMemory::open(cwd, 20);
    let mut session =
        ConversationSession::new(session_config, Arc::new(provider)).with_memory(Box::new(memory));
    let mut transport = NdjsonTransport::new(tokio::io::stdout());
    let chain = ChainContext::default();

    let reader = tokio::io::BufReader::new(tokio::io::stdin());
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            let _ = transport
                .emit(&ConversationEvent::Error {
                    message: "parse error: not valid JSON".to_owned(),
                    recoverable: Some(true),
                })
                .await;
            continue;
        };

        let action = val.get("action").and_then(|a| a.as_str()).unwrap_or("");

        if action == "run_strategy" {
            match serde_json::from_value::<strategy::StrategyRequest>(val) {
                Ok(req) => {
                    if let Err(e) = strategy::run_strategy(req, config, &mut transport).await {
                        let _ = transport
                            .emit(&ConversationEvent::Error {
                                message: e.to_string(),
                                recoverable: Some(false),
                            })
                            .await;
                    }
                }
                Err(e) => {
                    let _ = transport
                        .emit(&ConversationEvent::Error {
                            message: format!("invalid strategy request: {e}"),
                            recoverable: Some(true),
                        })
                        .await;
                }
            }
            continue;
        }

        // {"action":"run_skill","skill":"reflect","args":["topic"]}
        if action == "run_skill" {
            let slug = val
                .get("skill")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_owned();
            let extra: Vec<String> = val
                .get("args")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                        .collect()
                })
                .unwrap_or_default();

            if slug.is_empty() {
                let _ = transport
                    .emit(&ConversationEvent::Error {
                        message: "run_skill requires a 'skill' field".to_owned(),
                        recoverable: Some(true),
                    })
                    .await;
            } else {
                let mut skill_args = vec![slug];
                skill_args.extend(extra);
                if let Err(e) = crate::cli::skills::execute(config, &skill_args).await {
                    let _ = transport
                        .emit(&ConversationEvent::Error {
                            message: e.to_string(),
                            recoverable: Some(false),
                        })
                        .await;
                }
            }
            continue;
        }

        match action {
            "send_message" => {
                let text = val
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_owned();
                if !text.is_empty() {
                    session.clear_interrupt();
                    if let Err(e) = session.run_turn(&text, &mut transport, &chain).await {
                        let _ = transport
                            .emit(&ConversationEvent::Error {
                                message: e.to_string(),
                                recoverable: Some(false),
                            })
                            .await;
                    }
                }
            }
            "interrupt" => {
                session.interrupt();
                let _ = transport
                    .emit(&ConversationEvent::Error {
                        message: "interrupted".to_owned(),
                        recoverable: Some(true),
                    })
                    .await;
            }
            "ping" => {
                let _ = transport.emit(&ConversationEvent::Heartbeat).await;
            }
            _ => {
                let _ = transport
                    .emit(&ConversationEvent::Error {
                        message: format!("unknown action: {action}"),
                        recoverable: Some(true),
                    })
                    .await;
            }
        }
    }

    Ok(())
}

/// Run the agent in interactive TTY mode with strategy slash-command interception.
///
/// Reads lines from stdin. Lines matching `/strategy <kind> <goal>`, `/loop`, or
/// `/run` are dispatched to the strategy runner; all other input is forwarded to
/// the [`ConversationSession`] as conversational turns.
///
/// # Errors
///
/// Returns an error if the LLM client cannot be initialised from environment.
#[allow(clippy::too_many_lines)]
pub async fn run_interactive_with_strategies(
    cwd: &Path,
    config: &GatewayConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::io::AsyncWriteExt as _;

    if let Some(key) = std::env::var("LA_INHERITED_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
    {
        let backend = std::env::var("LA_INHERITED_BACKEND")
            .ok()
            .and_then(|b| match b.to_lowercase().as_str() {
                "anthropic" | "claude" => Some("ANTHROPIC_API_KEY"),
                "openai" | "codex" => Some("OPENAI_API_KEY"),
                "ollama" => Some("OLLAMA_API_KEY"),
                _ => None,
            })
            .unwrap_or("ANTHROPIC_API_KEY");
        persist_inherited_key(&key, backend);
    }

    let skill_prompt = crate::cli::skills::build_skill_system_prompt();
    let session_config = SessionConfig {
        cwd: cwd.to_path_buf(),
        system_prompt: Some(skill_prompt),
        ..SessionConfig::default()
    };

    // W6.3: shared executor tracks operator-invoked skills so the operator-wins
    // invariant can be enforced if the LLM emits tool_use for the same skill.
    let executor = Arc::new(crate::providers::GatewayToolExecutor::new_with_skills(
        Arc::new(config.clone()),
    ));

    let provider =
        crate::cli::skills::build_provider().map_err(Box::<dyn std::error::Error>::from)?;
    let memory = session_memory::HelixSessionMemory::open(cwd, 20);
    let restored = memory.restored_turn_count();
    let mut session =
        ConversationSession::new(session_config, Arc::new(provider)).with_memory(Box::new(memory));
    let mut transport = CapturingTransport::new(TtyTransport::new(tokio::io::stdout()));
    let chain = ChainContext::default();

    let mut stdout = tokio::io::stdout();
    let resume_note = if restored > 0 {
        format!(" ({restored} prior turns restored)")
    } else {
        String::new()
    };
    let banner = format!(
        "Light Architects agent — cwd: {}{resume_note}\n\
         Skills: /plan /build /reflect /scrum /gate /xea … (or just describe what you need)\n\
         Strategies: /strategy react|ach|itt|cove|reflexion <goal> | quit to exit\n\
         AYIN dashboard: http://127.0.0.1:3742\n",
        cwd.display()
    );
    let _ = stdout.write_all(banner.as_bytes()).await;

    // B5: warn if a non-default LLM endpoint is active (OWASP-LLM05/LLM06).
    for var in &["ANTHROPIC_BASE_URL", "OLLAMA_BASE_URL", "LLM_API_URL"] {
        if let Ok(url) = std::env::var(var) {
            if !url.is_empty() && !endpoint_policy::is_default_allowed(&url) {
                let warning = endpoint_policy::custom_endpoint_banner(&url);
                let _ = stdout.write_all(warning.as_bytes()).await;
            }
        }
    }

    let _ = stdout.flush().await;

    let reader = tokio::io::BufReader::new(tokio::io::stdin());
    let mut lines = reader.lines();

    // H15: Ctrl-C mid-generation → interrupt in-flight turn and return to prompt.
    // The watcher sets the session interrupt flag; run_turn's heartbeat loop checks it.
    let interrupt_flag = std::sync::Arc::clone(&session.state.interrupt_flag);
    let sigint_watcher = tokio::spawn(async move {
        loop {
            if tokio::signal::ctrl_c().await.is_ok() {
                interrupt_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
    });

    loop {
        let _ = stdout.write_all(b"> ").await;
        let _ = stdout.flush().await;

        let Ok(Some(line)) = lines.next_line().await else {
            break;
        };

        let input = line.trim();
        if input.is_empty() {
            continue;
        }
        if input.eq_ignore_ascii_case("quit") || input.eq_ignore_ascii_case("exit") {
            break;
        }

        // Explicit user slash commands — no LLM needed.
        if let Some(req) = strategy::parse_slash_command(input) {
            if let Err(e) = strategy::run_strategy(req, config, &mut transport).await {
                eprintln!("Strategy error: {e}");
            }
            continue;
        }

        if let Some((slug, skill_args)) = crate::cli::skills::parse_skill_slash_command(input) {
            // W6.3: mark the slug as operator-claimed before dispatching so that
            // any concurrent LLM tool_use for the same skill returns
            // SupersededByOperatorAction instead of double-executing.
            executor.mark_operator_invoked(&slug);
            let mut full_args = vec![slug];
            full_args.extend(skill_args);
            if let Err(e) = crate::cli::skills::execute(config, &full_args).await {
                eprintln!("Skill error: {e}");
            }
            continue;
        }

        // Clear the operator-claimed set at the start of each conversational turn
        // so stale claims from previous turns don't suppress new LLM tool_use.
        executor.clear_operator_invocations();

        // Conversational turn — LLM responds; inspect last line for implicit skill dispatch.
        session.clear_interrupt();
        if let Err(e) = session.run_turn(input, &mut transport, &chain).await {
            let _ = transport
                .emit(&ConversationEvent::Error {
                    message: e.to_string(),
                    recoverable: Some(true),
                })
                .await;
            continue;
        }

        let response = transport.take_buffer();
        if let Some((slug, skill_args)) = extract_skill_invocation(&response) {
            let mut full_args = vec![slug];
            full_args.extend(skill_args);
            if let Err(e) = crate::cli::skills::execute(config, &full_args).await {
                eprintln!("Skill error: {e}");
            }
            // Skill session complete — fall back to the parent conversation loop.
        }
    }

    sigint_watcher.abort();
    Ok(())
}

/// Run the agent in interactive TTY mode (human-facing REPL).
///
/// - Prompts the user with `> `, reads natural text from stdin.
/// - Prints agent responses in plain text.
/// - Blocking; returns when the user types `quit` or EOF.
///
/// # Errors
///
/// Returns an error if the LLM client cannot be initialised from environment.
pub async fn run_interactive(cwd: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(key) = std::env::var("LA_INHERITED_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
    {
        let backend = std::env::var("LA_INHERITED_BACKEND")
            .ok()
            .and_then(|b| match b.to_lowercase().as_str() {
                "anthropic" | "claude" => Some("ANTHROPIC_API_KEY"),
                "openai" | "codex" => Some("OPENAI_API_KEY"),
                "ollama" => Some("OLLAMA_API_KEY"),
                _ => None,
            })
            .unwrap_or("ANTHROPIC_API_KEY");
        persist_inherited_key(&key, backend);
    }
    let config = SessionConfig {
        cwd: cwd.to_path_buf(),
        ..SessionConfig::default()
    };
    let provider =
        crate::cli::skills::build_provider().map_err(Box::<dyn std::error::Error>::from)?;
    let mut session = ConversationSession::new(config, Arc::new(provider));
    let mut transport = TtyTransport::new(tokio::io::stdout());
    session.run_interactive_loop(&mut transport).await;
    Ok(())
}
