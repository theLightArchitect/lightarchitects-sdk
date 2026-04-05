//! `lightarchitects` — Light Architects SDK command-line interface.
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
#[command(name = "lightarchitects", version, about)]
struct Cli {
    /// Output format: `text` (default) or `json`.
    ///
    /// `json` serializes the typed response struct via `serde_json` and redacts
    /// fields whose key matches: `api_key`, token, secret, password, credential.
    #[arg(long = "output-format", global = true, default_value = "text")]
    output_format: OutputMode,

    #[command(subcommand)]
    command: commands::Commands,
}

#[tokio::main]
async fn main() {
    lightarchitects::init_tracing();

    let cli = Cli::parse();
    let cfg = CliConfig::resolve();
    let mode = cli.output_format;

    if let Err(err) = commands::dispatch(cli.command, &cfg, mode).await {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
