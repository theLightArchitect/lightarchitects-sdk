//! AYIN trace → SFT exporter.
//!
//! Reconstructs `ChatML` SFT training examples from two AYIN data sources:
//!
//! **Source A — Conversation traces** (`~/lightarchitects/soul/helix/ayin/conversations/*.jsonl`)
//!
//! Each JSONL file contains a chronological stream of typed events for every
//! tool call Claude made during that session.  Events are grouped by
//! `request_id` to reconstruct individual user-request → assistant-action
//! cycles.  When `pivot` events are present their `reason` text is interleaved
//! into the assistant turn, teaching the model error-recovery reasoning.
//!
//! **Source B — Sibling tool traces** (`~/lightarchitects/soul/helix/ayin/traces/{sibling}/{date}/*.json`)
//!
//! Each JSON file is one MCP tool invocation.  A synthetic user message is
//! constructed from the action name and metadata; the assistant turn describes
//! the outcome.  Only successful traces are included by default.
//!
//! Both sources emit `ChatML` format:
//! `[{"role":"system",...}, {"role":"user",...}, {"role":"assistant",...}]`

#![allow(clippy::cast_precision_loss)] // Counts are small; f64 precision is fine.

use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::arena::export::ExportError;

// ── Output format ────────────────────────────────────────────────────────────

/// A single message in the output `ChatML` conversation.
#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// One SFT training example.
#[derive(Debug, Serialize)]
struct AyinSftExample {
    conversations: Vec<ChatMessage>,
    /// Originating trace source for provenance.
    #[serde(skip_serializing_if = "str::is_empty")]
    source: String,
}

// ── Export configuration ─────────────────────────────────────────────────────

/// Configuration for the AYIN → SFT export pipeline.
#[derive(Debug, Clone)]
pub struct AyinExportConfig {
    /// `~/lightarchitects/soul/helix/ayin/conversations/` — JSONL conversation files.
    pub conversations_dir: PathBuf,
    /// `~/lightarchitects/soul/helix/ayin/traces/` — per-sibling JSON trace files.
    pub sibling_traces_dir: PathBuf,
    /// Output `.jsonl` path.
    pub output_path: PathBuf,
    /// If `true`, skip sibling traces whose `outcome.type` is `"Error"`.
    pub skip_error_traces: bool,
    /// Minimum tool calls required for a conversation example to be included.
    pub min_tool_calls: usize,
    /// If `true`, interleave pivot `reason` text into assistant turns.
    /// This teaches error-recovery reasoning.  Defaults to `true`.
    pub include_pivots: bool,
    /// System message injected into every example.
    pub system_message: String,
}

impl Default for AyinExportConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        Self {
            conversations_dir: PathBuf::from(format!("{home}/.soul/helix/ayin/conversations")),
            sibling_traces_dir: PathBuf::from(format!("{home}/.soul/helix/ayin/traces")),
            output_path: PathBuf::from("ayin_sft.jsonl"),
            skip_error_traces: true,
            min_tool_calls: 2,
            include_pivots: true,
            system_message: DEFAULT_SYSTEM_MESSAGE.to_string(),
        }
    }
}

const DEFAULT_SYSTEM_MESSAGE: &str = "\
You are an AI software engineering assistant with access to tools including \
file operations (Read, Write, Edit, Glob, Grep), shell execution (Bash), and \
sibling MCP servers (CORSO, EVA, SOUL, QUANTUM, SERAPH, AYIN). \
Use them methodically to complete the user's request. \
When an approach fails, reason about why and correct course.";

// ── Public entry point ────────────────────────────────────────────────────────

/// Export AYIN traces as SFT training data (JSONL).
///
/// Returns the number of examples written.
///
/// # Errors
///
/// Returns [`ExportError`] if output file I/O or JSON serialisation fails.
pub fn export_ayin_sft(config: &AyinExportConfig) -> Result<usize, ExportError> {
    let mut examples: Vec<AyinSftExample> = Vec::new();

    // Source A: conversation traces.
    examples.extend(load_conversation_examples(config));

    // Source B: sibling tool traces.
    examples.extend(load_sibling_examples(config));

    // Write JSONL.
    let file = std::fs::File::create(&config.output_path)?;
    let mut writer = BufWriter::new(file);
    let count = examples.len();

    for ex in examples {
        serde_json::to_writer(&mut writer, &ex)?;
        writer.write_all(b"\n")?;
    }

    writer.flush()?;
    Ok(count)
}

