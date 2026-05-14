//! HTTP handlers for `git.*` tools (EEF E3 — git-and-pr).
//!
//! Provides eight REST endpoints backed by `tokio::process::Command::new("git")`
//! (structured argv, never a shell string — T-7 CWE-78 mitigation):
//!
//! - `POST /api/git/status`    — porcelain working-tree status
//! - `POST /api/git/branch`    — list / create / switch / delete branches
//! - `POST /api/git/diff`      — staged or unstaged diff
//! - `POST /api/git/commit`    — commit staged changes (`--no-verify`)
//! - `POST /api/git/push`      — push to origin (force-push permanently blocked — T-5)
//! - `POST /api/git/pull`      — fast-forward pull
//! - `POST /api/git/pr/create` — create a GitHub pull request via REST API
//! - `POST /api/git/pr/review` — submit a GitHub PR review via REST API
//!
//! All endpoints require bearer authentication. The GitHub PAT is loaded via the
//! 3-tier ladder: OS keyring → macOS `security` CLI → env var `LIGHTARCHITECTS_GITHUB_PAT`.

use std::time::Duration;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use secrecy::{ExposeSecret, SecretBox};
use serde_json::{Value, json};

use crate::server::AppState;

// ── Constants ─────────────────────────────────────────────────────────────────

const TIMEOUT_STATUS: Duration = Duration::from_secs(5);
const TIMEOUT_BRANCH: Duration = Duration::from_secs(5);
const TIMEOUT_DIFF: Duration = Duration::from_secs(5);
const TIMEOUT_COMMIT: Duration = Duration::from_secs(15);
const TIMEOUT_PUSH: Duration = Duration::from_secs(60);
const TIMEOUT_PULL: Duration = Duration::from_secs(30);
const TIMEOUT_PR: Duration = Duration::from_secs(15);

const GITHUB_API: &str = "https://api.github.com";

// ── Type alias ────────────────────────────────────────────────────────────────

type Resp = (StatusCode, Json<Value>);

// ── Auth + response helpers ───────────────────────────────────────────────────

fn check_auth(headers: &HeaderMap, token: &str) -> Result<(), Resp> {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(unauthorized)?;
    if crate::auth::validate_bearer(authz, token) {
        Ok(())
    } else {
        Err(unauthorized())
    }
}

fn ok(body: Value) -> Resp {
    (StatusCode::OK, Json(body))
}

fn unauthorized() -> Resp {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "unauthorized" })),
    )
}

fn bad_request(msg: &str) -> Resp {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg })))
}

fn git_err(msg: &str) -> Resp {
    tracing::warn!(msg, "git operation failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "git_operation_failed" })),
    )
}

// ── Security helpers ──────────────────────────────────────────────────────────

/// Validate a branch name without shell expansion.
///
/// Allowed: `[a-zA-Z0-9]` at start + end; `[a-zA-Z0-9._\-/]` in body.
/// Rejected: any `..` substring (path-traversal / refname abuse).
fn validate_branch_name(name: &str) -> Result<(), &'static str> {
    if name.len() < 2 {
        return Err("branch name too short (minimum 2 chars)");
    }
    if name.contains("..") {
        return Err("branch name must not contain '..'");
    }
    let is_alnum = |c: char| c.is_ascii_alphanumeric();
    let is_body = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/');
    let mut chars = name.chars();
    let first = chars
        .next()
        .ok_or("branch name must start and end with [a-zA-Z0-9]")?;
    let last_char = name
        .chars()
        .last()
        .ok_or("branch name must start and end with [a-zA-Z0-9]")?;
    if !is_alnum(first) || !is_alnum(last_char) {
        return Err("branch name must start and end with [a-zA-Z0-9]");
    }
    if !chars.take(name.len() - 2).all(is_body) {
        return Err("branch name contains disallowed characters");
    }
    Ok(())
}

