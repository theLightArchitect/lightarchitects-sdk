//! Native skill execution for the `lightarchitects` CLI/TUI.
//!
//! Three dispatch tiers:
//!
//! 1. **SDK-native** — OBSERVE→AYIN HTTP `:3742`, RESEARCH→`QuantumClient`,
//!    ENRICH→`EvaClient`, SECURE→`SeraphClient`. Zero LLM calls.
//! 2. **LLM-assisted** — SKILL.md loaded as `SessionConfig.system_prompt`; user
//!    args become the first message, then enters an interactive TTY loop.
//! 3. **Claude Code–only** — BUILD / DEPLOY / GATE require the `Skill` tool;
//!    emit a clear warning and return an error so the user is not left confused.
//!
//! # Usage
//!
//! ```text
//! lightarchitects skill list
//! lightarchitects skill reflect
//! lightarchitects skill research "auth bug in soul handler"
//! lightarchitects skill observe traces
//! lightarchitects skill secure .
//! lightarchitects skill enrich "today we shipped the strategy loop wiring"
//! lightarchitects plan "my feature description"    ← alias for /plan
//! lightarchitects research "quantum helix topic"   ← alias for /research
//! lightarchitects observe status                   ← alias for /observe
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use lightarchitects::agent::conversation::{
    ConversationEvent, ConversationSession, SessionConfig, Transport, TtyTransport,
};
use lightarchitects::agent::{ChainContext, ClaudeCliProvider};
use tokio::io::AsyncBufReadExt as _;

use crate::config::GatewayConfig;
use crate::error::GatewayError;

// ── Skill spec ────────────────────────────────────────────────────────────────

/// Resolved metadata for a skill loaded from the plugin cache.
#[derive(Debug, Clone)]
pub struct SkillSpec {
    /// Human-readable name (from frontmatter `name:`).
    pub name: String,
    /// One-line description (from frontmatter `description:`).
    pub description: String,
    /// Slug used for file-system lookup (e.g. `"REFLECT"`).
    pub slug: String,
    /// Full SKILL.md content — used as system prompt in LLM-assisted mode.
    pub content: String,
    /// Absolute path to the SKILL.md file.
    pub path: PathBuf,
    /// Whether this skill is user-invocable (`user-invocable: true` in frontmatter).
    pub user_invocable: bool,
}

/// How a skill should be dispatched from the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchMode {
    /// Direct SDK call — no LLM tokens consumed.
    SdkNative,
    /// Load SKILL.md as system prompt; run interactive conversational session.
    LlmAssisted,
    /// Skill requires a live Claude Code session and the `Skill` tool.
    ClaudeCodeOnly,
}

impl SkillSpec {
    /// Return the dispatch mode for this skill based on its slug.
    pub fn dispatch_mode(&self) -> DispatchMode {
        match self.slug.to_uppercase().as_str() {
            "OBSERVE" | "AYIN" => DispatchMode::SdkNative,
            "RESEARCH" | "Q" | "QUANTUM" => DispatchMode::SdkNative,
            "ENRICH" | "EVA" => DispatchMode::SdkNative,
            "SECURE" | "SERAPH" => DispatchMode::SdkNative,
            // These require the Claude Code Skill tool — cannot run standalone.
            "BUILD" | "DEPLOY" | "GATE" | "XEA" | "SQUAD" => DispatchMode::ClaudeCodeOnly,
            _ => DispatchMode::LlmAssisted,
        }
    }
}

// ── Skill loader ──────────────────────────────────────────────────────────────

/// Return candidate SKILL.md paths for `slug`, in priority order.
fn skill_search_paths(slug: &str) -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
    // Plugin cache (installed via `lightarchitects init`)
    let cache_path = PathBuf::from(format!(
        "{home}/.claude/plugins/cache/light-architects/lightarchitects/1.0.0/skills/{slug}/SKILL.md"
    ));
    // Dev checkout fallback
    let dev_path = PathBuf::from(format!(
        "{home}/Projects/light-architects-plugins/lightarchitects/skills/{slug}/SKILL.md"
    ));
    vec![cache_path, dev_path]
}

