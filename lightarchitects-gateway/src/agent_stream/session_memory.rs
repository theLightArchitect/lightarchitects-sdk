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
//! /plan "add SSE streaming endpoint"
//!
//! ```
//!
//! Turns are recovered by scanning for `## {timestamp} {role}` headers.
//! Content extends until the next such header or EOF. This format renders
//! cleanly in any markdown viewer and is straightforward to parse.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use lightarchitects::agent::conversation::memory::{ConversationMemory, MessageRole, Turn};

// ── Path helpers ──────────────────────────────────────────────────────────────

/// Return the sessions directory: `~/lightarchitects/soul/helix/user/sessions/`.
fn sessions_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
    PathBuf::from(home).join("lightarchitects/soul/helix/user/sessions")
}

/// Derive a filesystem-safe slug from a project path.
///
/// Uses the last component of the path, lowercased with spaces replaced by
/// hyphens. Falls back to `"session"` for unusual paths.
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

/// Create the session file with YAML frontmatter if it doesn't already exist.
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

/// Append a single turn to the session file.
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

/// Parse turns from a session file, newest last.
///
/// Scans for `## {timestamp} {role}` headers; collects content until the next
/// header or EOF. Returns at most `limit` turns (from the end of the file).
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
        // Skip YAML frontmatter.
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
            // No frontmatter at all — fall through.
            fm_done = true;
        }

        // Turn header: `## {timestamp} {role}`
        if let Some(rest) = line.strip_prefix("## ") {
            // Flush previous turn.
            if let Some(role) = current_role.take() {
                let content = current_lines.join("\n").trim().to_owned();
                if !content.is_empty() {
                    turns.push(Turn { role, content });
                }
                current_lines.clear();
            }
            // Parse role from the header token after the timestamp.
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

    // Flush last turn.
    if let Some(role) = current_role {
        let content = current_lines.join("\n").trim().to_owned();
        if !content.is_empty() {
            turns.push(Turn { role, content });
        }
    }

    // Return only the last `limit` turns.
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
    /// If the file cannot be created (e.g. helix path absent) the memory falls
    /// back to ephemeral in-memory storage — the session still works, it just
    /// won't persist.
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
        // Best-effort write — failure doesn't break the session.
        let _ = append_turn(&self.path, role, &content);
        self.turns.push(Turn { role, content });
    }

    fn turns(&self) -> &[Turn] {
        &self.turns
    }

    fn clear(&mut self) {
        // Clear in-memory cache only — file history is permanent.
        self.turns.clear();
    }
}
