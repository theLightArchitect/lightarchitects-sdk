//! `ClaudeJsonlTailer` — JSONL file tailer that watches for Agent tool calls.
//!
//! # Security boundary
//!
//! Only the fields declared in [`AgentToolInput`] are ever read from the JSONL.
//! The `prompt` field (and all other undeclared fields) are silently discarded
//! via serde's default behaviour (no `deny_unknown_fields` — SCR1-F1: forward
//! compat with new Claude Code JSONL schema versions).
//!
//! # Path validation
//!
//! [`find_jsonl_for_session`] validates that the resolved path is a descendant
//! of `$HOME/.claude/projects/` before returning it, preventing path-traversal
//! attacks where a crafted `session_id` could escape the projects directory.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, AsyncSeekExt, BufReader, SeekFrom},
    sync::watch,
    task::JoinHandle,
    time::{Duration, sleep},
};
use tracing::{debug, warn};

use super::{
    error::FleetError,
    span::{AgentWaveContext, ExitPath},
    tracker::FleetTracker,
};

/// Poll interval for reading new JSONL lines.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

// ── JSONL record shapes ───────────────────────────────────────────────────────

/// Top-level JSONL record — we only care about `type` and `message`.
#[derive(Deserialize, Default)]
struct JsonlRecord {
    #[serde(rename = "type", default)]
    record_type: String,
    #[serde(default)]
    message: Option<MessageRecord>,
    /// Tool result records carry these fields at the top level.
    #[serde(default)]
    tool_use_id: Option<String>,
}

#[derive(Deserialize, Default)]
struct MessageRecord {
    #[serde(default)]
    role: String,
    #[serde(default)]
    content: Vec<ContentBlock>,
}

#[derive(Deserialize, Default)]
struct ContentBlock {
    #[serde(rename = "type", default)]
    block_type: String,
    /// Only present on `tool_use` blocks.
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    id: Option<String>,
    /// Raw JSON input — deserialized lazily into `AgentToolInput`.
    #[serde(default)]
    input: Option<serde_json::Value>,
}

/// Allowlisted fields from the Agent tool `input` object.
///
/// All other fields (including `prompt`) are silently ignored — `deny_unknown_fields`
/// is intentionally absent for forward compatibility (SCR1-F1).
#[derive(Deserialize, Default)]
pub(super) struct AgentToolInput {
    pub description: Option<String>,
    pub subagent_type: Option<String>,
    pub run_in_background: Option<bool>,
    /// `isolation` tag — retained for future worktree-path resolution (V2+).
    #[allow(dead_code)]
    pub isolation: Option<String>,
    /// Wave/task focus block emitted by `/BUILD` wave-dispatcher (B1 — Path B
    /// Phase 1). When present, triggers an immediate `agent_focused_on` call
    /// on the tracker so the spawn-then-focus pair lands atomically.
    ///
    /// Uses a tolerant deserializer: a malformed `wave_context` block (wrong
    /// type, garbage payload) yields `None` rather than failing the entire
    /// input. A buggy producer must never lose the agent spawn itself.
    #[serde(default, deserialize_with = "deserialize_wave_context_or_none")]
    pub wave_context: Option<WaveContextInput>,
}

/// Tolerant deserializer for [`AgentToolInput::wave_context`]. Returns `None`
/// for any input that can't be parsed as a `WaveContextInput` — including
/// strings, numbers, nulls, and structurally-wrong objects.
fn deserialize_wave_context_or_none<'de, D>(
    deserializer: D,
) -> Result<Option<WaveContextInput>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(v.and_then(|raw| serde_json::from_value(raw).ok()))
}

/// Deserializable mirror of [`AgentWaveContext`]. All fields are optional so a
/// partial context (e.g., just `wave_id`, no `task_id` yet) is honored verbatim.
#[derive(Deserialize, Default)]
pub(super) struct WaveContextInput {
    pub build_codename: Option<String>,
    pub wave_id: Option<String>,
    pub task_id: Option<String>,
    pub focus_target_fn: Option<String>,
}

