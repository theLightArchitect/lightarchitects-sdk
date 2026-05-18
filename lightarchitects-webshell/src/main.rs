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
    preflight,
    preflight::OverallStatus,
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
    lightarchitects_webshell::profile_checkpoint!("tracing_ready");

    let (docker_capable, preflight_basic) = tokio::join!(
        lightarchitects_webshell::container::probe::probe_docker(),
        preflight::run_basic(),
    );
    lightarchitects_webshell::profile_checkpoint!("docker_probed");

    let cli = Cli::parse();
    let config = match Config::resolve(cli) {
        Ok(c) => {
            lightarchitects_webshell::profile_checkpoint!("config_resolved");
            c
        }
        Err(e) => {
            error!(error = %e, "failed to resolve webshell configuration");
            return ExitCode::from(2);
        }
    };

    let preflight = preflight::run_full(&config.agent, docker_capable, preflight_basic).await;
    lightarchitects_webshell::profile_checkpoint!("preflight_complete");

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
    print_preflight_banner(&preflight);

    let (bound_port, driver) =
        match server::run_with_port_retry(config, docker_capable, preflight).await {
            Ok(pair) => pair,
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
                error!(error = %e, "webshell server bind error");
                return ExitCode::FAILURE;
            }
        };

    // Bind succeeded — checkpoint and, if a fallback port was used, re-print
    // the access URL so the user knows where to connect.
    lightarchitects_webshell::profile_checkpoint!("server_bound");
    if bound_port != port {
        eprintln!();
        eprintln!("  Note: port {port} was in use — started on port {bound_port}");
        eprintln!("  Open in your browser:");
        eprintln!("    http://localhost:{bound_port}#token={token}");
        eprintln!();
    }

    let shutdown_fut = lightarchitects_webshell::init::shutdown::wait_for_shutdown();

    tokio::select! {
        result = driver => {
            if let Err(e) = result {
                error!(error = %e, "webshell server exited with error");
                return ExitCode::FAILURE;
            }
            // Clean server exit (unexpected but harmless).
            ExitCode::SUCCESS
        }
        () = shutdown_fut => {
            tracing::info!("shutdown signal received, exiting");
            ExitCode::SUCCESS
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,lightarchitects_webshell=debug"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Prints a one-line preflight status banner, plus per-check detail for failures.
fn print_preflight_banner(report: &preflight::PreflightReport) {
    match report.overall {
        OverallStatus::Ready => {
            eprintln!(
                "  Status:   ✅ Ready  (all {} checks pass)",
                report.checks.len()
            );
        }
        OverallStatus::Degraded => {
            eprintln!("  Status:   ⚠  Degraded");
        }
        OverallStatus::Blocked => {
            eprintln!("  Status:   ❌ Blocked");
        }
    }
    for c in &report.checks {
        use preflight::CheckStatus;
        match c.status {
            CheckStatus::Pass => {}
            CheckStatus::Warn => eprintln!("  ⚠  {:20}  {}", c.id, c.detail),
            CheckStatus::Fail => eprintln!("  ❌  {:20}  {}", c.id, c.detail),
        }
    }
    if !matches!(report.overall, OverallStatus::Ready) {
        eprintln!();
    }
}
