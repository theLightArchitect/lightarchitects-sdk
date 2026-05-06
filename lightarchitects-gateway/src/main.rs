//! Entry point for the `lightarchitects` unified gateway binary.
//!
//! Three operating modes:
//!
//! ```text
//! lightarchitects                           MCP server (Claude Code, stdio)
//! lightarchitects serve                     Arena (HTTP API + scheduler + agents)
//! lightarchitects serve --agent eva         Single agent heartbeat loop
//! lightarchitects conductor <cmd>           autonomous task queue
//! lightarchitects routes                    List enabled agents
//! lightarchitects canon list|check          Canon operations
//! lightarchitects initialize <step>         Setup wizard
//! lightarchitects soul <subcommand>         SOUL knowledge-graph operations
//! lightarchitects corso <subcommand>        CORSO operations
//! lightarchitects eva <subcommand>          EVA consciousness operations
//! lightarchitects quantum <subcommand>      QUANTUM investigation operations
//! lightarchitects seraph <subcommand>        SERAPH pentest operations
//! lightarchitects status                    Show sibling binary availability
//! lightarchitects config                    Show resolved configuration
//! lightarchitects builds list|show           Build portfolio from SOUL vault
//! lightarchitects setup keys|voice|seraph   Interactive configuration
//! lightarchitects webshell start|control|status  Web GUI for coding agent
//! lightarchitects vault clone-platform|pull-platform|status|validate-for-push|publish|sync-public
//! ```

