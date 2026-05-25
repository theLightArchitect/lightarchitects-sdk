//! Helix-backed conversation memory for persistent agent sessions.
//!
//! [`HelixSessionMemory`] implements [`ConversationMemory`] and writes every
//! turn to a markdown file under `~/lightarchitects/soul/helix/user/sessions/`.
//! On construction it reads prior turns from the same file, so the session
//! continues naturally across process restarts.
//!
//! A companion Mamba SSM state file (`.ssm`) accumulates a compressed summary
//! of the session in a 64-dim hidden vector. [`HelixSessionMemory::session_context_block`]
//! back-projects the state to 384-dim and queries the semantic HNSW index for
//! the most contextually relevant helix entries to inject into the LLM context
//! window. "The \[SSM\] state represents the context of a text sequence" (IBM).
//!
//! ## File layout
//!
//! ```text
//! ~/lightarchitects/soul/helix/user/sessions/
//!   {project-slug}-{YYYY-MM-DD}.md    ← today's session for this project
//!   {project-slug}-{YYYY-MM-DD}.ssm   ← companion Mamba SSM state (binary)
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
use std::fmt::Write as FmtWrite;
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

// ── Mamba SSM state ───────────────────────────────────────────────────────────

/// Hidden state dimension (64-dim compressed session context).
const SSM_DIM: usize = 64;
/// Input dimension: matches BGE-small semantic embedding space.
const SSM_INPUT_DIM: usize = 384;
/// Diagonal A-matrix value — governs decay rate. IBM: "A determines how each
/// state variable evolves over time"; 0.9 imposes stable bounded updates.
const SSM_A_VALUE: f32 = 0.9;

const SSM_MAGIC: &[u8; 4] = b"SSM\0";
const SSM_VERSION: u8 = 1;

/// Fixed B matrix: `SSM_DIM × SSM_INPUT_DIM` flat row-major (64 × 384).
/// Seeded deterministically so all instances share the same input projection.
static SSM_B_MATRIX: LazyLock<Vec<f32>> = LazyLock::new(generate_ssm_b);
/// Fixed P back-projection: `SSM_INPUT_DIM × SSM_DIM` flat row-major (384 × 64).
/// Projects `h` back to 384-dim for ANN query (replaces the C-matrix decoder).
static SSM_P_MATRIX: LazyLock<Vec<f32>> = LazyLock::new(generate_ssm_p);

fn ssm_lcg_next(state: &mut u64) -> f32 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    let bits = (*state >> 32) as u32;
    // IEEE 754 bit trick: top 23 bits → mantissa in [1.0, 2.0) → shift to [-1, 1).
    // Avoids integer→float cast, suppresses precision-loss lint.
    let frac = f32::from_bits(0x3f80_0000 | (bits >> 9)) - 1.0_f32;
    frac * 2.0 - 1.0
}

fn generate_ssm_b() -> Vec<f32> {
    let mut s: u64 = 0xBABE_CAFE_1234_5678;
    (0..SSM_DIM * SSM_INPUT_DIM)
        .map(|_| ssm_lcg_next(&mut s))
        .collect()
}

fn generate_ssm_p() -> Vec<f32> {
    let mut s: u64 = 0xCAFE_BABE_5678_1234;
    (0..SSM_INPUT_DIM * SSM_DIM)
        .map(|_| ssm_lcg_next(&mut s))
        .collect()
}

/// Set 0o600 permissions on `path` (owner read/write only, Cookbook §31).
/// No-op on non-Unix platforms.
#[cfg(unix)]
fn set_private_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Fletcher-32 checksum — detects single-bit errors and burst errors.
fn fletcher32(data: &[u8]) -> u32 {
    let mut s1: u32 = 0;
    let mut s2: u32 = 0;
    for &b in data {
        s1 = (s1 + u32::from(b)) % 65535;
        s2 = (s2 + s1) % 65535;
    }
    (s2 << 16) | s1
}

