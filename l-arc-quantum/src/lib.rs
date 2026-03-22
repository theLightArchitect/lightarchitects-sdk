//! Typed client for the QUANTUM MCP server.
//!
//! QUANTUM exposes a single MCP tool — `qsTools` — with 13 actions covering
//! a complete forensic investigation cycle:
//!
//! ```text
//! SCAN → SWEEP → TRACE → PROBE → THEORIZE → VERIFY → CLOSE
//!   └── utilities: quick, research, helix, discover, list, workflow
//! ```
//!
//! All responses are AI-generated investigation prose. There are no
//! structured-JSON responses in the QUANTUM protocol.
//!
//! QUANTUM is the only sibling that requires an `mcp-server` subcommand when
//! spawned. The builder handles this automatically via the `Quantum` variant of
//! [`l_arc_core::SiblingId`].
//!
//! # Quick start
//!
//! ```no_run
//! use l_arc_quantum::QuantumClient;
//!
//! # async fn example() -> Result<(), l_arc_core::SdkError> {
//! let client = QuantumClient::builder()
//!     .timeout(std::time::Duration::from_secs(120))
//!     .build()
//!     .await?;
//!
//! // Begin a forensic investigation
//! let evidence = client.scan("auth token refresh intermittent failures").await?;
//! println!("{}", evidence.output);
//!
//! // Form and verify a hypothesis
//! let theory = client.theorize("clock skew causing JWT expiry errors", None).await?;
//! let verdict = client.verify("clock skew is the root cause").await?;
//! println!("{}", verdict.output);
//!
//! // Close the investigation
//! let report = client.close("Clock skew confirmed — NTP drift on node-3").await?;
//! println!("{}", report.output);
//! # Ok(()) }
//! ```

mod client;
mod content;
mod types;

// ── Public API surface ────────────────────────────────────────────────────────

pub use client::{QuantumClient, QuantumClientBuilder};
pub use types::ActionOutput;