/// Parse `name:`, `description:`, and `user-invocable:` from SKILL.md frontmatter.
///
/// Frontmatter is delimited by `---` lines; parsing stops at the closing `---`.
fn parse_frontmatter(content: &str) -> (String, String, bool) {
    let mut name = String::new();
    let mut description = String::new();
    let mut user_invocable = false;
    let mut fm_open = false;
    let mut fm_done = false;

    for line in content.lines() {
        if fm_done {
            break;
        }
        if line.trim() == "---" {
            if !fm_open {
                fm_open = true;
            } else {
                fm_done = true;
            }
            continue;
        }
        if fm_open {
            if let Some(rest) = line.strip_prefix("name:") {
                name = rest.trim().trim_matches('"').to_owned();
            } else if let Some(rest) = line.strip_prefix("description:") {
                description = rest.trim().trim_matches('"').to_owned();
            } else if let Some(rest) = line.strip_prefix("user-invocable:") {
                user_invocable = rest.trim() == "true";
            }
        }
    }
    (name, description, user_invocable)
}

/// Load a skill by name/slug (case-insensitive). Returns `None` if not found.
pub fn load(slug: &str) -> Option<SkillSpec> {
    let upper = slug.to_uppercase();
    for path in skill_search_paths(&upper) {
        if let Ok(content) = std::fs::read_to_string(&path) {
            let (name, description, user_invocable) = parse_frontmatter(&content);
            return Some(SkillSpec {
                name: if name.is_empty() { upper.clone() } else { name },
                description,
                slug: upper,
                content,
                path,
                user_invocable,
            });
        }
    }
    None
}

/// List all user-invocable skills from the plugin cache.
pub fn list_all() -> Vec<SkillSpec> {
    const KNOWN_SLUGS: &[&str] = &[
        "PLAN",
        "BUILD",
        "DEPLOY",
        "VERIFY",
        "SECURE",
        "OBSERVE",
        "REFLECT",
        "ENRICH",
        "ONBOARD",
        "OPTIMIZE",
        "RESEARCH",
        "REVIEW",
        "CODE-VERIFY",
        "SCRUM",
        "GATE",
        "XEA",
        "SQUAD",
        "RISK-ANALYSIS",
    ];

    let mut specs: Vec<SkillSpec> = KNOWN_SLUGS
        .iter()
        .filter_map(|s| load(s))
        .filter(|s| s.user_invocable)
        .collect();

    // Filesystem scan to catch any extra skills not in the known list.
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
    let skills_dir =
        format!("{home}/.claude/plugins/cache/light-architects/lightarchitects/1.0.0/skills");
    if let Ok(entries) = std::fs::read_dir(&skills_dir) {
        for entry in entries.flatten() {
            let slug = entry.file_name().to_string_lossy().to_uppercase();
            if KNOWN_SLUGS.contains(&slug.as_str()) {
                continue;
            }
            let skill_md = entry.path().join("SKILL.md");
            if let Ok(content) = std::fs::read_to_string(&skill_md) {
                let (name, description, user_invocable) = parse_frontmatter(&content);
                if user_invocable {
                    specs.push(SkillSpec {
                        name: if name.is_empty() { slug.clone() } else { name },
                        description,
                        slug: slug.clone(),
                        content,
                        path: skill_md,
                        user_invocable: true,
                    });
                }
            }
        }
    }

    specs
}

// ── Output helpers ─────────────────────────────────────────────────────────────

/// Print a formatted table of available skills.
pub fn cmd_list() -> Result<(), GatewayError> {
    let skills = list_all();
    if skills.is_empty() {
        println!("No skills found. Check that the lightarchitects plugin is installed.");
        println!("  Cache: ~/.claude/plugins/cache/light-architects/");
        return Ok(());
    }

    let col_w = skills
        .iter()
        .map(|s| s.slug.len())
        .max()
        .unwrap_or(10)
        .max(5);
    let mode_w = 12usize;

    println!(
        "{:<col_w$}  {:<mode_w$}  Description",
        "Skill",
        "Mode",
        col_w = col_w,
        mode_w = mode_w,
    );
    println!(
        "{:-<col_w$}  {:-<mode_w$}  {:-<50}",
        "",
        "",
        "",
        col_w = col_w,
        mode_w = mode_w
    );

    for s in &skills {
        let mode_label = match s.dispatch_mode() {
            DispatchMode::SdkNative => "sdk-native",
            DispatchMode::LlmAssisted => "llm-assisted",
            DispatchMode::ClaudeCodeOnly => "claude-code",
        };
        let desc = if s.description.len() > 60 {
            format!("{}…", &s.description[..59])
        } else {
            s.description.clone()
        };
        println!(
            "{:<col_w$}  {:<mode_w$}  {desc}",
            s.slug,
            mode_label,
            col_w = col_w,
            mode_w = mode_w,
        );
    }

    println!();
    println!("  sdk-native    No LLM required — calls sibling SDK clients directly");
    println!("  llm-assisted  Loads SKILL.md as system prompt; interactive session");
    println!("  claude-code   Requires Claude Code (lightarchitects or `claude` CLI)");
    Ok(())
}

