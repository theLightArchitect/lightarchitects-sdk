//! HTTP handlers for `git.*` tools (EEF E3 — git-and-pr).
//!
//! Provides endpoints backed by `tokio::process::Command::new("git")`
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
//! - `GET  /api/git/log`       — commit log + branch list for `GitForest` visualization
//!
//! All endpoints require bearer authentication. The GitHub PAT is loaded via the
//! 3-tier ladder: OS keyring → macOS `security` CLI → env var `LIGHTARCHITECTS_GITHUB_PAT`.

use std::{collections::HashMap, time::Duration};

use axum::{
    Json,
    extract::{Query, State},
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
const TIMEOUT_LOG: Duration = Duration::from_secs(10);
const TIMEOUT_PUSH: Duration = Duration::from_secs(60);
const TIMEOUT_PULL: Duration = Duration::from_secs(30);
const TIMEOUT_PR: Duration = Duration::from_secs(15);
const TIMEOUT_BLAME: Duration = Duration::from_secs(15);
const TIMEOUT_LOG_FILE: Duration = Duration::from_secs(10);

const GITHUB_API: &str = "https://api.github.com";

// ── Type alias ────────────────────────────────────────────────────────────────

type Resp = (StatusCode, Json<Value>);

// ── Auth + response helpers ───────────────────────────────────────────────────

fn check_auth(headers: &HeaderMap, token: &str) -> Result<(), Resp> {
    let bearer_ok = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| crate::auth::validate_bearer(s, token));
    if bearer_ok {
        return Ok(());
    }
    let cookie_ok = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| crate::auth::validate_session_cookie(s, token));
    if cookie_ok {
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

/// Validate a git ref (SHA, branch, tag, `HEAD`, refspecs) for use as a subprocess argument.
///
/// Rejects leading `-` (flag-smuggling CWE-88), leading `.`, `..` (path-traversal CWE-22),
/// and characters outside `[A-Za-z0-9._/@{}^~:-]`. Accepts SHAs, `HEAD`, branch refs, and
/// common refspecs like `HEAD@{1}` and `origin/main`.
fn validate_git_ref(r: &str) -> Result<(), &'static str> {
    if r.is_empty() {
        return Err("ref must not be empty");
    }
    if r.starts_with('-') {
        return Err("ref must not start with '-'");
    }
    if r.starts_with('.') {
        return Err("ref must not start with '.'");
    }
    if r.contains("..") {
        return Err("ref must not contain '..'");
    }
    if !r.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '.' | '_' | '/' | '-' | '@' | '{' | '}' | '^' | '~' | ':')
    }) {
        return Err("ref contains disallowed characters");
    }
    Ok(())
}

/// Validate a file path argument.
///
/// Rejects `..` (path-traversal CWE-22) and leading `:` (git pathspec magic).
fn validate_path_arg(p: &str) -> Result<(), &'static str> {
    if p.contains("..") {
        return Err("path must not contain '..'");
    }
    if p.starts_with(':') {
        return Err("path must not start with ':' (pathspec magic)");
    }
    Ok(())
}

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

// ── POST /api/git/worktrees ──────────────────────────────────────────────────

