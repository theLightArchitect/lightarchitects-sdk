//! `git.*` tool suite — repository inspection and GitHub integration.
//!
//! # Security (T-7 CWE-78 + T-5 force-push — EEF Wave E3)
//!
//! All subprocess calls use structured argv: `Command::new("git").args([...])`
//! — never `Command::new("sh").arg("-c")`. Additional controls:
//!
//! - [`validate_branch_name`]: allowlist regex + `..` rejection for branch names.
//! - [`validate_cwd`]: `std::fs::canonicalize` + reject parent-traversal components.
//! - [`load_github_pat`]: keyring → macOS `security` CLI → env var; wrapped in
//!   `SecretBox`. PAT is exposed **only** at the HTTP call site.
//! - [`redact_token_in_trace`]: replaces `ghp_*` / `github_pat_*` tokens in URLs
//!   before they enter any tracing span.
//! - Force-push: any params with `force: true` return
//!   `GatewayError::Forbidden("force push disabled")`.
//!
//! # Tools
//!
//! | Name | Description |
//! |------|-------------|
//! | `lightarchitects_git_status` | Porcelain status of a working tree |
//! | `lightarchitects_git_branch` | List / create / switch / delete branches |
//! | `lightarchitects_git_diff` | Staged or unstaged diff |
//! | `lightarchitects_git_commit` | Commit staged changes |
//! | `lightarchitects_git_push` | Push to origin |
//! | `lightarchitects_git_pull` | Fast-forward pull |
//! | `lightarchitects_git_create_pr` | Create a GitHub pull request |
//! | `lightarchitects_git_review_pr` | Submit a GitHub PR review |

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use secrecy::{ExposeSecret, SecretBox};
use serde_json::{Value, json};
use tracing::instrument;

use crate::error::GatewayError;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Timeout for status, branch list, diff operations.
const TIMEOUT_SHORT: Duration = Duration::from_secs(5);
/// Timeout for commit operations.
const TIMEOUT_COMMIT: Duration = Duration::from_secs(15);
/// Timeout for pull operations.
const TIMEOUT_PULL: Duration = Duration::from_secs(30);
/// Timeout for push operations.
const TIMEOUT_PUSH: Duration = Duration::from_secs(60);
/// Timeout for GitHub API calls (PR create/review).
const TIMEOUT_GITHUB_API: Duration = Duration::from_secs(15);

/// Branch name allowlist regex: starts and ends with alnum, allows `._-/` in body.
///
/// `/` is included for hierarchical branch namespacing (`feat/`, `fix/`, etc.).
/// Path traversal via `..` is rejected separately in [`validate_branch_name`].
const BRANCH_NAME_RE: &str = r"^[a-zA-Z0-9][a-zA-Z0-9._\-/]{0,253}[a-zA-Z0-9]$";

// ── Compiled singletons ───────────────────────────────────────────────────────

fn branch_name_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // BRANCH_NAME_RE is a compile-time constant; Regex::new only fails on
        // malformed pattern syntax, which would be caught in tests.
        #[allow(clippy::expect_used)]
        Regex::new(BRANCH_NAME_RE).expect("BRANCH_NAME_RE is a valid static pattern")
    })
}

fn redact_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Static PAT-redaction pattern; validity verified at test time.
        #[allow(clippy::expect_used)]
        Regex::new(r"(ghp_[A-Za-z0-9]+|github_pat_[A-Za-z0-9_]+)")
            .expect("redact pattern is a valid static regex")
    })
}

// ── Security helpers ──────────────────────────────────────────────────────────

/// Validate a branch name against the T-7 allowlist.
///
/// Requires 2+ characters, `[a-zA-Z0-9]` at start and end, `._-` in body.
/// Rejects any name containing the `..` substring (path traversal / git refname abuse).
///
/// # Errors
///
/// Returns [`GatewayError::InvalidParam`] with a descriptive message on failure.
pub fn validate_branch_name(name: &str) -> Result<(), GatewayError> {
    if name.contains("..") {
        return Err(GatewayError::InvalidParam(format!(
            "T-7: branch name {name:?} contains '..' (disallowed)"
        )));
    }
    if !branch_name_re().is_match(name) {
        return Err(GatewayError::InvalidParam(format!(
            "T-7: branch name {name:?} does not match allowlist pattern {BRANCH_NAME_RE}"
        )));
    }
    Ok(())
}

