//! REST handlers for the code-editor API surface (`/api/code/*`).
//!
//! All six endpoints are bearer-authenticated. Paths are validated against
//! `config.cwd`; any path that escapes that root is rejected with 403.
//!
//! Routes (registered in [`super::build_app`]):
//! - `GET  /api/code/read`         — read file content (optional line range)
//! - `GET  /api/code/list`         — list directory entries
//! - `POST /api/code/write`        — atomic file write (temp → rename)
//! - `POST /api/code/search`       — text search via ripgrep / grep
//! - `POST /api/code/preview-diff` — compute unified diff without applying
//! - `POST /api/code/apply-diff`   — apply a unified diff via `patch`

use std::{
    io::{BufRead as _, BufReader, Read as _},
    path::{Path, PathBuf},
};

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use similar::{ChangeTag, TextDiff};

use crate::{auth, server::AppState};

// ── Type aliases ─────────────────────────────────────────────────────────────

type Resp = (StatusCode, Json<Value>);

// ── Security helpers ─────────────────────────────────────────────────────────

/// Maximum bytes returned by `read` in a single response.
const MAX_READ_BYTES: usize = 1_000_000; // 1 MiB

/// Maximum bytes accepted for `write` body content.
const MAX_WRITE_BYTES: usize = 10_000_000; // 10 MiB

fn ok(body: Value) -> Resp {
    (StatusCode::OK, Json(body))
}

fn err(status: StatusCode, msg: &str) -> Resp {
    (status, Json(json!({ "error": msg })))
}

fn unauthorized() -> Resp {
    err(StatusCode::UNAUTHORIZED, "unauthorized")
}

fn forbidden() -> Resp {
    err(StatusCode::FORBIDDEN, "path outside allowed directory")
}

/// Validate bearer token from headers, returning `Err(unauthorized())` on failure.
fn check_auth(headers: &HeaderMap, token: &str) -> Result<(), Resp> {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(unauthorized)?;
    if auth::validate_bearer(authz, token) {
        Ok(())
    } else {
        Err(unauthorized())
    }
}

/// Resolve a caller-supplied path against `cwd` and verify it stays inside.
///
/// For existing paths, canonicalization prevents symlink escapes. For
/// non-existing paths (e.g. write target), we do a component-level check
/// against `..` and then join against the canonical `cwd`.
fn resolve_path(cwd: &Path, user_path: &str) -> Result<PathBuf, Resp> {
    // Reject explicit `..` traversal components early.
    let p = std::path::Path::new(user_path);
    if p.components().any(|c| c == std::path::Component::ParentDir) {
        return Err(forbidden());
    }

    let canonical_cwd = cwd
        .canonicalize()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "cwd resolution failed"))?;

    let joined = canonical_cwd.join(user_path);

    // For existing paths, canonicalize to resolve any remaining indirection.
    let resolved = if joined.exists() {
        joined
            .canonicalize()
            .map_err(|_| err(StatusCode::NOT_FOUND, "path not found"))?
    } else {
        // Non-existing path (write target): trust the joined path; the parent
        // must exist and be under cwd.
        let parent = joined
            .parent()
            .ok_or_else(|| err(StatusCode::BAD_REQUEST, "invalid path"))?;
        if parent.exists() {
            let canonical_parent = parent
                .canonicalize()
                .map_err(|_| err(StatusCode::BAD_REQUEST, "parent directory not found"))?;
            canonical_parent.join(
                joined
                    .file_name()
                    .ok_or_else(|| err(StatusCode::BAD_REQUEST, "path has no filename"))?,
            )
        } else {
            // Parent doesn't exist — still validate by component check only.
            joined
        }
    };

    if !resolved.starts_with(&canonical_cwd) {
        return Err(forbidden());
    }
    Ok(resolved)
}

// ── GET /api/code/read ────────────────────────────────────────────────────────

/// Query params for `GET /api/code/read`.
#[derive(Deserialize)]
pub struct ReadParams {
    /// Path relative to `cwd` (or absolute within `cwd`).
    pub path: String,
    /// 1-based line offset (inclusive). `None` = start from line 1.
    pub offset: Option<usize>,
    /// Maximum number of lines to return. `None` = all lines.
    pub limit: Option<usize>,
}