/// Per-worktree metadata: path, branch, head SHA, status, locked flag, head commit time.
///
/// Body: `{"cwd": string}` — any path inside the target git repository
/// Returns: `{"worktrees": [{path, branch, head_sha, status, locked, created_at}]}`
/// where `created_at` is the head commit time (ISO-8601 from `git log -1 --format=%cI`).
///
/// Closes spec §2.27 (webshell-mock-overlay-shipping 2026-05-20 STUB).
/// Removes the `MockBadge label="META" detail="locked/created_at pending"` from
/// `WorktreePanel.svelte` once this handler ships and the frontend wires the call.
pub async fn worktrees_handler(
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
    let out = match git_run(&["worktree", "list", "--porcelain"], &cwd, TIMEOUT_STATUS).await {
        Ok(o) => o,
        Err(r) => return r.into_response(),
    };
    if !out.status.success() {
        return git_err("git worktree list failed").into_response();
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed = parse_worktree_porcelain(&stdout);
    let enriched = enrich_with_created_at(&parsed, &cwd).await;
    ok(json!({ "worktrees": enriched })).into_response()
}

/// One worktree row parsed from `git worktree list --porcelain` output.
#[derive(Debug, Clone)]
struct WorktreeRow {
    path: String,
    branch: String,
    head_sha: String,
    locked: bool,
}

/// Parse `git worktree list --porcelain` output into structured rows.
///
/// Porcelain format (one block per worktree, blank-line separated):
/// ```text
/// worktree /path/to/wt
/// HEAD <sha>
/// branch refs/heads/<name>   (or `detached`)
/// locked                      (optional; may have a reason suffix)
/// ```
fn parse_worktree_porcelain(stdout: &str) -> Vec<WorktreeRow> {
    let mut result = Vec::new();
    let mut current: Option<WorktreeRow> = None;
    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(wt) = current.take() {
                result.push(wt);
            }
            current = Some(WorktreeRow {
                path: path.to_owned(),
                branch: String::new(),
                head_sha: String::new(),
                locked: false,
            });
        } else if let Some(sha) = line.strip_prefix("HEAD ") {
            if let Some(wt) = current.as_mut() {
                sha.clone_into(&mut wt.head_sha);
            }
        } else if let Some(branch) = line.strip_prefix("branch ") {
            if let Some(wt) = current.as_mut() {
                let name = branch.strip_prefix("refs/heads/").unwrap_or(branch);
                name.clone_into(&mut wt.branch);
            }
        } else if line == "detached" {
            if let Some(wt) = current.as_mut() {
                "(detached)".clone_into(&mut wt.branch);
            }
        } else if line == "locked" || line.starts_with("locked ") {
            if let Some(wt) = current.as_mut() {
                wt.locked = true;
            }
        }
    }
    if let Some(wt) = current.take() {
        result.push(wt);
    }
    result
}

/// Add `created_at` (ISO-8601 head commit time) per row via `git log -1 --format=%cI <sha>`.
async fn enrich_with_created_at(rows: &[WorktreeRow], cwd: &std::path::Path) -> Vec<Value> {
    let mut out = Vec::with_capacity(rows.len());
    for wt in rows {
        let created_at = if wt.head_sha.is_empty() {
            None
        } else {
            git_run(
                &["log", "-1", "--format=%cI", &wt.head_sha],
                cwd,
                TIMEOUT_STATUS,
            )
            .await
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_owned())
                } else {
                    None
                }
            })
        };
        out.push(json!({
            "path": wt.path,
            "branch": wt.branch,
            "head_sha": wt.head_sha,
            "status": "active",
            "locked": wt.locked,
            "created_at": created_at,
        }));
    }
    out
}

// ── GET /api/git/log ─────────────────────────────────────────────────────────