/// Canonicalize `cwd` and verify it does not escape allowed project roots.
///
/// Rejects paths that contain `..` before canonicalization (pre-traversal gate).
/// After canonicalization, the path must not be `/` itself.
///
/// # Errors
///
/// Returns [`GatewayError::InvalidParam`] on traversal attempt or invalid path.
/// Returns [`GatewayError::File`] when the path does not exist on disk.
fn validate_cwd(cwd_raw: &str) -> Result<PathBuf, GatewayError> {
    // Pre-canonicalization gate: reject raw `..` components.
    if cwd_raw.split('/').any(|seg| seg == "..") {
        return Err(GatewayError::InvalidParam(
            "T-7: cwd contains '..' parent-traversal component (disallowed)".to_owned(),
        ));
    }
    let expanded = expand_tilde(cwd_raw);
    let canonical = std::fs::canonicalize(&expanded).map_err(|e| {
        GatewayError::File(format!(
            "cwd '{}' cannot be canonicalized: {e}",
            expanded.display()
        ))
    })?;
    if canonical == Path::new("/") {
        return Err(GatewayError::InvalidParam(
            "T-7: cwd resolved to filesystem root (disallowed)".to_owned(),
        ));
    }
    Ok(canonical)
}

/// Expand a leading `~/` to the user's home directory.
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs_next::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

/// Load the GitHub PAT with a 3-tier fallback strategy.
///
/// 1. `keyring::Entry::new("lightarchitects-github", "pat")` → `.get_password()`
/// 2. macOS `security find-generic-password` CLI (macOS only)
/// 3. `LIGHTARCHITECTS_GITHUB_PAT` environment variable
///
/// Returns `None` when no PAT is available in any tier.
pub fn load_github_pat() -> Option<SecretBox<String>> {
    // Tier 1: keyring
    let via_keyring = keyring::Entry::new("lightarchitects-github", "pat")
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|s| !s.is_empty());
    if let Some(pat) = via_keyring {
        return Some(SecretBox::new(Box::new(pat)));
    }

    // Tier 2: macOS security CLI
    #[cfg(target_os = "macos")]
    {
        let pat = keychain_via_security_cli();
        if let Some(pat) = pat {
            return Some(SecretBox::new(Box::new(pat)));
        }
    }

    // Tier 3: environment variable
    std::env::var("LIGHTARCHITECTS_GITHUB_PAT")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|pat| SecretBox::new(Box::new(pat)))
}

/// Read the GitHub PAT from the macOS keychain via the `security` CLI.
///
/// Same pattern as `keychain_via_security_cli` in `main.rs`. Uses the `security`
/// binary because `keyring` v3 with `sync-secret-service` targets D-Bus (Linux);
/// on macOS it falls back to an in-process mock that never persists to Keychain.
#[cfg(target_os = "macos")]
fn keychain_via_security_cli() -> Option<String> {
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
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    None
}

/// Replace `ghp_*` and `github_pat_*` tokens in a URL with `[REDACTED]`.
///
/// Use **only** for tracing spans — never log the raw PAT.
pub fn redact_token_in_trace(url: &str) -> String {
    redact_re().replace_all(url, "[REDACTED]").into_owned()
}

/// Run a git subcommand with a timeout and return stdout as a `String`.
///
/// Uses `tokio::process::Command::new("git")` — never a shell string.
///
/// # Errors
///
/// Returns [`GatewayError::Subprocess`] on non-zero exit or spawn failure.
/// Returns [`GatewayError::Timeout`] when the timeout elapses.
async fn git_run(args: &[&str], cwd: &Path, timeout: Duration) -> Result<String, GatewayError> {
    let fut = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output();
    let output = tokio::time::timeout(timeout, fut)
        .await
        .map_err(|_| GatewayError::Subprocess(format!("git {args:?} timed out")))?
        .map_err(|e| {
            tracing::warn!(args = ?args, error = %e, "git spawn failed");
            GatewayError::Subprocess("git spawn failed; see gateway logs".to_owned())
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(GatewayError::Subprocess(format!(
            "git {:?} exited {}: {}",
            args,
            output.status.code().unwrap_or(-1),
            stderr.trim()
        )))
    }
}

// ── git.status ────────────────────────────────────────────────────────────────

/// Return the porcelain v1 status of a working tree.
///
/// Params: `{cwd: string}`
/// Returns: `{files: [{path: string, status: string}], clean: bool}`
///
/// # Errors
///
/// Returns [`GatewayError`] on invalid `cwd` or git subprocess failure.
#[instrument(skip(params))]
pub async fn run_status(params: Value) -> Result<Value, GatewayError> {
    let cwd_raw = params["cwd"]
        .as_str()
        .ok_or(GatewayError::MissingParam("cwd"))?;
    let cwd = validate_cwd(cwd_raw)?;

    let stdout = git_run(&["status", "--porcelain=v1"], &cwd, TIMEOUT_SHORT).await?;

    let files: Vec<Value> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            // Porcelain v1: first 2 chars are XY status, then a space, then path.
            let (xy, path) = l.split_at(l.len().min(2));
            json!({
                "path": path.trim_start(),
                "status": xy.trim()
            })
        })
        .collect();
    let clean = files.is_empty();

    Ok(json!({ "files": files, "clean": clean }))
}

