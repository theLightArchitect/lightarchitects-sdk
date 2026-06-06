//! Build progress HTTP endpoints — `/v1/platform/builds/*`.
//!
//! Implements the keystone contract at
//! `standards/canon/contracts/wire.http/gateway.get.v1-platform-builds-codename-progress.yaml`.
//!
//! Response shape is a hierarchical snapshot: `phases × waves × tasks`, joined
//! with live local git state. Two endpoints:
//!
//! - `GET /v1/platform/builds` — list all builds under `$HELIX/corso/builds/`
//! - `GET /v1/platform/builds/{codename}/progress` — full progress snapshot
//!
//! Design notes:
//! - Manifest parsing uses `serde_yaml → serde_json::Value` for forward
//!   compatibility (matches `lightarchitects-webshell::events::builds_handler`).
//! - Git operations shell out via `std::process::Command` (no `git2` in workspace).
//! - AYIN fleet join + GitHub PR fetch are stubbed for v0.1.0 (return `null`
//!   agents + `null` PR). v0.2.0 wires them per the contract's `fleet_required`
//!   and `include_pr_state` query params.
//! - Degraded-mode (M-5): manifests missing `phases[]` get synthesized into a
//!   single "Unstructured" phase so legacy builds still render in the cockpit.

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

use crate::http::routes::platform::respond_with_body_etag;
use crate::http::state::PlatformState;

/// Wire the build progress routes onto the platform router.
pub fn builds_routes() -> Router<Arc<PlatformState>> {
    Router::new()
        .route("/v1/platform/builds", get(builds_list))
        .route(
            "/v1/platform/builds/{codename}/progress",
            get(builds_progress),
        )
}

// ─────────────────────────────────────────────────────────────────────────────
// Query parameter shapes
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct BuildsProgressQuery {
    /// If true, return 503 when AYIN fleet is unreachable.
    /// If false (default), `agents[].state = "unknown"` on fleet miss.
    #[serde(default)]
    fleet_required: bool,
    /// If true (default), include PR state from `gh` CLI.
    #[serde(default = "default_true")]
    include_pr_state: bool,
}

const fn default_true() -> bool {
    true
}

// ─────────────────────────────────────────────────────────────────────────────
// Route handlers — kept <60 lines each per Cookbook §7.11.4
// ─────────────────────────────────────────────────────────────────────────────

/// `GET /v1/platform/builds` — list all builds under `$HELIX/corso/builds/`.
///
/// Returns each build's codename + minimal metadata from `manifest.yaml`
/// (tier, plan_status, build_status). Use the `/progress` endpoint for the
/// full snapshot of a specific build.
async fn builds_list(
    State(_s): State<Arc<PlatformState>>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    let builds_root = corso_builds_root();
    if !builds_root.exists() {
        let body = json!({ "builds": [], "count": 0 });
        return Ok(respond_with_body_etag(body, &headers));
    }
    let entries = list_build_codenames(&builds_root);
    let summaries: Vec<Value> = entries
        .iter()
        .filter_map(|codename| build_summary(&builds_root, codename))
        .collect();
    let body = json!({
        "builds": summaries,
        "count": summaries.len(),
    });
    Ok(respond_with_body_etag(body, &headers))
}

/// `GET /v1/platform/builds/{codename}/progress` — full snapshot per the
/// keystone contract.
async fn builds_progress(
    State(_s): State<Arc<PlatformState>>,
    Path(codename): Path<String>,
    Query(q): Query<BuildsProgressQuery>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    if let Some(e) = validate_codename(&codename) {
        return Err(e);
    }
    let dir = corso_builds_root().join(&codename);
    let manifest_path = dir.join("manifest.yaml");
    if !manifest_path.exists() {
        return Err(build_not_found(&codename));
    }
    let manifest = load_manifest(&manifest_path).ok_or_else(|| manifest_invalid(&codename))?;
    let worktree = worktree_from_manifest(&manifest);
    let git = worktree.as_deref().map(walk_git).unwrap_or_default();
    let body = assemble_progress(
        &codename,
        &manifest,
        &git,
        q.include_pr_state,
        q.fleet_required,
    );
    Ok(respond_with_body_etag(body, &headers))
}

