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

// ── HITL inbox SSRF allowlist ─────────────────────────────────────────────────

/// `(owner, repo)` pairs that may be queried for HITL PR search.
///
/// Stricter than [`TRACKED_REPOS`] — requires both owner and repo to match so
/// a fork with the same repo name cannot be added to the queue.
const HITL_TRACKED_REPOS: &[(&str, &str)] = &[
    ("TheLightArchitects", "lightarchitects-sdk"),
    ("TheLightArchitects", "SOUL-DEV"),
    ("TheLightArchitects", "CORSO-DEV"),
];

/// Returns `true` when `(owner, repo)` is in the HITL SSRF allowlist.
pub fn is_hitl_tracked(owner: &str, repo: &str) -> bool {
    HITL_TRACKED_REPOS
        .iter()
        .any(|(o, r)| *o == owner && *r == repo)
}

/// Parses and validates a `https://github.com/{owner}/{repo}/pull/{number}` URL.
///
/// Returns `(owner, repo, pr_number)` or an error when the URL is malformed
/// or the `(owner, repo)` pair is not in [`HITL_TRACKED_REPOS`].
///
/// # Errors
///
/// Returns `Err(String)` when the URL does not match the expected format or the
/// repository is not in the SSRF allowlist.
pub fn validate_html_url(url: &str) -> Result<(String, String, u64), String> {
    let path = url
        .trim()
        .strip_prefix("https://github.com/")
        .ok_or_else(|| "URL must start with https://github.com/".to_string())?;
    // Split on at most 5 parts to tolerate trailing query params / fragments.
    let parts: Vec<&str> = path.splitn(5, '/').collect();
    if parts.len() < 4 || parts[2] != "pull" {
        return Err("URL must match https://github.com/{owner}/{repo}/pull/{number}".to_string());
    }
    let owner = parts[0];
    let repo = parts[1];
    // Strip any query-string suffix from the PR number segment.
    let number_raw = parts[3].split('?').next().unwrap_or(parts[3]);
    let pr_number: u64 = number_raw
        .parse()
        .map_err(|_| "PR number must be a positive integer".to_string())?;
    if !is_hitl_tracked(owner, repo) {
        return Err(format!(
            "Repository {owner}/{repo} is not in the SSRF allowlist"
        ));
    }
    Ok((owner.to_string(), repo.to_string(), pr_number))
}

// ── HITL types ────────────────────────────────────────────────────────────────

/// A single open PR awaiting review — returned by [`fetch_hitl_search`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitlSearchItem {
    /// PR number.
    pub number: u64,
    /// PR title.
    pub title: String,
    /// `https://github.com/{owner}/{repo}/pull/{number}` deep-link.
    pub html_url: String,
    /// GitHub org or user that owns the repo.
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// Login of the PR author.
    pub author: String,
    /// ISO-8601 timestamp of the last update.
    pub updated_at: String,
    /// Whether the PR is a draft.
    pub draft: bool,
}

/// PR detail — returned by [`fetch_pr_metadata`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitlPrMetadata {
    /// PR number.
    pub number: u64,
    /// PR title.
    pub title: String,
    /// Deep-link URL.
    pub html_url: String,
    /// Repo owner.
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// Author login.
    pub author: String,
    /// `"open"` | `"closed"` | `"merged"`.
    pub state: String,
    /// Whether the PR is a draft.
    pub draft: bool,
    /// Head commit SHA.
    pub head_sha: String,
    /// ISO-8601 timestamp of the last update.
    pub updated_at: String,
}

// ── HITL caches ───────────────────────────────────────────────────────────────

/// Cache for [`fetch_hitl_search`] results — keyed by `"me"`.
pub type HitlSearchCache = Cache<String, Arc<Vec<HitlSearchItem>>>;

/// Cache for [`fetch_pr_metadata`] results — keyed by `"{owner}/{repo}/{number}"`.
pub type PrMetadataCache = Cache<String, Arc<HitlPrMetadata>>;

/// 60s TTL; max 32 entries (one per unique search query — effectively 1 in practice).
pub fn hitl_search_cache() -> HitlSearchCache {
    Cache::builder()
        .max_capacity(32)
        .time_to_live(Duration::from_secs(60))
        .build()
}

/// 60s TTL; max 256 PR entries.
pub fn pr_metadata_cache() -> PrMetadataCache {
    Cache::builder()
        .max_capacity(256)
        .time_to_live(Duration::from_secs(60))
        .build()
}

// ── GitHub response types (HITL) ──────────────────────────────────────────────

#[derive(Deserialize)]
struct GhSearchResponse {
    items: Vec<GhSearchItem>,
}

