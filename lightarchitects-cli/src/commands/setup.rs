//! `lightarchitects setup` — interactive configuration wizard.
//!
//! Supports component-scoped setup:
//! - Full wizard:    `lightarchitects setup`
//! - Keys only:      `lightarchitects setup keys`
//! - Single key:     `lightarchitects setup keys --key MISTRAL_API_KEY`
//! - Voice only:     `lightarchitects setup voice`
//!
//! Keys are written to `~/.lightarchitects/keys.toml` (chmod 600).
//! Voice provider is set via `soul.toml [voice.engine] provider`.

use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;
use std::io::Write as _;
use std::path::PathBuf;

use clap::Subcommand;
use lightarchitects_core::SdkError;

use crate::output::OutputMode;

// ── Component subcommands ─────────────────────────────────────────────────────

/// Which component to configure interactively.
#[derive(Debug, Subcommand)]
pub enum SetupComponent {
    /// Configure API keys (required, recommended, optional).
    ///
    /// Without `--key`, walks through all tiers.
    /// With `--key NAME`, prompts for that single key only.
    Keys {
        /// Configure a single named key (e.g. `MISTRAL_API_KEY`).
        #[arg(long)]
        key: Option<String>,
    },
    /// Configure voice synthesis provider and sibling voice profiles.
    ///
    /// Interactively select the TTS provider (`ElevenLabs` / `Cartesia` / `Voxtral`)
    /// and verify that the required credentials and voice IDs are in place.
    Voice,
}

// ── Key catalogue ─────────────────────────────────────────────────────────────

/// Where the setup wizard should write collected API keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeysStorage {
    /// OS-native keychain: macOS Keychain, Linux Secret Service, Windows Credential Store.
    Keyring,
    /// Plain file at `~/.lightarchitects/keys.toml` (chmod 600).
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tier {
    Required,
    Recommended,
    Optional,
}

struct KeySpec {
    env_var: &'static str,
    description: &'static str,
    used_by: &'static str,
    tier: Tier,
    /// Direct link to the sign-up / API key page for this service.
    signup_url: &'static str,
    /// Secondary env var that should mirror this key's value.
    ///
    /// Example: `OLLAMA_CLOUD_API_KEY` and `OLLAMA_API_KEY` are the same
    /// credential referenced by different siblings under different names.
    alias: Option<&'static str>,
}