// ─────────────────────────────────────────────────────────────────────────────
// Path + identifier resolution
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve `$HELIX/corso/builds/` — the directory containing all build manifests.
///
/// Honours the `HELIX_ROOT` env var override; otherwise falls back to
/// `~/lightarchitects/soul/helix`.
fn corso_builds_root() -> PathBuf {
    corso_builds_root_with(std::env::var("HELIX_ROOT").ok())
}

/// Pure helper for [`corso_builds_root`] — env var injected for testability.
fn corso_builds_root_with(helix_root: Option<String>) -> PathBuf {
    if let Some(env_root) = helix_root {
        return PathBuf::from(env_root).join("corso").join("builds");
    }
    let home = dirs_next::home_dir().unwrap_or_default();
    home.join("lightarchitects/soul/helix/corso/builds")
}

/// Validate codename per contract: `^[a-z0-9][a-z0-9-]{1,63}$`.
fn validate_codename(codename: &str) -> Option<Response> {
    let bytes = codename.as_bytes();
    let valid = (2..=64).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-');
    if valid {
        return None;
    }
    Some(
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {
                    "code": "invalid_codename",
                    "message": "Codename must match ^[a-z0-9][a-z0-9-]{1,63}$",
                    "status": 400,
                }
            })),
        )
            .into_response(),
    )
}

/// List directory names directly under `corso/builds/`, skipping hidden / hidden-arch entries.
fn list_build_codenames(root: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut codenames: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| !name.starts_with('_') && !name.starts_with('.'))
        .collect();
    codenames.sort();
    codenames
}

// ─────────────────────────────────────────────────────────────────────────────
// Manifest parsing — forward-compatible via serde_yaml → serde_json::Value
// ─────────────────────────────────────────────────────────────────────────────

/// Read + parse `manifest.yaml`. Returns `None` on read or parse failure.
fn load_manifest(path: &std::path::Path) -> Option<Value> {
    let content = std::fs::read_to_string(path).ok()?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(&content).ok()?;
    serde_json::to_value(yaml).ok()
}

/// Extract a brief build summary for the list endpoint.
fn build_summary(root: &std::path::Path, codename: &str) -> Option<Value> {
    let manifest = load_manifest(&root.join(codename).join("manifest.yaml"))?;
    Some(json!({
        "codename": codename,
        "title": manifest.get("title").and_then(Value::as_str),
        "tier": manifest.get("tier").and_then(Value::as_str),
        "plan_status": manifest.get("plan_status").and_then(Value::as_str),
        "build_status": manifest.get("build_status").and_then(Value::as_str),
        "started_at": manifest.get("started_at").and_then(Value::as_str),
    }))
}

/// Resolve the worktree path from `build_topology.worktree_target` (existing
/// schema) or `git.worktree` (new schema). Returns `None` if neither is present.
fn worktree_from_manifest(manifest: &Value) -> Option<String> {
    let candidate = manifest
        .get("git")
        .and_then(|g| g.get("worktree"))
        .and_then(Value::as_str)
        .or_else(|| {
            manifest
                .get("build_topology")
                .and_then(|t| t.get("worktree_target"))
                .and_then(Value::as_str)
        })?;
    Some(expand_tilde(candidate))
}

/// Expand a leading `~/` to the user's home directory.
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs_next::home_dir() {
            return home.join(rest).to_string_lossy().into_owned();
        }
    }
    path.to_owned()
}

// ─────────────────────────────────────────────────────────────────────────────
// Git state — shell-out to `git` CLI (no git2 in workspace)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
struct GitState {
    head: Option<String>,
    feature_branch: Option<String>,
    dirty: bool,
    ahead: u32,
    behind: u32,
}