/// Commit log + branch list for the `GitForest` visualization.
///
/// Query params: `cwd=<path>` (required), `limit=<n>` (optional, default 40, max 100).
/// Returns `{ commits: [...], branches: [...] }`.
///
/// Each commit: `{ sha, short_sha, message, author, timestamp, parent_shas, refs }`.
/// Each branch: `{ name, head_sha, is_current }`.
#[allow(clippy::implicit_hasher)]
pub async fn log_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if let Err(r) = check_auth(&headers, &state.config.token) {
        return r.into_response();
    }
    let Some(cwd_str) = params.get("cwd").filter(|s| !s.is_empty()) else {
        return bad_request("missing required query param: cwd").into_response();
    };
    let cwd = match safe_cwd(cwd_str) {
        Ok(p) => p,
        Err(e) => return bad_request(e).into_response(),
    };
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(40)
        .min(100);
    let limit_arg = format!("-n{limit}");

    // git log --all --format="%H\t%h\t%s\t%an\t%ai\t%P\t%D" -nN
    let log_out = match git_run(
        &[
            "log",
            "--all",
            "--format=%H\t%h\t%s\t%an\t%ai\t%P\t%D",
            &limit_arg,
        ],
        &cwd,
        TIMEOUT_LOG,
    )
    .await
    {
        Ok(o) => o,
        Err(r) => return r.into_response(),
    };
    if !log_out.status.success() {
        return git_err("git log failed").into_response();
    }

    let log_stdout = String::from_utf8_lossy(&log_out.stdout);
    let commits: Vec<Value> = log_stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let mut parts = line.splitn(7, '\t');
            let sha = parts.next().unwrap_or("");
            let short_sha = parts.next().unwrap_or("");
            let message = parts.next().unwrap_or("");
            let author = parts.next().unwrap_or("");
            let timestamp = parts.next().unwrap_or("");
            let parents_raw = parts.next().unwrap_or("");
            let refs_raw = parts.next().unwrap_or("");

            let parent_shas: Vec<&str> = parents_raw.split_whitespace().collect();
            let refs: Vec<&str> = refs_raw
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();

            json!({
                "sha": sha,
                "short_sha": short_sha,
                "message": message,
                "author": author,
                "timestamp": timestamp,
                "parent_shas": parent_shas,
                "refs": refs,
            })
        })
        .collect();

    // git branch -a --format=%(refname:short)\t%(objectname:short)\t%(HEAD)
    let branch_out = match git_run(
        &[
            "branch",
            "-a",
            "--format=%(refname:short)\t%(objectname:short)\t%(HEAD)",
        ],
        &cwd,
        TIMEOUT_LOG,
    )
    .await
    {
        Ok(o) => o,
        Err(r) => return r.into_response(),
    };

    let branch_stdout = String::from_utf8_lossy(&branch_out.stdout);
    let branches: Vec<Value> = branch_stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let mut parts = line.splitn(3, '\t');
            let name = parts.next().unwrap_or("");
            let head_sha = parts.next().unwrap_or("");
            let is_current = parts.next().unwrap_or("") == "*";
            json!({
                "name": name,
                "head_sha": head_sha,
                "is_current": is_current,
            })
        })
        .collect();

    ok(json!({ "commits": commits, "branches": branches })).into_response()
}

// ── POST /api/git/diff-file ───────────────────────────────────────────────────

/// Return a structured hunk diff for a single file between two refs.
///
/// Body: `{"cwd": string, "path": string, "base": string, "head": string}`
/// Returns: `{"hunks": [{header, old_start, old_count, new_start, new_count,
///   lines: [{type, content, old_n?, new_n?}]}], "stats": {added, removed, hunks}}`
///
/// @api POST /api/git/diff-file
/// @contract code.file-diff.v1
/// Security: path/refs validated for `..` (CWE-22); structured argv (CWE-78); opaque errors (CWE-209).
pub async fn diff_file_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Err(r) = check_auth(&headers, &state.config.token) {
        return r.into_response();
    }
    let (Some(cwd_str), Some(path_str), Some(base), Some(head)) = (
        body["cwd"].as_str(),
        body["path"].as_str(),
        body["base"].as_str(),
        body["head"].as_str(),
    ) else {
        return bad_request("missing required fields: cwd, path, base, head").into_response();
    };
    let cwd = match safe_cwd(cwd_str) {
        Ok(p) => p,
        Err(e) => return bad_request(e).into_response(),
    };
    if let Err(e) = validate_path_arg(path_str) {
        return bad_request(e).into_response();
    }
    if let Err(e) = validate_git_ref(base) {
        return bad_request(e).into_response();
    }
    if let Err(e) = validate_git_ref(head) {
        return bad_request(e).into_response();
    }
    let out = match git_run(&["diff", base, head, "--", path_str], &cwd, TIMEOUT_DIFF).await {
        Ok(o) => o,
        Err(r) => return r.into_response(),
    };
    if !out.status.success() {
        return git_err("git diff-file failed").into_response();
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    let (hunks, diff_stats) = parse_unified_diff(&raw);
    ok(json!({ "hunks": hunks, "stats": diff_stats })).into_response()
}

