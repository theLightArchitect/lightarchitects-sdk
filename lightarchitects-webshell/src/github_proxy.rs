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

/// Returns `true` when `repo` is in the SSRF allowlist.
///
/// Derived from [`HITL_TRACKED_REPOS`] to prevent the two lists from drifting.
/// Prefer [`is_hitl_tracked`] for write-path calls (owner + repo both required).
pub fn is_tracked_repo(repo: &str) -> bool {
    HITL_TRACKED_REPOS.iter().any(|(_, r)| *r == repo)
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
/// SSRF guard: `(owner, repo)` must be in [`HITL_TRACKED_REPOS`] — both owner
/// and repo are validated to prevent token exfiltration via forked repos.
///
/// # Errors
///
/// Returns `Err("403:ssrf")` when the repo is not allowlisted, or `Err(String)`
/// when the GitHub API returns a non-success status after all retries are
/// exhausted, or when the response body cannot be parsed.
#[instrument(skip(client, token, cache))]
pub async fn fetch_check_runs(
    client: &Client,
    token: Option<&GitHubToken>,
    cache: &CheckRunCache,
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<Arc<CheckRunSummary>, String> {
    if !is_hitl_tracked(owner, repo) {
        return Err("403:ssrf".to_string());
    }
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

/// `(owner, repo)` pairs that may be queried for HITL PR search and write-path ops.
///
/// Requires both owner and repo to match so a fork with the same repo name
/// cannot be added to the queue. [`is_tracked_repo`] is derived from this list.
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

    // Build repo filter and use `.query()` so reqwest percent-encodes the value.
    let repo_filter: String = HITL_TRACKED_REPOS
        .iter()
        .map(|(o, r)| format!("repo:{o}/{r}"))
        .collect::<Vec<_>>()
        .join(" ");
    let q = format!("type:pr state:open review-requested:@me {repo_filter}");

    let resp = client
        .get("https://api.github.com/search/issues")
        .query(&[("q", &q), ("per_page", &"30".to_string())])
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

// ── PR review submission ──────────────────────────────────────────────────────

/// The GitHub review event type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PrReviewEvent {
    /// Approve the PR — equivalent to "Looks good to me."
    Approve,
    /// Request changes before merging.
    RequestChanges,
    /// Leave a comment without approving or blocking.
    Comment,
}

// Inner response shapes for submit_pr_review — hoisted to avoid items-after-statements.
#[derive(Deserialize)]
struct PrHead {
    sha: String,
}
#[derive(Deserialize)]
struct PrHeadResp {
    head: PrHead,
}
#[derive(Serialize)]
struct ReviewPayload<'a> {
    event: &'a str,
    body: &'a str,
}

impl PrReviewEvent {
    fn as_github_str(&self) -> &'static str {
        match self {
            Self::Approve => "APPROVE",
            Self::RequestChanges => "REQUEST_CHANGES",
            Self::Comment => "COMMENT",
        }
    }
}

/// Origins the webshell UI is served from — used for CSRF origin check.
const ALLOWED_ORIGINS: &[&str] = &[
    "http://localhost:8733",
    "http://127.0.0.1:8733",
    "http://localhost:5173",
    "http://127.0.0.1:5173",
];

/// Parameters for [`submit_pr_review`].
pub struct PrReviewParams<'a> {
    /// PR number within the repo.
    pub pr_number: u64,
    /// Review event type (approve / request-changes / comment).
    pub event: PrReviewEvent,
    /// Review body text.
    pub body: String,
    /// When `Some`, the server fetches the current HEAD SHA and returns
    /// `Err("412:…")` if it differs (replay defense).
    pub if_match_sha: Option<&'a str>,
    /// HTTP `Origin` header value from the request (CSRF guard).
    pub request_origin: Option<&'a str>,
}

