//! `code.*` tool suite — file-system operations scoped to project roots.
//!
//! All write operations enforce T-2 path canonicalization: the target must
//! resolve to a path within the gateway's configured `allowed_directories`.
//! Read operations use the same path-validation logic as `lightarchitects_read`.
//!
//! # Tools
//!
//! | Name | Params | Returns |
//! |------|--------|---------|
//! | `code.read_file` | `{path}` | `{path, content, size, encoding, truncated?}` |
//! | `code.write_file` | `{path, content}` | `{path, bytes_written, mtime}` |
//! | `code.list_dir` | `{path, glob?}` | `{path, entries:[{name,type,size,mtime}]}` |
//! | `code.apply_diff` | `{path, diff}` | `{applied, conflicts, message}` |
//! | `code.search_text` | `{root, pattern, glob?}` | `{matches:[{path,line,match}]}` |
//! | `code.preview_diff` | `{path, content}` | `{unified_diff, line_count}` |

use std::fmt::Write as FmtWrite;
use std::process::Command;
use std::time::SystemTime;

use serde_json::{Value, json};
use similar::{ChangeTag, TextDiff};

use crate::config::GatewayConfig;
use crate::core_tools::security;
use crate::error::GatewayError;

/// 5 MiB read limit before switching to truncated-preview mode.
const MAX_PREVIEW_BYTES: u64 = 5 * 1024 * 1024;

/// 50 MiB hard refusal for files too large to edit in the webshell.
const MAX_HARD_BYTES: u64 = 50 * 1024 * 1024;

// ── code.read_file ────────────────────────────────────────────────────────────

/// Read a file's content, with optional line-range selection.
///
/// Files >50 MiB are refused; files >5 MiB return `truncated: true` with the
/// first 5 MiB of content.
///
/// # Errors
///
/// Returns [`GatewayError`] on missing parameters or filesystem failures.
pub fn run_read_file(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let path_str = params["path"]
        .as_str()
        .ok_or(GatewayError::MissingParam("path"))?;

    let canonical = security::validate_path(path_str, config)?;

    let meta = std::fs::metadata(&canonical)
        .map_err(|e| GatewayError::File(format!("{}: {e}", canonical.display())))?;

    if meta.len() > MAX_HARD_BYTES {
        return Err(GatewayError::File(format!(
            "file too large to edit in webshell ({} bytes, max {} MiB)",
            meta.len(),
            MAX_HARD_BYTES / 1024 / 1024
        )));
    }

    let mtime = system_time_unix(meta.modified().ok());
    let truncated = meta.len() > MAX_PREVIEW_BYTES;

    let content = if truncated {
        let raw = std::fs::read(&canonical)
            .map_err(|e| GatewayError::File(format!("{}: {e}", canonical.display())))?;
        let limit = usize::try_from(MAX_PREVIEW_BYTES).unwrap_or(usize::MAX);
        String::from_utf8_lossy(&raw[..limit]).into_owned()
    } else {
        std::fs::read_to_string(&canonical)
            .map_err(|e| GatewayError::File(format!("{}: {e}", canonical.display())))?
    };

    let payload = json!({
        "path": canonical.display().to_string(),
        "content": content,
        "size": meta.len(),
        "encoding": "utf8",
        "truncated": truncated,
        "mtime": mtime,
    });

    Ok(text_json(payload))
}

// ── code.write_file ───────────────────────────────────────────────────────────

/// Write content to a file atomically (tmp → rename).
///
/// Enforces T-2 path canonicalization: target must be within the gateway's
/// `allowed_directories`. Parent directories are created automatically.
///
/// # Errors
///
/// Returns [`GatewayError`] on missing parameters, path-traversal violations,
/// or filesystem failures.
pub fn run_write_file(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let path_str = params["path"]
        .as_str()
        .ok_or(GatewayError::MissingParam("path"))?;
    let content = params["content"]
        .as_str()
        .ok_or(GatewayError::MissingParam("content"))?;

    let expanded = crate::config::expand_tilde(path_str);

    // T-2: full security validation before any filesystem mutation.
    if expanded.exists() {
        security::validate_write_path(path_str, config)?;
    } else {
        security::check_write_denied(&expanded.to_string_lossy())?;
        if let Some(parent) = expanded.parent() {
            let mut ancestor = parent.to_path_buf();
            while !ancestor.exists() {
                security::check_write_denied(&ancestor.to_string_lossy())?;
                security::check_denied_components(&ancestor)?;
                match ancestor.parent() {
                    Some(p) if !p.as_os_str().is_empty() => ancestor = p.to_path_buf(),
                    _ => break,
                }
            }
            if ancestor.exists() {
                security::validate_path(&ancestor.to_string_lossy(), config)?;
            }
        }
    }

    // Create parent directories.
    if let Some(parent) = expanded.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                GatewayError::File(format!("create dirs {}: {e}", parent.display()))
            })?;
        }
    }

    // Atomic write: write to tmpfile in same directory, then rename.
    let tmp = expanded.with_extension(".__code_tmp__");
    std::fs::write(&tmp, content)
        .map_err(|e| GatewayError::File(format!("write tmp {}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, &expanded).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        GatewayError::File(format!("rename {}: {e}", expanded.display()))
    })?;

    let meta = std::fs::metadata(&expanded)
        .map_err(|e| GatewayError::File(format!("{}: {e}", expanded.display())))?;

    let payload = json!({
        "path": expanded.display().to_string(),
        "bytes_written": content.len(),
        "mtime": system_time_unix(meta.modified().ok()),
    });

    Ok(text_json(payload))
}