/// Parse unified diff text into structured hunks with per-line type annotation.
fn parse_unified_diff(raw: &str) -> (Vec<Value>, Value) {
    let mut hunks: Vec<Value> = Vec::new();
    let mut current_lines: Vec<Value> = Vec::new();
    let mut current_header = String::new();
    let mut current_old_start = 0u64;
    let mut current_old_count = 0u64;
    let mut current_new_start = 0u64;
    let mut current_new_count = 0u64;
    let mut old_n = 0u64;
    let mut new_n = 0u64;
    let mut total_added = 0u64;
    let mut total_removed = 0u64;

    for line in raw.lines() {
        if line.starts_with("@@ ") {
            if !current_header.is_empty() {
                hunks.push(flush_hunk(
                    &current_header,
                    current_old_start,
                    current_old_count,
                    current_new_start,
                    current_new_count,
                    &current_lines,
                ));
                current_lines.clear();
            }
            line.clone_into(&mut current_header);
            if let Some(rest) = line.strip_prefix("@@ ") {
                let mut parts = rest.splitn(3, ' ');
                let old_part = parts.next().unwrap_or("").trim_start_matches('-');
                let new_part = parts.next().unwrap_or("").trim_start_matches('+');
                (current_old_start, current_old_count) = parse_hunk_range(old_part);
                (current_new_start, current_new_count) = parse_hunk_range(new_part);
                old_n = current_old_start;
                new_n = current_new_start;
            }
        } else if line.starts_with('+') && !line.starts_with("+++") {
            current_lines.push(json!({ "type": "add", "content": &line[1..], "new_n": new_n }));
            new_n += 1;
            total_added += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            current_lines.push(json!({ "type": "del", "content": &line[1..], "old_n": old_n }));
            old_n += 1;
            total_removed += 1;
        } else if let Some(content) = line.strip_prefix(' ') {
            current_lines
                .push(json!({ "type": "ctx", "content": content, "old_n": old_n, "new_n": new_n }));
            old_n += 1;
            new_n += 1;
        }
    }
    if !current_header.is_empty() {
        hunks.push(flush_hunk(
            &current_header,
            current_old_start,
            current_old_count,
            current_new_start,
            current_new_count,
            &current_lines,
        ));
    }
    let hunk_count = hunks.len() as u64;
    (
        hunks,
        json!({ "added": total_added, "removed": total_removed, "hunks": hunk_count }),
    )
}

fn parse_hunk_range(s: &str) -> (u64, u64) {
    let mut it = s.splitn(2, ',');
    let start = it.next().and_then(|x| x.parse().ok()).unwrap_or(0u64);
    let count = it.next().and_then(|x| x.parse().ok()).unwrap_or(1u64);
    (start, count)
}

fn flush_hunk(
    header: &str,
    old_start: u64,
    old_count: u64,
    new_start: u64,
    new_count: u64,
    lines: &[Value],
) -> Value {
    json!({
        "header": header,
        "old_start": old_start,
        "old_count": old_count,
        "new_start": new_start,
        "new_count": new_count,
        "lines": lines,
    })
}

// ── POST /api/git/blame ───────────────────────────────────────────────────────

