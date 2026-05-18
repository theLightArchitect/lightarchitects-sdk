//! Server-side GitHub API proxy — Phase 4.
//!
//! Fetches CI check-run status for a commit SHA, caches results 60s via moka,
//! and enforces an SSRF allowlist so only tracked repos can be queried outbound.
//!
//! # Rate-limit budget
//!
//! GitHub authenticated API: 5,000 req/hr. With moka 60s TTL and at most
//! 30 branch refreshes/hr steady state: 30 req/hr — well within budget.
//! Documented here per plan exit criterion (Phase 4 rate-limit math).
//!
//! # SSRF allowlist
//!
//! Only `lightarchitects-sdk`, `SOUL-DEV`, `CORSO-DEV` may be queried outbound.
//! Untracked repos return `403 Forbidden` before any network call.

use std::{sync::Arc, time::Duration};

use moka::future::Cache;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tracing::{debug, instrument, warn};

use crate::github_token_store::GitHubToken;

// ── SSRF allowlist ────────────────────────────────────────────────────────────

/// Repos that may be queried against the GitHub API.
const TRACKED_REPOS: &[&str] = &["lightarchitects-sdk", "SOUL-DEV", "CORSO-DEV"];

/// Returns `true` when `repo` is in the SSRF allowlist.
pub fn is_tracked_repo(repo: &str) -> bool {
    TRACKED_REPOS.contains(&repo)
}

// ── Types ─────────────────────────────────────────────────────────────────────

/// Simplified CI check-run summary returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRunSummary {
    /// Total check runs found for the SHA.
    pub total_count: u32,
    /// Number of check runs with a successful conclusion.
    pub success_count: u32,
    /// Number of check runs with a failure/action-required conclusion.
    pub failure_count: u32,
    /// Number of check runs still in progress or queued.
    pub pending_count: u32,
    /// Derived overall status.
    pub status: CiSummaryStatus,
}

/// Derived overall CI status for a commit SHA.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CiSummaryStatus {
    /// All checks passed.
    Success,
    /// At least one check failed.
    Failure,
    /// At least one check is still running and none have failed.
    Pending,
    /// No check runs found or data unavailable.
    Unknown,
}

// ── Cache ─────────────────────────────────────────────────────────────────────

/// Cache key: `"{owner}/{repo}/{sha}"`.
pub type CheckRunCache = Cache<String, Arc<CheckRunSummary>>;

/// Build the 60s moka cache for CI check-run results (max 512 SHAs).
pub fn check_run_cache() -> CheckRunCache {
    Cache::builder()
        .max_capacity(512)
        .time_to_live(Duration::from_secs(60))
        .build()
}

// ── GitHub response types ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GhCheckRunsResponse {
    total_count: u32,
    check_runs: Vec<GhCheckRun>,
}

#[derive(Deserialize)]
struct GhCheckRun {
    status: String,             // "completed" | "in_progress" | "queued"
    conclusion: Option<String>, // "success" | "failure" | "neutral" | ...
}

// ── Main fetch ────────────────────────────────────────────────────────────────

/// Fetch CI check runs for `owner/repo` at `sha`.
///
/// Returns cached result when available. Performs exponential backoff on
/// HTTP 429 responses (max 3 retries, starting at 1s).
///
/// # Errors
///
/// Returns `Err(String)` when the GitHub API returns a non-success status
/// after all retries are exhausted, or when the response body cannot be parsed.
#[instrument(skip(client, token, cache))]
pub async fn fetch_check_runs(
    client: &Client,
    token: Option<&GitHubToken>,
    cache: &CheckRunCache,
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<Arc<CheckRunSummary>, String> {
    let key = format!("{owner}/{repo}/{sha}");

    if let Some(hit) = cache.get(&key).await {
        debug!(%key, "check-run cache hit");
        return Ok(hit);
    }

    let summary = fetch_with_backoff(client, token, owner, repo, sha).await?;
    let arc = Arc::new(summary);
    cache.insert(key, arc.clone()).await;
    Ok(arc)
}

async fn fetch_with_backoff(
    client: &Client,
    token: Option<&GitHubToken>,
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<CheckRunSummary, String> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{sha}/check-runs");

    let mut delay = Duration::from_secs(1);
    for attempt in 0..=3u32 {
        let mut req = client
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "lightarchitects-webshell/1.0");

        if let Some(t) = token {
            req = req.bearer_auth(t.as_str());
        }

        let resp = req.send().await.map_err(|e| e.to_string())?;

        match resp.status().as_u16() {
            200 => {
                let body: GhCheckRunsResponse = resp.json().await.map_err(|e| e.to_string())?;
                return Ok(summarise(body));
            }
            429 => {
                if attempt == 3 {
                    return Err("GitHub rate limit exceeded after 3 retries".to_string());
                }
                warn!(
                    attempt,
                    delay_ms = delay.as_millis(),
                    "GitHub 429 — backing off"
                );
                sleep(delay).await;
                delay *= 2;
            }
            401 | 403 => return Err(format!("GitHub auth error: {}", resp.status())),
            code => return Err(format!("GitHub API error: {code}")),
        }
    }
    Err("fetch_with_backoff: unreachable".to_string())
}

fn summarise(body: GhCheckRunsResponse) -> CheckRunSummary {
    let mut success = 0u32;
    let mut failure = 0u32;
    let mut pending = 0u32;
    for run in &body.check_runs {
        match run.status.as_str() {
            "completed" => match run.conclusion.as_deref() {
                Some("success" | "neutral" | "skipped") => success += 1,
                Some(_) => failure += 1,
                None => pending += 1,
            },
            _ => pending += 1,
        }
    }
    let status = if failure > 0 {
        CiSummaryStatus::Failure
    } else if pending > 0 {
        CiSummaryStatus::Pending
    } else if success > 0 {
        CiSummaryStatus::Success
    } else {
        CiSummaryStatus::Unknown
    };
    CheckRunSummary {
        total_count: body.total_count,
        success_count: success,
        failure_count: failure,
        pending_count: pending,
        status,
    }
}