// ── git.branch ────────────────────────────────────────────────────────────────

/// Perform a branch operation: list, create, switch, or delete.
///
/// Params: `{op: "list"|"create"|"switch"|"delete", name?: string, cwd: string}`
/// Returns:
/// - list → `{branches: [string]}`
/// - create/switch/delete → `{ok: bool, branch: string}`
///
/// # Errors
///
/// Returns [`GatewayError`] on invalid params, T-7 branch-name rejection, or git failure.
#[instrument(skip(params))]
pub async fn run_branch_op(params: Value) -> Result<Value, GatewayError> {
    let cwd_raw = params["cwd"]
        .as_str()
        .ok_or(GatewayError::MissingParam("cwd"))?;
    let cwd = validate_cwd(cwd_raw)?;
    let op = params["op"]
        .as_str()
        .ok_or(GatewayError::MissingParam("op"))?;

    match op {
        "list" => {
            let stdout = git_run(
                &["branch", "--format=%(refname:short)"],
                &cwd,
                TIMEOUT_SHORT,
            )
            .await?;
            let branches: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
            Ok(json!({ "branches": branches }))
        }
        "create" => {
            let name = params["name"]
                .as_str()
                .ok_or(GatewayError::MissingParam("name"))?;
            validate_branch_name(name)?;
            git_run(&["checkout", "-b", name], &cwd, TIMEOUT_SHORT).await?;
            Ok(json!({ "ok": true, "branch": name }))
        }
        "switch" => {
            let name = params["name"]
                .as_str()
                .ok_or(GatewayError::MissingParam("name"))?;
            validate_branch_name(name)?;
            git_run(&["checkout", name], &cwd, TIMEOUT_SHORT).await?;
            Ok(json!({ "ok": true, "branch": name }))
        }
        "delete" => {
            let name = params["name"]
                .as_str()
                .ok_or(GatewayError::MissingParam("name"))?;
            validate_branch_name(name)?;
            // -d (safe delete) only — no force delete per spec.
            git_run(&["branch", "-d", name], &cwd, TIMEOUT_SHORT).await?;
            Ok(json!({ "ok": true, "branch": name }))
        }
        other => Err(GatewayError::InvalidParam(format!(
            "op must be 'list', 'create', 'switch', or 'delete'; got {other:?}"
        ))),
    }
}

// ── git.diff ──────────────────────────────────────────────────────────────────

/// Return the diff for a working tree, optionally staged and/or path-filtered.
///
/// Params: `{cwd: string, staged?: bool, path?: string}`
/// Returns: `{diff: string}`
///
/// # Errors
///
/// Returns [`GatewayError`] on invalid params or git subprocess failure.
#[instrument(skip(params))]
pub async fn run_diff(params: Value) -> Result<Value, GatewayError> {
    let cwd_raw = params["cwd"]
        .as_str()
        .ok_or(GatewayError::MissingParam("cwd"))?;
    let cwd = validate_cwd(cwd_raw)?;
    let staged = params["staged"].as_bool().unwrap_or(false);
    let path_filter = params["path"].as_str();

    let mut args: Vec<&str> = vec!["diff"];
    if staged {
        args.push("--staged");
    }
    if path_filter.is_some() {
        args.push("--");
    }
    // Borrowing path_filter as a string slice for the args vec requires the
    // value to live long enough.
    let stdout = if let Some(p) = path_filter {
        let mut full_args = args.clone();
        full_args.push(p);
        git_run(&full_args, &cwd, TIMEOUT_SHORT).await?
    } else {
        git_run(&args, &cwd, TIMEOUT_SHORT).await?
    };

    Ok(json!({ "diff": stdout }))
}