// ── SDK-native dispatch ────────────────────────────────────────────────────────

/// Dispatch `lightarchitects observe [sub]` → AYIN HTTP API at `:3742`.
async fn dispatch_observe(args: &[String]) -> Result<(), GatewayError> {
    let port = std::env::var("AYIN_PORT").unwrap_or_else(|_| "3742".to_owned());
    let base = format!("http://127.0.0.1:{port}/api");

    let sub = args.first().map(String::as_str).unwrap_or("status");
    let url = match sub {
        "traces" | "trace" => format!("{base}/traces"),
        "spans" | "span" => format!("{base}/spans"),
        "metrics" | "metric" => format!("{base}/metrics"),
        "health" => format!("{base}/health"),
        _ => format!("{base}/status"),
    };

    let body = reqwest::get(&url)
        .await
        .map_err(|e| GatewayError::Internal(format!("AYIN HTTP: {e}")))?
        .text()
        .await
        .map_err(|e| GatewayError::Internal(format!("AYIN read: {e}")))?;

    println!("{body}");
    Ok(())
}

/// Dispatch `lightarchitects research <topic>` → `QuantumClient::research`.
async fn dispatch_research(args: &[String]) -> Result<(), GatewayError> {
    use lightarchitects::quantum::QuantumClient;

    let topic = if args.is_empty() {
        return Err(GatewayError::MissingParam("research topic"));
    } else {
        args.join(" ")
    };

    let client = QuantumClient::local_builder()
        .build()
        .await
        .map_err(|e| GatewayError::Internal(e.to_string()))?;

    let result = client
        .research(&topic)
        .await
        .map_err(|e| GatewayError::Internal(e.to_string()))?;

    println!("{}", result.output);
    Ok(())
}

/// Dispatch `lightarchitects enrich <text>` → `EvaClient::remember`.
async fn dispatch_enrich(args: &[String]) -> Result<(), GatewayError> {
    use lightarchitects::eva::EvaClient;

    let text = if args.is_empty() {
        return Err(GatewayError::MissingParam("content to enrich"));
    } else {
        args.join(" ")
    };

    let client = EvaClient::local_builder()
        .build()
        .await
        .map_err(|e| GatewayError::Internal(e.to_string()))?;

    let result = client
        .remember(&text, None)
        .await
        .map_err(|e| GatewayError::Internal(e.to_string()))?;

    if result.memories.is_empty() {
        println!("Stored. (no matching memories returned)");
    } else {
        for m in &result.memories {
            println!("[{}] {}", m.id, m.content);
        }
    }
    Ok(())
}

/// Dispatch `lightarchitects secure [target]` → `SeraphClient::analyze`.
async fn dispatch_secure(args: &[String]) -> Result<(), GatewayError> {
    use lightarchitects::seraph::SeraphClient;

    let target = if args.is_empty() {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| ".".to_owned())
    } else {
        args.join(" ")
    };

    let client = SeraphClient::local_builder()
        .build()
        .await
        .map_err(|e| GatewayError::Internal(e.to_string()))?;

    let result = client
        .analyze(&target)
        .await
        .map_err(|e| GatewayError::Internal(e.to_string()))?;

    println!("{}", result.output);
    Ok(())
}

// ── LLM-assisted dispatch ──────────────────────────────────────────────────────