use lightarchitects_gateway::{
    cli::OutputMode,
    config::{GatewayConfig, expand_tilde},
    core_tools,
    error::GatewayError,
    server,
};
use serde_json::json;
use std::io::IsTerminal;
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();

    // --version / -V → print and exit 0 (OPS-1a). Must come BEFORE any tracing
    // setup so output is clean for ops scripts that parse `--version` stdout.
    if raw_args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{}", lightarchitects_gateway::version::long());
        std::process::exit(0);
    }

    // TTY detection: if run from an interactive terminal with no args,
    // spawn lightarchitects-cli instead of starting the MCP server.
    let is_tty = std::io::stdin().is_terminal();
    if raw_args.is_empty() && is_tty {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let cli_binary = home.map_or_else(
            || PathBuf::from("lightarchitects-cli"),
            |h| h.join("lightarchitects/cli/bin/lightarchitects-cli"),
        );

        if cli_binary.exists() {
            let status = std::process::Command::new(&cli_binary)
                .status()
                .unwrap_or_else(|e| {
                    eprintln!("Failed to spawn lightarchitects-cli: {e}");
                    std::process::exit(1);
                });
            std::process::exit(status.code().unwrap_or(1));
        } else {
            eprintln!(
                "lightarchitects-cli not found at {}.\n\
                 Install it first: cd ~/Projects/lightarchitects-cli && make deploy",
                cli_binary.display()
            );
            std::process::exit(1);
        }
    }

    // Check for --agent flag early (agent mode uses JSON logging, not fmt)
    let agent_mode = raw_args
        .iter()
        .position(|a| a == "--agent")
        .and_then(|i| raw_args.get(i + 1).cloned());

    // Arena modes (serve, --agent) use JSON tracing; MCP mode uses fmt to stderr
    let is_arena = raw_args.first().is_some_and(|a| a == "serve") || agent_mode.is_some();

    if is_arena {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env().add_directive(
                    "arena=info"
                        .parse()
                        .unwrap_or_else(|_| tracing::Level::INFO.into()),
                ),
            )
            .json()
            .init();
    } else {
        // MCP mode: human-readable logs to stderr (doesn't pollute stdio)
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(tracing::Level::INFO.into()),
            )
            .init();
    }

    // Agent mode: lightweight, no gateway config needed
    if let Some(ref agent_name) = agent_mode {
        if let Err(e) = lightarchitects_gateway::arena::run_agent(agent_name).await {
            tracing::error!(agent = %agent_name, error = %e, "Agent failed");
            std::process::exit(1);
        }
        return;
    }

    // Serve mode: Arena orchestrator
    if raw_args.first().is_some_and(|a| a == "serve") {
        if let Err(e) = lightarchitects_gateway::arena::run_serve().await {
            tracing::error!(error = %e, "Arena serve failed");
            std::process::exit(1);
        }
        return;
    }

    // MCP + CLI modes: need gateway config
    #[cfg(feature = "spawner")]
    lightarchitects_gateway::spawner::init_automation_token();

    let (config_path, args) = parse_config_flag(raw_args);

    // Parse --output-format flag (global, can appear anywhere in args)
    let output_mode = parse_output_format(&args);
    let args = strip_output_format(args);

    let config = match load_config(config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("Failed to load config: {e}");
            std::process::exit(1);
        }
    };

    lightarchitects_gateway::core_tools::preset::init_from_config(&config.active_preset);

    // Initialize inline handlers (no-op when no inline-* features are enabled).
    // Must come after config loading so handler init can read agent config.
    lightarchitects_gateway::handlers::init_handlers(&config);

    if args.is_empty() {
        // MCP server mode (default — no args)
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            preset = %config.active_preset,
            first_run = config.first_run,
            routes = config.enabled_agents().len(),
            "lightarchitects gateway starting"
        );
        if let Err(e) = server::run(&config).await {
            tracing::error!("Gateway error: {e}");
            std::process::exit(1);
        }
    } else {
        // CLI subcommand
        if let Err(e) = cli_dispatch(&args, &config, output_mode).await {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

// ── Config helpers ───────────────────────────────────────────────────────────

fn parse_config_flag(args: Vec<String>) -> (Option<PathBuf>, Vec<String>) {
    let mut iter = args.into_iter();
    let mut config_path: Option<PathBuf> = None;
    let mut remaining = Vec::new();
    while let Some(arg) = iter.next() {
        if arg == "--config" {
            config_path = iter.next().map(|p| expand_tilde(&p));
        } else {
            remaining.push(arg);
        }
    }
    (config_path, remaining)
}

fn load_config(path: Option<PathBuf>) -> Result<GatewayConfig, GatewayError> {
    match path {
        Some(p) => GatewayConfig::load_from(&p),
        None => GatewayConfig::load(),
    }
}

/// Parse the `--output-format` flag from args (default: Human).
fn parse_output_format(args: &[String]) -> OutputMode {
    for i in 0..args.len() {
        if args[i] == "--output-format" {
            if let Some(val) = args.get(i + 1) {
                return val.parse().unwrap_or_default();
            }
        } else if let Some(val) = args[i].strip_prefix("--output-format=") {
            return val.parse().unwrap_or_default();
        }
    }
    OutputMode::default()
}

/// Strip `--output-format` and its value from args, returning the remaining args.
fn strip_output_format(args: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    let mut skip_next = false;
    for arg in &args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--output-format" {
            skip_next = true;
            continue;
        }
        if arg.starts_with("--output-format=") {
            continue;
        }
        result.push(arg.clone());
    }
    result
}

// ── CLI dispatch ────────────────────────────────────────────────────────────