impl From<WaveContextInput> for AgentWaveContext {
    fn from(i: WaveContextInput) -> Self {
        Self {
            build_codename: i.build_codename,
            wave_id: i.wave_id,
            task_id: i.task_id,
            focus_target_fn: i.focus_target_fn,
        }
    }
}

// ── Path resolution ───────────────────────────────────────────────────────────

/// Locate the JSONL file for a given Claude Code session ID.
///
/// Scans `~/.claude/projects/<hash>/<session_id>.jsonl` (one directory level).
/// Returns `None` if no file is found — callers should poll with exponential
/// backoff on cold-start (the file may not exist until the session emits its
/// first record).
///
/// # Security
///
/// The resolved path is validated to start with `$HOME/.claude/projects/` to
/// prevent path-traversal via a crafted `session_id`.
///
/// # Errors
///
/// Returns [`FleetError::Io`] if reading the projects directory fails.
pub fn find_jsonl_for_session(session_id: &str) -> Result<Option<PathBuf>, FleetError> {
    let home = dirs::home_dir().ok_or_else(|| FleetError::Io(home_dir_err()))?;
    let projects_dir = home.join(".claude/projects");
    let safe_prefix = projects_dir
        .canonicalize()
        .unwrap_or_else(|_| projects_dir.clone());

    // Walk exactly one level: projects_dir/<hash>/
    let entries = std::fs::read_dir(&projects_dir)?;
    for entry in entries.flatten() {
        let hash_dir = entry.path();
        if !hash_dir.is_dir() {
            continue;
        }
        let candidate = hash_dir.join(format!("{session_id}.jsonl"));
        if !candidate.exists() {
            continue;
        }
        // Security: confirm candidate is a descendant of the safe prefix.
        match candidate.canonicalize() {
            Ok(canonical) if canonical.starts_with(&safe_prefix) => {
                return Ok(Some(canonical));
            }
            Ok(canonical) => {
                warn!(
                    "path traversal blocked: {} is outside projects dir",
                    canonical.display()
                );
                return Ok(None);
            }
            Err(e) => {
                warn!("could not canonicalize candidate path: {e}");
            }
        }
    }
    Ok(None)
}

/// Build a synthetic `io::Error` for a missing home directory.
fn home_dir_err() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotFound, "HOME directory not found")
}

// ── Tailer ────────────────────────────────────────────────────────────────────

/// Tails a Claude Code JSONL session file and feeds agent spawn/completion
/// events into a [`FleetTracker`].
///
/// Spawns a background tokio task that polls the file every 250 ms.
/// Drop to stop tailing (the inner task is aborted via a `watch` channel).
pub struct ClaudeJsonlTailer {
    _stop_tx: watch::Sender<()>,
    handle: Option<JoinHandle<()>>,
}

impl ClaudeJsonlTailer {
    /// Start tailing `path`, feeding events into `tracker`.
    ///
    /// # Errors
    ///
    /// Returns [`FleetError::NotFound`] if `path` does not exist.
    /// Returns [`FleetError::Io`] on open failure.
    #[allow(clippy::unused_async)] // path.exists() is sync; async kept for consistent caller API
    pub async fn start(path: PathBuf, tracker: FleetTracker) -> Result<Self, FleetError> {
        if !path.exists() {
            return Err(FleetError::NotFound {
                path: path.display().to_string(),
            });
        }

        let (stop_tx, stop_rx) = watch::channel(());
        let handle = tokio::spawn(tail_loop(path, tracker, stop_rx));

        Ok(Self {
            _stop_tx: stop_tx,
            handle: Some(handle),
        })
    }
}