static KEY_SPECS: &[KeySpec] = &[
    // ── Required ──────────────────────────────────────────────────────────────
    KeySpec {
        env_var: "ANTHROPIC_API_KEY",
        description: "Claude API — primary AI for all siblings",
        used_by: "CORSO, EVA, QUANTUM",
        tier: Tier::Required,
        signup_url: "https://console.anthropic.com/settings/keys",
        alias: None,
    },
    KeySpec {
        env_var: "ELEVENLABS_API_KEY",
        description: "Voice synthesis — speak, converse, and dialogue actions",
        used_by: "SOUL",
        tier: Tier::Required,
        signup_url: "https://elevenlabs.io/app/settings/api-keys",
        alias: None,
    },
    KeySpec {
        env_var: "OLLAMA_API_KEY",
        description: "Ollama Cloud — AI inference fallback tier for EVA and QUANTUM",
        used_by: "EVA, QUANTUM",
        tier: Tier::Required,
        signup_url: "https://ollama.ai",
        // QUANTUM references the same credential under a different name.
        alias: Some("OLLAMA_CLOUD_API_KEY"),
    },
    // ── Recommended ───────────────────────────────────────────────────────────
    KeySpec {
        env_var: "PERPLEXITY_API_KEY",
        description: "Web research — Perplexity search in QUANTUM investigation cycle",
        used_by: "QUANTUM",
        tier: Tier::Recommended,
        signup_url: "https://www.perplexity.ai/settings/api",
        alias: None,
    },
    KeySpec {
        env_var: "BRAVE_API_KEY",
        description: "Web search — Brave Search API as a QUANTUM research signal",
        used_by: "QUANTUM",
        tier: Tier::Recommended,
        signup_url: "https://brave.com/search/api/",
        alias: None,
    },
    KeySpec {
        env_var: "HF_TOKEN",
        description: "HuggingFace — Genesis-14B inference bridge and model downloads",
        used_by: "SOUL",
        tier: Tier::Recommended,
        signup_url: "https://huggingface.co/settings/tokens",
        alias: None,
    },
    // ── Optional ──────────────────────────────────────────────────────────────
    KeySpec {
        env_var: "EXA_API_KEY",
        description: "Exa semantic search — additional QUANTUM research signal",
        used_by: "QUANTUM",
        tier: Tier::Optional,
        signup_url: "https://exa.ai/",
        alias: None,
    },
    KeySpec {
        env_var: "TAVILY_API_KEY",
        description: "Tavily search — additional QUANTUM research signal",
        used_by: "QUANTUM",
        tier: Tier::Optional,
        signup_url: "https://app.tavily.com/",
        alias: None,
    },
    KeySpec {
        env_var: "CARTESIA_API_KEY",
        description: "Cartesia TTS — alternative voice provider to ElevenLabs",
        used_by: "SOUL",
        tier: Tier::Optional,
        signup_url: "https://play.cartesia.ai/",
        alias: None,
    },
    KeySpec {
        env_var: "OPENAI_API_KEY",
        description: "OpenAI — EVA image generation (visualize action)",
        used_by: "EVA",
        tier: Tier::Optional,
        signup_url: "https://platform.openai.com/api-keys",
        alias: None,
    },
    KeySpec {
        env_var: "MISTRAL_API_KEY",
        description: "Mistral — Voxtral TTS voice synthesis (zero-shot voice cloning, 9 languages)",
        used_by: "SOUL",
        tier: Tier::Optional,
        signup_url: "https://console.mistral.ai/api-keys",
        alias: None,
    },
];

// ── Private helpers ───────────────────────────────────────────────────────────

fn home_dir() -> Result<PathBuf, SdkError> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| SdkError::Config("HOME environment variable is not set".to_owned()))
}

fn load_keys_file(path: &std::path::Path) -> Result<BTreeMap<String, String>, SdkError> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| SdkError::Config(format!("cannot read keys.toml: {e}")))?;
    toml::from_str(&content).map_err(|e| SdkError::Config(format!("keys.toml is malformed: {e}")))
}

/// Write the key map to `path` (or keychain) and print a confirmation line.
fn save_keys(
    storage: KeysStorage,
    keys_path: &std::path::Path,
    keys: &BTreeMap<String, String>,
) -> Result<(), SdkError> {
    match storage {
        KeysStorage::File => {
            write_keys_file(keys_path, keys)?;
            println!("  ✓ Keys written to {}", keys_path.display());
        }
        KeysStorage::Keyring => match try_write_to_keyring(keys) {
            Ok(()) => println!("  ✓ Keys saved to OS Keychain."),
            Err(e) => {
                println!("  ! Keychain unavailable ({e}) — falling back to keys.toml.");
                write_keys_file(keys_path, keys)?;
                println!("  ✓ Keys written to {}", keys_path.display());
            }
        },
    }
    Ok(())
}

/// Display a prompt for a single key and update `keys` if the user enters a value.
///
/// Returns `true` when the user entered a non-empty value (key was written).
fn prompt_key_spec(spec: &KeySpec, keys: &mut BTreeMap<String, String>) -> Result<bool, SdkError> {
    let in_env = std::env::var(spec.env_var).is_ok();
    let in_keys = keys.contains_key(spec.env_var);
    let status = if in_env {
        " [set via environment]"
    } else if in_keys {
        " [already configured]"
    } else {
        ""
    };

    println!();
    println!("  {}{}  ({})", spec.env_var, status, spec.used_by);
    println!("  {}", spec.description);
    println!("  Get yours → {}", spec.signup_url);
    print!("  > ");
    std::io::stdout()
        .flush()
        .map_err(|e| SdkError::Config(format!("stdout flush: {e}")))?;

    let value = rpassword::read_password()
        .map_err(|e| SdkError::Config(format!("failed to read input: {e}")))?;

    if value.is_empty() {
        return Ok(false);
    }

    if let Some(alias) = spec.alias {
        keys.insert(alias.to_owned(), value.clone());
    }
    keys.insert(spec.env_var.to_owned(), value);
    Ok(true)
}

