//! Preflight infrastructure checks — run at startup to surface missing dependencies.
//!
//! Two-phase API prevents a circular dependency with [`Config::resolve`]:
//! - [`run_basic`] — runs concurrently with Docker probe before config is resolved.
//!   Checks: `$SHELL` and `~/.lightarchitects/` (no agent type needed).
//! - [`run_full`] — runs after config is resolved; checks all 10 remaining dependencies
//!   concurrently via [`tokio::join!`].
//!
//! The [`PreflightReport`] is stored in [`AppState`](crate::server::AppState) and
//! served via `GET /api/preflight`.
//!
//! [`Config::resolve`]: crate::config::Config::resolve

#![deny(missing_docs)]

use std::time::Instant;

use chrono::{DateTime, Utc};

use crate::{config::AgentSession, container::types::DockerCapability};

pub mod checks;

pub use checks::{Category, CheckResult, CheckStatus};

/// Per-check timeout for subprocess-spawning checks (credentials, PAT, Ollama TCP).
///
/// Applies to [`checks::check_agent_credentials`], [`checks::check_github_pat`],
/// and [`checks::check_ollama_service`].
pub const PREFLIGHT_CHECK_TIMEOUT_MS: u64 = 400;

/// Result of the pre-[`Config::resolve`] basic preflight pass.
///
/// Carries the two checks that do not require a resolved agent type.
///
/// [`Config::resolve`]: crate::config::Config::resolve
pub struct BasicPreflight {
    /// `$SHELL` executable check.
    pub shell: CheckResult,
    /// `~/.lightarchitects/` writability check.
    pub la_config_dir: CheckResult,
}

/// Rolled-up readiness status derived from all [`CheckResult`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum OverallStatus {
    /// All Core checks pass; no Important failures.
    Ready,
    /// No Core failures; at least one Important check failed.
    Degraded,
    /// At least one Core check failed.
    Blocked,
}

/// Structured preflight report returned by [`run_full`] and stored in `AppState`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PreflightReport {
    /// UTC timestamp when this report was generated.
    pub timestamp: DateTime<Utc>,
    /// Rolled-up overall readiness status.
    pub overall: OverallStatus,
    /// Individual check results ordered: Core → Important → Optional.
    pub checks: Vec<CheckResult>,
    /// Wall-clock time across all concurrent checks in milliseconds.
    pub elapsed_ms: u64,
}

impl PreflightReport {
    fn derive_overall(checks: &[CheckResult]) -> OverallStatus {
        let has_core_fail = checks
            .iter()
            .any(|c| matches!(c.category, Category::Core) && matches!(c.status, CheckStatus::Fail));
        if has_core_fail {
            return OverallStatus::Blocked;
        }
        let has_important_fail = checks.iter().any(|c| {
            matches!(c.category, Category::Important) && matches!(c.status, CheckStatus::Fail)
        });
        if has_important_fail {
            OverallStatus::Degraded
        } else {
            OverallStatus::Ready
        }
    }
}

/// Phase 1 of preflight — runs concurrently with [`probe_docker`] before [`Config::resolve`].
///
/// Only runs infra checks that do not require a resolved agent type:
/// `$SHELL` executability and `~/.lightarchitects/` writability.
///
/// [`probe_docker`]: crate::container::probe::probe_docker
/// [`Config::resolve`]: crate::config::Config::resolve
pub async fn run_basic() -> BasicPreflight {
    let (shell, la_config_dir) = tokio::join!(checks::check_shell(), checks::check_la_config_dir());
    BasicPreflight {
        shell,
        la_config_dir,
    }
}

/// Phase 2 of preflight — runs after [`Config::resolve`] when the agent type is known.
///
/// Dispatches all 10 remaining checks concurrently via [`tokio::join!`].
/// Subprocess-spawning checks are bounded by [`PREFLIGHT_CHECK_TIMEOUT_MS`].
///
/// [`Config::resolve`]: crate::config::Config::resolve
pub async fn run_full(
    agent: &AgentSession,
    docker: DockerCapability,
    basic: BasicPreflight,
) -> PreflightReport {
    let span = tracing::info_span!("preflight.run_full");
    let _guard = span.entered();
    let started = Instant::now();

    let (
        agent_binary,
        agent_credentials,
        la_workspace,
        helix_vault,
        helix_db,
        session_store,
        ayin_service,
        docker_daemon,
        ollama_service,
        github_pat,
    ) = tokio::join!(
        checks::check_agent_binary(agent),
        checks::check_agent_credentials(agent),
        checks::check_la_workspace(),
        checks::check_helix_vault(),
        checks::check_helix_db(),
        checks::check_session_store(),
        checks::check_ayin_service(),
        checks::check_docker_daemon(docker),
        checks::check_ollama_service(agent),
        checks::check_github_pat(),
    );

    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    let checks_vec = vec![
        basic.shell,
        basic.la_config_dir,
        agent_binary,
        agent_credentials,
        la_workspace,
        helix_vault,
        helix_db,
        session_store,
        ayin_service,
        docker_daemon,
        ollama_service,
        github_pat,
    ];

    let overall = PreflightReport::derive_overall(&checks_vec);
    tracing::info!(overall = ?overall, elapsed_ms, "preflight.run_full complete");

    PreflightReport {
        timestamp: Utc::now(),
        overall,
        checks: checks_vec,
        elapsed_ms,
    }
}