/// `GET /api/code/read` — return file content (optionally sliced by line range).
///
/// Response: `{ "content": "...", "total_lines": N, "path": "..." }`.
/// Capped at 1 MiB; returns 413 when the resolved slice would exceed that.
pub async fn read_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(params): Query<ReadParams>,
) -> impl IntoResponse {
    if let Err(e) = check_auth(&headers, &state.config.token) {
        return e;
    }
    let abs_path = match resolve_path(&state.config.cwd, &params.path) {
        Ok(p) => p,
        Err(e) => return e,
    };
    if abs_path.is_dir() {
        return err(StatusCode::BAD_REQUEST, "path is a directory");
    }

    // Stream the file with an early byte cap to avoid reading large files into memory.
    let file = match std::fs::File::open(&abs_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(path = %abs_path.display(), error = %e, "code/read: open failed");
            return err(StatusCode::NOT_FOUND, "file not found");
        }
    };

    let reader = BufReader::new(file.take((MAX_READ_BYTES as u64) + 1));
    let all_lines: Vec<String> = match reader.lines().collect::<std::io::Result<_>>() {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(path = %abs_path.display(), error = %e, "code/read: read failed");
            return err(StatusCode::NOT_FOUND, "file not found or not UTF-8");
        }
    };

    let total_lines = all_lines.len();

    let offset = params.offset.unwrap_or(1).max(1);
    let start = offset.saturating_sub(1);
    let slice: Vec<&str> = if let Some(lim) = params.limit {
        all_lines[start.min(total_lines)..((start + lim).min(total_lines))]
            .iter()
            .map(String::as_str)
            .collect()
    } else {
        all_lines[start.min(total_lines)..]
            .iter()
            .map(String::as_str)
            .collect()
    };

    let content = slice.join("\n");
    if content.len() > MAX_READ_BYTES {
        return err(StatusCode::PAYLOAD_TOO_LARGE, "slice exceeds 1 MiB limit");
    }

    ok(json!({
        "content": content,
        "total_lines": total_lines,
        "path": abs_path.display().to_string(),
    }))
}

// ── GET /api/code/list ────────────────────────────────────────────────────────

/// Query params for `GET /api/code/list`.
#[derive(Deserialize)]
pub struct ListParams {
    /// Directory path relative to `cwd`. Defaults to `cwd` root.
    pub path: Option<String>,
}

/// One entry in a directory listing.
#[derive(Serialize)]
pub struct DirEntry {
    /// Filename (not full path).
    pub name: String,
    /// True when the entry is a directory.
    pub is_dir: bool,
    /// File size in bytes (`0` for directories).
    pub size: u64,
}

/// `GET /api/code/list` — list direct children of a directory.
///
/// Response: `{ "entries": [...], "path": "..." }`.
pub async fn list_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> impl IntoResponse {
    if let Err(e) = check_auth(&headers, &state.config.token) {
        return e;
    }
    let dir_path = params.path.as_deref().unwrap_or(".");
    let abs_dir = match resolve_path(&state.config.cwd, dir_path) {
        Ok(p) => p,
        Err(e) => return e,
    };
    if !abs_dir.is_dir() {
        return err(StatusCode::BAD_REQUEST, "path is not a directory");
    }

    let read_dir = match std::fs::read_dir(&abs_dir) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(path = %abs_dir.display(), error = %e, "code/list: read_dir failed");
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to list directory",
            );
        }
    };

    let mut entries: Vec<DirEntry> = read_dir
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some(DirEntry {
                name: e.file_name().to_string_lossy().into_owned(),
                is_dir: meta.is_dir(),
                size: if meta.is_file() { meta.len() } else { 0 },
            })
        })
        .collect();

    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));

    let entries_val: Vec<Value> = entries
        .iter()
        .map(|e| json!({ "name": e.name, "is_dir": e.is_dir, "size": e.size }))
        .collect();

    ok(json!({
        "entries": entries_val,
        "path": abs_dir.display().to_string(),
    }))
}

// ── POST /api/code/write ──────────────────────────────────────────────────────

/// Request body for `POST /api/code/write`.
#[derive(Deserialize)]
pub struct WriteBody {
    /// Path relative to `cwd`.
    pub path: String,
    /// New file content.
    pub content: String,
}

/// `POST /api/code/write` — atomically write a file (temp → rename).
///
/// Creates parent directories if they don't exist.
/// Response: `{ "written": N, "path": "..." }`.
pub async fn write_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<WriteBody>,
) -> impl IntoResponse {
    if let Err(e) = check_auth(&headers, &state.config.token) {
        return e;
    }
    if body.content.len() > MAX_WRITE_BYTES {
        return err(
            StatusCode::PAYLOAD_TOO_LARGE,
            "content exceeds 10 MiB limit",
        );
    }
    let abs_path = match resolve_path(&state.config.cwd, &body.path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    // Create parent directories.
    if let Some(parent) = abs_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(path = %parent.display(), error = %e, "code/write: mkdir failed");
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to create parent directory",
            );
        }
    }

    // Atomic write: temp in same directory → rename.
    let parent = abs_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let tmp_path = parent.join(format!(".__code_tmp_{}__", uuid::Uuid::new_v4().simple()));

    if let Err(e) = std::fs::write(&tmp_path, body.content.as_bytes()) {
        tracing::warn!(path = %tmp_path.display(), error = %e, "code/write: temp write failed");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "write failed");
    }

    if let Err(e) = std::fs::rename(&tmp_path, &abs_path) {
        let _ = std::fs::remove_file(&tmp_path);
        tracing::warn!(error = %e, "code/write: rename failed");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "atomic rename failed");
    }

    ok(json!({
        "written": body.content.len(),
        "path": abs_path.display().to_string(),
    }))
}

