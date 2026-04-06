//! `lightarchitects setup` — interactive configuration wizard.
//!
//! Supports component-scoped setup:
//! - Full wizard:    `lightarchitects setup`
//! - Keys only:      `lightarchitects setup keys`
//! - Single key:     `lightarchitects setup keys --key MISTRAL_API_KEY`
//! - Voice only:     `lightarchitects setup voice`
//! - SERAPH scope:   `lightarchitects setup seraph`
//!
//! Keys are written to `~/.lightarchitects/keys.toml` (chmod 600).
//! Voice provider is set via `soul.toml [voice.engine] provider`.
//! SERAPH scope is written to `~/.seraph/scope.toml` (chmod 600).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use lightarchitects_core::SdkError;
use lightarchitects_seraph::scope::{ScopeConstraint, ScopeDomain};

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
    /// Configure SERAPH engagement scope (`~/.seraph/scope.toml`).
    ///
    /// Prompts for an authorized target, tool, and engagement domain, validates
    /// each input through `ScopeConstraint::new()`, then writes
    /// `~/.seraph/scope.toml` with `0600` permissions using an atomic
    /// tmp-file + rename pattern.
    ///
    /// If `~/.seraph/scope.toml` already exists and is valid TOML, the wizard
    /// prints the current target and exits without overwriting.  Pass `--force`
    /// to re-run the wizard even when a valid scope file is present.
    ///
    /// Allowed tools include: nmap, masscan, nuclei, nikto, gobuster, rustscan,
    /// tshark, tcpdump, theHarvester, and the full SERAPH wings/services list.
    ///
    /// Allowed domains: web, network, cloud, physical, social.
    Seraph {
        /// Overwrite an existing `~/.seraph/scope.toml` without prompting.
        #[arg(long)]
        force: bool,
    },
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
            // Full wizard: keys → voice → SERAPH scope
            println!("\n  Light Architects Full Setup");
            println!("  ─────────────────────────────────────────────");
            run_keys_wizard(None)?;
            println!();
            run_voice_wizard()?;
            println!();
            run_seraph_scope_wizard(false)
        }
        Some(SetupComponent::Keys { key }) => run_keys_wizard(key.as_deref()),
        Some(SetupComponent::Voice) => run_voice_wizard(),
        Some(SetupComponent::Seraph { force }) => run_seraph_scope_wizard(force),
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
fn credential_status(home: &Path, key_file: &str, env_var: &str) -> &'static str {
    if std::env::var(env_var).is_ok() {
        "set via env"
    } else if home.join(".soul").join("config").join(key_file).exists() {
        "key file found"
    } else {
        "NOT configured"
    }
}

/// Check whether `MISTRAL_API_KEY` is available (env, keys.toml, or keychain).
fn mistral_credential_status(home: &Path) -> &'static str {
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
            let _ = write!(out, "provider = \"{new_provider}\"");
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

// ── SERAPH scope wizard ───────────────────────────────────────────────────────

/// Resolve `~/.seraph/scope.toml`.
fn seraph_scope_path() -> Result<PathBuf, SdkError> {
    Ok(home_dir()?.join(".seraph").join("scope.toml"))
}

/// Return `true` when the file at `path` exists and parses as valid TOML.
fn is_valid_toml_file(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|s| toml::from_str::<toml::Value>(&s).is_ok())
        .unwrap_or(false)
}

/// Parse the first `target` value from a `[[scope]]` array in a TOML string.
///
/// Returns `None` if the field is absent or the TOML is malformed.
fn extract_scope_target(content: &str) -> Option<String> {
    let val: toml::Value = toml::from_str(content).ok()?;
    val.get("scope")?
        .as_array()?
        .first()?
        .get("target")?
        .as_str()
        .map(ToOwned::to_owned)
}

