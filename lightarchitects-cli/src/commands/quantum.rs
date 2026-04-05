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
    /// Begin a forensic investigation on a subject.
    Scan {
        /// Subject to investigate (error, system, behaviour).
        subject: String,
    },
    /// Sweep for evidence patterns across collected data.
    Sweep {
        /// Evidence domain or scope to sweep.
        subject: String,
    },
    /// Trace a call chain or execution path.
    Trace {
        /// Execution path or identifier to trace.
        subject: String,
    },
    /// Probe for root causes of an observed symptom.
    Probe {
        /// Symptom or observation to probe.
        subject: String,
    },
    /// Form a hypothesis from collected evidence.
    Theorize {
        /// Subject of the hypothesis.
        subject: String,
        /// Optional prior evidence context.
        #[arg(long)]
        context: Option<String>,
    },
    /// Verify a hypothesis against evidence.
    Verify {
        /// Hypothesis statement to verify.
        hypothesis: String,
    },
    /// Close an investigation and produce a final report.
    Close {
        /// Investigation summary or final finding.
        summary: String,
    },
    /// Quick single-question investigation.
    Quick {
        /// Question to investigate.
        question: String,
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
        QuantumCommand::Close { summary } => client.close(&summary).await?.output,
        QuantumCommand::Quick { question } => client.quick(&question).await?.output,
    };

    print_text(mode, &output);
    Ok(())
}
