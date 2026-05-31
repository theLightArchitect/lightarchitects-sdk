//! Native skill execution for the `lightarchitects` CLI/TUI.
//!
//! Two dispatch tiers:
//!
//! 1. **SDK-native** — OBSERVE→AYIN HTTP `:3742`, RESEARCH→`QuantumClient`,
//!    ENRICH→`EvaClient`, SECURE→`SeraphClient`. Zero LLM calls.
//! 2. **LLM-assisted** — SKILL.md loaded as `SessionConfig.system_prompt`; any
//!    configured provider handles the session. Provider is auto-detected once at
//!    startup and reused for all skill dispatches in the same process.
//!
//! # Provider auto-detection (`detect_provider`)
//!
//! 1. `LA_LLM` env var set explicitly → that backend
//! 2. `OLLAMA_API_KEY` set (and no explicit override) → Ollama, model from `LA_MODEL`
//! 3. `ANTHROPIC_API_KEY` set (debug builds only) → Anthropic direct
//! 4. `claude` binary in PATH → Claude CLI (default)
//!
//! # Usage
//!
//! ```text
//! lightarchitects skill list
//! lightarchitects skill reflect
//! lightarchitects skill research "auth bug in soul handler"
//! lightarchitects plan "my feature description"    ← alias for /plan
//! LA_LLM=ollama LA_MODEL=glm-5.1:cloud lightarchitects plan "..."  ← Ollama cloud
//! ```

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use lightarchitects::agent::conversation::{
    ConversationEvent, ConversationSession, SessionConfig, Transport, TtyTransport,
};
use lightarchitects::agent::http::AnthropicHttpProvider;
use lightarchitects::agent::{
    AgentResponse, ChainContext, ClaudeCliProvider, LlmAgentProvider, OllamaCliProvider,
    OpenAICompatProvider, ProviderCapabilities, ProviderError, SanitizedAgentRequest,
};
use tokio::io::AsyncBufReadExt as _;

use crate::config::GatewayConfig;
use crate::error::GatewayError;

// ── Active-provider registry ──────────────────────────────────────────────────

/// Cached provider choice — detected once per process, then reused everywhere.
static ACTIVE_PROVIDER: OnceLock<ProviderKind> = OnceLock::new();

/// Which backend was selected at startup.
#[derive(Debug, Clone)]
enum ProviderKind {
    Claude,
    Ollama {
        model: String,
    },
    Anthropic {
        model: String,
        max_tokens: u32,
    },
    /// `LiteLLM` proxy — OpenAI-compatible endpoint; selected via `LA_LLM=litellm`.
    LiteLLM {
        base_url: String,
        api_key: String,
        model: String,
    },
}