#[derive(Deserialize)]
struct GhSearchItem {
    number: u64,
    title: String,
    html_url: String,
    draft: Option<bool>,
    updated_at: String,
    user: GhUser,
}

#[derive(Deserialize)]
struct GhUser {
    login: String,
}

#[derive(Deserialize)]
struct GhPullResponse {
    number: u64,
    title: String,
    html_url: String,
    state: String,
    draft: Option<bool>,
    updated_at: String,
    user: GhUser,
    head: GhHead,
}

#[derive(Deserialize)]
struct GhHead {
    sha: String,
}

// ── HITL fetch functions ──────────────────────────────────────────────────────

/// Fetch open PRs where review is requested from the authenticated user.
///
/// Uses GitHub's `review-requested:@me` shorthand to avoid a separate
/// `GET /user` login-resolution call. Results are filtered server-side to
/// only include repos in [`HITL_TRACKED_REPOS`]. Cached 60s.
///
/// # Errors
///
/// Returns `Err(String)` on network failure or GitHub API error.
#[instrument(skip(client, token, cache))]
pub async fn fetch_hitl_search(
    client: &Client,
    token: &GitHubToken,
    cache: &HitlSearchCache,
) -> Result<Arc<Vec<HitlSearchItem>>, String> {
    let key = "me".to_string();
    if let Some(hit) = cache.get(&key).await {
        debug!("hitl-search cache hit");
        return Ok(hit);
    }

    // Build repo filter: "repo:Owner/Repo1+repo:Owner/Repo2+..."
    let repo_filter: String = HITL_TRACKED_REPOS
        .iter()
        .map(|(o, r)| format!("repo:{o}/{r}"))
        .collect::<Vec<_>>()
        .join("+");
    let q = format!("type:pr+state:open+review-requested:@me+{repo_filter}");
    let url = format!("https://api.github.com/search/issues?q={q}&per_page=30");

    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "lightarchitects-webshell/1.0")
        .bearer_auth(token.as_str())
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("GitHub search error: {}", resp.status()));
    }

    let body: GhSearchResponse = resp.json().await.map_err(|e| e.to_string())?;

    let items: Vec<HitlSearchItem> = body
        .items
        .into_iter()
        .filter_map(|item| {
            // Extract owner/repo from html_url for the SSRF filter.
            let (owner, repo, _) = validate_html_url(&item.html_url).ok()?;
            // validate_html_url already checks is_hitl_tracked — double-check defensive.
            if !is_hitl_tracked(&owner, &repo) {
                warn!(html_url = %item.html_url, "dropping non-allowlisted PR from search results");
                return None;
            }
            Some(HitlSearchItem {
                number: item.number,
                title: item.title,
                html_url: item.html_url,
                owner,
                repo,
                author: item.user.login,
                updated_at: item.updated_at,
                draft: item.draft.unwrap_or(false),
            })
        })
        .collect();

    let arc = Arc::new(items);
    cache.insert(key, arc.clone()).await;
    Ok(arc)
}

/// Fetch detailed metadata for a single PR.
///
/// Validates `(owner, repo)` against [`HITL_TRACKED_REPOS`] before making any
/// outbound call. Cached 60s per `{owner}/{repo}/{pr_number}` key.
///
/// # Errors
///
/// Returns `Err(String)` when the repo is not allowlisted, the GitHub API
/// returns a non-success status, or the response body cannot be parsed.
#[instrument(skip(client, token, cache))]
pub async fn fetch_pr_metadata(
    client: &Client,
    token: &GitHubToken,
    cache: &PrMetadataCache,
    owner: &str,
    repo: &str,
    pr_number: u64,
) -> Result<Arc<HitlPrMetadata>, String> {
    if !is_hitl_tracked(owner, repo) {
        return Err(format!(
            "Repository {owner}/{repo} is not in the SSRF allowlist"
        ));
    }

    let key = format!("{owner}/{repo}/{pr_number}");
    if let Some(hit) = cache.get(&key).await {
        debug!(%key, "pr-metadata cache hit");
        return Ok(hit);
    }

    let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{pr_number}");
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "lightarchitects-webshell/1.0")
        .bearer_auth(token.as_str())
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("GitHub PR metadata error: {}", resp.status()));
    }

    let body: GhPullResponse = resp.json().await.map_err(|e| e.to_string())?;
    let meta = Arc::new(HitlPrMetadata {
        number: body.number,
        title: body.title,
        html_url: body.html_url,
        owner: owner.to_string(),
        repo: repo.to_string(),
        author: body.user.login,
        state: body.state,
        draft: body.draft.unwrap_or(false),
        head_sha: body.head.sha,
        updated_at: body.updated_at,
    });
    cache.insert(key, meta.clone()).await;
    Ok(meta)
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
