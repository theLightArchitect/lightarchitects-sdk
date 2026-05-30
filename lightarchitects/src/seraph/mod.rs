//! Typed client for the SERAPH MCP server.
//!
//! SERAPH exposes a single MCP tool -- `penTools` -- with 18 actions covering
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
//! [`lightarchitects::core::StdioTransport`] handles this automatically via the `Seraph`
//! variant of [`lightarchitects::core::SiblingId`].
//!
//! **All operations require prior authorisation.** Every call is scope-governed
//! by SERAPH's 5-gate `ScopeGovernor` (TTL -> target -> tool -> concurrent -> domain).
//! Ensure `~/lightarchitects/seraph/scope.toml` is configured with a valid engagement before use.
//!
//! # Quick start
//!
//! ```no_run
//! use lightarchitects::seraph::{SeraphClient, Wing};
//!
//! # async fn example() -> Result<(), lightarchitects::core::SdkError> {
//! let client = SeraphClient::builder()
//!     .timeout(std::time::Duration::from_secs(120))
//!     .build()?;
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

/// Canonical SERAPH action enum — pentest orchestration and investigation.
pub mod actions;
/// [`BcraExecutor`] bridge for `BcraStrategy` (requires `loops-core` feature).
#[cfg(feature = "loops-core")]
pub mod bcra_executor;
mod client;
mod content;
/// [`CoVeExecutor`] bridge for `CoVeStrategy` (requires `loops-core` feature).
#[cfg(feature = "loops-core")]
pub mod cove_executor;
/// Stateful driver for the SERAPH investigation lifecycle.
pub mod engagement;
/// Evidence chain accumulator and engagement logging.
pub mod evidence;
/// [`IttExecutor`] bridge for `IttStrategy` (requires `loops-core` feature).
#[cfg(feature = "loops-core")]
pub mod itt_executor;
/// Typed parameter builders for SERAPH wing actions.
pub mod params;
/// SSH session pool for reusing connections across calls.
#[cfg(feature = "ssh")]
pub mod pool;
/// [`RedTeamExecutor`] bridge for `RedTeamStrategy` (requires `loops-core` feature).
#[cfg(feature = "loops-core")]
pub mod red_team_executor;
/// Engagement scope management (`~/lightarchitects/seraph/scope.toml`).
pub mod scope;
/// SSH transport for remote SERAPH instances.
#[cfg(feature = "ssh")]
pub mod ssh;
mod types;

// ── Public API surface ──────────────────────────────────────────────────────

pub use actions::SeraphAction;
pub use client::{SeraphClient, SeraphClientBuilder};
pub use engagement::{EngagementPhase, SeraphEngagement};
pub use evidence::{EvidenceChain, EvidenceEntry, engagement_log};
pub use params::{
    AnalyzeParams, AnalyzeType, CaptureParams, MonitorAction, MonitorParams, OsintParams,
    OsintType, ScanParams, ScanType,
};
#[cfg(feature = "ssh")]
pub use pool::{PoolConfig, SessionPool};
pub use scope::{EngagementScope, ScopeConstraint, ScopeDomain};
#[cfg(feature = "ssh")]
pub use ssh::{
    CallbackPassphraseProvider, EnvPassphraseProvider, FilePassphraseProvider,
    KeyPassphraseProvider, SshSession, SshSessionBuilder,
};
pub use types::{
    ActionOutput, ExamineResult, ReconResult, ReportResult, ScopeResult, StrikeResult,
    SurveyResult, Wing,
};

#[cfg(feature = "loops-core")]
pub use bcra_executor::SeraphBcraExecutor;
#[cfg(feature = "loops-core")]
pub use cove_executor::SeraphCoVeExecutor;
#[cfg(feature = "loops-core")]
pub use itt_executor::SeraphIttExecutor;
#[cfg(feature = "loops-core")]
pub use red_team_executor::SeraphRedTeamExecutor;
