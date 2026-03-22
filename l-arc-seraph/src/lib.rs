//! Typed client for the SERAPH MCP server.
//!
//! SERAPH exposes a single MCP tool — `penTools` — with 18 actions covering
//! a complete penetration-testing lifecycle:
//!
//! ```text
//! Wings:       capture | scan | analyze | osint | monitor | execute
//! Services:    detonate | orchestrate | knowledge_search | knowledge_read | knowledge_stats
//! Investigation: start_investigation | advance_investigation | close_investigation | report
//! Utilities:   vault_sync | speak | status
//! ```
//!
//! All responses are AI-generated offensive-security prose. There are no
//! structured-JSON responses in the SERAPH protocol.
//!
//! SERAPH uses `Content-Length` header framing (not newline-delimited JSON).
//! [`StdioTransport`] handles this automatically via [`SiblingId::Seraph`].
//!
//! **All operations require prior authorisation.** Every call is scope-governed
//! by SERAPH's 5-gate `ScopeGovernor` (TTL → target → tool → concurrent → domain).
//! Ensure `~/.seraph/scope.toml` is configured with a valid engagement before use.
//!
//! # Quick start
//!
//! ```no_run
//! use l_arc_seraph::{SeraphClient, Wing};
//!
//! # async fn example() -> Result<(), l_arc_core::SdkError> {
//! let client = SeraphClient::builder()
//!     .timeout(std::time::Duration::from_secs(120))
//!     .build()
//!     .await?;
//!
//! // Recon: discover hosts on the authorised range
//! let hosts = client.scan("192.168.1.0/24").await?;
//! println!("{}", hosts.output);
//!
//! // OSINT: gather open-source intelligence on a target
//! let intel = client.osint("example.internal", None).await?;
//! println!("{}", intel.output);
//!
//! // Convenience: select wing by enum value
//! let result = client.wing(Wing::Analyze, "suspicious_binary.elf").await?;
//! println!("{}", result.output);
//! # Ok(()) }
//! ```

mod client;
mod content;
mod types;

// ── Public API surface ────────────────────────────────────────────────────────

pub use client::{SeraphClient, SeraphClientBuilder};
pub use types::{ActionOutput, Wing};