// ── Source A: conversation JSONL ─────────────────────────────────────────────

/// Load examples from all `conversations/*.jsonl` files.
fn load_conversation_examples(config: &AyinExportConfig) -> Vec<AyinSftExample> {
    let dir = &config.conversations_dir;
    if !dir.exists() {
        return Vec::new();
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut examples = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }

        examples.extend(examples_from_jsonl(&path, config));
    }

    examples
}

/// Parse one conversation JSONL file and produce SFT examples.
fn examples_from_jsonl(path: &Path, config: &AyinExportConfig) -> Vec<AyinSftExample> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    // Bucket all events by request_id.
    let mut by_request: HashMap<String, RequestBucket> = HashMap::new();

    for line in content.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(v): Result<Value, _> = serde_json::from_str(line) else {
            continue;
        };
        let Some(req_id) = v
            .get("request_id")
            .and_then(|x| x.as_str())
            .map(String::from)
        else {
            continue;
        };
        let bucket = by_request
            .entry(req_id.clone())
            .or_insert_with(|| RequestBucket {
                request_id: req_id,
                ..RequestBucket::default()
            });
        dispatch_event(bucket, &v);
    }

    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    by_request
        .into_values()
        .filter_map(|bucket| bucket_to_example(bucket, config, file_stem))
        .collect()
}

/// Dispatch one parsed JSON event into the correct bucket field.
fn dispatch_event(bucket: &mut RequestBucket, v: &Value) {
    match v.get("type").and_then(|t| t.as_str()) {
        Some("request") => {
            bucket.user_message = v
                .get("user_message")
                .and_then(|x| x.as_str())
                .map(String::from);
            bucket.session_id = v
                .get("session_id")
                .and_then(|x| x.as_str())
                .map(String::from);
        }
        Some("tool") => {
            let node_id = v.get("node_id").and_then(Value::as_u64).unwrap_or(u64::MAX);
            bucket.tool_events.push(ToolEvent {
                node_id,
                tool_name: v
                    .get("tool_name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                input_summary: v
                    .get("input_summary")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                success: v.get("success").and_then(Value::as_bool).unwrap_or(true),
                error: v.get("error").and_then(|x| x.as_str()).map(String::from),
            });
        }
        Some("pivot") => {
            bucket.pivots.push(PivotEvent {
                after_node: v.get("after_node").and_then(Value::as_u64).unwrap_or(0),
                reason: v
                    .get("reason")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                branch: v.get("branch").and_then(Value::as_u64).unwrap_or(0),
            });
        }
        Some("complete") => {
            bucket.total_tools = v
                .get("total_tools")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0);
            bucket.errors = v
                .get("errors")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0);
            bucket.complete = true;
        }
        _ => {}
    }
}

/// Convert a bucketed request cycle into an SFT example, or `None` if filtered.
fn bucket_to_example(
    bucket: RequestBucket,
    config: &AyinExportConfig,
    date: &str,
) -> Option<AyinSftExample> {
    // Require a user message — without it we can't form a proper example.
    let user_message = bucket.user_message?;

    // Apply minimum tool call filter.
    if bucket.tool_events.len() < config.min_tool_calls {
        return None;
    }

    // Build the assistant turn.
    let assistant_turn = build_assistant_turn(
        &bucket.tool_events,
        &bucket.pivots,
        bucket.errors,
        bucket.complete,
        config.include_pivots,
    );

    let source = format!("ayin/conversations/{date}/{}", bucket.request_id);

    Some(AyinSftExample {
        conversations: vec![
            ChatMessage {
                role: "system".into(),
                content: config.system_message.clone(),
            },
            ChatMessage {
                role: "user".into(),
                content: user_message,
            },
            ChatMessage {
                role: "assistant".into(),
                content: assistant_turn,
            },
        ],
        source,
    })
}