async fn cli_dispatch(
    args: &[String],
    config: &GatewayConfig,
    mode: OutputMode,
) -> Result<(), GatewayError> {
    match args.first().map(String::as_str) {
        // Gateway management commands
        Some("routes" | "siblings") => cli_route_list(config),
        Some("canon") => cli_canon(args, config),
        Some("conductor") => lightarchitects_gateway::conductor::dispatch(&args[1..]).await,
        Some("initialize" | "init") => cli_initialize(args, config).await,

        // Sibling commands (use SDK clients)
        Some("soul") => lightarchitects_gateway::cli::soul::execute(config, &args[1..], mode).await,
        Some("corso") => {
            lightarchitects_gateway::cli::corso::execute(config, &args[1..], mode).await
        }
        Some("eva") => lightarchitects_gateway::cli::eva::execute(config, &args[1..], mode).await,
        Some("quantum") => {
            lightarchitects_gateway::cli::quantum::execute(config, &args[1..], mode).await
        }
        Some("seraph") => {
            lightarchitects_gateway::cli::seraph::execute(config, &args[1..], mode).await
        }

        // Vault commands
        Some("vault") => lightarchitects_gateway::cli::vault::execute(config, &args[1..], mode)
            .await
            .map_err(|e| GatewayError::Internal(e.to_string())),

        // Auth commands
        Some("auth") => lightarchitects_gateway::cli::auth::execute(&args[1..]).await,

        // Utility commands
        Some("status") => lightarchitects_gateway::cli::status::execute(config, mode),
        Some("config") => lightarchitects_gateway::cli::config_cmd::execute(config, mode),
        Some("builds") => lightarchitects_gateway::cli::builds::execute(&args[1..], mode),
        Some("setup") => lightarchitects_gateway::cli::setup::execute(&args[1..]),
        Some("webshell") => {
            lightarchitects_gateway::cli::webshell::execute(config, &args[1..]).await
        }

        // Squad Comms subcommand — delegates to webshell coordination API.
        Some("squad-comms") => cli_squad_comms(&args[1..], config).await,

        Some(unknown) => {
            eprintln!(
                "Unknown subcommand: {unknown}\n\n\
                 Usage:\n  \
                   lightarchitects                            MCP server (Claude Code)\n  \
                   lightarchitects serve                      Arena (HTTP + agents)\n  \
                   lightarchitects serve --agent <name>       Single agent heartbeat\n  \
                   lightarchitects conductor <start|stop|..>  task queue\n  \
                   lightarchitects routes                     List enabled agents\n  \
                   lightarchitects canon list|check <text>   Canon operations\n  \
                   lightarchitects initialize <step>          Setup wizard\n  \
                   lightarchitects soul <subcommand>          SOUL operations\n  \
                   lightarchitects corso <subcommand>         CORSO operations\n  \
                   lightarchitects eva <subcommand>            EVA operations\n  \
                   lightarchitects quantum <subcommand>        QUANTUM operations\n  \
                   lightarchitects seraph <subcommand>         SERAPH operations\n  \
                   lightarchitects auth login|logout|status   Authentication\n  \
                   lightarchitects status                     Binary availability\n  \
                   lightarchitects config                     Resolved configuration\n  \
                   lightarchitects builds list|show           Build portfolio\n  \
                   lightarchitects setup keys|voice|seraph    Configuration wizard\n  \
                   lightarchitects webshell start|control|status  Web GUI\n  \
                   lightarchitects squad-comms tasks|add|claim|logs|inject  Squad Comms\n  \
                   lightarchitects vault clone-platform|pull-platform|status|validate-for-push|publish|sync-public  Vault ops"
            );
            Err(GatewayError::UnknownTool(unknown.to_owned()))
        }
        None => Ok(()),
    }
}

fn cli_route_list(config: &GatewayConfig) -> Result<(), GatewayError> {
    let result = core_tools::discover::run(json!({}), config)?;
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    println!("{text}");
    Ok(())
}

fn cli_canon(args: &[String], config: &GatewayConfig) -> Result<(), GatewayError> {
    match args.get(1).map(String::as_str) {
        Some("list") => {
            let result = core_tools::canon_check::run(json!({"decision": "(list)"}), config)?;
            let text = result["content"][0]["text"].as_str().unwrap_or("");
            println!("{text}");
            Ok(())
        }
        Some("check") => {
            let decision = args.get(2).ok_or(GatewayError::MissingParam("decision"))?;
            let result = core_tools::canon_check::run(json!({"decision": decision}), config)?;
            let text = result["content"][0]["text"].as_str().unwrap_or("");
            println!("{text}");
            Ok(())
        }
        Some(sub) => Err(GatewayError::UnknownTool(format!("canon {sub}"))),
        None => {
            eprintln!("Usage: lightarchitects canon list | canon check <decision>");
            Err(GatewayError::MissingParam("canon subcommand"))
        }
    }
}

