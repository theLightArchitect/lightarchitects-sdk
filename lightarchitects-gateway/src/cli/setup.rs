//! `lightarchitects setup` — interactive configuration wizard.
//!
//! Supports component-scoped setup:
//! - `lightarchitects setup keys [--key NAME]` — configure API keys
//! - `lightarchitects setup voice` — configure voice synthesis provider
//! - `lightarchitects setup seraph [--force]` — configure SERAPH engagement scope
//!
//! Keys are written to `~/.lightarchitects/keys.toml` (chmod 600) or the OS keyring.
//! SERAPH scope is written to `~/lightarchitects/seraph/scope.toml` (chmod 600).

use lightarchitects::seraph::scope::{ScopeConstraint, ScopeDomain};
use std::io::Write as _;

use crate::error::GatewayError;

/// Execute a setup subcommand.
///
/// # Errors
///
/// Returns [`GatewayError`] if I/O operations fail or user input is invalid.
pub fn execute(args: &[String]) -> Result<(), GatewayError> {
    match args.first().map(String::as_str) {
        Some("keys") => cmd_keys(args),
        Some("voice") => {
            cmd_voice();
            Ok(())
        }
        Some("seraph") => cmd_seraph(args),
        Some(other) => {
            eprintln!("Unknown setup component: {other}");
            eprintln!("Available: keys, voice, seraph");
            Err(GatewayError::UnknownTool(other.to_owned()))
        }
        None => {
            eprintln!("Usage: lightarchitects setup <keys|voice|seraph>");
            eprintln!("  keys [--key NAME]  Configure API keys");
            eprintln!("  voice              Configure voice synthesis");
            eprintln!("  seraph [--force]   Configure SERAPH scope");
            Ok(())
        }
    }
}

// ── Key catalogue ──────────────────────────────────────────────────────────

/// Key specification: name, tier, and description.
struct KeySpec {
    name: &'static str,
    tier: KeyTier,
    description: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyTier {
    Required,
    Recommended,
    Optional,
}

const KEY_SPECS: &[KeySpec] = &[
    KeySpec {
        name: "ANTHROPIC_API_KEY",
        tier: KeyTier::Required,
        description: "Anthropic Claude API",
    },
    KeySpec {
        name: "ELEVENLABS_API_KEY",
        tier: KeyTier::Required,
        description: "ElevenLabs TTS",
    },
    KeySpec {
        name: "OLLAMA_API_KEY",
        tier: KeyTier::Required,
        description: "Ollama Cloud",
    },
    KeySpec {
        name: "PERPLEXITY_API_KEY",
        tier: KeyTier::Recommended,
        description: "Perplexity research",
    },
    KeySpec {
        name: "BRAVE_API_KEY",
        tier: KeyTier::Recommended,
        description: "Brave Search API",
    },
    KeySpec {
        name: "HF_TOKEN",
        tier: KeyTier::Recommended,
        description: "Hugging Face models",
    },
    KeySpec {
        name: "EXA_API_KEY",
        tier: KeyTier::Optional,
        description: "Exa search",
    },
    KeySpec {
        name: "TAVILY_API_KEY",
        tier: KeyTier::Optional,
        description: "Tavily research",
    },
    KeySpec {
        name: "CARTESIA_API_KEY",
        tier: KeyTier::Optional,
        description: "Cartesia TTS",
    },
    KeySpec {
        name: "OPENAI_API_KEY",
        tier: KeyTier::Optional,
        description: "OpenAI models",
    },
    KeySpec {
        name: "MISTRAL_API_KEY",
        tier: KeyTier::Optional,
        description: "Mistral models",
    },
];

fn cmd_keys(args: &[String]) -> Result<(), GatewayError> {
    // If --key NAME is specified, prompt for just that key
    let single_key = if args.len() > 1 && args[1] == "--key" {
        args.get(2).cloned()
    } else {
        None
    };

    if let Some(key_name) = single_key {
        prompt_and_store_key(&key_name)?;
    } else {
        // Walk through all keys by tier
        let tiers = [
            (KeyTier::Required, "Required"),
            (KeyTier::Recommended, "Recommended"),
            (KeyTier::Optional, "Optional"),
        ];

        for (tier, label) in tiers {
            println!("\n--- {label} keys ---");
            for spec in KEY_SPECS.iter().filter(|s| s.tier == tier) {
                prompt_and_store_key(spec.name)?;
            }
        }
    }

    println!("\nKeys written to ~/.lightarchitects/keys.toml");
    Ok(())
}

fn prompt_and_store_key(key_name: &str) -> Result<(), GatewayError> {
    let spec = KEY_SPECS.iter().find(|s| s.name == key_name);
    let description = spec.map_or("API key", |s| s.description);

    print!("{key_name} ({description}): ");
    std::io::stdout()
        .flush()
        .map_err(|e| GatewayError::Internal(format!("flush error: {e}")))?;

    let value = rpassword::read_password()
        .map_err(|e| GatewayError::Internal(format!("failed to read password: {e}")))?;

    if value.is_empty() {
        println!("  (skipped)");
        return Ok(());
    }

    // Try OS keyring first
    if let Ok(entry) = keyring::Entry::new("lightarchitects", key_name) {
        if entry.set_password(&value).is_ok() {
            println!("  ✓ stored in OS keyring");
            return Ok(());
        }
    }

    // Fallback to keys.toml file
    write_key_to_file(key_name, &value)?;
    println!("  ✓ stored in ~/.lightarchitects/keys.toml");
    Ok(())
}

fn write_key_to_file(key_name: &str, value: &str) -> Result<(), GatewayError> {
    let home =
        std::env::var_os("HOME").ok_or(GatewayError::Config(crate::error::ConfigError::NoHome))?;
    let dir = std::path::PathBuf::from(&home).join(".lightarchitects");
    std::fs::create_dir_all(&dir)
        .map_err(|e| GatewayError::Internal(format!("mkdir error: {e}")))?;

    let path = dir.join("keys.toml");

    // Read existing keys or start fresh
    let mut keys: std::collections::HashMap<String, String> = if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| GatewayError::Internal(format!("read error: {e}")))?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        std::collections::HashMap::new()
    };

    keys.insert(key_name.to_owned(), value.to_owned());

    let toml_str = toml::to_string_pretty(&keys)
        .map_err(|e| GatewayError::Internal(format!("serialize error: {e}")))?;

    // Atomic write: tmp file + rename
    let tmp_path = path.with_extension("toml.tmp");
    {
        let mut file = std::fs::File::create(&tmp_path)
            .map_err(|e| GatewayError::Internal(format!("create error: {e}")))?;
        // Set permissions to 0600 (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            file.set_permissions(perms)
                .map_err(|e| GatewayError::Internal(format!("chmod error: {e}")))?;
        }
        file.write_all(toml_str.as_bytes())
            .map_err(|e| GatewayError::Internal(format!("write error: {e}")))?;
    }

    std::fs::rename(&tmp_path, &path)
        .map_err(|e| GatewayError::Internal(format!("rename error: {e}")))?;

    Ok(())
}

