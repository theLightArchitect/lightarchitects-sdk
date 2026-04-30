//! Binary entry point for the webshell server.
//!
//! Parses CLI args via [`Cli`], resolves configuration (auto-generating
//! and persisting the HMAC token if needed), prints the access URL, and
//! delegates to [`server::run`]. Exit codes:
//!
//! - `0` — clean shutdown.
//! - `1` — server run failure (bind error, IO error mid-run).
//! - `2` — configuration error (invalid cwd).

use std::process::ExitCode;

use clap::Parser;
use lightarchitects_webshell::{
    config::{Cli, Config, TokenSource},
    server::{self, ServerError},
};
use tracing::error;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    // --version / -V → print enriched version (sha + ui-bundle-hash + date) and
    // exit 0. Must come BEFORE clap parsing because clap's derive `version`
    // attribute only knows CARGO_PKG_VERSION; we want our build.rs-injected
    // metadata too. (OPS-1a, ops audit O-1.)
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    if raw_args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{}", lightarchitects_webshell::version::long());
        return ExitCode::SUCCESS;
    }

    init_tracing();

    let cli = Cli::parse();
    let config = match Config::resolve(cli) {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "failed to resolve webshell configuration");
            return ExitCode::from(2);
        }
    };

    // Print the access URL with the auth token hash fragment.
    // The browser frontend reads this from the URL hash and stores it
    // in sessionStorage for subsequent WebSocket/SSE connections.
    let port = config.port;
    let token = config.token.clone();
    let token_preview = if token.len() > 8 {
        format!("{}…{}", &token[..4], &token[token.len() - 4..])
    } else {
        token.clone()
    };
    let source_label = match config.token_source {
        TokenSource::EnvVar => "env var",
        TokenSource::Keyring => "keychain",
        TokenSource::File => "file",
        TokenSource::Ephemeral => "ephemeral",
    };
    eprintln!();
    eprintln!("Light Architects — Webshell");
    eprintln!();
    eprintln!("  Open in your browser:");
    eprintln!("    http://localhost:{port}#token={token}");
    eprintln!();
    eprintln!("  Token:    {token_preview}  (via {source_label})");
    eprintln!("  Keychain: ~/.lightarchitects/webshell/.token");
    eprintln!();

    match server::run_with_port_retry(config).await {
        Ok(bound_port) => {
            if bound_port != port {
                // A fallback port was used — re-print the access URL with the
                // actual port so the user knows where to connect.
                eprintln!();
                eprintln!("  Note: port {port} was in use — started on port {bound_port}");
                eprintln!("  Open in your browser:");
                eprintln!("    http://localhost:{bound_port}#token={token}");
                eprintln!();
            }
        }
        Err(ServerError::PortInUse { first_port, tried }) => {
            eprintln!();
            eprintln!("  ERROR: port {first_port} (and {tried} fallback(s)) are all in use.");
            eprintln!();
            eprintln!("  To diagnose:");
            eprintln!("    lsof -i :{first_port}");
            eprintln!();
            eprintln!("  To use a different port:");
            eprintln!("    lightarchitects-webshell --port <PORT>");
            eprintln!();
            error!(first_port, tried, "all ports in retry window are in use");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            error!(error = %e, "webshell server exited with error");
            return ExitCode::FAILURE;
        }
    }

    ExitCode::SUCCESS
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,lightarchitects_webshell=debug"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