// ── POST /api/code/search ─────────────────────────────────────────────────────

/// Request body for `POST /api/code/search`.
#[derive(Deserialize)]
pub struct SearchBody {
    /// Text or regex pattern to search for.
    pub pattern: String,
    /// Directory or file to search within (relative to `cwd`).
    #[serde(default)]
    pub path: Option<String>,
    /// Whether the pattern is case-sensitive. Default: `false`.
    #[serde(default)]
    pub case_sensitive: bool,
    /// Treat `pattern` as a literal string (no regex). Default: `false`.
    #[serde(default)]
    pub fixed_strings: bool,
}

/// One search match.
#[derive(Serialize)]
pub struct SearchMatch {
    /// Relative path to the file.
    pub file: String,
    /// 1-based line number.
    pub line: u64,
    /// Matching line text (trimmed).
    pub text: String,
}

/// `POST /api/code/search` — search for a pattern inside `path` (ripgrep preferred, grep fallback).
///
/// Response: `{ "matches": [...], "truncated": bool }`.
pub async fn search_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<SearchBody>,
) -> impl IntoResponse {
    if let Err(e) = check_auth(&headers, &state.config.token) {
        return e;
    }
    if body.pattern.is_empty() {
        return err(StatusCode::BAD_REQUEST, "pattern must not be empty");
    }

    let search_root = body.path.as_deref().unwrap_or(".");
    let abs_root = match resolve_path(&state.config.cwd, search_root) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let cwd_clone = state.config.cwd.clone();
    let pattern = body.pattern.clone();

    let task = tokio::task::spawn_blocking(move || {
        run_search(&abs_root, &pattern, body.case_sensitive, body.fixed_strings)
    });
    let output = match tokio::time::timeout(std::time::Duration::from_secs(30), task).await {
        Ok(join) => join.unwrap_or_else(|_| Err("search task panicked".to_owned())),
        Err(_) => Err("search timed out after 30 seconds".to_owned()),
    };

    match output {
        Ok((raw_lines, truncated)) => {
            let matches: Vec<Value> = raw_lines
                .iter()
                .filter_map(|line| parse_grep_line(line, &cwd_clone))
                .map(|m| json!({ "file": m.file, "line": m.line, "text": m.text }))
                .collect();
            ok(json!({ "matches": matches, "truncated": truncated }))
        }
        Err(e) => {
            tracing::warn!(error = %e, "code/search: search failed");
            err(StatusCode::INTERNAL_SERVER_ERROR, "search failed")
        }
    }
}