// ── code.list_dir ─────────────────────────────────────────────────────────────

/// List directory entries.
///
/// Returns each entry with `name`, `type` (`"file"` | `"dir"` | `"symlink"`),
/// `size` (bytes, 0 for dirs), and `mtime` (Unix timestamp).
///
/// # Errors
///
/// Returns [`GatewayError`] on missing parameters or filesystem failures.
pub fn run_list_dir(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let path_str = params["path"]
        .as_str()
        .ok_or(GatewayError::MissingParam("path"))?;

    let canonical = security::validate_path(path_str, config)?;

    if !canonical.is_dir() {
        return Err(GatewayError::File(format!(
            "not a directory: {}",
            canonical.display()
        )));
    }

    let glob_pat = params["glob"].as_str();

    let mut entries: Vec<Value> = Vec::new();
    let rd = std::fs::read_dir(&canonical)
        .map_err(|e| GatewayError::File(format!("{}: {e}", canonical.display())))?;

    for entry in rd {
        let entry = entry.map_err(|e| GatewayError::File(format!("read dir entry: {e}")))?;
        let name = entry.file_name().to_string_lossy().into_owned();

        // Apply glob filter if provided.
        if let Some(pat) = glob_pat {
            if !glob_match(pat, &name) {
                continue;
            }
        }

        let meta = entry
            .metadata()
            .map_err(|e| GatewayError::File(format!("{name}: {e}")))?;

        let kind = if meta.is_symlink() {
            "symlink"
        } else if meta.is_dir() {
            "dir"
        } else {
            "file"
        };

        entries.push(json!({
            "name": name,
            "type": kind,
            "size": if meta.is_file() { meta.len() } else { 0 },
            "mtime": system_time_unix(meta.modified().ok()),
        }));
    }

    // Sort: directories first, then files, alphabetically within each group.
    entries.sort_by(|a, b| {
        let a_dir = a["type"] == "dir";
        let b_dir = b["type"] == "dir";
        match (a_dir, b_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or("")),
        }
    });

    let payload = json!({
        "path": canonical.display().to_string(),
        "entries": entries,
    });

    Ok(text_json(payload))
}

// ── code.apply_diff ───────────────────────────────────────────────────────────

/// Apply a unified diff to a file.
///
/// Uses the system `patch` command for reliable hunk application. The target
/// file must exist and pass write-path validation (T-2). A `.orig` backup is
/// created by `patch` and immediately removed on success.
///
/// # Errors
///
/// Returns [`GatewayError`] on missing parameters, security violations, or when
/// the diff cannot be parsed/applied.
pub fn run_apply_diff(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let path_str = params["path"]
        .as_str()
        .ok_or(GatewayError::MissingParam("path"))?;
    let diff_str = params["diff"]
        .as_str()
        .ok_or(GatewayError::MissingParam("diff"))?;

    // Validate write permission.
    security::validate_write_path(path_str, config)?;
    let canonical = security::validate_path(path_str, config)?;

    // Write diff to a temp file.
    let diff_tmp = canonical.with_extension(".__code_diff__.patch");
    std::fs::write(&diff_tmp, diff_str)
        .map_err(|e| GatewayError::File(format!("write diff tmp: {e}")))?;

    let output = Command::new("patch")
        .args([
            "--unified",
            "--forward",
            "--backup",
            canonical.to_str().unwrap_or_default(),
            diff_tmp.to_str().unwrap_or_default(),
        ])
        .output()
        .map_err(|e| GatewayError::Subprocess(format!("patch command: {e}")))?;

    // Clean up diff tempfile.
    let _ = std::fs::remove_file(&diff_tmp);

    // Remove .orig backup if patch created one.
    let orig = canonical.with_extension("orig");
    let _ = std::fs::remove_file(&orig);

    let applied = output.status.success();
    let message = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr_msg = String::from_utf8_lossy(&output.stderr).trim().to_owned();

    let payload = json!({
        "applied": applied,
        "conflicts": if applied { vec![] } else { vec![stderr_msg.clone()] },
        "message": if applied { message } else {
            format!("patch failed (exit {}): {}", output.status.code().unwrap_or(-1), stderr_msg)
        },
    });

    Ok(text_json(payload))
}