/// Return per-line blame info with commit metadata for a file.
///
/// Body: `{"cwd": string, "path": string, "ref"?: string}`
/// Returns: `{"lines": [{n, content, commit: {hash, author_name, author_time, summary}}]}`
///
/// @api POST /api/git/blame
/// @contract code.blame.v1
/// Security: path validated for `..` (CWE-22); `author_email` not surfaced to client; opaque errors (CWE-209).
pub async fn blame_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Err(r) = check_auth(&headers, &state.config.token) {
        return r.into_response();
    }
    let (Some(cwd_str), Some(path_str)) = (body["cwd"].as_str(), body["path"].as_str()) else {
        return bad_request("missing required fields: cwd, path").into_response();
    };
    let cwd = match safe_cwd(cwd_str) {
        Ok(p) => p,
        Err(e) => return bad_request(e).into_response(),
    };
    if let Err(e) = validate_path_arg(path_str) {
        return bad_request(e).into_response();
    }
    let ref_arg = body["ref"].as_str().unwrap_or("HEAD");
    if let Err(e) = validate_git_ref(ref_arg) {
        return bad_request(e).into_response();
    }
    let out = match git_run(
        &["blame", "--porcelain", ref_arg, "--", path_str],
        &cwd,
        TIMEOUT_BLAME,
    )
    .await
    {
        Ok(o) => o,
        Err(r) => return r.into_response(),
    };
    if !out.status.success() {
        return git_err("git blame failed").into_response();
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    let lines = parse_blame_porcelain(&raw);
    ok(json!({ "lines": lines })).into_response()
}