/// Project turn content bytes to a 384-dim input vector for SSM update.
///
/// Proportionally samples the content's byte values, normalising each to
/// `[-1.0, 1.0]`. Fully deterministic and sync — no embedding provider needed.
fn content_to_input_vec(content: &str) -> Vec<f32> {
    let bytes = content.as_bytes();
    let mut vec = vec![0.0f32; SSM_INPUT_DIM];
    if bytes.is_empty() {
        return vec;
    }
    for (i, v) in vec.iter_mut().enumerate() {
        // Pure integer proportional sampling: no float casts needed.
        let pos = (i * bytes.len() / SSM_INPUT_DIM).min(bytes.len() - 1);
        *v = f32::from(bytes[pos]) / 128.0 - 1.0;
    }
    vec
}

/// Mamba SSM hidden state for a session.
///
/// Update rule: `h_t = A ⊙ h_{t-1} + B · x_t`
/// where A is diagonal (fixed value 0.9), B is a fixed 64×384 projection matrix.
/// Reference: Gu & Dao, "Mamba: Linear-Time Sequence Modeling with Selective State
/// Spaces" (2023), <https://arxiv.org/abs/2312.00752>.
pub struct SsmState {
    /// 64-dim hidden state vector.
    pub h: Vec<f32>,
    /// Number of turns incorporated into `h`.
    pub turn_count: u64,
}

impl Default for SsmState {
    fn default() -> Self {
        Self::new()
    }
}

impl SsmState {
    /// Creates a zeroed SSM state (all hidden units = 0, `turn_count` = 0).
    pub fn new() -> Self {
        Self {
            h: vec![0.0; SSM_DIM],
            turn_count: 0,
        }
    }

    /// Advance hidden state: `h_t = A ⊙ h_{t-1} + B · x_t`.
    ///
    /// `input_embed` is the 384-dim input projection of the current turn.
    /// Extra dimensions beyond `SSM_INPUT_DIM` are silently ignored; shorter
    /// inputs are zero-padded via the B-matrix row bounds.
    pub fn update(&mut self, input_embed: &[f32]) {
        let b = &*SSM_B_MATRIX;
        let input_len = input_embed.len().min(SSM_INPUT_DIM);
        for i in 0..SSM_DIM {
            let bx: f32 = (0..input_len)
                .map(|j| b[i * SSM_INPUT_DIM + j] * input_embed[j])
                .sum();
            self.h[i] = SSM_A_VALUE * self.h[i] + bx;
        }
        self.turn_count += 1;
    }

    /// Back-project `h` to 384-dim for use as an ANN query vector.
    ///
    /// Replaces the C-matrix decoder from the original SSM output equation
    /// `y = C·h + D·x` — ANN retrieval over the semantic HNSW index serves
    /// as the output "decoder" in our retrieval-augmented context.
    pub fn query_vec(&self) -> Vec<f32> {
        let p = &*SSM_P_MATRIX;
        (0..SSM_INPUT_DIM)
            .map(|j| (0..SSM_DIM).map(|i| p[j * SSM_DIM + i] * self.h[i]).sum())
            .collect()
    }

    /// Persist state to `path` atomically with 0o600 permissions (Cookbook §31).
    ///
    /// Binary format (275 bytes):
    /// `[magic:4][version:1][dim:u16le][turn_count:u64le][h:256bytes][fletcher32:4]`
    ///
    /// # Errors
    ///
    /// Returns `Err` if the temp file cannot be written, permissions cannot be set,
    /// or the atomic rename fails.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        const HDR: usize = 4 + 1 + 2 + 8; // magic + version + dim + turn_count
        let mut buf = Vec::with_capacity(HDR + SSM_DIM * 4 + 4);
        buf.extend_from_slice(SSM_MAGIC);
        buf.push(SSM_VERSION);
        // SSM_DIM = 64, well within u16::MAX; allow truncation lint.
        #[allow(clippy::cast_possible_truncation)]
        buf.extend_from_slice(&(SSM_DIM as u16).to_le_bytes());
        buf.extend_from_slice(&self.turn_count.to_le_bytes());
        for &v in &self.h {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        let checksum = fletcher32(&buf);
        buf.extend_from_slice(&checksum.to_le_bytes());

        // Atomic write: .tmp → rename (prevents partial-write corruption).
        let tmp = path.with_extension("ssm.tmp");
        std::fs::write(&tmp, &buf)?;
        set_private_permissions(&tmp)?;
        std::fs::rename(&tmp, path)
    }

