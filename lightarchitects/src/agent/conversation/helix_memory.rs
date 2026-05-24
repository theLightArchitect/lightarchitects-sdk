//! Helix-backed conversation memory for persistent agent sessions.
//!
//! [`HelixSessionMemory`] implements [`ConversationMemory`] and writes every
//! turn to a markdown file under `~/lightarchitects/soul/helix/user/sessions/`.
//! On construction it reads prior turns from the same file, so the session
//! continues naturally across process restarts.
//!
//! ## File layout
//!
//! ```text
//! ~/lightarchitects/soul/helix/user/sessions/
//!   {project-slug}-{YYYY-MM-DD}.md    ← today's session for this project
//!   {project-slug}-{YYYY-MM-DD}.md    ← prior days (read-only after rollover)
//! ```
//!
//! ## Note format
//!
//! ```markdown
//! ---
//! type: agent-session
//! project_path: /path/to/project
//! date: 2026-05-23
//! ---
//!
//! ## 2026-05-23T10:30:01Z user
//!
//! let's plan the SSE endpoint
//!
//! ## 2026-05-23T10:30:15Z agent
//!
//! Sure! I'll kick off the plan.
//!
//! ```
//!
//! Turns are recovered by scanning for `## {timestamp} {role}` headers.

use std::borrow::Cow;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use super::memory::{ConversationMemory, MessageRole, Turn};

// ── Secret redaction (B6) ─────────────────────────────────────────────────────

#[allow(clippy::expect_used)] // Regex literals are compile-time-validated.
static SECRET_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (
            Regex::new(r"(?i)(api[_-]?key|secret|token|password|bearer|credential)\s*[:=]\s*\S+")
                .expect("static regex"),
            "${1}=[REDACTED:secret-pattern]",
        ),
        (
            Regex::new(r"sk-[a-zA-Z0-9\-]{20,}").expect("static regex"),
            "[REDACTED:anthropic-key]",
        ),
        (
            Regex::new(r"ghp_[a-zA-Z0-9]{36}|github_pat_[a-zA-Z0-9_]{82,}").expect("static regex"),
            "[REDACTED:github-token]",
        ),
        (
            Regex::new(r"AKIA[0-9A-Z]{16}").expect("static regex"),
            "[REDACTED:aws-key-id]",
        ),
        (
            Regex::new(r"(?i)aws_secret_access_key\s*[:=]\s*\S+").expect("static regex"),
            "[REDACTED:aws-secret]",
        ),
    ]
});

fn redact_secrets(content: &str) -> Cow<'_, str> {
    let mut result: Cow<'_, str> = Cow::Borrowed(content);
    for (pattern, replacement) in SECRET_PATTERNS.iter() {
        if pattern.is_match(result.as_ref()) {
            let replaced = pattern
                .replace_all(result.as_ref(), *replacement)
                .into_owned();
            result = Cow::Owned(replaced);
        }
    }
    result
}

// ── Path helpers ──────────────────────────────────────────────────────────────

fn sessions_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
    PathBuf::from(home).join("lightarchitects/soul/helix/user/sessions")
}

fn project_slug(cwd: &Path) -> String {
    cwd.file_name().map_or_else(
        || "session".to_owned(),
        |n| {
            n.to_string_lossy()
                .to_ascii_lowercase()
                .replace(' ', "-")
                .replace(['/', '\\', '.'], "-")
        },
    )
}

/// Return today's session file path for `cwd`.
pub fn session_path(cwd: &Path) -> PathBuf {
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    sessions_dir().join(format!("{}-{date}.md", project_slug(cwd)))
}

// ── File I/O ──────────────────────────────────────────────────────────────────

fn ensure_file(path: &Path, cwd: &Path) -> std::io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let header = format!(
        "---\ntype: agent-session\nproject_path: {}\ndate: {date}\n---\n\n",
        cwd.display()
    );
    std::fs::write(path, header)
}

fn append_turn(path: &Path, role: MessageRole, content: &str) -> std::io::Result<()> {
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let role_str = match role {
        MessageRole::User => "user",
        MessageRole::Assistant => "agent",
        MessageRole::System => "system",
    };
    let block = format!("\n## {ts} {role_str}\n\n{content}\n");
    let mut f = std::fs::OpenOptions::new().append(true).open(path)?;
    f.write_all(block.as_bytes())
}