/// Parse `git blame --porcelain` output into per-line records with cached commit metadata.
fn parse_blame_porcelain(raw: &str) -> Vec<Value> {
    let mut lines_out: Vec<Value> = Vec::new();
    // hash → (author_name, author_time, summary) — populated on first occurrence.
    let mut commit_cache: HashMap<String, (String, i64, String)> = HashMap::new();
    let mut current_hash = String::new();
    let mut current_final_line: u64 = 0;
    let mut pending_author = String::new();
    let mut pending_author_time: i64 = 0;
    let mut pending_summary = String::new();

    for line in raw.lines() {
        if let Some(content) = line.strip_prefix('\t') {
            // Flush pending fields into cache on first occurrence.
            if !current_hash.is_empty() && !commit_cache.contains_key(&current_hash) {
                commit_cache.insert(
                    current_hash.clone(),
                    (
                        pending_author.clone(),
                        pending_author_time,
                        pending_summary.clone(),
                    ),
                );
            }
            if let Some((author_name, author_time, summary)) = commit_cache.get(&current_hash) {
                lines_out.push(json!({
                    "n": current_final_line,
                    "content": content,
                    "commit": {
                        "hash": &current_hash,
                        "author_name": author_name,
                        "author_time": author_time,
                        "summary": summary,
                    }
                }));
            }
            pending_author.clear();
            pending_author_time = 0;
            pending_summary.clear();
        } else if is_blame_hash_line(line) {
            let mut parts = line.split_ascii_whitespace();
            parts.next().unwrap_or("").clone_into(&mut current_hash);
            let _orig = parts.next();
            current_final_line = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("author ") {
            rest.clone_into(&mut pending_author);
        } else if let Some(rest) = line.strip_prefix("author-time ") {
            pending_author_time = rest.parse().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("summary ") {
            rest.clone_into(&mut pending_summary);
        }
    }
    lines_out
}

/// Return true if `line` is a porcelain blame hash header: 40 hex chars then a space.
fn is_blame_hash_line(line: &str) -> bool {
    let Some((hash, _)) = line.split_once(' ') else {
        return false;
    };
    hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit())
}

// ── POST /api/git/log-file ────────────────────────────────────────────────────

/// Return file-specific commit history with rename tracking and per-commit stats.
///
/// Body: `{"cwd": string, "path": string, "n"?: number}` (n default 20, max 100)
/// Returns: `{"commits": [{hash, summary, author_name, author_time, stats: {added, removed}}]}`
///
/// @api POST /api/git/log-file
/// @contract code.log-file.v1
/// Security: path validated for `..` (CWE-22); structured argv (CWE-78); opaque errors (CWE-209).
pub async fn log_file_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Err(r) = check_auth(&headers, &state.config.token) {
        return r.into_response();
    }
    let (Some(cwd_str), Some(path_str)) = (body["cwd"].as_str(), body["path"].as_str()) else {
        return bad_request("missing required fields: cwd, path").into_response();
    };
    let cwd = match safe_cwd(cwd_str) {
        Ok(p) => p,
        Err(e) => return bad_request(e).into_response(),
    };
    if let Err(e) = validate_path_arg(path_str) {
        return bad_request(e).into_response();
    }
    let n = body["n"].as_u64().unwrap_or(20).min(100);
    let n_arg = format!("-n{n}");
    // `--numstat` emits per-file add/remove counts after each commit header.
    // `SPLIT\t` sentinel separates commits; `--follow` tracks renames.
    let out = match git_run(
        &[
            "log",
            "--follow",
            "--numstat",
            "--format=SPLIT\t%H\t%s\t%an\t%at",
            &n_arg,
            "--",
            path_str,
        ],
        &cwd,
        TIMEOUT_LOG_FILE,
    )
    .await
    {
        Ok(o) => o,
        Err(r) => return r.into_response(),
    };
    if !out.status.success() {
        return git_err("git log-file failed").into_response();
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    let commits = parse_log_file_output(&raw);
    ok(json!({ "commits": commits })).into_response()
}

/// Parse `git log --follow --numstat --format=SPLIT\t%H\t%s\t%an\t%at` output.
fn parse_log_file_output(raw: &str) -> Vec<Value> {
    let mut commits: Vec<Value> = Vec::new();
    let mut current_hash = String::new();
    let mut current_summary = String::new();
    let mut current_author = String::new();
    let mut current_time: i64 = 0;
    let mut current_added: u64 = 0;
    let mut current_removed: u64 = 0;
    let mut in_commit = false;

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("SPLIT\t") {
            if in_commit {
                commits.push(make_log_entry(
                    &current_hash,
                    &current_summary,
                    &current_author,
                    current_time,
                    current_added,
                    current_removed,
                ));
            }
            let mut parts = rest.splitn(4, '\t');
            parts.next().unwrap_or("").clone_into(&mut current_hash);
            parts.next().unwrap_or("").clone_into(&mut current_summary);
            parts.next().unwrap_or("").clone_into(&mut current_author);
            current_time = parts
                .next()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
            current_added = 0;
            current_removed = 0;
            in_commit = true;
        } else if in_commit && !line.is_empty() && line.contains('\t') {
            // numstat line: "<added>\t<removed>\t<path>"
            let mut parts = line.splitn(3, '\t');
            current_added += parts
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            current_removed += parts
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
        }
    }
    if in_commit {
        commits.push(make_log_entry(
            &current_hash,
            &current_summary,
            &current_author,
            current_time,
            current_added,
            current_removed,
        ));
    }
    commits
}