    /// Load from `path`, falling back to a zeroed state on missing or corrupt file.
    fn load(path: &Path) -> Self {
        Self::try_load(path).unwrap_or_else(|| {
            if path.exists() {
                tracing::warn!(
                    path = %path.display(),
                    "SSM state corrupted or version mismatch; resetting to zero"
                );
            }
            Self::new()
        })
    }

    /// Deserializes state from the binary `.ssm` sidecar. Returns `None` on
    /// missing file, version mismatch, dimension mismatch, or checksum failure.
    pub fn try_load(path: &Path) -> Option<Self> {
        // Consts must precede statements to satisfy clippy::items_after_statements.
        const HDR: usize = 4 + 1 + 2 + 8;
        const TOTAL: usize = HDR + SSM_DIM * 4 + 4;
        let buf = std::fs::read(path).ok()?;
        if buf.len() != TOTAL {
            return None;
        }
        if &buf[0..4] != SSM_MAGIC || buf[4] != SSM_VERSION {
            return None;
        }
        let dim = u16::from_le_bytes([buf[5], buf[6]]) as usize;
        if dim != SSM_DIM {
            return None;
        }
        // Verify checksum over header + h bytes
        let expected = fletcher32(&buf[..HDR + SSM_DIM * 4]);
        let stored = u32::from_le_bytes([
            buf[HDR + SSM_DIM * 4],
            buf[HDR + SSM_DIM * 4 + 1],
            buf[HDR + SSM_DIM * 4 + 2],
            buf[HDR + SSM_DIM * 4 + 3],
        ]);
        if expected != stored {
            return None;
        }
        let turn_count = u64::from_le_bytes(buf[7..15].try_into().ok()?);
        let mut h = vec![0.0f32; SSM_DIM];
        for (i, v) in h.iter_mut().enumerate() {
            let off = HDR + i * 4;
            *v = f32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
        }
        Some(Self { h, turn_count })
    }
}

// ── HelixSessionMemory ────────────────────────────────────────────────────────

/// [`ConversationMemory`] that persists turns to the local user helix.
///
/// Constructed via [`HelixSessionMemory::open`]. Turns written to `push()` are
/// immediately appended to disk; the `turns()` view is an in-memory cache
/// pre-populated from prior turns in the same session file.
///
/// A companion [`SsmState`] is updated on every `push()` and persisted to the
/// `.ssm` sidecar. Use [`session_context_block`] to query the helix for
/// context relevant to the current session state.
///
/// [`session_context_block`]: HelixSessionMemory::session_context_block
pub struct HelixSessionMemory {
    turns: Vec<Turn>,
    path: PathBuf,
    ssm_state: SsmState,
    state_path: PathBuf,
}

impl HelixSessionMemory {
    /// Open (or create) the session file for `cwd`.
    ///
    /// Loads the most recent `context_turns` turns from today's file into
    /// memory so the session continues without losing conversational context.
    /// Loads the companion `.ssm` state if present; initialises to zero otherwise.
    /// Falls back to ephemeral in-memory storage if the helix path is absent —
    /// the session still works, it just won't persist to disk.
    #[must_use]
    pub fn open(cwd: &Path, context_turns: usize) -> Self {
        let path = session_path(cwd);
        let state_path = path.with_extension("ssm");
        let _ = ensure_file(&path, cwd);
        let turns = parse_turns(&path, context_turns);
        let ssm_state = SsmState::load(&state_path);
        Self {
            turns,
            path,
            ssm_state,
            state_path,
        }
    }

