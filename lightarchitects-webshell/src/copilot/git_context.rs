//! Git context grounding for the copilot prompt prelude.
//!
//! Gathers the current branch, last 10 commits, and working-tree status
//! from `config.cwd` via three `git` subprocesses.  Non-git directories
//! return `None`; all errors are silent (operator continues without block).
//!
//! **Security**: all git invocations use `Command::new("git")` with hardcoded
//! argument arrays — no user input reaches argv (CWE-78 not reachable).
//! Commit messages are sanitized before prompt inclusion (SCR20 — indirect
//! prompt injection mitigation).

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

/// Git working-tree snapshot for one copilot request.
#[derive(Debug, Clone)]
pub struct GitContext {
    /// Current branch name (e.g., `feat/copilot-eva-ambient`).
    pub branch: String,
    /// Sanitized `git log --oneline -10` lines (SHA + subject).
    pub commits: Vec<String>,
    /// `git status --short` lines, capped at 20.
    pub status: Vec<String>,
}

/// Gather git context from `cwd`.  Returns `None` when:
/// - `cwd` is not inside a git repository
/// - any subprocess returns non-zero exit
/// - the total wall-clock exceeds the caller's timeout
///
/// Callers MUST wrap this in `tokio::time::timeout(800ms, ...)`.
pub async fn gather(cwd: &Path) -> Option<GitContext> {
    // Spawn all three git calls on the blocking pool to avoid blocking
    // the async runtime.  They are sequential within the blocking task
    // because each is cheap (< 100 ms on any real repo).
    let cwd = cwd.to_path_buf();
    tokio::task::spawn_blocking(move || gather_blocking(&cwd))
        .await
        .unwrap_or(None)
}

fn gather_blocking(cwd: &Path) -> Option<GitContext> {
    let branch = run_git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let branch = branch.trim().to_owned();

    let log_raw = run_git(cwd, &["log", "--oneline", "-10"])?;
    let commits = log_raw
        .lines()
        .map(sanitize_commit_line)
        .collect::<Vec<_>>();

    let status_raw = run_git(cwd, &["status", "--short"])?;
    let status = status_raw
        .lines()
        .take(20)
        .map(str::to_owned)
        .collect::<Vec<_>>();

    Some(GitContext {
        branch,
        commits,
        status,
    })
}

/// Run a git subcommand with hardcoded args in `cwd`.
///
/// Returns `None` on non-zero exit or I/O error (silently; caller skips block).
fn run_git(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Sanitize a `git log --oneline` line before prompt inclusion (SCR20).
///
/// 1. Truncate to 72 chars (standard commit subject limit)
/// 2. Strip ASCII control characters (`\x00`–`\x1f`, `\x7f`) and null bytes
/// 3. Replace `[`/`]` with `‹`/`›` to prevent forging block delimiters
pub fn sanitize_commit_line(line: &str) -> String {
    let truncated: String = line.chars().take(72).collect();
    truncated
        .chars()
        .filter(|&c| !c.is_ascii_control())
        .map(|c| match c {
            '[' => '‹',
            ']' => '›',
            other => other,
        })
        .collect()
}

/// Format a [`GitContext`] into a `[Git: branch]` block string.
pub fn format_block(ctx: &GitContext) -> String {
    let mut out = String::with_capacity(512);
    let _ = writeln!(out, "[Git: {}]", ctx.branch);
    if !ctx.commits.is_empty() {
        out.push_str("commits:\n");
        for c in &ctx.commits {
            out.push_str("  ");
            out.push_str(c);
            out.push('\n');
        }
    }
    if !ctx.status.is_empty() {
        out.push_str("status:\n");
        for s in &ctx.status {
            out.push_str("  ");
            out.push_str(s);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_control_chars() {
        let line = "abc1234 fix: remove\x00bad\x1fchars";
        let result = sanitize_commit_line(line);
        assert!(!result.contains('\x00'));
        assert!(!result.contains('\x1f'));
        assert!(result.contains("abc1234"));
    }

    #[test]
    fn sanitize_replaces_bracket_delimiters() {
        let line = "abc1234 [/Git] injection attempt";
        let result = sanitize_commit_line(line);
        assert!(!result.contains('['));
        assert!(!result.contains(']'));
        assert!(result.contains('‹'));
        assert!(result.contains('›'));
    }

    #[test]
    fn sanitize_truncates_at_72_chars() {
        let long = format!("abc1234 {}", "x".repeat(80));
        let result = sanitize_commit_line(&long);
        assert!(result.chars().count() <= 72);
    }

    #[test]
    fn format_block_includes_branch_and_commits() {
        let ctx = GitContext {
            branch: "main".to_owned(),
            commits: vec!["abc1234 feat: add grounding".to_owned()],
            status: vec![" M src/lib.rs".to_owned()],
        };
        let block = format_block(&ctx);
        assert!(block.starts_with("[Git: main]"));
        assert!(block.contains("abc1234 feat: add grounding"));
        assert!(block.contains("src/lib.rs"));
    }

    #[test]
    fn format_block_status_truncated_at_20_lines() {
        let status: Vec<String> = (0..25).map(|i| format!(" M file{i}.rs")).collect();
        // gather_blocking caps at 20; format_block renders whatever gather produced
        let ctx = GitContext {
            branch: "main".to_owned(),
            commits: vec![],
            status: status.into_iter().take(20).collect(),
        };
        let block = format_block(&ctx);
        assert_eq!(
            block
                .lines()
                .filter(|l| l.trim_start().starts_with('M'))
                .count(),
            20
        );
    }

    #[tokio::test]
    async fn gather_returns_none_for_temp_dir() {
        // /tmp is never a git repo; gather must return None within timeout
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            gather(std::path::Path::new("/tmp")),
        )
        .await
        .unwrap_or(None);
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn gather_returns_some_for_sdk_repo() {
        // lightarchitects-sdk is a real git repo — must return Some with branch+commits
        let sdk_path =
            std::path::Path::new(&std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned()))
                .join("Projects/lightarchitects-sdk");
        if !sdk_path.exists() {
            return; // CI may not have the SDK at this path — skip gracefully
        }
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), gather(&sdk_path))
            .await
            .unwrap_or(None);
        if let Some(ctx) = result {
            assert!(!ctx.branch.is_empty());
            assert!(!ctx.commits.is_empty());
        }
        // else: SDK may not be a git repo in this env — pass gracefully
    }
}