/// Load the GitHub PAT via 3-tier ladder: OS keyring → macOS `security` CLI → env var.
fn load_github_pat() -> Option<SecretBox<String>> {
    let from_keyring = keyring::Entry::new("lightarchitects-github", "pat")
        .ok()
        .and_then(|e| e.get_password().ok());

    #[cfg(target_os = "macos")]
    let from_keychain = from_keyring.clone().or_else(|| {
        let out = std::process::Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                "lightarchitects-github",
                "-a",
                "pat",
                "-w",
            ])
            .output()
            .ok()?;
        if out.status.success() {
            let s = String::from_utf8(out.stdout).ok()?;
            let trimmed = s.trim().to_owned();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        } else {
            None
        }
    });

    #[cfg(not(target_os = "macos"))]
    let from_keychain = from_keyring;

    from_keychain
        .or_else(|| std::env::var("LIGHTARCHITECTS_GITHUB_PAT").ok())
        .filter(|t| !t.is_empty())
        .map(|t| SecretBox::new(Box::new(t)))
}

/// Canonicalize and validate a `cwd` path to prevent directory traversal.
fn safe_cwd(cwd: &str) -> Result<std::path::PathBuf, &'static str> {
    let path = std::path::Path::new(cwd);
    if path
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return Err("cwd must not contain '..'");
    }
    std::fs::canonicalize(path).map_err(|_| "cwd does not exist or is not accessible")
}

// ── Subprocess helper ─────────────────────────────────────────────────────────

/// Run a `git` command with structured argv (T-7 — never a shell string).
async fn git_run(
    args: &[&str],
    cwd: &std::path::Path,
    timeout: Duration,
) -> Result<std::process::Output, Resp> {
    let fut = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output();
    tokio::time::timeout(timeout, fut)
        .await
        .map_err(|_| git_err("git command timed out"))?
        .map_err(|e| {
            tracing::warn!(error = %e, "git spawn failed");
            git_err("git spawn failed")
        })
}

// ── POST /api/git/status ─────────────────────────────────────────────────────

/// Return the porcelain v1 status of a working tree.
///
/// Body: `{"cwd": string}`
/// Returns: `{"files": [{path, status}], "clean": bool}`
pub async fn status_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Err(r) = check_auth(&headers, &state.config.token) {
        return r.into_response();
    }
    let Some(cwd_str) = body["cwd"].as_str() else {
        return bad_request("missing required field: cwd").into_response();
    };
    let cwd = match safe_cwd(cwd_str) {
        Ok(p) => p,
        Err(e) => return bad_request(e).into_response(),
    };
    let out = match git_run(&["status", "--porcelain=v1"], &cwd, TIMEOUT_STATUS).await {
        Ok(o) => o,
        Err(r) => return r.into_response(),
    };
    if !out.status.success() {
        return git_err("git status failed").into_response();
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let files: Vec<Value> = stdout
        .lines()
        .filter(|l| l.len() >= 3)
        .map(|l| {
            let status = l[..2].trim().to_owned();
            let path = l[3..].to_owned();
            json!({"path": path, "status": status})
        })
        .collect();
    let clean = files.is_empty();
    ok(json!({"files": files, "clean": clean})).into_response()
}

// ── POST /api/git/branch ─────────────────────────────────────────────────────