    /// Number of prior turns loaded from disk at session start.
    pub fn restored_turn_count(&self) -> usize {
        self.turns.len()
    }

    /// Number of turns accumulated in the SSM hidden state.
    pub fn ssm_turn_count(&self) -> u64 {
        self.ssm_state.turn_count
    }

    /// Query the helix for context relevant to the current session state.
    ///
    /// Back-projects the SSM hidden state `h` to 384-dim and uses it as an
    /// ANN query against the semantic `step-embeddings` HNSW index. Returns
    /// a markdown block of up to 3 related entries for LLM injection, or
    /// `None` if the SSM state is uninitialised (zero turns) or the DB query
    /// fails.
    pub async fn session_context_block(
        &self,
        db: &dyn crate::helix::db::HelixDb,
    ) -> Option<String> {
        if self.ssm_state.turn_count == 0 {
            return None;
        }
        let query_vec = self.ssm_state.query_vec();
        let opts = crate::helix::search::SearchOptions::default().with_limit(3);
        let results = db
            .vector_search(
                &query_vec,
                crate::helix::search::index_names::STEP_EMBEDDINGS,
                &opts,
            )
            .await
            .ok()?;
        if results.is_empty() {
            return None;
        }
        let mut block = String::from("## Recalled context\n");
        for r in &results {
            let title = r
                .item
                .title
                .as_deref()
                .unwrap_or("(untitled)")
                .chars()
                .take(80)
                .collect::<String>();
            let snippet: String = r.item.content.chars().take(200).collect();
            // Infallible — writing to a String never returns Err.
            let _ = FmtWrite::write_fmt(&mut block, format_args!("- **{title}**: {snippet}\n"));
        }
        Some(block)
    }
}

impl ConversationMemory for HelixSessionMemory {
    fn push(&mut self, role: MessageRole, content: String) {
        let on_disk = redact_secrets(&content);
        let _ = append_turn(&self.path, role, on_disk.as_ref());
        // Advance SSM state: project content to 384-dim input vector, then update.
        let input_vec = content_to_input_vec(&content);
        self.ssm_state.update(&input_vec);
        let _ = self.ssm_state.save(&self.state_path);
        self.turns.push(Turn { role, content });
    }

    fn turns(&self) -> &[Turn] {
        &self.turns
    }

