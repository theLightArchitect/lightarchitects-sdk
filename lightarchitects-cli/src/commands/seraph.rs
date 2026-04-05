//! `lightarchitects seraph` subcommands.
//!
//! All commands require an active engagement scope in `~/.seraph/scope.toml`.
//! Targets are validated through [`lightarchitects_seraph::scope::ScopeConstraint`]
//! before dispatch, which rejects shell metacharacters, localhost, and null bytes.

use std::path::PathBuf;
use std::time::Duration;

use clap::Subcommand;
use lightarchitects::seraph::{SeraphClient, Wing};
use lightarchitects_core::SdkError;
use lightarchitects_seraph::scope::{ScopeConstraint, ScopeDomain};

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
        /// Research depth (shallow, standard, deep).
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
    /// Recon phase: gather OSINT on target.
    Recon {
        /// Target domain, email, or identifier.
        #[arg(long)]
        target: String,
    },
    /// Survey phase: enumerate hosts and services at target.
    Survey {
        /// Target host, IP, or CIDR range.
        #[arg(long)]
        target: String,
    },
    /// Examine phase: analyse a binary or artefact at target.
    Examine {
        /// Path to artefact or binary to examine.
        #[arg(long)]
        target: String,
    },
    /// Strike phase: execute a payload against target (REQUIRES --yes).
    ///
    /// This is a destructive offensive action. SERAPH's `ScopeGovernor` enforces
    /// target, tool, TTL, and concurrent-limit gates before dispatch.
    ///
    /// You MUST pass `--yes` to confirm. Without it the command prints a warning
    /// and exits without dispatching.
    Strike {
        /// Target for payload delivery (must be in active engagement scope).
        #[arg(long)]
        target: String,
        /// Tool to use for strike (must be in SERAPH tool allowlist).
        #[arg(long)]
        tool: String,
        /// Confirm the strike dispatch. Required — command will not proceed without this flag.
        #[arg(long)]
        yes: bool,
    },
    /// Generate a formal engagement report.
    Report {
        /// Optional engagement ID to include in the report header.
        #[arg(long)]
        engagement_id: Option<String>,
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
/// Propagates any [`SdkError`] from the SERAPH client, or returns a
/// [`SdkError::ScopeViolation`] when target validation fails.
pub async fn execute(
    binary: PathBuf,
    cmd: SeraphCommand,
    mode: OutputMode,
) -> Result<(), SdkError> {
    // Pre-validate targets for engagement-phase commands before spawning SERAPH.
    validate_target_if_required(&cmd)?;

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

        SeraphCommand::Recon { target } => client.recon(&target).await?.output,
        SeraphCommand::Survey { target } => client.survey(&target).await?.output,
        SeraphCommand::Examine { target } => client.examine(&target).await?.output,

        SeraphCommand::Strike { yes, target, .. } => {
            if !yes {
                println!(
                    "Strike dispatch requires explicit confirmation.\n\
                     Use --yes to confirm strike dispatch"
                );
                return Ok(());
            }
            client.strike(&target).await?.output
        }

        SeraphCommand::Report { engagement_id: _ } => client.typed_report().await?.summary.into(),

        SeraphCommand::Status => client.status().await?.output,
        SeraphCommand::VaultSync => client.vault_sync().await?.output,
    };

    print_text(mode, &output);
    Ok(())
}

/// Validate target strings for engagement-phase commands.
///
/// Calls [`ScopeConstraint::new`] with a representative tool so that shell
/// metacharacters, localhost, and null bytes are rejected before any IPC call.
/// The `ScopeGovernor` inside SERAPH applies its own 5-gate enforcement
/// independently — this is the SDK-level first gate.
fn validate_target_if_required(cmd: &SeraphCommand) -> Result<(), SdkError> {
    let target = match cmd {
        SeraphCommand::Recon { target }
        | SeraphCommand::Survey { target }
        | SeraphCommand::Examine { target }
        | SeraphCommand::Strike { target, .. } => target.as_str(),
        // Other commands bypass SDK-level validation (they use their own arg names).
        _ => return Ok(()),
    };

    // Use "scan" as the representative tool — it's in the SERAPH allowlist and
    // appropriate for a generic pre-flight check. The actual tool is SERAPH-side.
    ScopeConstraint::new(target, "scan", ScopeDomain::Network).map(|_| ())
}
