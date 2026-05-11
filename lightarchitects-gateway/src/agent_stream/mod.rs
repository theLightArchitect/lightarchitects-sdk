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

/// Run the agent in NDJSON streaming mode.
///
/// - Reads `ControlMessage` lines from stdin.
/// - Emits `AgentEvent` lines to stdout.
/// - Blocking; returns when stdin closes or an unrecoverable error occurs.
///
/// # Errors
///
/// Returns an error if the LLM client cannot be initialised from environment.
pub async fn run_ndjson(cwd: &Path) -> Result<(), Box<dyn std::error::Error>> {
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