/// Return `true` when the address string appears to be a publicly routable
/// IPv4/IPv6 address or hostname — i.e. not RFC 1918, not RFC 5737 (TEST-NET),
/// not RFC 6598 (CGN), not RFC 4193 (ULA), and not link-local.
///
/// This is a heuristic — it does not perform DNS resolution.  It is used only
/// to trigger a confirmation prompt, never to block a valid target.
fn looks_like_public_address(target: &str) -> bool {
    // Strip protocol prefix if present (e.g. "https://example.com").
    let host = target
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(target);

    // Private IPv4 ranges: 10/8, 172.16/12, 192.168/16.
    if host.starts_with("10.") || host.starts_with("192.168.") || is_in_172_16_range(host) {
        return false;
    }
    // RFC 5737 TEST-NET (documentation ranges).
    if host.starts_with("192.0.2.")
        || host.starts_with("198.51.100.")
        || host.starts_with("203.0.113.")
    {
        return false;
    }
    // RFC 6598 shared address space (carrier-grade NAT).
    if host.starts_with("100.") {
        if let Some(second) = host.split('.').nth(1) {
            if let Ok(n) = second.parse::<u8>() {
                if (64..=127).contains(&n) {
                    return false;
                }
            }
        }
    }
    // Link-local IPv4 (169.254/16) and loopback (handled by ScopeConstraint).
    if host.starts_with("169.254.") || host.starts_with("127.") {
        return false;
    }
    // IPv6 ULA (fc00::/7) and link-local (fe80::/10).
    let lower = host.to_lowercase();
    if lower.starts_with("fc") || lower.starts_with("fd") || lower.starts_with("fe80") {
        return false;
    }
    // If it looks like a bare IPv4 address, anything not caught above is public.
    // If it looks like a domain (contains a dot, no leading digit context matched),
    // treat it as potentially public.
    true
}

/// Check whether a dotted-quad string falls in the 172.16.0.0/12 range.
fn is_in_172_16_range(host: &str) -> bool {
    let mut parts = host.split('.');
    let a: Option<u8> = parts.next().and_then(|s| s.parse().ok());
    let b: Option<u8> = parts.next().and_then(|s| s.parse().ok());
    match (a, b) {
        (Some(172), Some(b)) => (16..=31).contains(&b),
        _ => false,
    }
}

/// Parse a `ScopeDomain` from a user-supplied string (case-insensitive).
fn parse_domain(s: &str) -> Option<ScopeDomain> {
    match s.trim().to_lowercase().as_str() {
        "web" => Some(ScopeDomain::Web),
        "network" => Some(ScopeDomain::Network),
        "cloud" => Some(ScopeDomain::Cloud),
        "physical" => Some(ScopeDomain::Physical),
        "social" => Some(ScopeDomain::Social),
        _ => None,
    }
}

/// Prompt the user for a non-empty trimmed line of input.
fn prompt_line(prompt: &str) -> Result<String, SdkError> {
    print!("{prompt}");
    std::io::stdout()
        .flush()
        .map_err(|e| SdkError::Config(format!("stdout flush: {e}")))?;
    let mut buf = String::new();
    std::io::stdin()
        .read_line(&mut buf)
        .map_err(|e| SdkError::Config(format!("failed to read input: {e}")))?;
    Ok(buf.trim().to_owned())
}

/// Prompt for a yes/no question; returns `true` only on explicit "y" or "Y".
fn prompt_yes_no(question: &str) -> Result<bool, SdkError> {
    let answer = prompt_line(question)?;
    Ok(matches!(answer.as_str(), "y" | "Y"))
}

