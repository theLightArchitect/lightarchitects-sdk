//! Typed client for the QUANTUM MCP server.
//!
//! QUANTUM exposes a single MCP tool — `qsTools` — with 13 actions covering
//! a complete forensic investigation cycle:
//!
//! ```text
//! TRIAGE → SWEEP → TRACE → PROBE → THEORIZE → VERIFY → CLOSE
//!   └── utilities: quick, research, helix, discover, list, workflow
//! ```
//!
//! All responses are AI-generated investigation prose. There are no
//! structured-JSON responses in the QUANTUM protocol.
//!
//! QUANTUM is the only sibling that requires an `mcp-server` subcommand when
//! spawned. The builder handles this automatically via the `Quantum` variant of
//! [`lightarchitects::core::SiblingId`].
//!
//! # Quick start
//!
//! ```no_run
//! use lightarchitects::quantum::{QuantumClient, QuantumInvestigation};
//!
//! # async fn example() -> Result<(), lightarchitects::core::SdkError> {
//! let client = QuantumClient::builder()
//!     .timeout(std::time::Duration::from_secs(120))
//!     .build()?;
//!
//! // Stateful investigation via [`QuantumInvestigation`]
//! let mut inv = QuantumInvestigation::new(&client, "auth token refresh failures");
//! inv.triage().await?;
//! inv.sweep().await?;
//! inv.theorize(None).await?;
//! inv.verify("clock skew is the root cause").await?;
//! let report = inv.close("NTP drift confirmed — clock skew on node-3").await?;
//! println!("{}", report.output);
//!
//! // Or call client methods directly (each returns a per-action typed result)
//! let evidence = client.triage("unexpected 502s on gateway").await?;
//! println!("{}", evidence.output);
//! # Ok(()) }
//! ```

/// Canonical QUANTUM action enum — forensic investigation lifecycle.
pub mod actions;
mod client;
mod content;
/// Stateful driver for the QUANTUM forensic investigation lifecycle.
pub mod investigation;
/// Response types and investigation state tracking.
pub mod types;

// ── Public API surface ────────────────────────────────────────────────────────

pub use actions::QuantumAction;
pub use client::{QuantumClient, QuantumClientBuilder};
pub use investigation::{InvestigationPhase, QuantumInvestigation};
pub use types::{
    ActionOutput, CloseResult, DiscoverResult, HelixResult, InvestigationState, ListResult,
    MAX_ADVANCE_STEPS, PhaseRecord, ProbeResult, QuickResult, ResearchResult, SweepResult,
    TheorizeResult, TraceResult, TriageResult, VerifyResult, WorkflowResult,
};