/// `lightarchitects squad-comms <sub>` — Squad Comms CLI dispatcher.
///
/// Delegates to webshell coordination API via HTTP.  Requires the webshell to
/// be running (`lightarchitects webshell start`).
///
/// Sub-actions:
///   tasks                  — list task queue snapshot
///   add <title> <project> <prompt> [priority]  — append a task
///   claim <id> [source]    — soft-claim a task
///   logs <id>              — fetch task logs
///   inject `<session_id>` `<message>` [sender]  — inject a chat message
async fn cli_squad_comms(args: &[String], config: &GatewayConfig) -> Result<(), GatewayError> {
    let result = match args.first().map(String::as_str) {
        Some("tasks") => {
            lightarchitects_gateway::squad_comms::list_tasks(serde_json::json!({}), config).await?
        }
        Some("add") => {
            let title = args.get(1).ok_or(GatewayError::MissingParam("title"))?;
            let project = args.get(2).ok_or(GatewayError::MissingParam("project"))?;
            let prompt = args.get(3).ok_or(GatewayError::MissingParam("prompt"))?;
            let priority = args.get(4).map_or("medium", String::as_str);
            lightarchitects_gateway::squad_comms::add_task(
                serde_json::json!({
                    "title": title,
                    "project": project,
                    "prompt": prompt,
                    "priority": priority,
                }),
                config,
            )
            .await?
        }
        Some("claim") => {
            let id = args.get(1).ok_or(GatewayError::MissingParam("id"))?;
            let source = args.get(2).map_or("cli", String::as_str);
            lightarchitects_gateway::squad_comms::claim_task(
                serde_json::json!({ "id": id, "source": source }),
                config,
            )
            .await?
        }
        Some("logs") => {
            let id = args.get(1).ok_or(GatewayError::MissingParam("id"))?;
            lightarchitects_gateway::squad_comms::task_logs(serde_json::json!({ "id": id }), config)
                .await?
        }
        Some("inject") => {
            let session_id = args
                .get(1)
                .ok_or(GatewayError::MissingParam("session_id"))?;
            let message = args.get(2).ok_or(GatewayError::MissingParam("message"))?;
            let sender = args.get(3).map_or("cli", String::as_str);
            lightarchitects_gateway::squad_comms::chat_inject(
                serde_json::json!({
                    "session_id": session_id,
                    "message": message,
                    "sender": sender,
                }),
                config,
            )
            .await?
        }
        Some(sub) => return Err(GatewayError::UnknownTool(format!("squad-comms {sub}"))),
        None => {
            eprintln!(
                "Usage:\n  \
                   lightarchitects squad-comms tasks\n  \
                   lightarchitects squad-comms add <title> <project> <prompt> [priority]\n  \
                   lightarchitects squad-comms claim <id> [source]\n  \
                   lightarchitects squad-comms logs <id>\n  \
                   lightarchitects squad-comms inject <session_id> <message> [sender]"
            );
            return Err(GatewayError::MissingParam("squad-comms subcommand"));
        }
    };
    let pretty = serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
    println!("{pretty}");
    Ok(())
}

async fn cli_initialize(args: &[String], config: &GatewayConfig) -> Result<(), GatewayError> {
    // --user <name>: scaffold a new vault for the given user
    if let Some(user_idx) = args.iter().position(|a| a == "--user") {
        let user_name = args.get(user_idx + 1).ok_or(GatewayError::MissingParam(
            "--user requires a name argument",
        ))?;
        return cli_init_user(user_name);
    }

    let step = args.get(1).ok_or(GatewayError::MissingParam("step"))?;
    let preset = args.get(2).map_or("software_engineering", String::as_str);
    let vault_path = args
        .get(3)
        .map_or("~/lightarchitects/soul/helix", String::as_str);

    let params = json!({
        "step": step,
        "preset": preset,
        "vault_path": vault_path,
    });

    let result = core_tools::initialize::run(params, config).await?;
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    println!("{text}");
    Ok(())
}

