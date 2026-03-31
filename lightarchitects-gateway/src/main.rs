//! Entry point for the `lightarchitects` unified gateway binary.
//!
//! Three operating modes:
//!
//! ```text
//! lightarchitects                           MCP server (Claude Code, stdio)
//! lightarchitects serve                     Arena (HTTP API + scheduler + agents)
//! lightarchitects serve --agent eva         Single agent heartbeat loop
//! lightarchitects conductor <cmd>           LVL8 autonomous task queue
//! lightarchitects routes                    List enabled agents
//! lightarchitects canon list|check          Canon operations
//! lightarchitects initialize <step>         Setup wizard
//! ```

use lightarchitects_gateway::{
    config::{GatewayConfig, expand_tilde},
    core_tools,
    error::GatewayError,
    server,
};
use serde_json::json;
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();

    // Check for --agent flag early (agent mode uses JSON logging, not fmt)
    let agent_mode = raw_args
        .iter()
        .position(|a| a == "--agent")
        .and_then(|i| raw_args.get(i + 1).cloned());

    // Arena modes (serve, --agent) use JSON tracing; MCP mode uses fmt to stderr
    let is_arena = raw_args.first().map_or(false, |a| a == "serve") || agent_mode.is_some();

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
    if raw_args.first().map_or(false, |a| a == "serve") {
        if let Err(e) = lightarchitects_gateway::arena::run_serve().await {
            tracing::error!(error = %e, "Arena serve failed");
            std::process::exit(1);
        }
        return;
    }

    // MCP + CLI modes: need gateway config
    lightarchitects_gateway::spawner::init_automation_token();

    let (config_path, args) = parse_config_flag(raw_args);

    let config = match load_config(config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("Failed to load config: {e}");
            std::process::exit(1);
        }
    };

    lightarchitects_gateway::core_tools::preset::init_from_config(&config.active_preset);

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
        if let Err(e) = cli_dispatch(&args, &config).await {
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

// ── CLI dispatch ────────────────────────────────────────────────────────────

async fn cli_dispatch(args: &[String], config: &GatewayConfig) -> Result<(), GatewayError> {
    match args.first().map(String::as_str) {
        Some("routes" | "siblings") => cli_route_list(config),
        Some("canon") => cli_canon(args, config),
        Some("conductor") => lightarchitects_gateway::conductor::dispatch(&args[1..]).await,
        Some("initialize" | "init") => cli_initialize(args, config).await,
        Some(unknown) => {
            eprintln!(
                "Unknown subcommand: {unknown}\n\n\
                 Usage:\n  \
                   lightarchitects                            MCP server (Claude Code)\n  \
                   lightarchitects serve                      Arena (HTTP + agents)\n  \
                   lightarchitects serve --agent <name>       Single agent heartbeat\n  \
                   lightarchitects conductor <start|stop|..>  LVL8 task queue\n  \
                   lightarchitects routes                     List enabled agents\n  \
                   lightarchitects canon list|check <text>    Canon operations\n  \
                   lightarchitects initialize <step> [preset] Setup wizard"
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

async fn cli_initialize(args: &[String], config: &GatewayConfig) -> Result<(), GatewayError> {
    let step = args.get(1).ok_or(GatewayError::MissingParam("step"))?;
    let preset = args.get(2).map_or("software_engineering", String::as_str);
    let vault_path = args.get(3).map_or("~/.soul/helix", String::as_str);

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