/// Submit a PR review via the GitHub API.
///
/// Security controls (all server-side):
/// - CSRF: `params.request_origin` must be in [`ALLOWED_ORIGINS`].
/// - SSRF: `(owner, repo)` must be in [`HITL_TRACKED_REPOS`].
/// - Replay: fetches current HEAD SHA and compares to `params.if_match_sha`;
///   returns `Err("412:sha-mismatch")` on mismatch.
///
/// # Errors
///
/// Returns `Err("403:bad-origin")` on CSRF violation, `Err("403:ssrf")` when
/// the repo is not allowlisted, `Err("412:sha-mismatch")` on HEAD SHA mismatch,
/// or `Err("502:…")` on GitHub API failure.
pub async fn submit_pr_review(
    client: &Client,
    token: &GitHubToken,
    owner: &str,
    repo: &str,
    params: PrReviewParams<'_>,
) -> Result<(), String> {
    // CSRF guard
    match params.request_origin {
        Some(o) if ALLOWED_ORIGINS.contains(&o) => {}
        _ => return Err("403:bad-origin".to_string()),
    }
    // SSRF guard
    if !is_hitl_tracked(owner, repo) {
        return Err("403:ssrf".to_string());
    }
    // Replay defense — fetch current head SHA and compare
    if let Some(expected_sha) = params.if_match_sha {
        let url = format!(
            "https://api.github.com/repos/{owner}/{repo}/pulls/{}",
            params.pr_number
        );
        let resp = client
            .get(&url)
            .bearer_auth(token.as_str())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "lightarchitects-webshell")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("502:pr-fetch-{}", resp.status()));
        }
        let pr: PrHeadResp = resp.json().await.map_err(|e| e.to_string())?;
        if pr.head.sha != expected_sha {
            return Err("412:sha-mismatch".to_string());
        }
    }
    // Submit review
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/pulls/{}/reviews",
        params.pr_number
    );
    let payload = ReviewPayload {
        event: params.event.as_github_str(),
        body: &params.body,
    };
    let resp = client
        .post(&url)
        .bearer_auth(token.as_str())
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "lightarchitects-webshell")
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("502:review-submit-{}", resp.status()))
    }
}

// ── Commit metadata ────────────────────────────────────────────────────────────

/// Lightweight commit metadata returned to the cockpit PR detail view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMetadata {
    /// Full 40-character commit SHA.
    pub sha: String,
    /// First line of the commit message only.
    pub message: String,
    /// GitHub login of the commit author (empty string when unavailable).
    pub author_login: String,
    /// ISO 8601 commit timestamp (empty string when unavailable).
    pub committed_at: String,
}

/// Cache key: `"{owner}/{repo}/{sha}"`.
pub type CommitMetadataCache = Cache<String, Arc<CommitMetadata>>;

/// Build the 60s moka cache for commit metadata results (max 512 SHAs).
pub fn commit_metadata_cache() -> CommitMetadataCache {
    Cache::builder()
        .max_capacity(512)
        .time_to_live(Duration::from_secs(60))
        .build()
}

#[derive(Deserialize)]
struct GhCommitAuthor {
    login: Option<String>,
}

#[derive(Deserialize)]
struct GhCommitDetail {
    message: String,
    author: Option<GhInnerAuthor>,
}

#[derive(Deserialize)]
struct GhInnerAuthor {
    date: Option<String>,
}

#[derive(Deserialize)]
struct GhCommitResponse {
    sha: String,
    commit: GhCommitDetail,
    author: Option<GhCommitAuthor>,
}