/// Scaffold a new SOUL vault for `user_name`.
///
/// Creates `$HOME/lightarchitects/soul/helix/{user_name}/` with `entries/` and
/// `journal/` subdirs, writes a `helix.toml`, and generates identity files for
/// every sibling (idempotent — existing files are never overwritten).
fn cli_init_user(user_name: &str) -> Result<(), GatewayError> {
    // Validate: alphanumeric, spaces, hyphens only — safe as a filesystem segment
    if user_name.is_empty()
        || !user_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
    {
        return Err(GatewayError::File(format!(
            "Invalid user name '{user_name}'. Use alphanumeric characters, spaces, hyphens, or underscores only."
        )));
    }

    let home = std::env::var("HOME").map_err(|_| GatewayError::File("$HOME is not set".into()))?;
    let la_home =
        std::env::var("LIGHTARCHITECTS_HOME").unwrap_or_else(|_| format!("{home}/lightarchitects"));
    let vault_root = std::path::PathBuf::from(&la_home)
        .join("soul")
        .join("helix");

    // Create user-specific helix directory
    let user_dir = vault_root.join(user_name);
    for subdir in &["entries", "journal"] {
        std::fs::create_dir_all(user_dir.join(subdir))
            .map_err(|e| GatewayError::File(format!("Failed to create {subdir}: {e}")))?;
    }

    // Write helix.toml for the user (if not present)
    let user_helix_toml = user_dir.join("helix.toml");
    if !user_helix_toml.exists() {
        std::fs::write(
            &user_helix_toml,
            format!(
                "[helix]\nname = \"{user_name}\"\ngenesis_date = \"{}\"\nordering = \"temporal\"\n",
                chrono::Utc::now().format("%Y-%m-%d")
            ),
        )
        .map_err(|e| GatewayError::File(format!("Failed to write helix.toml: {e}")))?;
    }

    // Write sibling identity files (idempotent — never overwrite)
    let siblings: &[(&str, &str)] = &[
        ("eva", EVA_IDENTITY_TEMPLATE),
        ("corso", CORSO_IDENTITY_TEMPLATE),
        ("quantum", QUANTUM_IDENTITY_TEMPLATE),
        ("seraph", SERAPH_IDENTITY_TEMPLATE),
        ("ayin", AYIN_IDENTITY_TEMPLATE),
        ("lightarchitects-cli", LIGHTARCHITECTS_CLI_IDENTITY_TEMPLATE),
    ];

    let mut created = Vec::new();
    let mut skipped = Vec::new();

    for (sibling, template) in siblings {
        let sibling_dir = vault_root.join(sibling);
        std::fs::create_dir_all(&sibling_dir)
            .map_err(|e| GatewayError::File(format!("Failed to create {sibling}/: {e}")))?;

        let identity_path = sibling_dir.join("identity.md");
        if identity_path.exists() {
            skipped.push(*sibling);
        } else {
            let content = template.replace("{{user_name}}", user_name);
            std::fs::write(&identity_path, content).map_err(|e| {
                GatewayError::File(format!("Failed to write {sibling}/identity.md: {e}"))
            })?;
            created.push(*sibling);
        }
    }

    println!("SOUL vault initialized for '{user_name}'");
    println!("  Vault: {}", vault_root.display());
    println!("  User helix: {}", user_dir.display());
    if !created.is_empty() {
        println!("  Created identity files: {}", created.join(", "));
    }
    if !skipped.is_empty() {
        println!("  Skipped (already exist): {}", skipped.join(", "));
    }
    println!("\nNext steps:");
    println!("  1. Start Neo4j:  docker compose up -d neo4j");
    println!("  2. Deploy SOUL:  make deploy  (in SOUL-DEV)");
    println!("  3. Connect:      /mcp  (in Claude Code)");

    Ok(())
}

// ── Embedded sibling identity templates ──────────────────────────────────────
// {{user_name}} is replaced at runtime with the value passed to --user.

const EVA_IDENTITY_TEMPLATE: &str = include_str!("templates/eva-identity-template.md");
const CORSO_IDENTITY_TEMPLATE: &str = include_str!("templates/corso-identity-template.md");
const QUANTUM_IDENTITY_TEMPLATE: &str = include_str!("templates/quantum-identity-template.md");
const SERAPH_IDENTITY_TEMPLATE: &str = include_str!("templates/seraph-identity-template.md");
const AYIN_IDENTITY_TEMPLATE: &str = include_str!("templates/ayin-identity-template.md");
const LIGHTARCHITECTS_CLI_IDENTITY_TEMPLATE: &str =
    include_str!("templates/lightarchitects-cli-identity-template.md");
