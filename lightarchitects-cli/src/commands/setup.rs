//! `lightarchitects setup` — interactive API key configuration wizard.
//!
//! Walks through required, recommended, and optional API keys tier by tier.
//! For each key, prints the sign-up URL and prompts with hidden input.
//! Keys are written to `~/.lightarchitects/keys.toml` (chmod 600).
//! The gateway reads this file at startup and injects keys into sibling
//! processes for any key not already present in the process environment.

use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;
use std::io::Write as _;
use std::path::PathBuf;

use lightarchitects_core::SdkError;

use crate::output::OutputMode;

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
];

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

/// Run the interactive setup wizard.
///
/// Reads existing keys from `~/.lightarchitects/keys.toml`, prompts for any
/// missing or to-be-updated keys (hidden input), and writes the result back.
///
/// # Errors
///
/// Returns [`SdkError::Config`] if the keys file cannot be read or written.
pub fn execute(_mode: OutputMode) -> Result<(), SdkError> {
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| SdkError::Config("HOME environment variable is not set".to_owned()))?;
    let keys_path = home.join(".lightarchitects").join("keys.toml");

    // Load existing keys (if any) — preserve anything already configured.
    let mut keys: BTreeMap<String, String> = if keys_path.exists() {
        let content = std::fs::read_to_string(&keys_path)
            .map_err(|e| SdkError::Config(format!("cannot read keys.toml: {e}")))?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        BTreeMap::new()
    };

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
                continue;
            }

            // Mirror to alias before moving value.
            if let Some(alias) = spec.alias {
                keys.insert(alias.to_owned(), value.clone());
            }
            keys.insert(spec.env_var.to_owned(), value);
            any_written = true;
        }
        println!();
    }

    if !any_written {
        println!("  No changes.");
        return Ok(());
    }

    match storage {
        KeysStorage::File => {
            write_keys_file(&keys_path, &keys)?;
            println!("  ✓ Keys written to {}", keys_path.display());
        }
        KeysStorage::Keyring => match try_write_to_keyring(&keys) {
            Ok(()) => println!("  ✓ Keys saved to OS Keychain."),
            Err(e) => {
                println!("  ! Keychain unavailable ({e}) — falling back to keys.toml.");
                write_keys_file(&keys_path, &keys)?;
                println!("  ✓ Keys written to {}", keys_path.display());
            }
        },
    }

    println!("  Run `lightarchitects status` to verify sibling binaries are present.\n");

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