/// Fetch commit metadata for a single SHA, with 60s moka cache.
///
/// SSRF guard: `(owner, repo)` must be in [`HITL_TRACKED_REPOS`].
///
/// # Errors
///
/// Returns `Err("403:ssrf")` when the repo is not allowlisted, or
/// `Err("502:commit-fetch-<status>")` on GitHub API failure.
pub async fn fetch_commit_metadata(
    client: &Client,
    token: &GitHubToken,
    cache: &CommitMetadataCache,
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<Arc<CommitMetadata>, String> {
    if !is_hitl_tracked(owner, repo) {
        return Err("403:ssrf".to_string());
    }
    let key = format!("{owner}/{repo}/{sha}");
    if let Some(cached) = cache.get(&key).await {
        return Ok(cached);
    }
    let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{sha}");
    let resp = client
        .get(&url)
        .bearer_auth(token.as_str())
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "lightarchitects-webshell")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("502:commit-fetch-{}", resp.status()));
    }
    let raw: GhCommitResponse = resp.json().await.map_err(|e| e.to_string())?;
    let first_line = raw.commit.message.lines().next().unwrap_or("").to_string();
    let meta = Arc::new(CommitMetadata {
        sha: raw.sha,
        message: first_line,
        author_login: raw.author.and_then(|a| a.login).unwrap_or_default(),
        committed_at: raw.commit.author.and_then(|a| a.date).unwrap_or_default(),
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::github_token_store::GitHubToken;

    fn fake_token() -> GitHubToken {
        GitHubToken::new("fake-pat".to_string())
    }

    // ── SSRF allowlist ────────────────────────────────────────────────────────

    #[test]
    fn is_hitl_tracked_accepts_allowlisted_pairs() {
        assert!(is_hitl_tracked("TheLightArchitects", "lightarchitects-sdk"));
        assert!(is_hitl_tracked("TheLightArchitects", "SOUL-DEV"));
    }

    #[test]
    fn is_hitl_tracked_rejects_unknown_repo() {
        assert!(!is_hitl_tracked("TheLightArchitects", "unknown-repo"));
    }

    #[test]
    fn is_hitl_tracked_rejects_unknown_owner() {
        assert!(!is_hitl_tracked("attacker", "lightarchitects-sdk"));
    }

    // ── PrReviewEvent ────────────────────────────────────────────────────────

    #[test]
    fn pr_review_event_github_strings() {
        assert_eq!(PrReviewEvent::Approve.as_github_str(), "APPROVE");
        assert_eq!(
            PrReviewEvent::RequestChanges.as_github_str(),
            "REQUEST_CHANGES"
        );
        assert_eq!(PrReviewEvent::Comment.as_github_str(), "COMMENT");
    }

    #[test]
    fn pr_review_event_serde_roundtrip() {
        let json = serde_json::to_string(&PrReviewEvent::Approve).expect("serialize");
        assert_eq!(json, "\"APPROVE\"");
        let parsed: PrReviewEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, PrReviewEvent::Approve);
    }

    // ── validate_html_url ────────────────────────────────────────────────────

    #[test]
    fn validate_html_url_accepts_valid_pr() {
        let (owner, repo, num) =
            validate_html_url("https://github.com/TheLightArchitects/lightarchitects-sdk/pull/42")
                .expect("valid PR URL");
        assert_eq!(owner, "TheLightArchitects");
        assert_eq!(repo, "lightarchitects-sdk");
        assert_eq!(num, 42);
    }

    #[test]
    fn validate_html_url_rejects_malformed_url() {
        assert!(validate_html_url("not-a-url").is_err());
        assert!(
            validate_html_url("https://evil.com/TheLightArchitects/lightarchitects-sdk/pull/1")
                .is_err()
        );
    }

    #[test]
    fn validate_html_url_rejects_non_allowlisted_repo() {
        assert!(
            validate_html_url("https://github.com/TheLightArchitects/private-repo/pull/1").is_err()
        );
    }

    // ── submit_pr_review security controls ───────────────────────────────────

    #[tokio::test]
    async fn submit_pr_review_rejects_foreign_origin() {
        let client = Client::new();
        let token = fake_token();
        let err = submit_pr_review(
            &client,
            &token,
            "TheLightArchitects",
            "lightarchitects-sdk",
            PrReviewParams {
                pr_number: 1,
                event: PrReviewEvent::Comment,
                body: "test".to_string(),
                if_match_sha: None,
                request_origin: Some("https://evil.com"),
            },
        )
        .await
        .expect_err("should reject foreign origin");
        assert!(err.contains("403:bad-origin"), "got: {err}");
    }

    #[tokio::test]
    async fn submit_pr_review_rejects_non_allowlisted_repo() {
        let client = Client::new();
        let token = fake_token();
        let err = submit_pr_review(
            &client,
            &token,
            "TheLightArchitects",
            "private-repo",
            PrReviewParams {
                pr_number: 1,
                event: PrReviewEvent::Comment,
                body: "test".to_string(),
                if_match_sha: None,
                request_origin: Some("http://localhost:8733"),
            },
        )
        .await
        .expect_err("should reject non-allowlisted repo");
        assert!(err.contains("403:ssrf"), "got: {err}");
    }

    // ── fetch_commit_metadata SSRF guard ─────────────────────────────────────

    #[tokio::test]
    async fn fetch_commit_metadata_rejects_non_allowlisted_repo() {
        let client = Client::new();
        let token = fake_token();
        let cache = commit_metadata_cache();
        let result = fetch_commit_metadata(
            &client,
            &token,
            &cache,
            "TheLightArchitects",
            "private-repo",
            "abc123",
        )
        .await;
        let err = result.expect_err("should reject non-allowlisted repo");
        assert!(err.contains("403:ssrf"), "got: {err}");
    }
}