/// Run a skill in LLM-assisted mode.
///
/// Loads the SKILL.md content as `SessionConfig.system_prompt`, sends any
/// provided CLI args as the first user message, then enters a TTY REPL loop.
pub async fn dispatch_llm_assisted(spec: &SkillSpec, args: &[String]) -> Result<(), GatewayError> {
    use tokio::io::AsyncWriteExt as _;

    let config = SessionConfig {
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        system_prompt: Some(spec.content.clone()),
        ..SessionConfig::default()
    };
    let mut session = ConversationSession::new(config, Arc::new(ClaudeCliProvider::default()));
    let mut transport = TtyTransport::new(tokio::io::stdout());
    let chain = ChainContext::default();

    let mut stdout = tokio::io::stdout();
    let banner = format!(
        "/{} — {}\nType 'quit' or press Ctrl-D to exit.\n\n",
        spec.slug, spec.description
    );
    let _ = stdout.write_all(banner.as_bytes()).await;
    let _ = stdout.flush().await;

    // Use CLI args as the initial user message if provided.
    if !args.is_empty() {
        let initial = args.join(" ");
        session.clear_interrupt();
        if let Err(e) = session.run_turn(&initial, &mut transport, &chain).await {
            let _ = transport
                .emit(&ConversationEvent::Error {
                    message: e.to_string(),
                    recoverable: Some(true),
                })
                .await;
        }
    }

    // Interactive follow-up loop.
    let reader = tokio::io::BufReader::new(tokio::io::stdin());
    let mut lines = reader.lines();

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

        session.clear_interrupt();
        if let Err(e) = session.run_turn(input, &mut transport, &chain).await {
            let _ = transport
                .emit(&ConversationEvent::Error {
                    message: e.to_string(),
                    recoverable: Some(true),
                })
                .await;
        }
    }

    Ok(())
}

// ── Public execute ─────────────────────────────────────────────────────────────

/// Execute a skill from the CLI.
///
/// `args[0]` is the skill name/slug; `args[1..]` are passed to the skill.
/// With no args (or `list`), prints the skill table and exits.
pub async fn execute(_config: &GatewayConfig, args: &[String]) -> Result<(), GatewayError> {
    let slug = match args.first().map(String::as_str) {
        None | Some("list" | "ls") => return cmd_list(),
        Some(s) => s,
    };

    let spec = load(slug).ok_or_else(|| {
        GatewayError::UnknownTool(format!(
            "Unknown skill '{slug}'. Run `lightarchitects skill list` to see available skills."
        ))
    })?;

    let skill_args = if args.len() > 1 { &args[1..] } else { &[] };

    match spec.dispatch_mode() {
        DispatchMode::SdkNative => match spec.slug.as_str() {
            "OBSERVE" | "AYIN" => dispatch_observe(skill_args).await,
            "RESEARCH" | "Q" | "QUANTUM" => dispatch_research(skill_args).await,
            "ENRICH" | "EVA" => dispatch_enrich(skill_args).await,
            "SECURE" | "SERAPH" => dispatch_secure(skill_args).await,
            _ => dispatch_llm_assisted(&spec, skill_args).await,
        },
        DispatchMode::LlmAssisted => dispatch_llm_assisted(&spec, skill_args).await,
        DispatchMode::ClaudeCodeOnly => {
            eprintln!(
                "/{} requires Claude Code — the Skill tool is not available outside a Claude session.",
                spec.slug
            );
            eprintln!("  In Claude Code, type:  /{}", spec.slug.to_lowercase());
            eprintln!("  Or launch the agent:   lightarchitects");
            Err(GatewayError::UnknownTool(format!(
                "{} is Claude Code–only",
                spec.slug
            )))
        }
    }
}

/// Parse a `/skill <name> [args]` slash command from the TTY REPL.
///
/// Recognises `/skill`, `/plan`, `/research`, `/reflect`, `/observe`,
/// `/enrich`, `/secure`, `/review`, `/optimize`, `/scrum`, `/onboard`.
pub fn parse_skill_slash_command(line: &str) -> Option<(String, Vec<String>)> {
    const SKILL_ALIASES: &[&str] = &[
        "/skill",
        "/plan",
        "/research",
        "/reflect",
        "/observe",
        "/enrich",
        "/secure",
        "/review",
        "/optimize",
        "/scrum",
        "/onboard",
        "/code-verify",
        "/verify",
        "/risk",
        "/risk-analysis",
    ];

    let lower = line.trim().to_lowercase();
    let matched = SKILL_ALIASES
        .iter()
        .find(|&&alias| lower == alias || lower.starts_with(&format!("{alias} ")))?;

    // Everything after the matched alias token is the args string.
    let rest = line.trim()[matched.len()..].trim();
    let mut parts = rest.split_whitespace();

    // For `/skill <name> [args…]` the first word is the skill name.
    // For all other aliases the alias itself IS the skill name.
    let (slug, remaining): (String, Vec<String>) = if *matched == "/skill" {
        let name = parts.next().map(|s| s.to_owned()).unwrap_or_default();
        (name, parts.map(|s| s.to_owned()).collect())
    } else {
        let slug = matched.trim_start_matches('/').to_owned();
        (slug, parts.map(|s| s.to_owned()).collect())
    };

    if slug.is_empty() {
        return None;
    }

    Some((slug, remaining))
}