impl Drop for ClaudeJsonlTailer {
    /// Abort the background task.  The `watch::Sender` drop signals the loop.
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

// ── Tail loop (private) ───────────────────────────────────────────────────────

/// Background task: seek to EOF on open, then poll for new lines every 250 ms.
async fn tail_loop(path: PathBuf, tracker: FleetTracker, mut stop: watch::Receiver<()>) {
    match open_at_eof(&path).await {
        Ok(reader) => run_tail(reader, &path, tracker, &mut stop).await,
        Err(e) => warn!("ClaudeJsonlTailer: failed to open {:?}: {e}", path),
    }
}

/// Open the file and seek to its current end so we only process new records.
async fn open_at_eof(path: &Path) -> Result<BufReader<File>, std::io::Error> {
    let mut file = File::open(path).await?;
    file.seek(SeekFrom::End(0)).await?;
    Ok(BufReader::new(file))
}

/// Inner poll loop — reads lines, dispatches events, stops on signal.
async fn run_tail(
    mut reader: BufReader<File>,
    path: &Path,
    tracker: FleetTracker,
    stop: &mut watch::Receiver<()>,
) {
    let mut line = String::new();
    loop {
        tokio::select! {
            _ = stop.changed() => {
                debug!("ClaudeJsonlTailer: stop signal received for {:?}", path);
                return;
            }
            () = sleep(POLL_INTERVAL) => {
                drain_new_lines(&mut reader, &tracker, &mut line).await;
            }
        }
    }
}

/// Read all available new lines from the reader, dispatching fleet events.
async fn drain_new_lines(reader: &mut BufReader<File>, tracker: &FleetTracker, buf: &mut String) {
    loop {
        buf.clear();
        match reader.read_line(buf).await {
            Ok(0) => return, // EOF — no new data yet
            Ok(_) => process_line(buf.trim_end(), tracker).await,
            Err(e) => {
                warn!("ClaudeJsonlTailer: read error: {e}");
                return;
            }
        }
    }
}

/// Parse one JSONL line and dispatch spawn/completion events.
async fn process_line(line: &str, tracker: &FleetTracker) {
    if line.is_empty() {
        return;
    }
    let record: JsonlRecord = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            debug!("ClaudeJsonlTailer: skipping unparseable line: {e}");
            return;
        }
    };

    match record.record_type.as_str() {
        "assistant" => handle_assistant_record(record, tracker).await,
        "tool_result" => handle_tool_result_record(record, tracker).await,
        _ => {}
    }
}

/// Handle an `assistant` record: look for Agent `tool_use` blocks.
async fn handle_assistant_record(record: JsonlRecord, tracker: &FleetTracker) {
    let message = match record.message {
        Some(m) if m.role == "assistant" => m,
        _ => return,
    };

    for block in &message.content {
        if block.block_type != "tool_use" {
            continue;
        }
        let Some(ref name) = block.name else { continue };
        if name != "Agent" {
            continue;
        }
        let Some(ref id) = block.id else { continue };

        let input = extract_agent_input(block.input.as_ref());
        let description = input.description.unwrap_or_default();
        let agent_type = input.subagent_type.unwrap_or_else(|| "unknown".into());
        let run_in_background = input.run_in_background.unwrap_or(false);
        let wave_context = input.wave_context;

        tracker
            .agent_spawned(id.clone(), agent_type, description, run_in_background)
            .await;

        // B1: if the /BUILD wave-dispatcher tagged this spawn with a
        // wave_context block, propagate it as a focus update immediately.
        if let Some(ctx) = wave_context {
            tracker.agent_focused_on(id, ctx.into());
        }
    }
}

/// Handle a `tool_result` record: mark the corresponding agent complete.
async fn handle_tool_result_record(record: JsonlRecord, tracker: &FleetTracker) {
    let Some(tool_use_id) = record.tool_use_id else {
        return;
    };
    // V1: all completions from tool_result are treated as Completed.
    // Error paths (non-zero exit) are indistinguishable in V1 — future work.
    tracker
        .agent_completed(&tool_use_id, ExitPath::Completed)
        .await;
}