/// Perform a branch operation: list, create, switch, or delete.
///
/// Body: `{"op": "list"|"create"|"switch"|"delete", "name"?: string, "cwd": string}`
pub async fn branch_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Err(r) = check_auth(&headers, &state.config.token) {
        return r.into_response();
    }
    let Some(op) = body["op"].as_str() else {
        return bad_request("missing required field: op").into_response();
    };
    let Some(cwd_str) = body["cwd"].as_str() else {
        return bad_request("missing required field: cwd").into_response();
    };
    let cwd = match safe_cwd(cwd_str) {
        Ok(p) => p,
        Err(e) => return bad_request(e).into_response(),
    };
    let name = body["name"].as_str();
    match op {
        "list" => {
            let out = match git_run(
                &["branch", "--format=%(refname:short)"],
                &cwd,
                TIMEOUT_BRANCH,
            )
            .await
            {
                Ok(o) => o,
                Err(r) => return r.into_response(),
            };
            if !out.status.success() {
                return git_err("git branch list failed").into_response();
            }
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            let branches: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
            ok(json!({ "branches": branches })).into_response()
        }
        "create" => {
            let Some(name) = name else {
                return bad_request("missing required field: name for op=create").into_response();
            };
            if let Err(e) = validate_branch_name(name) {
                return bad_request(e).into_response();
            }
            let out = match git_run(&["checkout", "-b", name], &cwd, TIMEOUT_BRANCH).await {
                Ok(o) => o,
                Err(r) => return r.into_response(),
            };
            if !out.status.success() {
                return git_err("git checkout -b failed").into_response();
            }
            ok(json!({ "created": name })).into_response()
        }
        "switch" => {
            let Some(name) = name else {
                return bad_request("missing required field: name for op=switch").into_response();
            };
            if let Err(e) = validate_branch_name(name) {
                return bad_request(e).into_response();
            }
            let out = match git_run(&["checkout", name], &cwd, TIMEOUT_BRANCH).await {
                Ok(o) => o,
                Err(r) => return r.into_response(),
            };
            if !out.status.success() {
                return git_err("git checkout failed").into_response();
            }
            ok(json!({ "switched_to": name })).into_response()
        }
        "delete" => {
            let Some(name) = name else {
                return bad_request("missing required field: name for op=delete").into_response();
            };
            if let Err(e) = validate_branch_name(name) {
                return bad_request(e).into_response();
            }
            let out = match git_run(&["branch", "-d", name], &cwd, TIMEOUT_BRANCH).await {
                Ok(o) => o,
                Err(r) => return r.into_response(),
            };
            if !out.status.success() {
                return git_err("git branch -d failed").into_response();
            }
            ok(json!({ "deleted": name })).into_response()
        }
        _ => bad_request("op must be one of: list, create, switch, delete").into_response(),
    }
}

// ── POST /api/git/diff ───────────────────────────────────────────────────────

/// Return the diff for a working tree.
///
/// Body: `{"cwd": string, "staged"?: bool, "path"?: string}`
/// Returns: `{"diff": string}`
pub async fn diff_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Err(r) = check_auth(&headers, &state.config.token) {
        return r.into_response();
    }
    let Some(cwd_str) = body["cwd"].as_str() else {
        return bad_request("missing required field: cwd").into_response();
    };
    let cwd = match safe_cwd(cwd_str) {
        Ok(p) => p,
        Err(e) => return bad_request(e).into_response(),
    };
    let staged = body["staged"].as_bool().unwrap_or(false);
    let mut args = vec!["diff"];
    if staged {
        args.push("--staged");
    }
    // Optional path restriction — validated to not contain `..`.
    let path_buf;
    if let Some(p) = body["path"].as_str() {
        if p.contains("..") {
            return bad_request("path must not contain '..'").into_response();
        }
        args.push("--");
        path_buf = p.to_owned();
        args.push(&path_buf);
    }
    let out = match git_run(&args, &cwd, TIMEOUT_DIFF).await {
        Ok(o) => o,
        Err(r) => return r.into_response(),
    };
    if !out.status.success() {
        return git_err("git diff failed").into_response();
    }
    let diff = String::from_utf8_lossy(&out.stdout).into_owned();
    ok(json!({ "diff": diff })).into_response()
}

// ── POST /api/git/commit ─────────────────────────────────────────────────────

/// Commit staged changes with `--no-verify` (T-8 — hook bypass disclosed in UI).
///
/// Body: `{"cwd": string, "message": string}`
/// Returns: `{"sha": string, "message": string}`
pub async fn commit_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Err(r) = check_auth(&headers, &state.config.token) {
        return r.into_response();
    }
    let (Some(cwd_str), Some(message)) = (body["cwd"].as_str(), body["message"].as_str()) else {
        return bad_request("missing required fields: cwd, message").into_response();
    };
    let cwd = match safe_cwd(cwd_str) {
        Ok(p) => p,
        Err(e) => return bad_request(e).into_response(),
    };
    let out = match git_run(
        &["commit", "--no-verify", "-m", message],
        &cwd,
        TIMEOUT_COMMIT,
    )
    .await
    {
        Ok(o) => o,
        Err(r) => return r.into_response(),
    };
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        tracing::warn!(stderr = %stderr, "git commit failed");
        return git_err("git commit failed").into_response();
    }
    // Parse sha from `[branch abc1234] message` line.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let sha = stdout
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or("unknown")
        .to_owned();
    ok(json!({ "sha": sha, "message": message })).into_response()
}