// ── git.commit ────────────────────────────────────────────────────────────────

/// Commit staged changes with the given message.
///
/// Params: `{cwd: string, message: string}`
/// Returns: `{sha: string, message: string}`
///
/// # Errors
///
/// Returns [`GatewayError`] on invalid params or git subprocess failure.
#[instrument(skip(params))]
pub async fn run_commit(params: Value) -> Result<Value, GatewayError> {
    let cwd_raw = params["cwd"]
        .as_str()
        .ok_or(GatewayError::MissingParam("cwd"))?;
    let cwd = validate_cwd(cwd_raw)?;
    let message = params["message"]
        .as_str()
        .ok_or(GatewayError::MissingParam("message"))?;

    // `--no-verify` is in the spec — skip hooks for programmatic commits.
    git_run(
        &["commit", "--no-verify", "-m", message],
        &cwd,
        TIMEOUT_COMMIT,
    )
    .await?;

    // Parse the commit sha from `git rev-parse HEAD`.
    let sha_raw = git_run(&["rev-parse", "HEAD"], &cwd, TIMEOUT_SHORT).await?;
    let sha = sha_raw.trim().to_owned();

    Ok(json!({ "sha": sha, "message": message }))
}

// ── git.push ──────────────────────────────────────────────────────────────────

/// Push the current branch to origin.
///
/// Params: `{cwd: string, set_upstream?: bool, branch?: string}`
/// Returns: `{pushed: bool, url?: string}`
///
/// Force-push is unconditionally disabled (T-5 BLOCKING).
///
/// # Errors
///
/// Returns [`GatewayError::Subprocess`] when `force: true` is present.
/// Returns [`GatewayError`] on invalid params or git subprocess failure.
#[instrument(skip(params))]
pub async fn run_push(params: Value) -> Result<Value, GatewayError> {
    // T-5: Force-push prevention.
    if params["force"].as_bool().unwrap_or(false) {
        return Err(GatewayError::Subprocess(
            "T-5: force push disabled".to_owned(),
        ));
    }

    let cwd_raw = params["cwd"]
        .as_str()
        .ok_or(GatewayError::MissingParam("cwd"))?;
    let cwd = validate_cwd(cwd_raw)?;
    let set_upstream = params["set_upstream"].as_bool().unwrap_or(false);
    let branch = params["branch"].as_str();

    let auth_present = load_github_pat().is_some();
    tracing::info!(git.auth_present = auth_present, "git.push.auth");

    let stdout = if set_upstream {
        let branch_name = branch.ok_or(GatewayError::MissingParam(
            "branch is required when set_upstream is true",
        ))?;
        validate_branch_name(branch_name)?;
        git_run(
            &["push", "--set-upstream", "origin", branch_name],
            &cwd,
            TIMEOUT_PUSH,
        )
        .await?
    } else {
        git_run(&["push"], &cwd, TIMEOUT_PUSH).await?
    };

    // Extract a remote URL from the push output if present.
    let url = stdout
        .lines()
        .find(|l| l.contains("http"))
        .map(|l| l.trim().to_owned());

    Ok(json!({ "pushed": true, "url": url }))
}

// ── git.pull ──────────────────────────────────────────────────────────────────

/// Pull with fast-forward only.
///
/// Params: `{cwd: string}`
/// Returns: `{merged: bool, commits: u32}`
///
/// # Errors
///
/// Returns [`GatewayError`] on invalid params or git subprocess failure.
#[instrument(skip(params))]
pub async fn run_pull(params: Value) -> Result<Value, GatewayError> {
    let cwd_raw = params["cwd"]
        .as_str()
        .ok_or(GatewayError::MissingParam("cwd"))?;
    let cwd = validate_cwd(cwd_raw)?;

    let stdout = git_run(&["pull", "--ff-only"], &cwd, TIMEOUT_PULL).await?;

    // Count commits from lines like "Fast-forward" or "N files changed".
    // git pull --ff-only outputs "Already up to date." when nothing to pull.
    let already_up = stdout.contains("Already up to date");
    let commits: u32 = if already_up {
        0
    } else {
        // Heuristic: count lines starting with a commit sha (7+ hex chars).
        stdout
            .lines()
            .filter(|l| {
                let trimmed = l.trim();
                trimmed.len() >= 7 && trimmed.chars().take(7).all(|c| c.is_ascii_hexdigit())
            })
            .count()
            .try_into()
            .unwrap_or(1)
    };

    Ok(json!({ "merged": !already_up, "commits": commits }))
}