/// Deserialize `AgentToolInput` from the raw `input` JSON value, if present.
fn extract_agent_input(input: Option<&serde_json::Value>) -> AgentToolInput {
    input
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_tracker() -> FleetTracker {
        FleetTracker::new()
    }

    // ── Path resolution ───────────────────────────────────────────────────────

    #[test]
    fn find_jsonl_returns_none_for_unknown_session() {
        // A UUID that does not exist on disk.
        let result = find_jsonl_for_session("00000000-0000-0000-0000-000000000000");
        // Either Ok(None) or Ok(Some(...)) if a matching file exists; never Err.
        assert!(result.is_ok());
        // On a clean test machine this should be None.
        // (We can't assert None definitively without sandbox control.)
    }

    // ── AgentToolInput deserialization ────────────────────────────────────────

    #[test]
    #[allow(clippy::expect_used)]
    fn agent_tool_input_ignores_unknown_fields() {
        let json = r#"{
            "description": "do the thing",
            "subagent_type": "engineer",
            "run_in_background": true,
            "isolation": "worktree",
            "prompt": "SENSITIVE — must be ignored",
            "future_field_v2": 42
        }"#;
        let input: AgentToolInput = serde_json::from_str(json).expect("deserialize");
        assert_eq!(input.description.as_deref(), Some("do the thing"));
        assert_eq!(input.subagent_type.as_deref(), Some("engineer"));
        assert_eq!(input.run_in_background, Some(true));
        assert_eq!(input.isolation.as_deref(), Some("worktree"));
        // No field for `prompt` — it was silently dropped.
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn agent_tool_input_defaults_on_missing_fields() {
        let input: AgentToolInput = serde_json::from_str("{}").expect("deserialize");
        assert!(input.description.is_none());
        assert!(input.subagent_type.is_none());
        assert!(input.run_in_background.is_none());
        assert!(input.isolation.is_none());
    }

    // ── process_line ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn process_assistant_spawns_agent() {
        let tracker = make_tracker();
        let line = r#"{
            "type": "assistant",
            "message": {
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "name": "Agent",
                    "id": "tu_abc123",
                    "input": {
                        "description": "analyze code",
                        "subagent_type": "quality",
                        "run_in_background": false
                    }
                }]
            }
        }"#;
        process_line(line, &tracker).await;

        let snap = tracker.snapshot();
        assert_eq!(snap.nodes.len(), 1);
        assert_eq!(snap.nodes[0].agent_id, "tu_abc123");
        assert_eq!(snap.nodes[0].agent_type, "quality");
        assert_eq!(snap.nodes[0].description, "analyze code");
    }

    #[tokio::test]
    async fn process_tool_result_completes_agent() {
        let tracker = make_tracker();
        // Spawn first.
        tracker
            .agent_spawned("tu_xyz".into(), "eng".into(), "task".into(), false)
            .await;

        let line = r#"{"type": "tool_result", "tool_use_id": "tu_xyz"}"#;
        process_line(line, &tracker).await;

        let snap = tracker.snapshot();
        assert_eq!(
            snap.nodes[0].status,
            super::super::span::FleetStatus::Completed
        );
    }

    #[tokio::test]
    async fn unknown_record_type_is_ignored() {
        let tracker = make_tracker();
        process_line(r#"{"type": "human", "message": {}}"#, &tracker).await;
        assert_eq!(tracker.snapshot().nodes.len(), 0);
    }

    #[tokio::test]
    async fn non_agent_tool_use_is_ignored() {
        let tracker = make_tracker();
        let line = r#"{
            "type": "assistant",
            "message": {
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "name": "Read",
                    "id": "tu_read1",
                    "input": {"file_path": "/tmp/foo"}
                }]
            }
        }"#;
        process_line(line, &tracker).await;
        assert_eq!(tracker.snapshot().nodes.len(), 0);
    }

    #[tokio::test]
    async fn malformed_json_is_skipped_gracefully() {
        let tracker = make_tracker();
        process_line("this is not json {{{", &tracker).await;
        assert_eq!(tracker.snapshot().nodes.len(), 0);
    }

    // ── B1 — wave_context propagation ────────────────────────────────────────

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn process_assistant_propagates_wave_context_when_present() {
        let tracker = make_tracker();
        let line = r#"{
            "type": "assistant",
            "message": {
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "name": "Agent",
                    "id": "corso-w3.2-test-001",
                    "input": {
                        "description": "cover handle_message branches",
                        "subagent_type": "engineer",
                        "wave_context": {
                            "build_codename": "webshell-copilot-providers",
                            "wave_id": "w3.2",
                            "task_id": "t3.2.1",
                            "focus_target_fn": "fn handle_message"
                        }
                    }
                }]
            }
        }"#;
        process_line(line, &tracker).await;

        let snap = tracker.snapshot();
        let node = snap
            .nodes
            .iter()
            .find(|n| n.agent_id == "corso-w3.2-test-001")
            .unwrap();
        assert_eq!(
            node.build_codename.as_deref(),
            Some("webshell-copilot-providers")
        );
        assert_eq!(node.wave_id.as_deref(), Some("w3.2"));
        assert_eq!(node.task_id.as_deref(), Some("t3.2.1"));
        assert_eq!(node.focus_target_fn.as_deref(), Some("fn handle_message"));
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn process_assistant_without_wave_context_leaves_focus_unset() {
        // Backwards compat: ad-hoc Agent calls (no /BUILD pipeline) carry no
        // wave_context, and FleetNode focus fields must remain `None`.
        let tracker = make_tracker();
        let line = r#"{
            "type": "assistant",
            "message": {
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "name": "Agent",
                    "id": "adhoc-001",
                    "input": {
                        "description": "ad-hoc research",
                        "subagent_type": "researcher"
                    }
                }]
            }
        }"#;
        process_line(line, &tracker).await;

        let snap = tracker.snapshot();
        let node = snap
            .nodes
            .iter()
            .find(|n| n.agent_id == "adhoc-001")
            .unwrap();
        assert!(node.build_codename.is_none());
        assert!(node.wave_id.is_none());
        assert!(node.task_id.is_none());
        assert!(node.focus_target_fn.is_none());
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn wave_context_with_partial_fields_propagates_what_it_has() {
        // F-3 / B1: agent might be wave-bound but not yet task-focused.
        let tracker = make_tracker();
        let line = r#"{
            "type": "assistant",
            "message": {
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "name": "Agent",
                    "id": "wave-bound-001",
                    "input": {
                        "description": "wave init",
                        "subagent_type": "engineer",
                        "wave_context": {
                            "build_codename": "test-build",
                            "wave_id": "w1"
                        }
                    }
                }]
            }
        }"#;
        process_line(line, &tracker).await;

        let snap = tracker.snapshot();
        let node = snap
            .nodes
            .iter()
            .find(|n| n.agent_id == "wave-bound-001")
            .unwrap();
        assert_eq!(node.build_codename.as_deref(), Some("test-build"));
        assert_eq!(node.wave_id.as_deref(), Some("w1"));
        assert!(node.task_id.is_none());
        assert!(node.focus_target_fn.is_none());
    }

    #[tokio::test]
    #[allow(clippy::expect_used)]
    async fn wave_context_malformed_does_not_block_spawn() {
        // Robustness: if the wave_context block is malformed JSON, spawn must
        // still succeed and the focus fields stay None — never lose the agent.
        let input_json: serde_json::Value = serde_json::from_str(
            r#"{"description":"x","subagent_type":"engineer","wave_context":"not an object"}"#,
        )
        .expect("input_json parses");
        let input = extract_agent_input(Some(&input_json));
        // The serde Option<WaveContextInput> deserializer returns None on type
        // mismatch (since we don't deny_unknown_fields), so wave_context is None.
        assert!(input.wave_context.is_none());
        assert_eq!(input.description.as_deref(), Some("x"));
    }

    // ── File tailer integration ───────────────────────────────────────────────

    #[tokio::test]
    async fn tailer_returns_not_found_for_missing_file() {
        let tracker = make_tracker();
        let result = ClaudeJsonlTailer::start(
            PathBuf::from("/tmp/this_file_does_not_exist_atf.jsonl"),
            tracker,
        )
        .await;
        assert!(matches!(result, Err(FleetError::NotFound { .. })));
    }

    #[tokio::test]
    #[allow(clippy::expect_used)]
    async fn tailer_drop_aborts_task() {
        // Create a real temp file so start() succeeds.
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        writeln!(tmp, "{{\"type\":\"system\"}}").expect("write");
        let path = tmp.path().to_path_buf();
        let tracker = make_tracker();
        let tailer = ClaudeJsonlTailer::start(path, tracker)
            .await
            .expect("start");
        // Drop must not panic.
        drop(tailer);
    }
}