/// Build the assistant turn from a sorted tool sequence with optional pivot annotations.
///
/// Pivots are inserted after the tool event at `pivot.after_node`, surfacing the
/// error-recovery reasoning that made the model change approach.  This is the
/// training signal that teaches a model to debug rather than repeat failures.
fn build_assistant_turn(
    tools: &[ToolEvent],
    pivots: &[PivotEvent],
    errors: usize,
    complete: bool,
    include_pivots: bool,
) -> String {
    // Index pivots by the node_id they follow.
    let mut pivots_by_node: HashMap<u64, Vec<&PivotEvent>> = HashMap::new();
    if include_pivots {
        for p in pivots {
            pivots_by_node.entry(p.after_node).or_default().push(p);
        }
    }

    // Sort tools by node_id for chronological order.
    let mut sorted_tools: Vec<&ToolEvent> = tools.iter().collect();
    sorted_tools.sort_by_key(|t| t.node_id);

    let mut parts: Vec<String> = Vec::with_capacity(tools.len() + pivots.len() + 2);

    for tool in &sorted_tools {
        // Format tool call.
        let status = if tool.success { "✓" } else { "✗" };
        let entry = if tool.input_summary.is_empty() {
            format!("[{status} {}]", tool.tool_name)
        } else {
            format!("[{status} {} — {}]", tool.tool_name, tool.input_summary)
        };
        parts.push(entry);

        // Append error note if the tool failed.
        if let Some(ref err) = tool.error {
            parts.push(format!("  Error: {err}"));
        }

        // Inject any pivot reasoning that follows this node.
        if let Some(pivot_list) = pivots_by_node.get(&tool.node_id) {
            for pivot in pivot_list {
                parts.push(format!(
                    "\n[Pivoting → branch {}] {}",
                    pivot.branch, pivot.reason
                ));
            }
        }
    }

    // Closing summary.
    if complete {
        let outcome = if errors == 0 {
            "Task complete.".to_string()
        } else {
            format!("Task complete ({errors} error(s) encountered).")
        };
        parts.push(String::new());
        parts.push(outcome);
    }

    parts.join("\n")
}

// ── Source B: sibling trace JSON files ───────────────────────────────────────