/// Write `content` to `path` using an atomic tmp-file + rename pattern.
///
/// The temporary file is created in the same directory as `path` to guarantee
/// that rename is atomic (same filesystem).  Permissions are set to `0600`
/// before rename so the file is never world-readable even transiently.
///
/// # Errors
///
/// Returns [`SdkError::Config`] on any filesystem error.
fn atomic_write_0600(path: &Path, content: &str) -> Result<(), SdkError> {
    let parent = path
        .parent()
        .ok_or_else(|| SdkError::Config(format!("scope path has no parent: {}", path.display())))?;

    std::fs::create_dir_all(parent)
        .map_err(|e| SdkError::Config(format!("cannot create {}: {e}", parent.display())))?;

    // Write to a sibling temp file first.
    let tmp_path = path.with_extension("toml.tmp");
    std::fs::write(&tmp_path, content)
        .map_err(|e| SdkError::Config(format!("cannot write tmp file: {e}")))?;

    // Restrict permissions before the rename so the final path is never
    // world-readable even briefly.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| SdkError::Config(format!("cannot chmod tmp file: {e}")))?;
    }

    std::fs::rename(&tmp_path, path)
        .map_err(|e| SdkError::Config(format!("cannot rename tmp to scope.toml: {e}")))?;

    Ok(())
}

/// Render the `scope.toml` content from validated fields.
fn render_scope_toml(target: &str, tool: &str, domain: &str) -> String {
    format!(
        "# SERAPH scope authorization — generated by lightarchitects setup\n\
         # Edit manually to add multiple targets or tools\n\
         \n\
         [[scope]]\n\
         target = {target_toml}\n\
         tool = {tool_toml}\n\
         domain = {domain_toml}\n",
        target_toml = toml::Value::String(target.to_owned()),
        tool_toml = toml::Value::String(tool.to_owned()),
        domain_toml = toml::Value::String(domain.to_owned()),
    )
}

/// Run the SERAPH scope setup wizard.
///
/// If `~/.seraph/scope.toml` already exists and is valid TOML, prints the
/// current target and returns immediately unless `force` is `true`.
///
/// The wizard prompts for target, tool, and domain, validates each through
/// [`ScopeConstraint::new`], warns on public addresses, then writes
/// `~/.seraph/scope.toml` via atomic tmp+rename with `0600` permissions.
fn run_seraph_scope_wizard(force: bool) -> Result<(), SdkError> {
    let scope_path = seraph_scope_path()?;

    println!("\n  SERAPH Scope Setup");
    println!("  ─────────────────────────────────────────────");
    println!("  Configures the authorized engagement scope for SERAPH pentest operations.");
    println!("  Written to: {}", scope_path.display());
    println!("  Permissions: 0600 (owner read/write only)\n");

    // Idempotency check.
    if !force && scope_path.exists() && is_valid_toml_file(&scope_path) {
        let existing = std::fs::read_to_string(&scope_path)
            .map_err(|e| SdkError::Config(format!("cannot read scope file: {e}")))?;
        let target_display =
            extract_scope_target(&existing).unwrap_or_else(|| "<multiple targets>".to_owned());
        println!("  SERAPH scope: configured (target: {target_display})");
        println!("  Pass `--force` to reconfigure.");
        return Ok(());
    }

    // Ask if the user wants to set up scope now (in full-wizard mode this
    // is the first prompt; in --force or missing-file mode we proceed).
    if scope_path.exists() && !force {
        // File exists but failed TOML validation — warn and continue.
        println!("  ! scope.toml exists but is not valid TOML — re-running wizard.");
    } else if !scope_path.exists() {
        let proceed = prompt_yes_no("  SERAPH scope not configured. Set up now? [y/N]: ")?;
        if !proceed {
            println!("  Skipped. Run `lightarchitects setup seraph` to configure later.");
            return Ok(());
        }
    }

    // Collect and validate each field — re-prompt on validation failure.
    let target = prompt_scope_target()?;
    let tool = prompt_scope_tool()?;
    let domain_str = prompt_scope_domain()?;

    // Build ScopeConstraint to run full SDK-side validation.
    let domain = parse_domain(&domain_str)
        .ok_or_else(|| SdkError::Config(format!("unrecognised domain '{domain_str}'")))?;
    // This validates target (shell metacharacters, null bytes, localhost)
    // and tool (allowlist).  Errors here indicate a logic bug since we
    // already validated interactively, but we propagate rather than panic.
    let _constraint = ScopeConstraint::new(&target, &tool, domain)
        .map_err(|e| SdkError::Config(format!("scope validation failed: {e}")))?;

    let content = render_scope_toml(&target, &tool, &domain_str);
    atomic_write_0600(&scope_path, &content)?;

    println!("\n  ✓ SERAPH scope written to {}", scope_path.display());
    println!("  ✓ Permissions: 0600");
    println!("  Run `lightarchitects seraph status` to verify the SERAPH binary is present.\n");
    Ok(())
}

