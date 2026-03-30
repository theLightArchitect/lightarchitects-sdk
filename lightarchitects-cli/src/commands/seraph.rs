//! `lightarchitects seraph` subcommands.

use std::path::PathBuf;
use std::time::Duration;

use clap::Subcommand;
use lightarchitects::seraph::{SeraphClient, Wing};
use lightarchitects_core::SdkError;

use crate::output::{OutputMode, print_text};

/// SERAPH pentest-orchestration commands.
///
/// **All commands require an active engagement scope in `~/.seraph/scope.toml`.**
#[derive(Debug, Subcommand)]
pub enum SeraphCommand {
    /// Packet capture and traffic interception.
    Capture {
        /// Target host, interface, or CIDR range.
        target: String,
    },
    /// Host and service discovery scan.
    Scan {
        /// Target host, IP, or CIDR range.
        target: String,
    },
    /// Artefact and binary analysis.
    Analyze {
        /// Path to artefact or binary.
        target: String,
    },
    /// Open-source intelligence gathering.
    Osint {
        /// Target domain, email, or identifier.
        target: String,
        /// Research depth (shallow, deep).
        #[arg(long)]
        depth: Option<String>,
    },
    /// Continuous network monitoring.
    Monitor {
        /// Target network segment or host.
        target: String,
    },
    /// Payload delivery and exploitation (authorised scope only).
    Execute {
        /// Target for payload delivery.
        target: String,
    },
    /// Show SERAPH engagement status and scope governance state.
    Status,
    /// Synchronise the SERAPH knowledge vault.
    VaultSync,
}

/// Execute a SERAPH subcommand.
///
/// # Errors
///
/// Propagates any [`SdkError`] from the SERAPH client.
pub async fn execute(
    binary: PathBuf,
    cmd: SeraphCommand,
    mode: OutputMode,
) -> Result<(), SdkError> {
    let client = SeraphClient::builder()
        .binary_path(binary)
        .timeout(Duration::from_secs(180))
        .build()
        .await?;

    let output = match cmd {
        SeraphCommand::Capture { target } => client.wing(Wing::Capture, &target).await?.output,
        SeraphCommand::Scan { target } => client.wing(Wing::Scan, &target).await?.output,
        SeraphCommand::Analyze { target } => client.wing(Wing::Analyze, &target).await?.output,
        SeraphCommand::Osint { target, depth } => {
            client.osint(&target, depth.as_deref()).await?.output
        }
        SeraphCommand::Monitor { target } => client.wing(Wing::Monitor, &target).await?.output,
        SeraphCommand::Execute { target } => client.wing(Wing::Execute, &target).await?.output,
        SeraphCommand::Status => client.status().await?.output,
        SeraphCommand::VaultSync => client.vault_sync().await?.output,
    };

    print_text(mode, &output);
    Ok(())
}