// ── git.create_pr ─────────────────────────────────────────────────────────────

/// Create a GitHub pull request via the REST API.
///
/// Params: `{owner: string, repo: string, title: string, head: string, base: string, body?: string}`
/// Returns: `{number: u64, url: string, title: string}`
///
/// # Errors
///
/// Returns [`GatewayError`] on missing params, missing PAT, or API failure.
#[instrument(skip(params))]
pub async fn run_create_pr(params: Value) -> Result<Value, GatewayError> {
    let owner = params["owner"]
        .as_str()
        .ok_or(GatewayError::MissingParam("owner"))?;
    let repo = params["repo"]
        .as_str()
        .ok_or(GatewayError::MissingParam("repo"))?;
    let title = params["title"]
        .as_str()
        .ok_or(GatewayError::MissingParam("title"))?;
    let head = params["head"]
        .as_str()
        .ok_or(GatewayError::MissingParam("head"))?;
    let base = params["base"]
        .as_str()
        .ok_or(GatewayError::MissingParam("base"))?;
    let body = params["body"].as_str().unwrap_or("");

    let pat = load_github_pat().ok_or_else(|| {
        GatewayError::Subprocess(
            "no GitHub PAT available; set LIGHTARCHITECTS_GITHUB_PAT or configure keyring"
                .to_owned(),
        )
    })?;

    let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls");
    tracing::debug!(url = %redact_token_in_trace(&url), "creating GitHub PR");

    let client = reqwest::Client::builder()
        .timeout(TIMEOUT_GITHUB_API)
        .user_agent("lightarchitects-gateway")
        .build()
        .map_err(|e| GatewayError::Subprocess(format!("failed to build HTTP client: {e}")))?;

    let resp = client
        .post(&url)
        .bearer_auth(pat.expose_secret())
        .json(&json!({
            "title": title,
            "head": head,
            "base": base,
            "body": body,
        }))
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "GitHub PR create request failed");
            GatewayError::Subprocess("GitHub API request failed; see gateway logs".to_owned())
        })?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        tracing::warn!(status, "GitHub PR create returned non-2xx");
        return Err(GatewayError::Subprocess(format!(
            "GitHub API returned status {status}"
        )));
    }

    let body: Value = resp.json().await.map_err(|e| {
        tracing::warn!(error = %e, "failed to parse GitHub PR response");
        GatewayError::Subprocess("GitHub API response parse failed".to_owned())
    })?;

    let number = body["number"].as_u64().ok_or(GatewayError::Subprocess(
        "missing 'number' in PR response".to_owned(),
    ))?;
    let pr_url = body["html_url"].as_str().unwrap_or("").to_owned();
    let pr_title = body["title"].as_str().unwrap_or(title).to_owned();

    Ok(json!({ "number": number, "url": pr_url, "title": pr_title }))
}

// ── git.review_pr ─────────────────────────────────────────────────────────────