/// Auto-detect the active LLM provider from the process environment.
///
/// Called at most once (results cached in [`ACTIVE_PROVIDER`]).
fn detect_provider() -> ProviderKind {
    // 1. Explicit override — `LA_LLM=ollama|anthropic|claude`
    let explicit = std::env::var("LA_LLM").unwrap_or_default().to_lowercase();
    let model = std::env::var("LA_MODEL").unwrap_or_default();

    match explicit.as_str() {
        "ollama" => {
            return ProviderKind::Ollama {
                model: if model.is_empty() {
                    "glm-5.1:cloud".to_owned()
                } else {
                    model
                },
            };
        }
        "anthropic" | "claude-api" => {
            let max_tokens = std::env::var("LA_MAX_TOKENS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8192);
            return ProviderKind::Anthropic {
                model: if model.is_empty() {
                    "claude-sonnet-4-6".to_owned()
                } else {
                    model
                },
                max_tokens,
            };
        }
        "claude" => return ProviderKind::Claude,
        "litellm" => {
            let base_url = std::env::var("LA_LITELLM_BASE_URL").unwrap_or_default();
            let api_key = std::env::var("LA_LITELLM_API_KEY").unwrap_or_default();
            let litellm_model = std::env::var("LA_LITELLM_MODEL").unwrap_or_default();
            return ProviderKind::LiteLLM {
                base_url,
                api_key,
                model: litellm_model,
            };
        }
        _ => {}
    }

    // 2. OLLAMA_API_KEY set → Ollama is configured and active
    if std::env::var("OLLAMA_API_KEY")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
    {
        return ProviderKind::Ollama {
            model: if model.is_empty() {
                "glm-5.1:cloud".to_owned()
            } else {
                model
            },
        };
    }

    // 3. ANTHROPIC_API_KEY set (debug builds only — release uses Keychain)
    #[cfg(debug_assertions)]
    if std::env::var("ANTHROPIC_API_KEY")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
    {
        let max_tokens = std::env::var("LA_MAX_TOKENS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8192);
        return ProviderKind::Anthropic {
            model: if model.is_empty() {
                "claude-sonnet-4-6".to_owned()
            } else {
                model
            },
            max_tokens,
        };
    }

    // 4. Default: Claude CLI (requires `claude` binary in PATH)
    ProviderKind::Claude
}

/// Return the active provider kind, detecting once and caching.
fn active_provider_kind() -> &'static ProviderKind {
    ACTIVE_PROVIDER.get_or_init(detect_provider)
}

// ── AnyProvider — unified enum wrapping all three concrete providers ───────────

/// A single type that can hold any of the three supported LLM providers.
///
/// Implements [`LlmAgentProvider`] by delegating to the inner variant.
/// Used by both the main agent session and all skill dispatch calls so they
/// always run through exactly the same backend.
pub enum AnyProvider {
    /// Claude CLI backend — spawns `claude -p`.
    Claude(ClaudeCliProvider),
    /// Ollama backend — local daemon or cloud-routed model.
    Ollama(OllamaCliProvider),
    /// Anthropic HTTP backend — direct API call.
    Anthropic(AnthropicHttpProvider),
    /// `LiteLLM` proxy backend — OpenAI-compatible HTTP dispatch.
    LiteLLM(OpenAICompatProvider),
}

#[async_trait]
impl LlmAgentProvider for AnyProvider {
    fn name(&self) -> &'static str {
        match self {
            Self::Claude(p) => p.name(),
            Self::Ollama(p) => p.name(),
            Self::Anthropic(p) => p.name(),
            Self::LiteLLM(p) => p.name(),
        }
    }

    async fn spawn(&self, req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
        match self {
            Self::Claude(p) => p.spawn(req).await,
            Self::Ollama(p) => p.spawn(req).await,
            Self::Anthropic(p) => p.spawn(req).await,
            Self::LiteLLM(p) => p.spawn(req).await,
        }
    }

    fn capabilities(&self) -> ProviderCapabilities {
        match self {
            Self::Claude(p) => p.capabilities(),
            Self::Ollama(p) => p.capabilities(),
            Self::Anthropic(p) => p.capabilities(),
            Self::LiteLLM(p) => p.capabilities(),
        }
    }

    fn estimate_cost(&self, input_tokens: u32, max_output_tokens: u32) -> f64 {
        match self {
            Self::Claude(p) => p.estimate_cost(input_tokens, max_output_tokens),
            Self::Ollama(p) => p.estimate_cost(input_tokens, max_output_tokens),
            Self::Anthropic(p) => p.estimate_cost(input_tokens, max_output_tokens),
            Self::LiteLLM(p) => p.estimate_cost(input_tokens, max_output_tokens),
        }
    }
}

/// Build an [`AnyProvider`] from the cached provider detection result.
///
/// # Errors
///
/// Returns an error string if the provider cannot be constructed (e.g. unknown
/// Ollama model slug or Anthropic token limit out of range).
pub fn build_provider() -> Result<AnyProvider, String> {
    match active_provider_kind() {
        ProviderKind::Ollama { model } => {
            // Gateway CLI is a short-lived process; reading env here is the
            // only point of capture, mirroring AppState::la_native_api_key
            // in the webshell.  No TOCTOU window because the process exits
            // after a single command.
            let auth_token = std::env::var("OLLAMA_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
                .map(secrecy::SecretString::from);
            OllamaCliProvider::new(model, auth_token)
                .map(AnyProvider::Ollama)
                .map_err(|e| e.to_string())
        }
        ProviderKind::Anthropic { model, max_tokens } => {
            AnthropicHttpProvider::new(model, *max_tokens)
                .map(AnyProvider::Anthropic)
                .map_err(|e| e.to_string())
        }
        ProviderKind::Claude => Ok(AnyProvider::Claude(ClaudeCliProvider::default())),
        ProviderKind::LiteLLM {
            base_url,
            api_key,
            model,
        } => OpenAICompatProvider::for_litellm(
            if base_url.is_empty() {
                None
            } else {
                Some(base_url.clone())
            },
            api_key.as_str(),
            model.clone(),
        )
        .map(AnyProvider::LiteLLM),
    }
}

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
    /// Optional JSON Schema for the `tool_use` input when this skill is exposed as an
    /// LLM tool. Parsed from `tool_schema:` in the SKILL.md frontmatter. When absent
    /// the default schema `{args: string[]}` is used by `GatewayToolExecutor`.
    pub tool_schema: Option<serde_json::Value>,
}