/// Run `rg` (ripgrep) if available, fall back to `grep -rn`.
///
/// Streams subprocess stdout line-by-line and stops after [`MAX_MATCHES`]
/// to bound memory use. Returns raw `file:line:text` lines and a truncated flag.
fn run_search(
    root: &Path,
    pattern: &str,
    case_sensitive: bool,
    fixed_strings: bool,
) -> Result<(Vec<String>, bool), String> {
    const MAX_MATCHES: usize = 200;

    let use_rg = std::process::Command::new("rg")
        .arg("--version")
        .output()
        .is_ok();

    let mut child = if use_rg {
        let mut cmd = std::process::Command::new("rg");
        cmd.arg("--line-number")
            .arg("--no-heading")
            .arg("--color=never");
        if !case_sensitive {
            cmd.arg("--ignore-case");
        }
        if fixed_strings {
            cmd.arg("--fixed-strings");
        }
        cmd.arg(pattern)
            .arg(root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?
    } else {
        let mut cmd = std::process::Command::new("grep");
        cmd.arg("-rn");
        if !case_sensitive {
            cmd.arg("-i");
        }
        if fixed_strings {
            cmd.arg("-F");
        }
        cmd.arg(pattern)
            .arg(root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?
    };

    let stdout = child.stdout.take().ok_or("no stdout handle")?;
    let mut lines: Vec<String> = BufReader::new(stdout)
        .lines()
        .take(MAX_MATCHES + 1)
        .collect::<std::io::Result<_>>()
        .map_err(|e| e.to_string())?;

    // Terminate the child early once we have enough lines.
    let _ = child.kill();
    let _ = child.wait();

    let truncated = lines.len() > MAX_MATCHES;
    lines.truncate(MAX_MATCHES);
    Ok((lines, truncated))
}

/// Parse a single `file:line:text` grep output line into a [`SearchMatch`].
fn parse_grep_line(line: &str, cwd: &Path) -> Option<SearchMatch> {
    let mut parts = line.splitn(3, ':');
    let file_raw = parts.next()?;
    let line_str = parts.next()?;
    let text = parts.next().unwrap_or("").trim().to_owned();
    let line_num: u64 = line_str.trim().parse().ok()?;

    let file_path = std::path::Path::new(file_raw);
    let rel = file_path
        .strip_prefix(cwd)
        .unwrap_or(file_path)
        .to_string_lossy()
        .into_owned();

    Some(SearchMatch {
        file: rel,
        line: line_num,
        text,
    })
}

// ── POST /api/code/preview-diff ───────────────────────────────────────────────

/// Request body for `POST /api/code/preview-diff`.
#[derive(Deserialize)]
pub struct PreviewDiffBody {
    /// File path relative to `cwd`.
    pub path: String,
    /// Proposed new content to diff against current on-disk content.
    pub new_content: String,
}

/// `POST /api/code/preview-diff` — compute a unified diff without applying it.
///
/// Uses `similar::TextDiff` (Myers algorithm). Returns the diff as a unified
/// diff string that `patch(1)` can apply.
/// Response: `{ "diff": "...", "has_changes": bool }`.
pub async fn preview_diff_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<PreviewDiffBody>,
) -> impl IntoResponse {
    if let Err(e) = check_auth(&headers, &state.config.token) {
        return e;
    }
    let abs_path = match resolve_path(&state.config.cwd, &body.path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let old_content = if abs_path.exists() {
        match std::fs::read_to_string(&abs_path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(path = %abs_path.display(), error = %e, "code/preview-diff: read failed");
                return err(StatusCode::NOT_FOUND, "file not found or not UTF-8");
            }
        }
    } else {
        String::new()
    };

    let diff = TextDiff::from_lines(&old_content, &body.new_content);
    let mut unified = String::new();
    let path_str = abs_path.display().to_string();

    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        use std::fmt::Write as _;
        let _ = write!(unified, "--- {path_str}\n+++ {path_str}\n{hunk}");
    }

    let has_changes = diff.iter_all_changes().any(|c| c.tag() != ChangeTag::Equal);

    ok(json!({
        "diff": unified,
        "has_changes": has_changes,
        "path": path_str,
    }))
}

// ── POST /api/code/apply-diff ─────────────────────────────────────────────────

/// Request body for `POST /api/code/apply-diff`.
#[derive(Deserialize)]
pub struct ApplyDiffBody {
    /// Target file path relative to `cwd`.
    pub path: String,
    /// Unified diff string to apply via `patch`.
    pub diff: String,
}

/// `POST /api/code/apply-diff` — apply a unified diff to a file via `patch(1)`.
///
/// Response: `{ "applied": true, "path": "..." }` on success.
pub async fn apply_diff_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<ApplyDiffBody>,
) -> impl IntoResponse {
    if let Err(e) = check_auth(&headers, &state.config.token) {
        return e;
    }
    if body.diff.is_empty() {
        return err(StatusCode::BAD_REQUEST, "diff must not be empty");
    }
    let abs_path = match resolve_path(&state.config.cwd, &body.path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    // Write diff to a temp file so we can pass it to `patch` without shell injection.
    let tmp_dir = std::env::temp_dir();
    let tmp_diff = tmp_dir.join(format!("la_diff_{}.patch", uuid::Uuid::new_v4().simple()));

    if let Err(e) = std::fs::write(&tmp_diff, body.diff.as_bytes()) {
        tracing::warn!(error = %e, "code/apply-diff: failed to write temp diff");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "failed to stage diff");
    }

    let output = std::process::Command::new("patch")
        .arg("--silent")
        .arg("--forward")
        .arg("--no-backup-if-mismatch")
        .arg("-r")
        .arg("/dev/null")
        .arg(&abs_path)
        .arg(&tmp_diff)
        .output();

    let _ = std::fs::remove_file(&tmp_diff);

    match output {
        Ok(o) if o.status.success() => ok(json!({
            "applied": true,
            "path": abs_path.display().to_string(),
        })),
        Ok(o) => {
            let msg = String::from_utf8_lossy(&o.stderr);
            tracing::warn!(path = %abs_path.display(), stderr = %msg, "code/apply-diff: patch failed");
            err(StatusCode::UNPROCESSABLE_ENTITY, "patch command failed")
        }
        Err(e) => {
            tracing::warn!(error = %e, "code/apply-diff: failed to invoke patch");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "patch command unavailable",
            )
        }
    }
}
