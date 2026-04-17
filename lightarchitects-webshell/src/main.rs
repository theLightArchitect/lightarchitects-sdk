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
    server,
};
use tracing::error;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
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
    let token = &config.token;
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
    eprintln!("  ╔══════════════════════════════════════════════════════════════╗");
    eprintln!("  ║  Light Architects — Webshell                                  ║");
    eprintln!("  ║                                                              ║");
    eprintln!("  ║  Open in your browser:                                       ║");
    eprintln!("  ║                                                              ║");
    eprintln!("  ║  http://localhost:{port}#token={token}    ║");
    eprintln!("  ║                                                              ║");
    eprintln!("  ║  Token: {token_preview}  (via {source_label})                    ║");
    eprintln!("  ║  Keychain: ~/.lightarchitects/webshell/.token                ║");
    eprintln!("  ╚══════════════════════════════════════════════════════════════╝");
    eprintln!();

    if let Err(e) = server::run(config).await {
        error!(error = %e, "webshell server exited with error");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,lightarchitects_webshell=debug"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