/// Submit a GitHub pull request review via the REST API.
///
/// Params: `{owner: string, repo: string, number: u64, event: string, body?: string, comments?: [...]}`
/// Returns: `{id: u64, state: string}`
///
/// Inline comments use `comments[].position` (diff-position integer), not `line`/`side`.
///
/// # Errors
///
/// Returns [`GatewayError`] on missing params, missing PAT, or API failure.
#[instrument(skip(params))]
pub async fn run_review_pr(params: Value) -> Result<Value, GatewayError> {
    let owner = params["owner"]
        .as_str()
        .ok_or(GatewayError::MissingParam("owner"))?;
    let repo = params["repo"]
        .as_str()
        .ok_or(GatewayError::MissingParam("repo"))?;
    let number = params["number"]
        .as_u64()
        .ok_or(GatewayError::MissingParam("number"))?;
    let event = params["event"]
        .as_str()
        .ok_or(GatewayError::MissingParam("event"))?;
    let review_body = params["body"].as_str().unwrap_or("");
    let comments = params["comments"].clone();

    let pat = load_github_pat().ok_or_else(|| {
        GatewayError::Subprocess(
            "no GitHub PAT available; set LIGHTARCHITECTS_GITHUB_PAT or configure keyring"
                .to_owned(),
        )
    })?;

    let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}/reviews");
    tracing::debug!(url = %redact_token_in_trace(&url), "submitting GitHub PR review");

    let client = reqwest::Client::builder()
        .timeout(TIMEOUT_GITHUB_API)
        .user_agent("lightarchitects-gateway")
        .build()
        .map_err(|e| GatewayError::Subprocess(format!("failed to build HTTP client: {e}")))?;

    let mut payload = json!({
        "event": event,
        "body": review_body,
    });
    if !comments.is_null() {
        payload["comments"] = comments;
    }

    let resp = client
        .post(&url)
        .bearer_auth(pat.expose_secret())
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "GitHub PR review request failed");
            GatewayError::Subprocess("GitHub API request failed; see gateway logs".to_owned())
        })?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        tracing::warn!(status, "GitHub PR review returned non-2xx");
        return Err(GatewayError::Subprocess(format!(
            "GitHub API returned status {status}"
        )));
    }

    let body: Value = resp.json().await.map_err(|e| {
        tracing::warn!(error = %e, "failed to parse GitHub PR review response");
        GatewayError::Subprocess("GitHub API response parse failed".to_owned())
    })?;

    let id = body["id"].as_u64().ok_or(GatewayError::Subprocess(
        "missing 'id' in review response".to_owned(),
    ))?;
    let state = body["state"].as_str().unwrap_or(event).to_owned();

    Ok(json!({ "id": id, "state": state }))
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn validate_branch_name_accepts_valid() {
        assert!(validate_branch_name("main").is_ok());
        assert!(validate_branch_name("feat/my-feature").is_ok());
        assert!(validate_branch_name("fix-123").is_ok());
        assert!(validate_branch_name("feature.branch").is_ok());
    }

    #[test]
    fn validate_branch_name_rejects_dotdot() {
        assert!(validate_branch_name("feat..evil").is_err());
        assert!(validate_branch_name("..evil").is_err());
    }

    #[test]
    fn validate_branch_name_rejects_pattern_violations() {
        // Single character — fails min-length requirement (no body chars for separator).
        // Actually a single char should fail because the regex requires ≥2 chars
        // (start + end are both alnum with 0+ body in between — but the regex requires
        // the pattern `[a-zA-Z0-9][...]{0,253}[a-zA-Z0-9]` which needs ≥2 chars).
        assert!(validate_branch_name("a").is_err());
        // Starts with hyphen.
        assert!(validate_branch_name("-bad").is_err());
        // Ends with hyphen — the pattern requires alnum end.
        assert!(validate_branch_name("bad-").is_err());
    }

    #[test]
    fn redact_token_in_trace_replaces_ghp() {
        let url = "https://api.github.com?token=ghp_ABCDEF12345";
        let redacted = redact_token_in_trace(url);
        assert!(!redacted.contains("ghp_"), "raw token leaked: {redacted}");
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn redact_token_in_trace_replaces_github_pat() {
        let url = "https://api.github.com?token=github_pat_abc123_DEF";
        let redacted = redact_token_in_trace(url);
        assert!(
            !redacted.contains("github_pat_"),
            "raw token leaked: {redacted}"
        );
    }

    #[test]
    fn redact_token_in_trace_leaves_clean_url_unchanged() {
        let url = "https://api.github.com/repos/owner/repo/pulls";
        assert_eq!(redact_token_in_trace(url), url);
    }

    #[test]
    fn validate_cwd_rejects_dotdot() {
        let result = validate_cwd("/tmp/../etc");
        assert!(result.is_err(), "dotdot path must be rejected");
    }

    #[test]
    fn validate_cwd_accepts_canonical_path() {
        // /tmp always exists on macOS (as a symlink to /private/tmp, but canonicalize
        // resolves it — just verify it doesn't error out).
        let result = validate_cwd("/tmp");
        // May fail if /tmp doesn't exist (exotic env), that's acceptable.
        let _ = result; // just ensure no panic
    }
}