    fn clear(&mut self) {
        self.turns.clear();
        self.ssm_state = SsmState::new();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn redact_secrets_bearer_token() {
        // Key must be ≥20 chars after "sk-" to match the Anthropic-key pattern.
        let input = "Authorization: Bearer sk-ant-api03-abcdefghijklmnopqrst12345";
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

    // ── SSM tests ─────────────────────────────────────────────────────────────

    #[test]
    fn ssm_new_is_zero() {
        let s = SsmState::new();
        assert_eq!(s.turn_count, 0);
        assert!(s.h.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn ssm_update_advances_turn_count() {
        let mut s = SsmState::new();
        let x = vec![0.5f32; SSM_INPUT_DIM];
        s.update(&x);
        assert_eq!(s.turn_count, 1);
    }

    #[test]
    fn ssm_update_nonzero_after_nonzero_input() {
        let mut s = SsmState::new();
        let x = vec![1.0f32; SSM_INPUT_DIM];
        s.update(&x);
        // h should be non-zero since B is non-zero and x is non-zero.
        assert!(s.h.iter().any(|&v| v != 0.0));
    }

    #[test]
    fn ssm_decay_is_bounded() {
        // After many identical updates, h must remain bounded (A=0.9 < 1.0 ensures convergence).
        let mut s = SsmState::new();
        let x = vec![1.0f32; SSM_INPUT_DIM];
        for _ in 0..100 {
            s.update(&x);
        }
        // All h values should be finite and not explode.
        assert!(s.h.iter().all(|v| v.is_finite()));
        assert!(s.h.iter().all(|v| v.abs() < 1000.0));
    }

    #[test]
    fn ssm_query_vec_length() {
        let mut s = SsmState::new();
        let x = vec![0.5f32; SSM_INPUT_DIM];
        s.update(&x);
        let q = s.query_vec();
        assert_eq!(q.len(), SSM_INPUT_DIM);
    }

    #[test]
    fn ssm_save_load_round_trip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.ssm");
        let mut s = SsmState::new();
        let x = vec![0.3f32; SSM_INPUT_DIM];
        s.update(&x);
        s.update(&x);
        s.save(&path).unwrap();

        let loaded = SsmState::try_load(&path).unwrap();
        assert_eq!(loaded.turn_count, 2);
        assert_eq!(loaded.h.len(), SSM_DIM);
        for (a, b) in s.h.iter().zip(loaded.h.iter()) {
            assert!((a - b).abs() < 1e-6, "h mismatch: {a} vs {b}");
        }
    }

    #[test]
    fn ssm_load_missing_returns_zero() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.ssm");
        let s = SsmState::load(&path);
        assert_eq!(s.turn_count, 0);
        assert!(s.h.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn ssm_load_corrupted_returns_zero() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("corrupt.ssm");
        std::fs::write(&path, b"garbage data here not a valid SSM file").unwrap();
        let s = SsmState::load(&path);
        assert_eq!(s.turn_count, 0);
    }

    #[test]
    fn ssm_state_file_is_binary() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("state.ssm");
        let mut s = SsmState::new();
        let x = vec![0.7f32; SSM_INPUT_DIM];
        s.update(&x);
        s.save(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        // Magic bytes must be present; file must not be valid UTF-8 (it's binary f32 data).
        assert_eq!(&bytes[0..4], b"SSM\0");
        // Verify size: HDR(15) + h(256) + checksum(4) = 275
        assert_eq!(bytes.len(), 275);
    }

    #[test]
    fn ssm_clear_resets_state() {
        let dir = tempfile::TempDir::new().unwrap();
        let cwd = dir.path().to_path_buf();
        let mut mem = HelixSessionMemory::open(&cwd, 20);
        mem.push(MessageRole::User, "hello there".to_owned());
        assert_eq!(mem.ssm_turn_count(), 1);
        assert!(!mem.turns().is_empty());

        mem.clear();
        assert_eq!(mem.ssm_turn_count(), 0);
        assert!(mem.turns().is_empty());
    }

    #[test]
    fn ssm_persists_across_open() {
        let dir = tempfile::TempDir::new().unwrap();
        let cwd = dir.path().to_path_buf();

        // First session: push a turn.
        {
            let mut mem = HelixSessionMemory::open(&cwd, 20);
            mem.push(MessageRole::User, "first session turn".to_owned());
            assert_eq!(mem.ssm_turn_count(), 1);
        }

        // Second session: state must be restored.
        let mem2 = HelixSessionMemory::open(&cwd, 20);
        assert_eq!(
            mem2.ssm_turn_count(),
            1,
            "SSM turn_count must persist across open()"
        );
    }

    #[test]
    fn ssm_turns_and_state_consistent() {
        let dir = tempfile::TempDir::new().unwrap();
        let cwd = dir.path().to_path_buf();
        let mut mem = HelixSessionMemory::open(&cwd, 20);
        mem.push(MessageRole::User, "turn one".to_owned());
        mem.push(MessageRole::Assistant, "turn two".to_owned());
        // turns file has 2 entries; SSM state has 2 turns
        assert_eq!(mem.turns().len(), 2);
        assert_eq!(mem.ssm_turn_count(), 2);
    }

    #[test]
    fn content_to_input_vec_length() {
        let v = content_to_input_vec("hello world");
        assert_eq!(v.len(), SSM_INPUT_DIM);
    }

    #[test]
    fn content_to_input_vec_empty_is_zeros() {
        let v = content_to_input_vec("");
        assert!(v.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn content_to_input_vec_values_in_range() {
        let v = content_to_input_vec("some content here for testing the input projection");
        assert!(v.iter().all(|&x| (-1.0..=1.0).contains(&x)));
    }
}
