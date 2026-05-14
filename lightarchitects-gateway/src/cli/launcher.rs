//! Interactive launcher onboarding + preference persistence.
//!
//! When `lightarchitects` is run from a TTY with no arguments, the gateway
//! reads `~/.lightarchitects/launcher.toml`.  If it is missing (first run),
//! an interactive wizard asks four questions, writes the file, then acts on
//! the answers immediately.
//!
//! ```toml
//! [launcher]
//! always_webshell = true
//! sandbox = false
//! backend = "claude"    # "claude" | "codex" | "ollama_launch"
//! model   = "claude-sonnet-4-6"
//! ```

use std::io::{self, Write as _};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Backend coding agent selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// Anthropic Claude Code CLI.
    Claude,
    /// `OpenAI` Codex CLI.
    Codex,
    /// Ollama persistent subprocess (replicates `ollama launch claude`).
    OllamaLaunch,
}

impl Default for Backend {
    fn default() -> Self {
        Self::Claude
    }
}

impl std::fmt::Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Claude => write!(f, "claude"),
            Self::Codex => write!(f, "codex"),
            Self::OllamaLaunch => write!(f, "ollama_launch"),
        }
    }
}

/// Persisted launcher preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherConfig {
    /// If true, `lightarchitects` (no args) starts the webshell instead of
    /// execing `lightarchitects-cli`.
    #[serde(default)]
    pub always_webshell: bool,
    /// If true, sets `LA_CONTAINER_MODE=1` for the webshell container probe.
    #[serde(default)]
    pub sandbox: bool,
    /// Backend coding agent.
    #[serde(default)]
    pub backend: Backend,
    /// Model identifier (backend-specific).
    #[serde(default = "default_model")]
    pub model: String,
    /// Set to true after the wizard has run once.
    #[serde(default)]
    pub first_run_done: bool,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            always_webshell: false,
            sandbox: false,
            backend: Backend::default(),
            model: default_model(),
            first_run_done: false,
        }
    }
}

fn default_model() -> String {
    "claude-sonnet-4-6".to_owned()
}

/// Wrapper so the TOML serializes under `[launcher]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LauncherToml {
    launcher: LauncherConfig,
}

impl LauncherConfig {
    /// Load from `~/.lightarchitects/launcher.toml` or return defaults.
    ///
    /// Tries the `[launcher]` table format first, then falls back to the
    /// older flat key-value format for backward compatibility.
    #[must_use]
    pub fn load() -> Self {
        let Some(path) = launcher_path() else {
            return Self::default();
        };
        if !path.exists() {
            return Self::default();
        }
        let content = std::fs::read_to_string(&path).unwrap_or_default();

        // 1. Try new `[launcher]` format.
        if let Ok(wrapped) = toml::from_str::<LauncherToml>(&content) {
            return wrapped.launcher;
        }
        // 2. Try old flat format (backward compat).
        toml::from_str::<LauncherConfig>(&content).unwrap_or_default()
    }

    /// Atomically write to `~/.lightarchitects/launcher.toml` with 0o600.
    ///
    /// The parent directory is created with 0o700 if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the home directory or write fails.
    pub fn save(&self) -> io::Result<()> {
        let Some(path) = launcher_path() else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "$HOME not set"));
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let wrapped = LauncherToml {
            launcher: self.clone(),
        };
        let toml = toml::to_string_pretty(&wrapped)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &toml)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
}

fn launcher_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| {
        PathBuf::from(h)
            .join(".lightarchitects")
            .join("launcher.toml")
    })
}

// ── Interactive wizard ───────────────────────────────────────────────────────

/// Run the onboarding wizard on stdin/stdout.
///
/// Returns the filled config (already saved to disk).
pub fn run_onboarding() -> LauncherConfig {
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║      Welcome to Light Architects — First-time setup          ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // 1. Webshell preference
    let always_webshell = ask_yes_no(
        "Would you like to always open the webshell when running 'lightarchitects'?",
        false,
    );

    // 2. Sandbox
    let sandbox = ask_yes_no(
        "Would you like to use sandboxed (containerized) agents by default?",
        false,
    );

    // 3. Backend agent
    println!("\nWhich backend coding agent do you want to use?");
    println!("  1) Claude Code  (Anthropic Claude — default)");
    println!("  2) GitHub Copilot / Codex  (OpenAI)");
    println!("  3) Ollama Cloud  (self-hosted / third-party models)");
    let backend_choice = ask_number("Select backend", 1, 3, 1);
    let backend = match backend_choice {
        2 => Backend::Codex,
        3 => Backend::OllamaLaunch,
        _ => Backend::Claude,
    };

    // 4. Model
    let model = match backend {
        Backend::Claude => {
            println!("\nWhich Claude model would you like to use?");
            println!("  1) Claude Sonnet 4.6  (balanced, fast)");
            println!("  2) Claude Opus 4.7     (most capable, slower)");
            println!("  3) Claude Haiku 4.5    (lightweight)");
            match ask_number("Select model", 1, 3, 1) {
                2 => "claude-opus-4-7",
                3 => "claude-haiku-4-5",
                _ => "claude-sonnet-4-6",
            }
        }
        Backend::Codex => {
            println!("\nWhich Codex model would you like to use?");
            println!("  1) Codex Latest");
            println!("  2) Codex Mini");
            match ask_number("Select model", 1, 2, 1) {
                2 => "codex-mini",
                _ => "codex-latest",
            }
        }
        Backend::OllamaLaunch => {
            println!("\nWhich Ollama Cloud model would you like to use?");
            println!("  1) Qwen3 Coder 480B");
            println!("  2) Nemotron 3 Super");
            println!("  3) GLM-5");
            println!("  4) GPT-OSS 120B");
            match ask_number("Select model", 1, 4, 1) {
                2 => "nemotron-3-super:cloud",
                3 => "glm-5:cloud",
                4 => "gpt-oss:120b-cloud",
                _ => "qwen3-coder:480b-cloud",
            }
        }
    };

    let cfg = LauncherConfig {
        always_webshell,
        sandbox,
        backend,
        model: model.to_owned(),
        first_run_done: true,
    };

    if let Err(e) = cfg.save() {
        eprintln!("Warning: could not save launcher preferences: {e}");
    } else {
        println!("\nPreferences saved to ~/.lightarchitects/launcher.toml\n");
    }

    cfg
}

// ── Prompt helpers ───────────────────────────────────────────────────────────

fn ask_yes_no(prompt: &str, default: bool) -> bool {
    let default_str = if default { "Y/n" } else { "y/N" };
    loop {
        print!("{prompt} [{default_str}]: ");
        let _ = io::stdout().flush();
        let mut buf = String::new();
        if io::stdin().read_line(&mut buf).is_err() {
            return default;
        }
        let trimmed = buf.trim().to_lowercase();
        if trimmed.is_empty() {
            return default;
        }
        if trimmed.starts_with('y') {
            return true;
        }
        if trimmed.starts_with('n') {
            return false;
        }
        println!("Please answer y or n.");
    }
}

fn ask_number(prompt: &str, min: u8, max: u8, default: u8) -> u8 {
    loop {
        print!("{prompt} [{default}]: ");
        let _ = io::stdout().flush();
        let mut buf = String::new();
        if io::stdin().read_line(&mut buf).is_err() {
            return default;
        }
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            return default;
        }
        if let Ok(n) = trimmed.parse::<u8>() {
            if (min..=max).contains(&n) {
                return n;
            }
        }
        println!("Please enter a number between {min} and {max}.");
    }
}