/// How a skill should be dispatched from the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchMode {
    /// Direct SDK call — no LLM tokens consumed.
    SdkNative,
    /// Load SKILL.md as system prompt; run interactive conversational session.
    /// Applies to every skill — any configured LLM provider is sufficient.
    LlmAssisted,
}

impl SkillSpec {
    /// Return the dispatch mode for this skill based on its slug.
    pub fn dispatch_mode(&self) -> DispatchMode {
        match self.slug.to_uppercase().as_str() {
            "OBSERVE" | "AYIN" | "RESEARCH" | "Q" | "QUANTUM" | "ENRICH" | "EVA" | "SECURE"
            | "SERAPH" => DispatchMode::SdkNative,
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

/// Parse `name:`, `description:`, `user-invocable:`, and `tool_schema:` from SKILL.md frontmatter.
///
/// Frontmatter is delimited by `---` lines; parsing stops at the closing `---`.
/// `tool_schema:` must be a single-line JSON value (e.g. an inlined object).
fn parse_frontmatter(content: &str) -> (String, String, bool, Option<serde_json::Value>) {
    let mut name = String::new();
    let mut description = String::new();
    let mut user_invocable = false;
    let mut tool_schema: Option<serde_json::Value> = None;
    let mut fm_open = false;
    let mut fm_done = false;

    for line in content.lines() {
        if fm_done {
            break;
        }
        if line.trim() == "---" {
            if fm_open {
                fm_done = true;
            } else {
                fm_open = true;
            }
            continue;
        }
        if fm_open {
            if let Some(rest) = line.strip_prefix("name:") {
                rest.trim().trim_matches('"').clone_into(&mut name);
            } else if let Some(rest) = line.strip_prefix("description:") {
                rest.trim().trim_matches('"').clone_into(&mut description);
            } else if let Some(rest) = line.strip_prefix("user-invocable:") {
                user_invocable = rest.trim() == "true";
            } else if let Some(rest) = line.strip_prefix("tool_schema:") {
                tool_schema = serde_json::from_str(rest.trim()).ok();
            }
        }
    }
    (name, description, user_invocable, tool_schema)
}

/// Load a skill by name/slug (case-insensitive). Returns `None` if not found.
///
/// On load the content is verified against the [`crate::cli::skill_trust`] ledger:
/// the first load pins the hash; subsequent loads verify it. A hash mismatch
/// emits `tracing::warn!` (non-blocking — the skill still loads so the session
/// continues, but the operator is alerted).
pub fn load(slug: &str) -> Option<SkillSpec> {
    let upper = slug.to_uppercase();
    for path in skill_search_paths(&upper) {
        if let Ok(content) = std::fs::read_to_string(&path) {
            // Trust ledger: pin on first load; warn on hash mismatch.
            let _ = crate::cli::skill_trust::verify_or_pin(&upper, &content);
            let (name, description, user_invocable, tool_schema) = parse_frontmatter(&content);
            return Some(SkillSpec {
                name: if name.is_empty() { upper.clone() } else { name },
                description,
                slug: upper,
                content,
                path,
                user_invocable,
                tool_schema,
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
                let (name, description, user_invocable, tool_schema) = parse_frontmatter(&content);
                if user_invocable {
                    specs.push(SkillSpec {
                        name: if name.is_empty() { slug.clone() } else { name },
                        description,
                        slug: slug.clone(),
                        content,
                        path: skill_md,
                        user_invocable: true,
                        tool_schema,
                    });
                }
            }
        }
    }

    specs
}

// ── Skill-aware system prompt ─────────────────────────────────────────────────

/// Build a compact system prompt that advertises the LA skill surface to the LLM.
///
/// The prompt tells the LLM which slash commands it can emit to invoke skills,
/// and instructs it to emit the command as the **last line** of its response so
/// the TUI can intercept and dispatch it without ambiguity.
pub fn build_skill_system_prompt() -> String {
    let skills = list_all();

    let mut lines = vec![
        "You are the Light Architects agent — a full-stack engineering assistant.".to_owned(),
        String::new(),
        "## Available skills".to_owned(),
        String::new(),
        "When the user asks for work that maps to a skill below, emit the corresponding".to_owned(),
        "slash command as the LAST LINE of your response (nothing after it). The TUI will"
            .to_owned(),
        "intercept it and launch the skill session. Do not emit a skill command unless the"
            .to_owned(),
        "user's intent clearly maps to one.".to_owned(),
        String::new(),
    ];

    for s in &skills {
        let mode = match s.dispatch_mode() {
            DispatchMode::SdkNative => "sdk",
            DispatchMode::LlmAssisted => "llm",
        };
        lines.push(format!(
            "  /{:<16} ({mode})  {}",
            s.slug.to_lowercase(),
            s.description
        ));
    }

    lines.push(String::new());
    lines.push(
        "Example: user says \"plan a new SSE endpoint\" → respond with a brief plan summary,"
            .to_owned(),
    );
    lines.push("then on the last line: /plan \"add SSE streaming endpoint\"".to_owned());
    lines.push(String::new());
    lines.push(
        "For general engineering questions answer directly without invoking a skill.".to_owned(),
    );

    lines.join("\n")
}

// ── Output helpers ─────────────────────────────────────────────────────────────

/// Print a formatted table of available skills.
///
/// # Errors
///
/// Returns an error if a skill slug cannot be resolved (unreachable in current implementation).
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
    println!("  llm-assisted  Loads SKILL.md as system prompt; works with any LLM provider");
    Ok(())
}

// ── SDK-native dispatch ────────────────────────────────────────────────────────

/// Dispatch `lightarchitects observe [sub]` → AYIN HTTP API at `:3742`.
async fn dispatch_observe(args: &[String]) -> Result<(), GatewayError> {
    let port = std::env::var("AYIN_PORT").unwrap_or_else(|_| "3742".to_owned());
    let base = format!("http://127.0.0.1:{port}/api");

    let sub = args.first().map_or("status", String::as_str);
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
            .map_or_else(|_| ".".to_owned(), |p| p.to_string_lossy().into_owned())
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

/// Inner session runner — generic over any [`LlmAgentProvider`].
///
/// Monomorphised at each call site to avoid boxing overhead.
async fn run_session_with_provider<P: LlmAgentProvider>(
    spec: &SkillSpec,
    args: &[String],
    provider: Arc<P>,
) -> Result<(), GatewayError> {
    use tokio::io::AsyncWriteExt as _;

    let config = SessionConfig {
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        system_prompt: Some(spec.content.clone()),
        ..SessionConfig::default()
    };
    let mut session = ConversationSession::new(config, provider);
    let mut transport = TtyTransport::new(tokio::io::stdout());
    let chain = ChainContext::default();

    let mut stdout = tokio::io::stdout();
    let banner = format!(
        "/{} — {}\nType 'quit' or press Ctrl-D to exit.\n\n",
        spec.slug, spec.description
    );
    let _ = stdout.write_all(banner.as_bytes()).await;
    let _ = stdout.flush().await;

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

/// Run a skill in LLM-assisted mode.
///
/// Uses the process-wide cached provider (detected once via [`build_provider`]).
/// All skill dispatches in a session therefore use the same backend as the main
/// agent session — coherent by construction.
///
/// # Errors
///
/// Returns an error if the provider cannot be constructed or the session fails.
pub async fn dispatch_llm_assisted(spec: &SkillSpec, args: &[String]) -> Result<(), GatewayError> {
    let p = build_provider().map_err(GatewayError::Internal)?;
    run_session_with_provider(spec, args, Arc::new(p)).await
}

// ── Public execute ─────────────────────────────────────────────────────────────

/// Execute a skill from the CLI.
///
/// `args[0]` is the skill name/slug; `args[1..]` are passed to the skill.
/// With no args (or `list`), prints the skill table and exits.
///
/// # Errors
///
/// Returns an error if the skill is not found or dispatch fails.
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
    }
}

/// Parse a `/skill <name> [args]` slash command from the TTY REPL.
///
/// Recognises `/skill` (generic dispatcher) plus a direct alias for every
/// known skill slug. All skills are available — any LLM provider is sufficient.
pub fn parse_skill_slash_command(line: &str) -> Option<(String, Vec<String>)> {
    const SKILL_ALIASES: &[&str] = &[
        "/skill",
        // Lifecycle
        "/plan",
        "/build",
        "/deploy",
        "/verify",
        "/gate",
        "/xea",
        "/squad",
        // Domain
        "/secure",
        "/observe",
        "/reflect",
        "/enrich",
        "/research",
        "/review",
        "/optimize",
        "/scrum",
        "/onboard",
        "/code-verify",
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
        let name = parts.next().map(str::to_owned).unwrap_or_default();
        (name, parts.map(str::to_owned).collect())
    } else {
        let slug = matched.trim_start_matches('/').to_owned();
        (slug, parts.map(str::to_owned).collect())
    };

    if slug.is_empty() {
        return None;
    }

    Some((slug, remaining))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn litellm_any_provider_constructs_ok() {
        let result = OpenAICompatProvider::for_litellm(
            Some("http://localhost:4000".to_owned()),
            "test-key",
            "anthropic/claude-opus-4-7".to_owned(),
        )
        .map(AnyProvider::LiteLLM);
        assert!(result.is_ok());
    }
}
