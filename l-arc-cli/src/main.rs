//! `l-arc` — Light Architects SDK command-line interface.
//!
//! Provides typed access to all five MCP siblings:
//! SOUL, CORSO, EVA, QUANTUM, and SERAPH.

use clap::Parser;

mod commands;
mod config;
mod output;

use config::CliConfig;
use output::OutputMode;

/// Light Architects SDK CLI — typed access to all MCP siblings.
#[derive(Parser)]
#[command(name = "l-arc", version, about)]
struct Cli {
    /// Output raw JSON instead of formatted text.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: commands::Commands,
}

#[tokio::main]
async fn main() {
    l_arc::init_tracing();

    let cli = Cli::parse();
    let cfg = CliConfig::resolve();
    let mode = if cli.json {
        OutputMode::Json
    } else {
        OutputMode::Human
    };

    if let Err(err) = commands::dispatch(cli.command, &cfg, mode).await {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