// ── Storage backend prompt ────────────────────────────────────────────────────

/// Ask the user where API keys should be stored.
///
/// Defaults to [`KeysStorage::Keyring`] (OS-native, most secure) when the
/// user presses Enter without input.
fn prompt_storage_backend() -> Result<KeysStorage, SdkError> {
    println!("  Where should keys be stored?");
    println!(
        "  1. OS Keychain  (macOS Keychain · Linux Secret Service · Windows Credential Store)  [default]"
    );
    println!("  2. ~/.lightarchitects/keys.toml  (filesystem, chmod 600)");
    print!("  Choice [1]: ");
    std::io::stdout()
        .flush()
        .map_err(|e| SdkError::Config(format!("stdout flush: {e}")))?;
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| SdkError::Config(format!("failed to read choice: {e}")))?;
    if input.trim() == "2" {
        Ok(KeysStorage::File)
    } else {
        Ok(KeysStorage::Keyring)
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Route to the appropriate setup wizard based on the optional component.
///
/// # Errors
///
/// Returns [`SdkError::Config`] if any configuration cannot be read or written.
pub fn execute(component: Option<SetupComponent>, _mode: OutputMode) -> Result<(), SdkError> {
    match component {
        None => {
            // Full wizard: keys first, then voice
            println!("\n  Light Architects Full Setup");
            println!("  ─────────────────────────────────────────────");
            run_keys_wizard(None)?;
            println!();
            run_voice_wizard()
        }
        Some(SetupComponent::Keys { key }) => run_keys_wizard(key.as_deref()),
        Some(SetupComponent::Voice) => run_voice_wizard(),
    }
}

// ── Keys wizard ───────────────────────────────────────────────────────────────

/// Run the API keys wizard.
///
/// When `single_key` is `Some("KEY_NAME")`, delegates to [`run_single_key_wizard`].
/// When `None`, walks all tiers (required → recommended → optional).
fn run_keys_wizard(single_key: Option<&str>) -> Result<(), SdkError> {
    if let Some(key_name) = single_key {
        return run_single_key_wizard(key_name);
    }

    let home = home_dir()?;
    let keys_path = home.join(".lightarchitects").join("keys.toml");
    let mut keys = load_keys_file(&keys_path)?;

    println!("\n  Light Architects Setup");
    println!("  ─────────────────────────────────────────────");
    println!("  Configures API keys for all siblings.");
    println!("  Input is hidden — paste your key and press Enter.");
    println!("  Press Enter with no input to skip a key.\n");

    let storage = prompt_storage_backend()?;
    println!();

    let mut any_written = false;

    for tier in [Tier::Required, Tier::Recommended, Tier::Optional] {
        let tier_label = match tier {
            Tier::Required => "Required  — core features",
            Tier::Recommended => "Recommended — significant functionality",
            Tier::Optional => "Optional  — additional research signals",
        };
        println!("  ── {tier_label} ─────────────────────────────");

        for spec in KEY_SPECS.iter().filter(|s| s.tier == tier) {
            if prompt_key_spec(spec, &mut keys)? {
                any_written = true;
            }
        }
        println!();
    }

    if !any_written {
        println!("  No changes.");
        return Ok(());
    }

    save_keys(storage, &keys_path, &keys)?;
    println!("  Run `lightarchitects status` to verify sibling binaries are present.\n");
    Ok(())
}

/// Prompt for a single named key and write it to the chosen storage backend.
fn run_single_key_wizard(key_name: &str) -> Result<(), SdkError> {
    let home = home_dir()?;
    let keys_path = home.join(".lightarchitects").join("keys.toml");
    let mut keys = load_keys_file(&keys_path)?;

    let spec = KEY_SPECS
        .iter()
        .find(|s| s.env_var.eq_ignore_ascii_case(key_name))
        .ok_or_else(|| {
            SdkError::Config(format!(
                "unknown key '{key_name}'. \
                 Run `lightarchitects setup keys` to walk all keys."
            ))
        })?;

    println!("\n  Light Architects Setup — {}", spec.env_var);
    println!("  ─────────────────────────────────────────────");
    println!("  Input is hidden — paste your key and press Enter.\n");

    let storage = prompt_storage_backend()?;
    println!();

    let written = prompt_key_spec(spec, &mut keys)?;

    if !written {
        println!("  No changes.");
        return Ok(());
    }

    save_keys(storage, &keys_path, &keys)?;
    println!("  Run `lightarchitects status` to verify sibling binaries are present.\n");
    Ok(())
}

// ── Voice wizard ──────────────────────────────────────────────────────────────

/// Run the interactive voice provider wizard.
///
/// Reads the current provider from `~/.soul/config/soul.toml [voice.engine]`,
/// shows credential status for each provider, and writes the user's selection
/// back to `soul.toml` without disturbing comments or other settings.
fn run_voice_wizard() -> Result<(), SdkError> {
    let home = home_dir()?;
    let soul_toml = home.join(".soul").join("config").join("soul.toml");

    let content = std::fs::read_to_string(&soul_toml)
        .map_err(|e| SdkError::Config(format!("cannot read soul.toml: {e}")))?;
    let current = read_voice_engine_provider(&content);

    let el = credential_status(&home, "elevenlabs.key", "ELEVENLABS_API_KEY");
    let ca = credential_status(&home, "cartesia.key", "CARTESIA_API_KEY");
    let vx = mistral_credential_status(&home);

    println!("\n  Voice Provider Setup");
    println!("  ─────────────────────────────────────────────");
    println!("  Current provider: {current}");
    println!();
    println!("  1. ElevenLabs   — premium neural voices, audio tags, multi-language  [{el}]");
    println!("  2. Cartesia     — instant voice clones, <150ms latency               [{ca}]");
    println!("  3. Voxtral      — Mistral zero-shot voice cloning, 9 languages       [{vx}]");
    println!("  4. Auto         — best available (Voxtral → ElevenLabs → Cartesia)");
    println!("  5. Disabled     — silence all voice synthesis");
    println!("  6. Keep current  [{current}]  [default]");

    print!("\n  Choice [6]: ");
    std::io::stdout()
        .flush()
        .map_err(|e| SdkError::Config(format!("stdout flush: {e}")))?;

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| SdkError::Config(format!("failed to read choice: {e}")))?;

    let new_provider = match input.trim() {
        "1" => "elevenlabs",
        "2" => "cartesia",
        "3" => "voxtral",
        "4" => "auto",
        "5" => "disabled",
        _ => {
            println!("  No change.");
            return Ok(());
        }
    };

    if new_provider == current.as_str() {
        println!("  Provider unchanged.");
        return Ok(());
    }

    update_soul_toml_provider(&soul_toml, &content, new_provider)?;
    println!("  ✓ Voice provider set to: {new_provider}");

    if new_provider == "voxtral" && vx == "NOT configured" {
        println!(
            "  ℹ  MISTRAL_API_KEY not found. \
             Run `lightarchitects setup keys --key MISTRAL_API_KEY` to add it."
        );
    }

    println!("  Run `lightarchitects status` to verify sibling binaries are present.\n");
    Ok(())
}

