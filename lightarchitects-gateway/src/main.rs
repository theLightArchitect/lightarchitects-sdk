//! Entry point for the `lightarchitects` MCP gateway binary.
//!
//! When invoked with no arguments (or only `--config <path>`): runs as an MCP server over stdio.
//! When invoked with subcommands: dispatches a CLI subcommand and exits.
//!
//! # Flags
//!
//! ```text
//! --config <path>   Load config from <path> instead of ~/.lightarchitects/config.toml
//! ```
//!
//! # CLI subcommands
//!
//! ```text
//! lightarchitects siblings                  List enabled siblings
//! lightarchitects canon list                List ratified canons
//! lightarchitects canon check <decision>    Check decision against canon
//! lightarchitects initialize <step>         Run setup wizard step
//! lightarchitects initialize <step> <preset>
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
    // Initialise tracing to stderr so it does not pollute the MCP stdout stream.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    // Generate a per-startup automation nonce for HITL bypass hardening.
    lightarchitects_gateway::spawner::init_automation_token();

    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let (config_path, args) = parse_config_flag(raw_args);

    let config = match load_config(config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("Failed to load config: {e}");
            std::process::exit(1);
        }
    };

    if args.is_empty() {
        // MCP server mode.
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            siblings = config.enabled_siblings().len(),
            "lightarchitects gateway starting"
        );
        if let Err(e) = server::run(&config).await {
            tracing::error!("Gateway error: {e}");
            std::process::exit(1);
        }
    } else {
        // CLI mode.
        if let Err(e) = cli_dispatch(&args, &config).await {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

/// Strip `--config <path>` from `args`, returning the override path and remaining args.
///
/// If `--config` appears multiple times, the last occurrence wins.
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

/// Load [`GatewayConfig`] from an explicit path, or the default `~/.lightarchitects/config.toml`.
fn load_config(path: Option<PathBuf>) -> Result<GatewayConfig, GatewayError> {
    match path {
        Some(p) => GatewayConfig::load_from(&p),
        None => GatewayConfig::load(),
    }
}

// ── CLI dispatch ──────────────────────────────────────────────────────────────

/// Dispatch a CLI subcommand and print the result to stdout.
async fn cli_dispatch(args: &[String], config: &GatewayConfig) -> Result<(), GatewayError> {
    match args.first().map(String::as_str) {
        Some("siblings") => cli_sibling_list(config),
        Some("canon") => cli_canon(args, config),
        Some("conductor") => lightarchitects_gateway::conductor::dispatch(&args[1..]).await,
        Some("initialize" | "init") => cli_initialize(args, config).await,
        Some(unknown) => {
            eprintln!(
                "Unknown subcommand: {unknown}\n\n\
                 Usage:\n  \
                   lightarchitects siblings\n  \
                   lightarchitects canon list\n  \
                   lightarchitects canon check <decision>\n  \
                   lightarchitects conductor <start|stop|status|add|logs>\n  \
                   lightarchitects initialize <step> [preset]\n  \
                   lightarchitects initialize <step> [preset] [vault_path]"
            );
            Err(GatewayError::UnknownTool(unknown.to_owned()))
        }
        None => Ok(()),
    }
}

/// Print the list of enabled siblings.
fn cli_sibling_list(config: &GatewayConfig) -> Result<(), GatewayError> {
    let result = core_tools::discover::run(json!({}), config)?;
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    println!("{text}");
    Ok(())
}

/// Dispatch canon subcommands: `list` or `check <decision>`.
fn cli_canon(args: &[String], config: &GatewayConfig) -> Result<(), GatewayError> {
    match args.get(1).map(String::as_str) {
        Some("list") => {
            // canon check with an empty decision returns just the headers.
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

/// Dispatch the initialize wizard: `initialize <step> [preset] [vault_path]`.
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