/// Prompt for an authorized target with re-prompt loop on validation failure.
fn prompt_scope_target() -> Result<String, SdkError> {
    loop {
        let target = prompt_line(
            "  Authorized target (domain or IP CIDR, e.g. example.com or 10.0.0.0/8):\n  > ",
        )?;
        if target.is_empty() {
            println!("  Target cannot be empty.");
            continue;
        }
        // Pre-validate via ScopeConstraint (we pass a known-good tool for this check).
        match ScopeConstraint::new(&target, "nmap", ScopeDomain::Network) {
            Ok(_) => {}
            Err(e) => {
                println!("  Invalid target: {e}");
                continue;
            }
        }
        // Public address warning.
        if looks_like_public_address(&target) {
            println!(
                "\n  Warning: target appears to be a public address. \
                 Confirm this is an authorized engagement target."
            );
            let confirmed = prompt_yes_no("  Confirm? [y/N]: ")?;
            if !confirmed {
                println!("  Enter a different target.");
                continue;
            }
        }
        return Ok(target);
    }
}

/// Prompt for an authorized tool with re-prompt loop on validation failure.
fn prompt_scope_tool() -> Result<String, SdkError> {
    println!(
        "\n  Authorized tool allowlist hint:\n  \
         nmap, masscan, rustscan, nuclei, nikto, gobuster, whatweb,\n  \
         tshark, tcpdump, theHarvester, osint, scan, analyze, capture,\n  \
         monitor, execute, detonate, orchestrate, impacket-<name>, ..."
    );
    loop {
        let tool = prompt_line("  Authorized tool:\n  > ")?;
        if tool.is_empty() {
            println!("  Tool cannot be empty.");
            continue;
        }
        match ScopeConstraint::new("192.168.1.1", &tool, ScopeDomain::Network) {
            Ok(_) => return Ok(tool),
            Err(e) => {
                println!("  Invalid tool: {e}");
            }
        }
    }
}