fn parse_turns(path: &Path, limit: usize) -> Vec<Turn> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut turns: Vec<Turn> = Vec::new();
    let mut current_role: Option<MessageRole> = None;
    let mut current_lines: Vec<&str> = Vec::new();
    let mut in_frontmatter = false;
    let mut fm_done = false;

    for line in text.lines() {
        if !fm_done {
            if line.trim() == "---" {
                if in_frontmatter {
                    fm_done = true;
                } else {
                    in_frontmatter = true;
                }
                continue;
            }
            if in_frontmatter {
                continue;
            }
            fm_done = true;
        }

        if let Some(rest) = line.strip_prefix("## ") {
            if let Some(role) = current_role.take() {
                let content = current_lines.join("\n").trim().to_owned();
                if !content.is_empty() {
                    turns.push(Turn { role, content });
                }
                current_lines.clear();
            }
            let role = rest.split_whitespace().nth(1).and_then(|r| match r {
                "user" => Some(MessageRole::User),
                "agent" => Some(MessageRole::Assistant),
                "system" => Some(MessageRole::System),
                _ => None,
            });
            current_role = role;
            continue;
        }

        if current_role.is_some() {
            current_lines.push(line);
        }
    }

    if let Some(role) = current_role {
        let content = current_lines.join("\n").trim().to_owned();
        if !content.is_empty() {
            turns.push(Turn { role, content });
        }
    }

    if turns.len() > limit {
        turns.drain(..turns.len() - limit);
    }
    turns
}

// ── HelixSessionMemory ────────────────────────────────────────────────────────

/// [`ConversationMemory`] that persists turns to the local user helix.
///
/// Constructed via [`HelixSessionMemory::open`]. Turns written to `push()` are
/// immediately appended to disk; the `turns()` view is an in-memory cache
/// pre-populated from prior turns in the same session file.
pub struct HelixSessionMemory {
    turns: Vec<Turn>,
    path: PathBuf,
}

impl HelixSessionMemory {
    /// Open (or create) the session file for `cwd`.
    ///
    /// Loads the most recent `context_turns` turns from today's file into
    /// memory so the session continues without losing conversational context.
    /// Falls back to ephemeral in-memory storage if the helix path is absent —
    /// the session still works, it just won't persist to disk.
    #[must_use]
    pub fn open(cwd: &Path, context_turns: usize) -> Self {
        let path = session_path(cwd);
        let _ = ensure_file(&path, cwd);
        let turns = parse_turns(&path, context_turns);
        Self { turns, path }
    }

    /// Number of prior turns loaded from disk at session start.
    pub fn restored_turn_count(&self) -> usize {
        self.turns.len()
    }
}

impl ConversationMemory for HelixSessionMemory {
    fn push(&mut self, role: MessageRole, content: String) {
        let on_disk = redact_secrets(&content);
        let _ = append_turn(&self.path, role, on_disk.as_ref());
        self.turns.push(Turn { role, content });
    }

    fn turns(&self) -> &[Turn] {
        &self.turns
    }

    fn clear(&mut self) {
        self.turns.clear();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn redact_secrets_bearer_token() {
        let input = "Authorization: Bearer sk-ant-api03-abc123";
        let out = redact_secrets(input);
        assert!(!out.contains("sk-ant"));
    }

    #[test]
    fn redact_secrets_clean_content_borrows() {
        let input = "this is safe content";
        let out = redact_secrets(input);
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    #[test]
    fn parse_turns_empty_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("empty.md");
        std::fs::write(&path, "").unwrap();
        let turns = parse_turns(&path, 20);
        assert!(turns.is_empty());
    }

    #[test]
    fn parse_turns_round_trip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("session.md");
        std::fs::write(
            &path,
            "---\ntype: agent-session\n---\n\n## 2026-05-24T10:00:00Z user\n\nhello\n\n## 2026-05-24T10:00:01Z agent\n\nhi there\n",
        )
        .unwrap();
        let turns = parse_turns(&path, 20);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].content, "hello");
        assert_eq!(turns[1].content, "hi there");
    }

    #[test]
    fn parse_turns_limit() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("session.md");
        // 5 turns; parse_turns(limit=3) should return the last 3.
        let content = concat!(
            "---\n---\n\n",
            "## 2026-05-24T10:00:01Z user\n\nturn 1\n",
            "## 2026-05-24T10:00:02Z user\n\nturn 2\n",
            "## 2026-05-24T10:00:03Z user\n\nturn 3\n",
            "## 2026-05-24T10:00:04Z user\n\nturn 4\n",
            "## 2026-05-24T10:00:05Z user\n\nturn 5\n",
        );
        std::fs::write(&path, content).unwrap();
        let turns = parse_turns(&path, 3);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].content, "turn 3");
    }

    #[test]
    fn project_slug_derives_from_path() {
        let path = std::path::Path::new("/home/user/my-project");
        assert_eq!(project_slug(path), "my-project");
    }
}