/// Walk `traces/{sibling}/{date}/*.json` and build SFT examples.
fn load_sibling_examples(config: &AyinExportConfig) -> Vec<AyinSftExample> {
    let dir = &config.sibling_traces_dir;
    if !dir.exists() {
        return Vec::new();
    }

    // Walk: sibling/ → date/ → *.json
    let Ok(sibling_entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut examples = Vec::new();

    for sibling_entry in sibling_entries.flatten() {
        let sibling_path = sibling_entry.path();
        if !sibling_path.is_dir() {
            continue;
        }
        let sibling_name = sibling_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let Ok(date_entries) = std::fs::read_dir(&sibling_path) else {
            continue;
        };

        for date_entry in date_entries.flatten() {
            let date_path = date_entry.path();
            if !date_path.is_dir() {
                continue;
            }
            let Ok(trace_entries) = std::fs::read_dir(&date_path) else {
                continue;
            };

            for trace_entry in trace_entries.flatten() {
                let trace_path = trace_entry.path();
                if trace_path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Some(ex) = sibling_trace_to_example(&trace_path, &sibling_name, config) {
                    examples.push(ex);
                }
            }
        }
    }

    examples
}

/// Parse one sibling trace JSON and produce an SFT example.
fn sibling_trace_to_example(
    path: &Path,
    sibling: &str,
    config: &AyinExportConfig,
) -> Option<AyinSftExample> {
    let content = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&content).ok()?;

    let action = v
        .get("action")
        .and_then(|x| x.as_str())
        .unwrap_or("unknown");
    let outcome_type = v
        .get("outcome")
        .and_then(|o| o.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("Continue");

    // Filter out error traces if configured.
    if config.skip_error_traces && outcome_type == "Error" {
        return None;
    }

    // Extract metadata as a freeform value.
    let metadata = v.get("metadata");

    // Build a synthetic user message from the action context.
    let user_message = synthesize_user_message(action, sibling, metadata);

    // Build the assistant turn from the outcome.
    let assistant_turn = synthesize_assistant_turn(action, sibling, outcome_type, metadata, &v);

    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let source = format!("ayin/traces/{sibling}/{file_stem}");

    Some(AyinSftExample {
        conversations: vec![
            ChatMessage {
                role: "system".into(),
                content: config.system_message.clone(),
            },
            ChatMessage {
                role: "user".into(),
                content: user_message,
            },
            ChatMessage {
                role: "assistant".into(),
                content: assistant_turn,
            },
        ],
        source,
    })
}

/// Synthesize a plausible user request from a sibling trace action + metadata.
fn synthesize_user_message(action: &str, sibling: &str, metadata: Option<&Value>) -> String {
    let sibling_upper = sibling.to_uppercase();

    match action {
        "dialogue" => {
            let prompt = metadata
                .and_then(|m| m.get("prompt"))
                .and_then(|p| p.as_str())
                .unwrap_or("the current topic");
            format!("Have a squad dialogue about: {prompt}")
        }
        "helix" => {
            let query = extract_metadata_field(metadata, &["query", "input", "term"])
                .unwrap_or_else(|| "recent entries".into());
            format!("Query the helix for: {query}")
        }
        "write_note" | "write" => {
            let path = extract_metadata_field(metadata, &["path", "key"])
                .unwrap_or_else(|| "the vault".into());
            format!("Save this note to the vault: {path}")
        }
        "read_note" | "read_file" => {
            let path = extract_metadata_field(metadata, &["path", "key"])
                .unwrap_or_else(|| "the vault".into());
            format!("Read the vault note at: {path}")
        }
        "search" => {
            let query = extract_metadata_field(metadata, &["query", "term", "input"])
                .unwrap_or_else(|| "relevant entries".into());
            format!("Search the vault for: {query}")
        }
        "list_notes" => "List the notes in the vault.".into(),
        "guard" => {
            format!("Run {sibling_upper} security review on the current changes.")
        }
        "sniff" => {
            let task = extract_metadata_field(metadata, &["task", "prompt", "input"])
                .unwrap_or_else(|| "the required feature".into());
            format!("Generate code for: {task}")
        }
        "scout" => {
            let feature = extract_metadata_field(metadata, &["feature", "task", "input"])
                .unwrap_or_else(|| "the proposed changes".into());
            format!("Create a build plan for: {feature}")
        }
        "remember" | "enrich" => "Enrich and preserve this moment to the helix.".into(),
        "research" => {
            let topic = extract_metadata_field(metadata, &["topic", "query", "input"])
                .unwrap_or_else(|| "the current task".into());
            format!("Research: {topic}")
        }
        _ => {
            format!("Call {sibling_upper} {action} with the provided context.")
        }
    }
}

/// Extract a string value from one of several possible metadata field names.
fn extract_metadata_field(metadata: Option<&Value>, keys: &[&str]) -> Option<String> {
    let m = metadata?;

    // Metadata may be a JSON string (double-encoded) or a JSON object.
    let obj = if let Some(s) = m.as_str() {
        serde_json::from_str::<Value>(s).ok()?
    } else {
        m.clone()
    };

    for key in keys {
        if let Some(val) = obj.get(key).and_then(|v| v.as_str()) {
            return Some(val.trim().to_string());
        }
    }
    None
}

/// Synthesize the assistant's response from the trace outcome.
fn synthesize_assistant_turn(
    action: &str,
    sibling: &str,
    outcome_type: &str,
    metadata: Option<&Value>,
    span: &Value,
) -> String {
    let sibling_upper = sibling.to_uppercase();
    let duration_ms = span.get("duration_ms").and_then(Value::as_u64).unwrap_or(0);

    // Decision points give us the routing + confidence path.
    let decision_summary = span
        .get("decision_points")
        .and_then(|d| d.as_array())
        .map(|pts| {
            pts.iter()
                .filter_map(|p| {
                    let name = p.get("name").and_then(|x| x.as_str())?;
                    let decision = p.get("decision").and_then(|x| x.as_str())?;
                    let confidence = p
                        .get("confidence")
                        .and_then(Value::as_f64)
                        .map(|c| format!(" (confidence: {c:.2})"))
                        .unwrap_or_default();
                    Some(format!("  {name}: {decision}{confidence}"))
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();

    let outcome_detail = span
        .get("outcome")
        .and_then(|o| o.get("detail"))
        .and_then(|d| d.as_str())
        .unwrap_or("");

    let strand_note = span
        .get("strand_activations")
        .and_then(|s| s.as_array())
        .filter(|a| !a.is_empty())
        .map(|activations| {
            let strands: Vec<&str> = activations
                .iter()
                .filter_map(|a| a.get("strand").and_then(|s| s.as_str()))
                .collect();
            format!("Active strands: {}.", strands.join(", "))
        })
        .unwrap_or_default();

    // Build the output for dialogue-type traces (rich turn data available).
    if action == "dialogue" {
        if let Some(turns) = metadata
            .and_then(|m| {
                if let Some(s) = m.as_str() {
                    serde_json::from_str::<Value>(s).ok()
                } else {
                    Some(m.clone())
                }
            })
            .and_then(|m| m.get("turns").cloned())
            .and_then(|t| t.as_array().cloned())
        {
            let dialogue: String = turns
                .iter()
                .filter_map(|turn| {
                    let sib = turn.get("sibling").and_then(|s| s.as_str())?;
                    let text = turn.get("text").and_then(|t| t.as_str())?;
                    Some(format!("[{}] {}", sib.to_uppercase(), text))
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            if !dialogue.is_empty() {
                return dialogue;
            }
        }
    }

    // Generic outcome description.
    let mut parts: Vec<String> = Vec::new();

    parts.push(format!(
        "Called {sibling_upper} {action} ({duration_ms}ms)."
    ));

    if !decision_summary.is_empty() {
        parts.push(format!("Routing decisions:\n{decision_summary}"));
    }

    match outcome_type {
        "Continue" | "Success" => {
            parts.push("Result: success.".into());
        }
        "Error" => {
            if outcome_detail.is_empty() {
                parts.push("Result: error.".into());
            } else {
                parts.push(format!("Result: error — {outcome_detail}"));
            }
        }
        other => {
            parts.push(format!("Result: {other}."));
        }
    }

    if !strand_note.is_empty() {
        parts.push(strand_note);
    }

    parts.join("\n")
}

// ── Internal types ────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct RequestBucket {
    request_id: String,
    user_message: Option<String>,
    session_id: Option<String>,
    tool_events: Vec<ToolEvent>,
    pivots: Vec<PivotEvent>,
    total_tools: usize,
    errors: usize,
    complete: bool,
}

#[derive(Debug)]
struct ToolEvent {
    node_id: u64,
    tool_name: String,
    input_summary: String,
    success: bool,
    error: Option<String>,
}

#[derive(Debug)]
struct PivotEvent {
    after_node: u64,
    reason: String,
    branch: u64,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn default_config_paths_resolve() {
        let cfg = AyinExportConfig::default();
        // Just verify the paths contain expected segments.
        assert!(
            cfg.conversations_dir
                .to_str()
                .unwrap()
                .contains("conversations")
        );
        assert!(cfg.sibling_traces_dir.to_str().unwrap().contains("traces"));
    }

    #[test]
    fn synthesize_user_message_dialogue() {
        let meta: Value = serde_json::json!({"prompt": "platform architecture"});
        let msg = synthesize_user_message("dialogue", "soul", Some(&meta));
        assert!(msg.contains("platform architecture"), "got: {msg}");
    }

    #[test]
    fn synthesize_user_message_helix() {
        let meta: Value = serde_json::json!({"query": "trust"});
        let msg = synthesize_user_message("helix", "soul", Some(&meta));
        assert!(msg.contains("trust"), "got: {msg}");
    }

    #[test]
    fn synthesize_user_message_unknown_action() {
        let msg = synthesize_user_message("exotic_action", "corso", None);
        assert!(msg.contains("CORSO"), "got: {msg}");
        assert!(msg.contains("exotic_action"), "got: {msg}");
    }

    #[test]
    fn build_assistant_turn_interleaves_pivot() {
        let tools = vec![
            ToolEvent {
                node_id: 5,
                tool_name: "Bash".into(),
                input_summary: "cargo build".into(),
                success: false,
                error: Some("type mismatch".into()),
            },
            ToolEvent {
                node_id: 8,
                tool_name: "Edit".into(),
                input_summary: "src/lib.rs".into(),
                success: true,
                error: None,
            },
        ];
        let pivots = vec![PivotEvent {
            after_node: 5,
            reason: "Build failed — flatten the iterator.".into(),
            branch: 1,
        }];

        let turn = build_assistant_turn(&tools, &pivots, 1, true, true);

        assert!(turn.contains("cargo build"), "missing tool: {turn}");
        assert!(
            turn.contains("flatten the iterator"),
            "missing pivot: {turn}"
        );
        assert!(turn.contains("src/lib.rs"), "missing follow-up: {turn}");
        assert!(turn.contains("Pivoting"), "pivot marker missing: {turn}");
    }

    #[test]
    fn build_assistant_turn_no_pivot_when_disabled() {
        let tools = vec![ToolEvent {
            node_id: 1,
            tool_name: "Read".into(),
            input_summary: "src/main.rs".into(),
            success: true,
            error: None,
        }];
        let pivots = vec![PivotEvent {
            after_node: 1,
            reason: "Should not appear.".into(),
            branch: 1,
        }];

        let turn = build_assistant_turn(&tools, &pivots, 0, true, false);
        assert!(
            !turn.contains("Pivoting"),
            "pivot should be suppressed: {turn}"
        );
        assert!(
            !turn.contains("Should not appear"),
            "pivot reason leaked: {turn}"
        );
    }

    #[test]
    fn extract_metadata_field_handles_string_encoded_json() {
        // Some SOUL traces store metadata as a double-encoded JSON string.
        let raw = r#""{\"path\": \"helix/entries/foo.md\", \"content\": \"bar\"}""#;
        let v: Value = serde_json::from_str(raw).unwrap();
        let result = extract_metadata_field(Some(&v), &["path"]);
        assert_eq!(result.as_deref(), Some("helix/entries/foo.md"));
    }

    #[test]
    fn export_ayin_sft_empty_dirs_writes_zero() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let output = dir.path().join("out.jsonl");

        let config = AyinExportConfig {
            conversations_dir: dir.path().join("conversations"),
            sibling_traces_dir: dir.path().join("traces"),
            output_path: output.clone(),
            ..AyinExportConfig::default()
        };

        let count = export_ayin_sft(&config).expect("export");
        assert_eq!(count, 0);
        assert!(
            output.exists(),
            "output file should be created even if empty"
        );
    }

    #[test]
    fn export_ayin_sft_reads_conversation_jsonl() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let conv_dir = dir.path().join("conversations");
        std::fs::create_dir_all(&conv_dir).unwrap();

        // Write a minimal conversation with request + tools + complete.
        let events = [
            serde_json::json!({
                "type": "request",
                "request_id": "req-test-1",
                "timestamp": "2026-04-04T10:00:00Z",
                "user_message": "Fix the failing build",
                "session_id": "sess-abc"
            }),
            serde_json::json!({
                "type": "tool",
                "request_id": "req-test-1",
                "node_id": 1,
                "timestamp": "2026-04-04T10:00:01Z",
                "tool_name": "Bash",
                "input_summary": "cmd: cargo build",
                "duration_ms": 3000,
                "success": false,
                "error": "linker error",
                "branch": 0
            }),
            serde_json::json!({
                "type": "pivot",
                "request_id": "req-test-1",
                "after_node": 1,
                "timestamp": "2026-04-04T10:00:02Z",
                "reason": "Build failed — missing dependency in Cargo.toml.",
                "branch": 1
            }),
            serde_json::json!({
                "type": "tool",
                "request_id": "req-test-1",
                "node_id": 2,
                "timestamp": "2026-04-04T10:00:03Z",
                "tool_name": "Edit",
                "input_summary": "Cargo.toml",
                "duration_ms": 10,
                "success": true,
                "error": null,
                "branch": 1
            }),
            serde_json::json!({
                "type": "complete",
                "request_id": "req-test-1",
                "timestamp": "2026-04-04T10:00:04Z",
                "total_tools": 2,
                "errors": 1,
                "pivots": 1,
                "duration_ms": 4000
            }),
        ];

        let mut jsonl = String::new();
        for e in &events {
            jsonl.push_str(&serde_json::to_string(e).unwrap());
            jsonl.push('\n');
        }
        std::fs::write(conv_dir.join("2026-04-04.jsonl"), jsonl).unwrap();

        let output = dir.path().join("sft.jsonl");
        let config = AyinExportConfig {
            conversations_dir: conv_dir,
            sibling_traces_dir: dir.path().join("traces"),
            output_path: output.clone(),
            min_tool_calls: 1,
            ..AyinExportConfig::default()
        };

        let count = export_ayin_sft(&config).expect("export");
        assert_eq!(count, 1, "should produce exactly 1 example");

        // Parse and validate the output.
        let raw = std::fs::read_to_string(&output).unwrap();
        let parsed: Value = serde_json::from_str(raw.trim()).unwrap();

        let convs = parsed["conversations"].as_array().unwrap();
        assert_eq!(convs[0]["role"], "system");
        assert_eq!(convs[1]["role"], "user");
        assert_eq!(convs[2]["role"], "assistant");

        assert!(
            convs[1]["content"]
                .as_str()
                .unwrap()
                .contains("Fix the failing build"),
            "user message should contain original request"
        );

        let assistant = convs[2]["content"].as_str().unwrap();
        assert!(
            assistant.contains("missing dependency"),
            "pivot reason should appear in assistant turn: {assistant}"
        );
        assert!(
            assistant.contains("Pivoting"),
            "pivot marker should appear: {assistant}"
        );
    }
}