// ── POST /api/git/push ───────────────────────────────────────────────────────

/// Push the current branch to origin.
///
/// Force-push is permanently blocked (T-5). `--no-verify` hook bypass is noted.
///
/// Body: `{"cwd": string, "set_upstream"?: bool, "branch"?: string}`
/// Returns: `{"pushed": bool, "url"?: string}`
pub async fn push_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Err(r) = check_auth(&headers, &state.config.token) {
        return r.into_response();
    }
    let Some(cwd_str) = body["cwd"].as_str() else {
        return bad_request("missing required field: cwd").into_response();
    };
    // T-5: force-push permanently blocked.
    if body["force"].as_bool().unwrap_or(false) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "force push is permanently disabled (T-5)" })),
        )
            .into_response();
    }
    let cwd = match safe_cwd(cwd_str) {
        Ok(p) => p,
        Err(e) => return bad_request(e).into_response(),
    };
    // Record auth-present flag before any await — EnteredSpan is !Send and must not
    // cross an .await boundary in an axum handler (breaks Handler<_, _> bound).
    let auth_present = load_github_pat().is_some();
    tracing::info!(git.auth_present = auth_present, "git push auth check");
    let set_upstream = body["set_upstream"].as_bool().unwrap_or(false);
    let args: Vec<&str> = if set_upstream {
        let branch_raw = body["branch"].as_str().unwrap_or("HEAD");
        if let Err(e) = validate_branch_name(branch_raw) {
            return bad_request(e).into_response();
        }
        vec!["push", "--set-upstream", "origin", branch_raw]
    } else {
        vec!["push"]
    };
    let out = match git_run(&args, &cwd, TIMEOUT_PUSH).await {
        Ok(o) => o,
        Err(r) => return r.into_response(),
    };
    if !out.status.success() {
        return git_err("git push failed").into_response();
    }
    ok(json!({ "pushed": true })).into_response()
}

// ── POST /api/git/pull ───────────────────────────────────────────────────────

/// Pull with `--ff-only` to refuse merge commits.
///
/// Body: `{"cwd": string}`
/// Returns: `{"merged": bool, "commits": u32}`
pub async fn pull_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Err(r) = check_auth(&headers, &state.config.token) {
        return r.into_response();
    }
    let Some(cwd_str) = body["cwd"].as_str() else {
        return bad_request("missing required field: cwd").into_response();
    };
    let cwd = match safe_cwd(cwd_str) {
        Ok(p) => p,
        Err(e) => return bad_request(e).into_response(),
    };
    let out = match git_run(&["pull", "--ff-only"], &cwd, TIMEOUT_PULL).await {
        Ok(o) => o,
        Err(r) => return r.into_response(),
    };
    if !out.status.success() {
        return git_err("git pull failed").into_response();
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let already_up = stdout.contains("Already up to date");
    ok(json!({ "merged": !already_up, "commits": 0u32 })).into_response()
}

// ── POST /api/git/pr/create ──────────────────────────────────────────────────

