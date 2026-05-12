//! Interactive coding agent — NDJSON streaming mode for webshell bridge.
//!
//! Entry point: [`run_ndjson`] — reads [`ControlMessage`] from stdin,
//! runs an agent turn, emits [`AgentEvent`] to stdout.
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

pub mod protocol;
pub mod runner;

use runner::AgentRunner;

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
        validate_system_prompt(sp).map_err(|e| Box::<dyn std::error::Error>::from(e))?;
    }
    if let Some(key) = std::env::var("LA_INHERITED_API_KEY").ok().filter(|s| !s.is_empty()) {
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
    let mut runner = AgentRunner::new(cwd)?;
    if let Some(sp) = system_prompt {
        runner.set_system_prompt(sp);
    }
    runner.run_ndjson_loop().await;
    Ok(())
}

/// Persist an inherited API key to `~/.lightarchitects/keys.toml`.
fn persist_inherited_key(key: &str, key_name: &str) {
    let Some(home) = std::env::var_os("HOME") else { return };
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
    let _ = std::fs::write(&path, toml::to_string_pretty(&keys).unwrap_or_default());
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
    let mut runner = AgentRunner::new(cwd)?;
    runner.run_interactive_loop().await;
    Ok(())
}