fn cmd_voice() {
    println!("Voice configuration wizard (placeholder)");
    println!("This will be implemented in a future release.");
    println!("Configure voice provider in: ~/lightarchitects/soul/config/soul.toml [voice.engine]");
}

fn cmd_seraph(args: &[String]) -> Result<(), GatewayError> {
    let force = args.iter().any(|a| a == "--force");

    let home =
        std::env::var_os("HOME").ok_or(GatewayError::Config(crate::error::ConfigError::NoHome))?;
    let scope_path = std::path::PathBuf::from(&home)
        .join("lightarchitects")
        .join("seraph")
        .join("scope.toml");

    // Check if scope file already exists
    if scope_path.exists() && !force {
        println!(
            "SERAPH scope file already exists at: {}",
            scope_path.display()
        );
        println!("Use --force to reconfigure.");
        return Ok(());
    }

    println!("SERAPH scope configuration wizard");
    println!("Target domain (web, network, cloud, physical, social):");

    let mut domain_input = String::new();
    std::io::stdin()
        .read_line(&mut domain_input)
        .map_err(|e| GatewayError::Internal(format!("read error: {e}")))?;
    let domain = domain_input.trim();

    println!("Authorized target (e.g., 192.168.1.0/24, *.example.com):");
    let mut target_input = String::new();
    std::io::stdin()
        .read_line(&mut target_input)
        .map_err(|e| GatewayError::Internal(format!("read error: {e}")))?;

    // Validate via ScopeConstraint
    let scope_domain = match domain {
        "web" => ScopeDomain::Web,
        "network" => ScopeDomain::Network,
        "cloud" => ScopeDomain::Cloud,
        "physical" => ScopeDomain::Physical,
        "social" => ScopeDomain::Social,
        other => {
            eprintln!(
                "Unknown domain: {other}. Must be one of: web, network, cloud, physical, social"
            );
            return Err(GatewayError::InvalidParam(format!(
                "invalid domain: {other}"
            )));
        }
    };

    let target = target_input.trim();
    ScopeConstraint::new(target, "scan", scope_domain)
        .map_err(|e| GatewayError::InvalidParam(format!("scope validation failed: {e}")))?;

    // Write scope.toml
    let scope_content = format!("[scope]\ntarget = \"{target}\"\ndomain = \"{domain}\"\n");

    if let Some(parent) = scope_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| GatewayError::Internal(format!("mkdir error: {e}")))?;
    }

    atomic_write(&scope_path, &scope_content)?;

    println!("✓ SERAPH scope written to: {}", scope_path.display());
    Ok(())
}

/// Atomic file write: tmp file + rename with 0600 permissions.
fn atomic_write(path: &std::path::Path, content: &str) -> Result<(), GatewayError> {
    let tmp_path = path.with_extension("toml.tmp");
    {
        let mut file = std::fs::File::create(&tmp_path)
            .map_err(|e| GatewayError::Internal(format!("create error: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            file.set_permissions(perms)
                .map_err(|e| GatewayError::Internal(format!("chmod error: {e}")))?;
        }
        file.write_all(content.as_bytes())
            .map_err(|e| GatewayError::Internal(format!("write error: {e}")))?;
    }

    std::fs::rename(&tmp_path, path)
        .map_err(|e| GatewayError::Internal(format!("rename error: {e}")))?;
    Ok(())
}