/// Extract the `provider` value from `[voice.engine]` in a `soul.toml` string.
///
/// Falls back to `"elevenlabs"` if the section or key is absent.
fn read_voice_engine_provider(content: &str) -> String {
    let mut in_engine = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_engine = t == "[voice.engine]";
        }
        if in_engine && t.starts_with("provider") {
            if let Some(raw) = t.split('=').nth(1) {
                // Strip optional inline comment: `"elevenlabs"  # comment` → `elevenlabs`
                let without_comment = raw.split_once('#').map_or(raw, |(v, _)| v);
                return without_comment.trim().trim_matches('"').to_owned();
            }
        }
    }
    "elevenlabs".to_owned()
}

/// Check whether a Soul voice provider key is available.
///
/// Checks environment variable first, then `~/.soul/config/<key_file>`.
fn credential_status(home: &PathBuf, key_file: &str, env_var: &str) -> &'static str {
    if std::env::var(env_var).is_ok() {
        "set via env"
    } else if home.join(".soul").join("config").join(key_file).exists() {
        "key file found"
    } else {
        "NOT configured"
    }
}

/// Check whether `MISTRAL_API_KEY` is available (env, keys.toml, or keychain).
fn mistral_credential_status(home: &PathBuf) -> &'static str {
    if std::env::var("MISTRAL_API_KEY").is_ok() {
        return "set via env";
    }
    let path = home.join(".lightarchitects").join("keys.toml");
    if let Ok(c) = std::fs::read_to_string(&path) {
        if c.contains("MISTRAL_API_KEY") {
            return "keys.toml found";
        }
    }
    "NOT configured"
}