// ── code.search_text ─────────────────────────────────────────────────────────

/// Search file contents within a directory using ripgrep (fallback: grep).
///
/// # Errors
///
/// Returns [`GatewayError`] on missing parameters or subprocess failures.
pub fn run_search_text(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let root_str = params["root"]
        .as_str()
        .ok_or(GatewayError::MissingParam("root"))?;
    let pattern = params["pattern"]
        .as_str()
        .ok_or(GatewayError::MissingParam("pattern"))?;
    let glob_pat = params["glob"].as_str();

    let canonical = security::validate_path(root_str, config)?;

    // Try ripgrep first; fall back to grep.
    let mut cmd = if which_rg() {
        let mut c = Command::new("rg");
        c.args(["--json", "--max-count=50", pattern]);
        if let Some(g) = glob_pat {
            c.args(["--glob", g]);
        }
        c.arg(canonical.as_os_str());
        c
    } else {
        let mut c = Command::new("grep");
        c.args(["-r", "-n", "--max-count=50", pattern]);
        if let Some(g) = glob_pat {
            c.args(["--include", g]);
        }
        c.arg(canonical.as_os_str());
        c
    };

    let output = cmd
        .output()
        .map_err(|e| GatewayError::Subprocess(format!("search: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let matches = parse_search_output(&stdout, which_rg());

    let payload = json!({ "matches": matches });
    Ok(text_json(payload))
}

// ── code.preview_diff ────────────────────────────────────────────────────────

/// Generate a unified diff between a file's current content and proposed content.
///
/// # Errors
///
/// Returns [`GatewayError`] on missing parameters or filesystem failures.
pub fn run_preview_diff(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let path_str = params["path"]
        .as_str()
        .ok_or(GatewayError::MissingParam("path"))?;
    let proposed = params["content"]
        .as_str()
        .ok_or(GatewayError::MissingParam("content"))?;

    let canonical = security::validate_path(path_str, config)?;

    let current = std::fs::read_to_string(&canonical)
        .map_err(|e| GatewayError::File(format!("{}: {e}", canonical.display())))?;

    let label = canonical
        .file_name()
        .map_or("file", |n| n.to_str().unwrap_or("file"));
    let diff = build_unified_diff(label, &current, proposed);
    let line_count = diff.lines().count();

    let payload = json!({
        "unified_diff": diff,
        "line_count": line_count,
    });

    Ok(text_json(payload))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a unified diff string using the `similar` crate.
fn build_unified_diff(filename: &str, original: &str, modified: &str) -> String {
    let diff = TextDiff::from_lines(original, modified);
    let mut out = String::new();
    let _ = writeln!(out, "--- {filename}");
    let _ = writeln!(out, "+++ {filename}");
    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        let _ = writeln!(out, "{}", hunk.header());
        for change in hunk.iter_changes() {
            let prefix = match change.tag() {
                ChangeTag::Delete => '-',
                ChangeTag::Insert => '+',
                ChangeTag::Equal => ' ',
            };
            let _ = write!(out, "{}{}", prefix, change.value());
            if !change.value().ends_with('\n') {
                let _ = writeln!(out);
            }
        }
    }
    out
}

/// Wrap a JSON value in the standard MCP text-result envelope.
fn text_json(payload: Value) -> Value {
    json!({
        "content": [{"type": "text", "text": payload.to_string()}]
    })
}

/// Convert a `SystemTime` to a Unix timestamp (seconds since epoch), or 0 on failure.
fn system_time_unix(t: Option<SystemTime>) -> u64 {
    t.and_then(|st| st.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

/// Minimal glob match supporting `*` (any chars within segment) and `?` (one char).
fn glob_match(pattern: &str, name: &str) -> bool {
    let mut p = pattern.chars().peekable();
    let mut n = name.chars().peekable();
    glob_match_inner(&mut p, &mut n)
}

fn glob_match_inner(
    p: &mut std::iter::Peekable<std::str::Chars<'_>>,
    n: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> bool {
    loop {
        match p.peek().copied() {
            None => return n.peek().is_none(),
            Some('*') => {
                p.next();
                // consume any remaining `*`
                while p.peek() == Some(&'*') {
                    p.next();
                }
                if p.peek().is_none() {
                    return true;
                }
                // try matching the rest of pattern against every suffix of name
                let p_rest: String = p.collect();
                let n_rest: String = n.collect();
                for i in 0..=n_rest.len() {
                    if i <= n_rest.len() && glob_match(&p_rest, &n_rest[i..]) {
                        return true;
                    }
                }
                return false;
            }
            Some('?') => {
                p.next();
                if n.next().is_none() {
                    return false;
                }
            }
            Some(pc) => {
                p.next();
                match n.next() {
                    Some(nc) if nc == pc => {}
                    _ => return false,
                }
            }
        }
    }
}

/// Check if `rg` (ripgrep) is available on PATH.
fn which_rg() -> bool {
    Command::new("which")
        .arg("rg")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Parse search output from ripgrep (JSON) or grep (text) into match objects.
fn parse_search_output(output: &str, is_rg_json: bool) -> Vec<Value> {
    let mut matches = Vec::new();

    if is_rg_json {
        for line in output.lines() {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if v["type"] == "match" {
                let data = &v["data"];
                let path = data["path"]["text"].as_str().unwrap_or("").to_owned();
                let line_no = data["line_number"].as_u64().unwrap_or(0);
                let text = data["lines"]["text"].as_str().unwrap_or("").to_owned();
                matches.push(json!({
                    "path": path,
                    "line": line_no,
                    "match": text.trim_end_matches('\n'),
                }));
            }
        }
    } else {
        // grep -n output: "path:line_no:match"
        for line in output.lines() {
            let mut parts = line.splitn(3, ':');
            let path = parts.next().unwrap_or("").to_owned();
            let line_no: u64 = parts.next().unwrap_or("0").parse().unwrap_or(0);
            let match_text = parts.next().unwrap_or("").to_owned();
            if !path.is_empty() {
                matches.push(json!({
                    "path": path,
                    "line": line_no,
                    "match": match_text,
                }));
            }
        }
    }

    matches
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg() -> GatewayConfig {
        GatewayConfig::default()
    }

    #[test]
    fn read_file_returns_content() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("hello.txt");
        std::fs::write(&f, "hello\nworld\n").unwrap();
        let result = run_read_file(json!({"path": f.to_str().unwrap()}), &cfg()).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        let obj: Value = serde_json::from_str(text).unwrap();
        assert_eq!(obj["content"], "hello\nworld\n");
        assert_eq!(obj["truncated"], false);
    }

    #[test]
    fn read_file_missing_path_returns_error() {
        let result = run_read_file(json!({}), &cfg());
        assert!(matches!(result, Err(GatewayError::MissingParam("path"))));
    }

    #[test]
    fn write_file_creates_and_returns_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("out.txt");
        let result = run_write_file(
            json!({"path": f.to_str().unwrap(), "content": "data"}),
            &cfg(),
        )
        .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        let obj: Value = serde_json::from_str(text).unwrap();
        assert_eq!(obj["bytes_written"], 4);
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "data");
    }

    #[test]
    fn list_dir_returns_sorted_entries() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();
        std::fs::create_dir(dir.path().join("a_dir")).unwrap();
        let result = run_list_dir(json!({"path": dir.path().to_str().unwrap()}), &cfg()).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        let obj: Value = serde_json::from_str(text).unwrap();
        let entries = obj["entries"].as_array().unwrap();
        assert_eq!(entries[0]["name"], "a_dir");
        assert_eq!(entries[0]["type"], "dir");
        assert_eq!(entries[1]["name"], "b.txt");
        assert_eq!(entries[1]["type"], "file");
    }

    #[test]
    fn preview_diff_produces_unified_diff() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("f.txt");
        std::fs::write(&f, "line1\nline2\n").unwrap();
        let result = run_preview_diff(
            json!({"path": f.to_str().unwrap(), "content": "line1\nchanged\n"}),
            &cfg(),
        )
        .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        let obj: Value = serde_json::from_str(text).unwrap();
        let diff = obj["unified_diff"].as_str().unwrap();
        assert!(diff.contains("-line2"));
        assert!(diff.contains("+changed"));
    }

    #[test]
    fn glob_match_star_matches_any() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(!glob_match("*.rs", "main.ts"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("foo*bar", "fooXXXbar"));
    }

    #[test]
    fn glob_match_question_matches_single_char() {
        assert!(glob_match("?.txt", "a.txt"));
        assert!(!glob_match("?.txt", "ab.txt"));
    }
}