fn make_log_entry(
    hash: &str,
    summary: &str,
    author_name: &str,
    author_time: i64,
    added: u64,
    removed: u64,
) -> Value {
    json!({
        "hash": hash,
        "summary": summary,
        "author_name": author_name,
        "author_time": author_time,
        "stats": { "added": added, "removed": removed },
    })
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

    // ── Worktree porcelain parser tests (spec §2.27) ──────────────────────────

    #[test]
    fn parse_worktree_porcelain_handles_single_main_worktree() {
        let stdout =
            "worktree /Users/kft/Projects/repo\nHEAD abc123def\nbranch refs/heads/main\n\n";
        let rows = super::parse_worktree_porcelain(stdout);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "/Users/kft/Projects/repo");
        assert_eq!(rows[0].branch, "main");
        assert_eq!(rows[0].head_sha, "abc123def");
        assert!(!rows[0].locked);
    }

    #[test]
    fn parse_worktree_porcelain_handles_multiple_worktrees_with_lock() {
        let stdout = "worktree /a\nHEAD aaa\nbranch refs/heads/main\n\n\
                      worktree /b\nHEAD bbb\nbranch refs/heads/feat/foo\nlocked\n\n\
                      worktree /c\nHEAD ccc\ndetached\n";
        let rows = super::parse_worktree_porcelain(stdout);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].branch, "main");
        assert!(!rows[0].locked);
        assert_eq!(rows[1].branch, "feat/foo");
        assert!(rows[1].locked);
        assert_eq!(rows[2].branch, "(detached)");
    }

    #[test]
    fn parse_worktree_porcelain_handles_locked_with_reason() {
        let stdout =
            "worktree /a\nHEAD aaa\nbranch refs/heads/main\nlocked manual lock for testing\n";
        let rows = super::parse_worktree_porcelain(stdout);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].locked);
    }

    #[test]
    fn parse_worktree_porcelain_handles_empty_input() {
        let rows = super::parse_worktree_porcelain("");
        assert!(rows.is_empty());
    }

    #[test]
    fn parse_unified_diff_empty_input() {
        let (hunks, stats) = super::parse_unified_diff("");
        assert!(hunks.is_empty());
        assert_eq!(stats["added"], 0);
        assert_eq!(stats["removed"], 0);
        assert_eq!(stats["hunks"], 0);
    }

    #[test]
    fn parse_unified_diff_single_hunk() {
        let raw = "@@ -1,3 +1,4 @@\n context\n-removed\n+added\n+extra\n context\n";
        let (hunks, stats) = super::parse_unified_diff(raw);
        assert_eq!(hunks.len(), 1);
        assert_eq!(stats["added"], 2);
        assert_eq!(stats["removed"], 1);
        let lines = hunks[0]["lines"].as_array().unwrap();
        assert_eq!(lines[0]["type"], "ctx");
        assert_eq!(lines[1]["type"], "del");
        assert_eq!(lines[2]["type"], "add");
    }

    #[test]
    fn parse_hunk_range_with_count() {
        assert_eq!(super::parse_hunk_range("10,5"), (10, 5));
    }

    #[test]
    fn parse_hunk_range_without_count_defaults_to_one() {
        assert_eq!(super::parse_hunk_range("7"), (7, 1));
    }

    #[test]
    fn is_blame_hash_line_accepts_valid() {
        let line = "a".repeat(40) + " 1 1 1";
        assert!(super::is_blame_hash_line(&line));
    }

    #[test]
    fn is_blame_hash_line_rejects_short() {
        assert!(!super::is_blame_hash_line("abc123 1 1"));
    }

    #[test]
    fn parse_blame_porcelain_basic() {
        let hash = "a".repeat(40);
        let raw = format!(
            "{hash} 1 1 1\nauthor Alice\nauthor-time 1000\nsummary Fix bug\nfilename foo.rs\n\tcontent here\n"
        );
        let lines = super::parse_blame_porcelain(&raw);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["n"], 1);
        assert_eq!(lines[0]["content"], "content here");
        assert_eq!(lines[0]["commit"]["author_name"], "Alice");
        assert_eq!(lines[0]["commit"]["summary"], "Fix bug");
    }

    #[test]
    fn parse_blame_porcelain_empty_input() {
        assert!(super::parse_blame_porcelain("").is_empty());
    }

    #[test]
    fn parse_log_file_output_basic() {
        let raw = "SPLIT\tabc123\tFix the bug\tAlice\t1000\n\n3\t1\tpath/file.rs\n";
        let commits = super::parse_log_file_output(raw);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0]["hash"], "abc123");
        assert_eq!(commits[0]["summary"], "Fix the bug");
        assert_eq!(commits[0]["stats"]["added"], 3);
        assert_eq!(commits[0]["stats"]["removed"], 1);
    }

    #[test]
    fn parse_log_file_output_empty_input() {
        assert!(super::parse_log_file_output("").is_empty());
    }
}