/// Rewrite the `provider` field in `[voice.engine]` without disturbing comments.
///
/// Performs a targeted line-by-line substitution so the rest of `soul.toml`
/// (comments, spacing, other sections) is preserved exactly.
fn update_soul_toml_provider(
    path: &std::path::Path,
    content: &str,
    new_provider: &str,
) -> Result<(), SdkError> {
    let mut in_engine = false;
    let mut replaced = false;
    let mut out = String::with_capacity(content.len());

    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_engine = t == "[voice.engine]";
        }
        if in_engine && !replaced && t.starts_with("provider") {
            // Preserve inline comment if present.
            let comment = line
                .find('#')
                .map(|i| format!("  #{}", &line[i + 1..]))
                .unwrap_or_default();
            write!(out, "provider = \"{new_provider}\"").expect("write to String is infallible");
            out.push_str(&comment);
            out.push('\n');
            replaced = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }

    if !replaced {
        return Err(SdkError::Config(
            "[voice.engine] provider not found in soul.toml — is the file well-formed?".to_owned(),
        ));
    }

    std::fs::write(path, out)
        .map_err(|e| SdkError::Config(format!("cannot update soul.toml: {e}")))?;

    Ok(())
}

// ── Writers ───────────────────────────────────────────────────────────────────

/// Write each key to the OS keychain under the `"lightarchitects"` service.
///
/// Returns the underlying `keyring` error so the caller can fall back to the
/// file writer rather than hard-failing on headless systems.
fn try_write_to_keyring(keys: &BTreeMap<String, String>) -> Result<(), keyring::Error> {
    for (name, value) in keys {
        keyring::Entry::new("lightarchitects", name)?.set_password(value)?;
    }
    Ok(())
}

fn write_keys_file(
    path: &std::path::Path,
    keys: &BTreeMap<String, String>,
) -> Result<(), SdkError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| SdkError::Config(format!("cannot create config directory: {e}")))?;
    }

    let mut content = String::from(
        "# Light Architects API keys — written by `lightarchitects setup`.\n\
         # Keep this file secure. Do not commit to version control.\n\
         # Keys here are injected into sibling processes at spawn time.\n\
         # Environment variables always take priority over values in this file.\n\n",
    );
    for (k, v) in keys {
        // Use toml::Value::String for proper escaping of special characters.
        let _ = writeln!(content, "{k} = {}", toml::Value::String(v.clone()));
    }

    std::fs::write(path, &content)
        .map_err(|e| SdkError::Config(format!("cannot write keys.toml: {e}")))?;

    // Restrict to owner-only read/write (0600) — this file contains credentials.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms)
            .map_err(|e| SdkError::Config(format!("cannot set file permissions: {e}")))?;
    }

    Ok(())
}