/// Prompt for an engagement domain with re-prompt loop on invalid input.
fn prompt_scope_domain() -> Result<String, SdkError> {
    loop {
        let domain =
            prompt_line("\n  Engagement domain [web/network/cloud/physical/social]:\n  > ")?;
        if parse_domain(&domain).is_some() {
            return Ok(domain.trim().to_lowercase());
        }
        println!(
            "  Invalid domain '{domain}'. Choose one of: web, network, cloud, physical, social."
        );
    }
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

    atomic_write_0600(path, &content)?;

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;

    // ── SERAPH scope helpers ─────────────────────────────────────────────────

    #[test]
    fn render_scope_toml_produces_valid_toml_with_expected_fields() {
        let content = render_scope_toml("10.0.0.0/8", "nmap", "network");
        // Must parse as valid TOML.
        let val: toml::Value =
            toml::from_str(&content).expect("render_scope_toml output is not valid TOML");
        // [[scope]] must exist with target, tool, domain.
        let scope_arr = val
            .get("scope")
            .and_then(|v| v.as_array())
            .expect("[[scope]] array missing");
        let first = scope_arr.first().expect("[[scope]] array is empty");
        assert_eq!(
            first.get("target").and_then(|v| v.as_str()),
            Some("10.0.0.0/8")
        );
        assert_eq!(first.get("tool").and_then(|v| v.as_str()), Some("nmap"));
        assert_eq!(
            first.get("domain").and_then(|v| v.as_str()),
            Some("network")
        );
    }

    #[test]
    fn render_scope_toml_contains_expected_comment_header() {
        let content = render_scope_toml("192.168.1.0/24", "nuclei", "web");
        assert!(
            content.contains("SERAPH scope authorization"),
            "missing header comment"
        );
        assert!(
            content.contains("lightarchitects setup"),
            "missing attribution comment"
        );
    }

    #[test]
    fn extract_scope_target_returns_first_target() {
        let toml_str = "[[scope]]\ntarget = \"10.0.0.1\"\ntool = \"nmap\"\ndomain = \"network\"\n";
        let target = extract_scope_target(toml_str).expect("should parse target");
        assert_eq!(target, "10.0.0.1");
    }

    #[test]
    fn extract_scope_target_returns_none_on_malformed_toml() {
        assert!(extract_scope_target("not = [valid toml").is_none());
    }

    #[test]
    fn extract_scope_target_returns_none_on_missing_scope_key() {
        let toml_str = "[other]\nfoo = \"bar\"\n";
        assert!(extract_scope_target(toml_str).is_none());
    }

    #[test]
    fn is_valid_toml_file_returns_false_for_nonexistent_path() {
        let p = std::path::Path::new("/tmp/lightarchitects_test_nonexistent_scope.toml");
        assert!(!is_valid_toml_file(p));
    }

    #[test]
    fn is_valid_toml_file_returns_true_for_valid_toml() {
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(tmp.path(), "key = \"value\"\n").expect("write");
        assert!(is_valid_toml_file(tmp.path()));
    }

    #[test]
    fn is_valid_toml_file_returns_false_for_invalid_toml() {
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(tmp.path(), "not = [valid toml").expect("write");
        assert!(!is_valid_toml_file(tmp.path()));
    }

    // ── atomic_write_0600 ───────────────────────────────────────────────────

    #[test]
    fn atomic_write_0600_creates_file_with_correct_content() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("scope.toml");
        atomic_write_0600(&path, "target = \"10.0.0.1\"\n").expect("write should succeed");
        let content = std::fs::read_to_string(&path).expect("read back");
        assert_eq!(content, "target = \"10.0.0.1\"\n");
    }

    #[test]
    #[cfg(unix)]
    fn atomic_write_0600_sets_0600_permissions() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("scope.toml");
        atomic_write_0600(&path, "# test\n").expect("write");
        let meta = std::fs::metadata(&path).expect("metadata");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "scope.toml must be 0600, got {mode:#o}");
    }

    #[test]
    #[cfg(unix)]
    fn atomic_write_0600_not_world_readable() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("scope.toml");
        atomic_write_0600(&path, "# test\n").expect("write");
        let meta = std::fs::metadata(&path).expect("metadata");
        let mode = meta.permissions().mode();
        // No world read (bit 4), no world write (bit 2), no world exec (bit 1).
        assert_eq!(
            mode & 0o007,
            0,
            "scope.toml must not be world-readable: mode={mode:#o}"
        );
        // No group read/write/exec either.
        assert_eq!(
            mode & 0o070,
            0,
            "scope.toml must not be group-readable: mode={mode:#o}"
        );
    }

    #[test]
    fn atomic_write_0600_is_idempotent_overwrite() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("scope.toml");
        atomic_write_0600(&path, "first = true\n").expect("first write");
        atomic_write_0600(&path, "second = true\n").expect("second write");
        let content = std::fs::read_to_string(&path).expect("read");
        assert_eq!(content, "second = true\n");
    }

    // ── Public address detection ─────────────────────────────────────────────

    #[test]
    fn public_address_detection_private_ranges_are_not_public() {
        for private in &[
            "10.0.0.1",
            "10.255.255.255",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "192.168.0.0/16",
        ] {
            assert!(
                !looks_like_public_address(private),
                "{private} should not be flagged as public"
            );
        }
    }

    #[test]
    fn public_address_detection_rfc5737_not_public() {
        for doc in &["192.0.2.1", "198.51.100.1", "203.0.113.1"] {
            assert!(
                !looks_like_public_address(doc),
                "{doc} (RFC5737) should not be flagged as public"
            );
        }
    }

    #[test]
    fn public_address_detection_public_domain_flagged() {
        assert!(
            looks_like_public_address("example.com"),
            "public domain should be flagged"
        );
    }

    #[test]
    fn public_address_detection_https_prefix_stripped() {
        // 192.168.x.x behind https:// should still not be public.
        assert!(!looks_like_public_address("https://192.168.1.1/api"));
        // A public domain behind https:// should still be flagged.
        assert!(looks_like_public_address("https://evil.corp/shell"));
    }

    // ── Domain parsing ───────────────────────────────────────────────────────

    #[test]
    fn parse_domain_accepts_all_valid_domains() {
        use lightarchitects_seraph::scope::ScopeDomain;
        assert_eq!(parse_domain("web"), Some(ScopeDomain::Web));
        assert_eq!(parse_domain("network"), Some(ScopeDomain::Network));
        assert_eq!(parse_domain("cloud"), Some(ScopeDomain::Cloud));
        assert_eq!(parse_domain("physical"), Some(ScopeDomain::Physical));
        assert_eq!(parse_domain("social"), Some(ScopeDomain::Social));
    }

    #[test]
    fn parse_domain_is_case_insensitive() {
        use lightarchitects_seraph::scope::ScopeDomain;
        assert_eq!(parse_domain("WEB"), Some(ScopeDomain::Web));
        assert_eq!(parse_domain("Network"), Some(ScopeDomain::Network));
    }

    #[test]
    fn parse_domain_rejects_unknown() {
        assert!(parse_domain("recon").is_none());
        assert!(parse_domain("").is_none());
        assert!(parse_domain("internet").is_none());
    }

    // ── is_in_172_16_range ───────────────────────────────────────────────────

    #[test]
    fn is_in_172_16_range_boundaries() {
        assert!(is_in_172_16_range("172.16.0.1"));
        assert!(is_in_172_16_range("172.31.255.255"));
        assert!(!is_in_172_16_range("172.15.0.1"));
        assert!(!is_in_172_16_range("172.32.0.1"));
        assert!(!is_in_172_16_range("10.0.0.1"));
    }

    // ── Full scope write + read roundtrip ────────────────────────────────────

    #[test]
    #[cfg(unix)]
    fn scope_write_roundtrip_produces_valid_toml_with_0600_perms() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(".seraph").join("scope.toml");

        let content = render_scope_toml("192.168.1.0/24", "nmap", "network");
        atomic_write_0600(&path, &content).expect("write");

        // 1. File must exist and be valid TOML.
        assert!(is_valid_toml_file(&path), "written file is not valid TOML");

        // 2. Permissions must be 0600.
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:#o}");

        // 3. Target must round-trip correctly.
        let raw = std::fs::read_to_string(&path).expect("read");
        let extracted = extract_scope_target(&raw).expect("target should be present");
        assert_eq!(extracted, "192.168.1.0/24");
    }

    // ── HOME-override: seraph_scope_path ─────────────────────────────────────

    #[test]
    fn seraph_scope_path_uses_home_env() {
        // SAFETY: test-only; isolated environment variable mutation.
        unsafe { std::env::set_var("HOME", "/tmp/test_home_la") };
        let p = seraph_scope_path().expect("should succeed with HOME set");
        assert_eq!(
            p,
            std::path::PathBuf::from("/tmp/test_home_la/.seraph/scope.toml")
        );
        // Restore — do not leave test pollution.
        unsafe { std::env::remove_var("HOME") };
    }
}
