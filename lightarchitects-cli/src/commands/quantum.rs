//! `lightarchitects quantum` subcommands.

use std::path::PathBuf;
use std::time::Duration;

use clap::Subcommand;
use lightarchitects::quantum::QuantumClient;
use lightarchitects_core::SdkError;

use crate::output::{OutputMode, print_text};

/// QUANTUM investigation-toolkit commands.
#[derive(Debug, Subcommand)]
pub enum QuantumCommand {
    /// Begin a forensic investigation on a subject (Phase 1 — triage).
    Scan {
        /// Subject to investigate (error, system, behaviour).
        subject: String,
    },
    /// Sweep for evidence patterns across collected data (Phase 2).
    Sweep {
        /// Evidence domain or scope to sweep.
        subject: String,
    },
    /// Trace a call chain or execution path (Phase 3).
    Trace {
        /// Execution path or identifier to trace.
        subject: String,
    },
    /// Deep-probe a target for root cause analysis (Phase 4).
    Probe {
        /// Symptom, file, symbol, or process to probe.
        #[arg(long)]
        subject: String,
    },
    /// Form hypotheses from collected evidence (Phase 5).
    Theorize {
        /// Subject of the hypothesis.
        #[arg(long)]
        subject: String,
        /// Optional prior evidence context.
        #[arg(long)]
        context: Option<String>,
    },
    /// Verify a hypothesis against evidence (Phase 6).
    Verify {
        /// Hypothesis statement to verify.
        #[arg(long)]
        hypothesis: String,
    },
    /// Close the investigation and produce a final report (Phase 7).
    Close {
        /// Investigation subject or final finding to close on.
        #[arg(long)]
        subject: String,
    },
    /// Quick single-question investigation (compressed scan→verify→close).
    Quick {
        /// Question to investigate.
        question: String,
    },
    /// Execute a named investigation workflow.
    Workflow {
        /// Workflow name (e.g. auth-audit, dep-scan).
        #[arg(long)]
        name: String,
    },
    /// Discover patterns in a codebase path, log stream, or dataset.
    Discover {
        /// Target to discover patterns in.
        #[arg(long)]
        subject: String,
    },
    /// Research a topic across documentation, helix, web, and papers.
    Research {
        /// Research query.
        #[arg(long)]
        query: String,
    },
}

/// Execute a QUANTUM subcommand.
///
/// # Errors
///
/// Propagates any [`SdkError`] from the QUANTUM client.
pub async fn execute(
    binary: PathBuf,
    cmd: QuantumCommand,
    mode: OutputMode,
) -> Result<(), SdkError> {
    let client = QuantumClient::builder()
        .binary_path(binary)
        .timeout(Duration::from_secs(120))
        .build()
        .await?;

    let output = match cmd {
        QuantumCommand::Scan { subject } => client.triage(&subject).await?.output,
        QuantumCommand::Sweep { subject } => client.sweep(&subject).await?.output,
        QuantumCommand::Trace { subject } => client.trace(&subject).await?.output,
        QuantumCommand::Probe { subject } => client.probe(&subject).await?.output,
        QuantumCommand::Theorize { subject, context } => {
            client.theorize(&subject, context.as_deref()).await?.output
        }
        QuantumCommand::Verify { hypothesis } => client.verify(&hypothesis).await?.output,
        QuantumCommand::Close { subject } => client.close(&subject).await?.output,
        QuantumCommand::Quick { question } => client.quick(&question).await?.output,
        QuantumCommand::Workflow { name } => client.workflow(&name).await?.output,
        QuantumCommand::Discover { subject } => client.discover(&subject).await?.output,
        QuantumCommand::Research { query } => client.research(&query).await?.output,
    };

    print_text(mode, &output);
    Ok(())
}