/// Create a GitHub pull request via the REST API.
///
/// Body: `{"owner": string, "repo": string, "title": string, "head": string, "base": string, "body"?: string}`
/// Returns: `{"number": u64, "url": string, "title": string}`
pub async fn create_pr_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Err(r) = check_auth(&headers, &state.config.token) {
        return r.into_response();
    }
    let required = ["owner", "repo", "title", "head", "base"];
    if required.iter().any(|f| body[f].as_str().is_none()) {
        return bad_request("missing required fields: owner, repo, title, head, base")
            .into_response();
    }
    let Some(pat) = load_github_pat() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "github_pat_not_configured" })),
        )
            .into_response();
    };
    let owner = body["owner"].as_str().unwrap_or_default();
    let repo = body["repo"].as_str().unwrap_or_default();
    let url = format!("{GITHUB_API}/repos/{owner}/{repo}/pulls");
    let payload = json!({
        "title": body["title"],
        "head":  body["head"],
        "base":  body["base"],
        "body":  body["body"].as_str().unwrap_or(""),
    });
    let client = reqwest::Client::new();
    let resp = match tokio::time::timeout(
        TIMEOUT_PR,
        client
            .post(&url)
            .bearer_auth(pat.expose_secret())
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "lightarchitects-webshell/0.2")
            .json(&payload)
            .send(),
    )
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "GitHub PR create request failed");
            return git_err("GitHub API request failed").into_response();
        }
        Err(_) => return git_err("GitHub API request timed out").into_response(),
    };
    let gh: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "GitHub PR create response parse failed");
            return git_err("GitHub API response parse failed").into_response();
        }
    };
    if let (Some(number), Some(url_str), Some(title)) = (
        gh["number"].as_u64(),
        gh["html_url"].as_str(),
        gh["title"].as_str(),
    ) {
        ok(json!({ "number": number, "url": url_str, "title": title })).into_response()
    } else {
        tracing::warn!(response = ?gh, "GitHub PR create unexpected response shape");
        git_err("unexpected GitHub API response").into_response()
    }
}

// ── POST /api/git/pr/review ──────────────────────────────────────────────────

/// Submit a GitHub PR review via the REST API.
///
/// Inline comments use `comments[].position` (diff-position integer), not `line`/`side`.
///
/// Body: `{"owner": string, "repo": string, "number": u64, "event": string, "body"?: string, "comments"?: [...]}`
/// Returns: `{"id": u64, "state": string}`
pub async fn review_pr_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Err(r) = check_auth(&headers, &state.config.token) {
        return r.into_response();
    }
    if body["owner"].as_str().is_none()
        || body["repo"].as_str().is_none()
        || body["number"].as_u64().is_none()
        || body["event"].as_str().is_none()
    {
        return bad_request("missing required fields: owner, repo, number, event").into_response();
    }
    let Some(pat) = load_github_pat() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "github_pat_not_configured" })),
        )
            .into_response();
    };
    let owner = body["owner"].as_str().unwrap_or_default();
    let repo = body["repo"].as_str().unwrap_or_default();
    let number = body["number"].as_u64().unwrap_or_default();
    let url = format!("{GITHUB_API}/repos/{owner}/{repo}/pulls/{number}/reviews");
    let payload = json!({
        "event":    body["event"],
        "body":     body["body"].as_str().unwrap_or(""),
        "comments": body["comments"].as_array().cloned().unwrap_or_default(),
    });
    let client = reqwest::Client::new();
    let resp = match tokio::time::timeout(
        TIMEOUT_PR,
        client
            .post(&url)
            .bearer_auth(pat.expose_secret())
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "lightarchitects-webshell/0.2")
            .json(&payload)
            .send(),
    )
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "GitHub PR review request failed");
            return git_err("GitHub API request failed").into_response();
        }
        Err(_) => return git_err("GitHub API request timed out").into_response(),
    };
    let gh: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "GitHub PR review response parse failed");
            return git_err("GitHub API response parse failed").into_response();
        }
    };
    if let (Some(id), Some(state_str)) = (gh["id"].as_u64(), gh["state"].as_str()) {
        ok(json!({ "id": id, "state": state_str })).into_response()
    } else {
        tracing::warn!(response = ?gh, "GitHub PR review unexpected response shape");
        git_err("unexpected GitHub API response").into_response()
    }
}

// ── Smoke tests (Canon XXVII suite 6) ─────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, StatusCode};

    use super::*;

    #[test]
    fn check_auth_rejects_missing_header() {
        let headers = HeaderMap::new();
        let result = check_auth(&headers, "secret");
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn check_auth_rejects_wrong_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer wrong-token"),
        );
        let result = check_auth(&headers, "correct-token");
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn check_auth_accepts_correct_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer correct-token"),
        );
        assert!(check_auth(&headers, "correct-token").is_ok());
    }

    #[test]
    fn bad_request_response_shape() {
        let (status, Json(body)) = bad_request("missing cwd");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "missing cwd");
    }

    #[test]
    fn ok_response_shape() {
        let (status, Json(body)) = ok(serde_json::json!({"branch": "main"}));
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["branch"], "main");
    }
}