/// Walk local git state for a worktree path. Returns defaults on any error
/// (degraded mode — UI surfaces "git unavailable" without breaking the response).
fn walk_git(worktree: &str) -> GitState {
    let head = git_cmd(worktree, &["rev-parse", "HEAD"]).map(|s| s.trim().to_owned());
    let feature_branch =
        git_cmd(worktree, &["rev-parse", "--abbrev-ref", "HEAD"]).map(|s| s.trim().to_owned());
    let dirty = git_cmd(worktree, &["status", "--porcelain"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let (ahead, behind) = git_ahead_behind(worktree).unwrap_or((0, 0));
    GitState {
        head,
        feature_branch,
        dirty,
        ahead,
        behind,
    }
}

/// Run `git <args>` in `worktree` and return stdout on success.
fn git_cmd(worktree: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

/// `git rev-list --left-right --count main...HEAD` — returns `(ahead, behind)`.
fn git_ahead_behind(worktree: &str) -> Option<(u32, u32)> {
    let raw = git_cmd(
        worktree,
        &["rev-list", "--left-right", "--count", "main...HEAD"],
    )?;
    let mut parts = raw.split_whitespace();
    let behind: u32 = parts.next()?.parse().ok()?;
    let ahead: u32 = parts.next()?.parse().ok()?;
    Some((ahead, behind))
}

// ─────────────────────────────────────────────────────────────────────────────
// Assembly — manifest + git state → BuildProgressSnapshot Value
// ─────────────────────────────────────────────────────────────────────────────

/// Assemble the full progress snapshot. Phase/wave/task structure passes
/// through verbatim when present in manifest; falls back to degraded-mode
/// synthesis (M-5) when only legacy fields exist.
fn assemble_progress(
    codename: &str,
    manifest: &Value,
    git: &GitState,
    include_pr_state: bool,
    _fleet_required: bool,
) -> Value {
    let now = chrono::Utc::now().to_rfc3339();
    let worktree = worktree_from_manifest(manifest);
    let pr = resolve_pr_state(
        include_pr_state,
        worktree.as_deref(),
        git.feature_branch.as_deref(),
    );
    json!({
        "codename": codename,
        "title": manifest.get("title").and_then(Value::as_str),
        "tier": manifest.get("tier").and_then(Value::as_str),
        "template_version": manifest.get("lasdlc_template_version").and_then(Value::as_str),
        "started_at": manifest.get("started_at").and_then(Value::as_str),
        "elapsed_ms": elapsed_from_started(manifest),
        "status": resolve_build_status(manifest),
        "git": git_to_value(manifest, git),
        "pr": pr,
        "phases": resolve_phases(manifest),
        "captured_at": now,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// PR state (B2) — shell out to `gh pr view <branch> --json …`
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve PR state for the build's feature branch via the `gh` CLI.
///
/// Returns `Value::Null` when any of the following hold:
///   - `include_pr_state == false` (operator opted out)
///   - `worktree` or `feature_branch` unknown (degraded-mode manifest)
///   - `gh` CLI not installed / not authenticated / no PR for branch
///   - response JSON shape doesn't match expected fields
///
/// The contract treats `pr: null` as "no PR or unknown" — the cockpit shows
/// nothing in the PR slot rather than surfacing the failure mode to the operator.
fn resolve_pr_state(include: bool, worktree: Option<&str>, feature_branch: Option<&str>) -> Value {
    if !include {
        return Value::Null;
    }
    let Some(wt) = worktree else {
        return Value::Null;
    };
    let Some(branch) = feature_branch else {
        return Value::Null;
    };
    fetch_pr_state(wt, branch).unwrap_or(Value::Null)
}

/// Invoke `gh pr view <branch> --json …` from `worktree` and return the
/// parsed response. Any failure (gh missing, no PR, malformed JSON) → `None`.
fn fetch_pr_state(worktree: &str, branch: &str) -> Option<Value> {
    let out = std::process::Command::new("gh")
        .current_dir(worktree)
        .args([
            "pr",
            "view",
            branch,
            "--json",
            "state,number,url,isDraft,baseRepository",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = std::str::from_utf8(&out.stdout).ok()?;
    parse_pr_response(raw)
}

/// Pure JSON parser for the gh CLI response — unit-testable.
/// Maps gh's `{state, isDraft, …}` to the contract's `state` enum:
///   `OPEN` + isDraft → `"draft"`, `OPEN` → `"open"`, `MERGED` → `"merged"`,
///   `CLOSED` → `"closed"`. Any unrecognized state → `None`.
fn parse_pr_response(raw: &str) -> Option<Value> {
    let parsed: Value = serde_json::from_str(raw).ok()?;
    let number = parsed.get("number")?.as_i64()?;
    let state_raw = parsed.get("state")?.as_str()?;
    let is_draft = parsed
        .get("isDraft")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let url = parsed.get("url").and_then(Value::as_str)?.to_owned();
    let repo = parsed
        .get("baseRepository")
        .and_then(|r| r.get("nameWithOwner"))
        .and_then(Value::as_str)?
        .to_owned();
    let state = match (state_raw.to_ascii_uppercase().as_str(), is_draft) {
        ("OPEN", true) => "draft",
        ("OPEN", false) => "open",
        ("MERGED", _) => "merged",
        ("CLOSED", _) => "closed",
        _ => return None,
    };
    Some(json!({
        "number": number,
        "state": state,
        "url": url,
        "repo": repo,
        // v0.1.0 polling model — always "fresh" since the gateway fetches synchronously per request.
        "last_sync_ms_ago": 0,
    }))
}

/// Resolve `build_status` from the new structured field, falling back to the
/// legacy pipe-separated `status` field (M-1 / M-5).
fn resolve_build_status(manifest: &Value) -> &'static str {
    if let Some(s) = manifest.get("build_status").and_then(Value::as_str) {
        return canonical_status(s);
    }
    // Legacy: pipe-separated `status: PROMOTED | MERGED`, or a `merged_sha`
    // field on older manifests pre-dating that form.
    let raw = manifest.get("status").and_then(Value::as_str).unwrap_or("");
    let merged_in_status = raw.to_ascii_uppercase().contains("MERGED");
    let has_merge_sha = manifest.get("merged_sha").and_then(Value::as_str).is_some();
    if merged_in_status || has_merge_sha {
        "complete"
    } else {
        "active"
    }
}

const fn canonical_status(s: &str) -> &'static str {
    match s.as_bytes() {
        b"pending" => "pending",
        b"active" => "active",
        b"complete" => "complete",
        b"failed" => "failed",
        _ => "active",
    }
}

/// Assemble the `git` object combining manifest declarations + live git state.
fn git_to_value(manifest: &Value, live: &GitState) -> Value {
    let worktree = worktree_from_manifest(manifest);
    let manifest_branch = manifest
        .get("build_topology")
        .and_then(|t| t.get("branch_target"))
        .and_then(Value::as_str);
    let feature_branch = live
        .feature_branch
        .as_deref()
        .or(manifest_branch)
        .unwrap_or("unknown");
    json!({
        "worktree": worktree,
        "feature_branch": feature_branch,
        "base_branch": "main",
        "ahead": live.ahead,
        "behind": live.behind,
        "dirty": live.dirty,
        "head": live.head.as_deref().unwrap_or("unknown"),
    })
}

/// Compute elapsed ms from `started_at` (RFC 3339) to now. Returns `0` if absent.
fn elapsed_from_started(manifest: &Value) -> u64 {
    let started = manifest.get("started_at").and_then(Value::as_str);
    let Some(started) = started else { return 0 };
    let Ok(t) = chrono::DateTime::parse_from_rfc3339(started) else {
        return 0;
    };
    let delta = chrono::Utc::now() - t.with_timezone(&chrono::Utc);
    delta.num_milliseconds().max(0) as u64
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase resolution — pass-through if structured, degraded-mode otherwise (M-5)
// ─────────────────────────────────────────────────────────────────────────────

/// Pass through `manifest.phases[]` when it's an array; otherwise synthesize
/// a single "Unstructured" phase from legacy fields (per M-5).
fn resolve_phases(manifest: &Value) -> Value {
    match manifest.get("phases") {
        Some(Value::Array(arr)) => Value::Array(arr.clone()),
        // `phases: 7` (integer count) is legacy schema — synthesize.
        Some(Value::Number(_)) | None => Value::Array(vec![degraded_phase(manifest)]),
        // Defensive: any other shape → degraded.
        Some(_) => Value::Array(vec![degraded_phase(manifest)]),
    }
}

/// Synthesize a single "Unstructured" phase for legacy manifests (M-5).
fn degraded_phase(manifest: &Value) -> Value {
    let merged = manifest.get("merged_sha").and_then(Value::as_str);
    let merge_msg = manifest.get("merge_commit").and_then(Value::as_str);
    let waves = if let Some(sha) = merged {
        vec![json!({
            "id": "w0",
            "name": "wave-0 · legacy merge",
            "status": "complete",
            "commits": [{
                "sha": sha,
                "short": &sha[..sha.len().min(7)],
                "msg": merge_msg.unwrap_or("legacy merge commit"),
                "ago": null,
                "files_count": 0,
            }],
            "agents": [],
            "tasks": [],
        })]
    } else {
        Vec::new()
    };
    json!({
        "id": "p0",
        "name": "Unstructured (legacy manifest)",
        "status": if merged.is_some() { "complete" } else { "pending" },
        "gates": {},
        "amendments": [],
        "waves": waves,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Error responses — match platform.rs convention `{error: {code, message, status}}`
// ─────────────────────────────────────────────────────────────────────────────

fn build_not_found(codename: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": {
                "code": "build_not_found",
                "message": format!("No build named \"{codename}\" — check /v1/platform/builds"),
                "status": 404,
            }
        })),
    )
        .into_response()
}

fn manifest_invalid(codename: &str) -> Response {
    (
        StatusCode::CONFLICT,
        Json(json!({
            "error": {
                "code": "manifest_invalid",
                "message": format!("Build \"{codename}\" manifest.yaml failed to parse"),
                "status": 409,
            }
        })),
    )
        .into_response()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_codename_accepts_canonical() {
        assert!(validate_codename("webshell-copilot-providers").is_none());
        assert!(validate_codename("a1").is_none());
        assert!(validate_codename("agent-runner-policy-store-wiring").is_none());
    }

    #[test]
    fn validate_codename_rejects_too_short() {
        assert!(validate_codename("a").is_some());
        assert!(validate_codename("").is_some());
    }

    #[test]
    fn validate_codename_rejects_uppercase() {
        assert!(validate_codename("WebShell").is_some());
    }

    #[test]
    fn validate_codename_rejects_path_traversal() {
        assert!(validate_codename("../etc/passwd").is_some());
        assert!(validate_codename("a/b").is_some());
        assert!(validate_codename(".hidden").is_some());
    }

    #[test]
    fn expand_tilde_works() {
        let home = dirs_next::home_dir().unwrap_or_default();
        let expanded = expand_tilde("~/foo/bar");
        assert!(expanded.starts_with(&home.to_string_lossy().as_ref()));
        assert!(expanded.ends_with("foo/bar"));
    }

    #[test]
    fn expand_tilde_passes_through_absolute() {
        assert_eq!(expand_tilde("/etc/foo"), "/etc/foo");
        assert_eq!(expand_tilde("relative/path"), "relative/path");
    }

    #[test]
    fn resolve_build_status_prefers_structured() {
        let m = json!({ "build_status": "active" });
        assert_eq!(resolve_build_status(&m), "active");
    }

    #[test]
    fn resolve_build_status_falls_back_to_merged_sha() {
        // Legacy manifest — no structured field, just merged_sha.
        let m = json!({ "merged_sha": "abc123" });
        assert_eq!(resolve_build_status(&m), "complete");
    }

    #[test]
    fn resolve_build_status_parses_pipe_form() {
        let m = json!({ "status": "PROMOTED | MERGED" });
        assert_eq!(resolve_build_status(&m), "complete");
    }

    #[test]
    fn resolve_build_status_active_when_unknown() {
        let m = json!({});
        assert_eq!(resolve_build_status(&m), "active");
    }

    #[test]
    fn resolve_phases_passes_through_structured() {
        let m = json!({ "phases": [{"id": "p1", "name": "X", "status": "complete"}] });
        let p = resolve_phases(&m);
        assert_eq!(p.as_array().unwrap().len(), 1);
        assert_eq!(p[0]["id"], "p1");
    }

    #[test]
    fn resolve_phases_synthesizes_for_legacy_integer_count() {
        let m = json!({ "phases": 7, "merged_sha": "deadbee" });
        let p = resolve_phases(&m);
        assert_eq!(p.as_array().unwrap().len(), 1);
        assert_eq!(p[0]["id"], "p0");
        assert_eq!(p[0]["status"], "complete");
    }

    #[test]
    fn resolve_phases_synthesizes_pending_when_no_merge() {
        let m = json!({});
        let p = resolve_phases(&m);
        assert_eq!(p[0]["status"], "pending");
        assert_eq!(p[0]["waves"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn worktree_from_manifest_prefers_new_schema() {
        let m = json!({
            "git": { "worktree": "~/new-path" },
            "build_topology": { "worktree_target": "~/legacy-path" }
        });
        let wt = worktree_from_manifest(&m).unwrap();
        assert!(wt.ends_with("new-path"));
    }

    #[test]
    fn worktree_from_manifest_falls_back_to_build_topology() {
        let m = json!({
            "build_topology": { "worktree_target": "~/legacy-path" }
        });
        let wt = worktree_from_manifest(&m).unwrap();
        assert!(wt.ends_with("legacy-path"));
    }

    #[test]
    fn worktree_from_manifest_none_when_absent() {
        let m = json!({});
        assert!(worktree_from_manifest(&m).is_none());
    }

    #[test]
    fn corso_builds_root_honours_env_override() {
        let root = corso_builds_root_with(Some("/tmp/test-helix".to_owned()));
        assert_eq!(root, PathBuf::from("/tmp/test-helix/corso/builds"));
    }

    #[test]
    fn corso_builds_root_falls_back_to_home() {
        let root = corso_builds_root_with(None);
        assert!(root.ends_with("lightarchitects/soul/helix/corso/builds"));
    }

    // ── B2 — PR state parsing (pure, no gh CLI required) ──────────────────

    #[test]
    fn parse_pr_response_maps_open_to_open() {
        let raw = r#"{
            "number": 247,
            "state": "OPEN",
            "url": "https://github.com/owner/repo/pull/247",
            "isDraft": false,
            "baseRepository": {"nameWithOwner": "owner/repo"}
        }"#;
        let pr = parse_pr_response(raw).expect("parse");
        assert_eq!(pr["state"], "open");
        assert_eq!(pr["number"], 247);
        assert_eq!(pr["repo"], "owner/repo");
        assert_eq!(pr["url"], "https://github.com/owner/repo/pull/247");
        assert_eq!(pr["last_sync_ms_ago"], 0);
    }

    #[test]
    fn parse_pr_response_maps_draft_when_is_draft() {
        let raw = r#"{
            "number": 99,
            "state": "OPEN",
            "url": "https://github.com/owner/repo/pull/99",
            "isDraft": true,
            "baseRepository": {"nameWithOwner": "owner/repo"}
        }"#;
        let pr = parse_pr_response(raw).expect("parse");
        assert_eq!(pr["state"], "draft");
    }

    #[test]
    fn parse_pr_response_maps_merged() {
        let raw = r#"{
            "number": 42,
            "state": "MERGED",
            "url": "https://github.com/o/r/pull/42",
            "isDraft": false,
            "baseRepository": {"nameWithOwner": "o/r"}
        }"#;
        let pr = parse_pr_response(raw).expect("parse");
        assert_eq!(pr["state"], "merged");
    }

    #[test]
    fn parse_pr_response_maps_closed() {
        let raw = r#"{
            "number": 7,
            "state": "CLOSED",
            "url": "https://github.com/o/r/pull/7",
            "isDraft": false,
            "baseRepository": {"nameWithOwner": "o/r"}
        }"#;
        let pr = parse_pr_response(raw).expect("parse");
        assert_eq!(pr["state"], "closed");
    }

    #[test]
    fn parse_pr_response_rejects_unknown_state() {
        let raw = r#"{
            "number": 1,
            "state": "MYSTERY",
            "url": "https://github.com/o/r/pull/1",
            "isDraft": false,
            "baseRepository": {"nameWithOwner": "o/r"}
        }"#;
        assert!(parse_pr_response(raw).is_none());
    }

    #[test]
    fn parse_pr_response_rejects_missing_number() {
        let raw = r#"{
            "state": "OPEN",
            "url": "https://github.com/o/r/pull/1",
            "isDraft": false,
            "baseRepository": {"nameWithOwner": "o/r"}
        }"#;
        assert!(parse_pr_response(raw).is_none());
    }

    #[test]
    fn parse_pr_response_rejects_missing_repo() {
        let raw = r#"{
            "number": 1,
            "state": "OPEN",
            "url": "https://github.com/o/r/pull/1",
            "isDraft": false
        }"#;
        assert!(parse_pr_response(raw).is_none());
    }

    #[test]
    fn parse_pr_response_rejects_malformed_json() {
        assert!(parse_pr_response("not json at all").is_none());
        assert!(parse_pr_response("").is_none());
    }

    #[test]
    fn resolve_pr_state_returns_null_when_opt_out() {
        let v = resolve_pr_state(false, Some("/tmp"), Some("feat-x"));
        assert!(v.is_null());
    }

    #[test]
    fn resolve_pr_state_returns_null_without_worktree() {
        let v = resolve_pr_state(true, None, Some("feat-x"));
        assert!(v.is_null());
    }

    #[test]
    fn resolve_pr_state_returns_null_without_branch() {
        let v = resolve_pr_state(true, Some("/tmp"), None);
        assert!(v.is_null());
    }
}
