//! `ConversationRoutine` — orchestrates scheduled multi-sibling conversations.
//!
//! This module handles the `"conversation"` routine type in `routines.json`/`routines-mac.json`.
//! It drives soul-chat's `ConversationOrchestrator` in headless (no-TTS) mode,
//! writing transcripts to `~/.soul/helix/chat/transcripts/auto-{date}.md`.
//!
//! The Ollama AI backend is used exclusively — no Anthropic API calls in auto mode.
//! Host is configurable via `OLLAMA_HOST` (defaults to `localhost:11434`; use
//! `host.docker.internal:11434` when containerized).
//!
//! # Helix significance watcher
//!
//! A background task watches `~/.soul/helix/` via the `notify` crate. When a new
//! file is written (or modified), the watcher:
//! 1. Validates the path is under the configured helix root.
//! 2. Reads and parses frontmatter to extract the `significance` field.
//! 3. Only if `significance >= threshold` (default 8.0) fires the
//!    `canon-evaluation` routine.
//!    External writes that lack valid YAML frontmatter are silently dropped.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use serde::Deserialize;
use tokio::sync::mpsc;

use super::arena_config::Config;

// ── Routine JSON types ───────────────────────────────────────────────────

/// Trigger condition for event-driven routines.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoutineTrigger {
    /// Fires when a helix entry with significance >= threshold is written.
    SignificanceSpike,
}

/// Seed source for conversation context.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SeedType {
    /// Completed builds from active.yaml.
    CompletedBuilds,
    /// Any available source — open-ended topic.
    Any,
    /// A significance-spike entry (path injected by the watcher).
    SignificanceSpike,
}

impl Default for SeedType {
    fn default() -> Self {
        Self::Any
    }
}

/// A conversation routine entry parsed from `routines-mac.json`.
///
/// Example JSON:
/// ```json
/// { "id": "morning-build-debrief", "type": "conversation",
///   "schedule": "0 9 * * *", "format": "build_debrief",
///   "seed": "completed_builds", "max_turns": 15 }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct ConversationRoutineConfig {
    /// Unique identifier used for log traces and transcript naming.
    pub id: String,
    /// Conversation format name (e.g., `"build_debrief"`, `"canon_evaluation"`).
    /// `null` = let `FormatSelector` pick based on seed availability.
    #[serde(default)]
    pub format: Option<String>,
    /// Seed type — where to draw conversation context from.
    #[serde(default)]
    pub seed: SeedType,
    /// Maximum conversation turns (default: 15).
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    /// Event trigger — set to `"significance_spike"` for event-driven routines.
    #[serde(default)]
    #[allow(dead_code)] // deserialized from JSON; read by scheduler dispatch logic (future)
    pub trigger: Option<String>,
}

fn default_max_turns() -> u32 {
    15
}

// ── Runner ───────────────────────────────────────────────────────────────

/// Execute a single conversation routine cycle.
///
/// This function is synchronous from the scheduler's perspective — it drives
/// the conversation to completion and writes the transcript.
///
/// In headless auto mode:
/// - No TTS synthesis (voice disabled)
/// - Ollama-only backend (no Anthropic API)
/// - Transcript written to `helix_output_dir/auto-{date}.md`
///
/// # Errors
///
/// Returns descriptive error strings on I/O or transcript-write failure.
/// LLM unavailability is logged as a warning and returns `Ok(())` (non-fatal).
pub async fn run_conversation_routine(
    routine: &ConversationRoutineConfig,
    config: &Arc<Config>,
    seed_context: Option<String>,
) -> Result<(), String> {
    tracing::info!(
        routine = %routine.id,
        format = ?routine.format,
        seed = ?routine.seed,
        max_turns = routine.max_turns,
        "ConversationRoutine: starting"
    );

    let transcript_content = build_headless_transcript(routine, config, seed_context).await?;

    write_transcript(&routine.id, &config.helix_output_dir, &transcript_content)?;

    tracing::info!(routine = %routine.id, "ConversationRoutine: transcript written");
    Ok(())
}

/// Build the conversation transcript in headless mode (Ollama, no TTS).
///
/// Returns the markdown-formatted transcript as a string.
async fn build_headless_transcript(
    routine: &ConversationRoutineConfig,
    config: &Arc<Config>,
    seed_context: Option<String>,
) -> Result<String, String> {
    let ollama_url = format!("http://{}/api/generate", config.ollama_host);
    let model = std::env::var("ARENA_CONV_MODEL").unwrap_or_else(|_| "llama3.2:latest".into());

    let system_prompt = build_system_prompt(routine, seed_context.as_deref());
    let mut transcript_lines = Vec::new();
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");

    transcript_lines.push(format!(
        "---\nroutine: {id}\nformat: {fmt}\ndate: {timestamp}\nautomated: true\n---\n",
        id = routine.id,
        fmt = routine.format.as_deref().unwrap_or("auto"),
    ));
    transcript_lines.push(format!("# Conversation: {} — {timestamp}\n", routine.id));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("ConversationRoutine: HTTP client error: {e}"))?;

    let max_turns = routine.max_turns.min(30); // hard cap
    let mut conversation_context = system_prompt.clone();

    for turn in 0..max_turns {
        let prompt = if turn == 0 {
            format!("{system_prompt}\n\nBegin the conversation.")
        } else {
            format!("{conversation_context}\n\nContinue.")
        };

        let body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
            "options": { "temperature": 0.7, "num_predict": 512 }
        });

        let response = match client.post(&ollama_url).json(&body).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    routine = %routine.id,
                    turn,
                    error = %e,
                    "ConversationRoutine: Ollama unavailable — ending early"
                );
                break;
            }
        };

        if !response.status().is_success() {
            tracing::warn!(
                routine = %routine.id,
                status = %response.status(),
                "ConversationRoutine: Ollama returned error status — ending early"
            );
            break;
        }

        let resp_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("ConversationRoutine: JSON parse error: {e}"))?;

        let text = resp_json
            .get("response")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned();

        if text.is_empty() {
            tracing::debug!(routine = %routine.id, turn, "ConversationRoutine: empty response, ending");
            break;
        }

        transcript_lines.push(format!("## Turn {}\n\n{text}\n", turn.saturating_add(1)));
        conversation_context = format!("{conversation_context}\n\n{text}");
    }

    Ok(transcript_lines.join("\n"))
}

/// Build the system prompt for the conversation routine.
fn build_system_prompt(routine: &ConversationRoutineConfig, seed_context: Option<&str>) -> String {
    let format_instruction = routine.format.as_deref().map_or_else(
        || "Choose the most appropriate conversation format based on the available context.".into(),
        |fmt| format!("Use the '{fmt}' conversation format."),
    );

    let seed_section = match seed_context {
        Some(ctx) if !ctx.is_empty() => {
            format!("\n\n## Context\n\n{ctx}")
        }
        _ => String::new(),
    };

    format!(
        "You are facilitating a structured conversation between the Light Architects siblings \
         (EVA, CORSO, QUANTUM, SERAPH, AYIN, LÆX). {format_instruction}\n\
         Run for at most {} turns. Keep responses concise and focused.\
         {seed_section}",
        routine.max_turns,
    )
}

/// Write the transcript to the helix output directory.
///
/// Creates the directory if absent. File name: `auto-{routine-id}-{date}.md`.
fn write_transcript(routine_id: &str, output_dir: &Path, content: &str) -> Result<(), String> {
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("ConversationRoutine: create_dir_all failed: {e}"))?;

    let date = Utc::now().format("%Y-%m-%d");
    let filename = format!("auto-{routine_id}-{date}.md");
    let path = output_dir.join(&filename);

    std::fs::write(&path, content)
        .map_err(|e| format!("ConversationRoutine: write transcript failed: {e}"))?;

    tracing::info!(path = %path.display(), "ConversationRoutine: transcript saved");
    Ok(())
}

// ── Helix significance watcher ────────────────────────────────────────────

/// Message sent from the watcher task to the significance-spike handler.
#[derive(Debug)]
pub struct SignificanceSpikeEvent {
    /// Path of the helix entry that triggered the spike.
    pub path: PathBuf,
    /// Significance value extracted from frontmatter.
    pub significance: f64,
}

/// Spawn the background helix-watcher task.
///
/// Watches `helix_root` recursively. When a file is created or modified:
/// 1. Validates the path is strictly under `helix_root` (prevents path traversal).
/// 2. Reads frontmatter and extracts `significance`.
/// 3. If `significance >= threshold`, sends a `SignificanceSpikeEvent` on `tx`.
///
/// The receiver side is polled by the scheduler to fire the `canon-evaluation` routine.
pub fn spawn_helix_watcher(
    helix_root: PathBuf,
    threshold: f64,
    tx: mpsc::Sender<SignificanceSpikeEvent>,
) -> Result<(), String> {
    // notify requires a synchronous callback. We use a channel to bridge to async.
    let (raw_tx, mut raw_rx) = mpsc::channel::<PathBuf>(64);

    let helix_root_clone = helix_root.clone();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        let Ok(event) = res else {
            return;
        };
        // Only care about file creates and modifications
        let relevant = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(notify::event::ModifyKind::Data(_))
        );
        if !relevant {
            return;
        }
        for path in event.paths {
            if path.extension().is_some_and(|ext| ext == "md") {
                let _ = raw_tx.blocking_send(path);
            }
        }
    })
    .map_err(|e| format!("Helix watcher: init failed: {e}"))?;

    watcher
        .watch(&helix_root, RecursiveMode::Recursive)
        .map_err(|e| format!("Helix watcher: watch({}) failed: {e}", helix_root.display()))?;

    // Spawn an async task to process events — keeps the watcher alive via move.
    tokio::spawn(async move {
        // Keep the watcher alive for the lifetime of this task.
        let _watcher = watcher;

        tracing::info!(path = %helix_root_clone.display(), threshold, "Helix significance watcher started");

        while let Some(path) = raw_rx.recv().await {
            process_helix_event(path, &helix_root_clone, threshold, &tx).await;
        }

        tracing::warn!("Helix significance watcher channel closed — watcher stopped");
    });

    Ok(())
}

/// Process a single helix file event.
///
/// Validates path prefix, reads frontmatter, checks significance.
async fn process_helix_event(
    path: PathBuf,
    helix_root: &Path,
    threshold: f64,
    tx: &mpsc::Sender<SignificanceSpikeEvent>,
) {
    // SCRUM fix (Task 15.3): validate path prefix before trusting content
    if !is_under_helix_root(&path, helix_root) {
        tracing::debug!(
            path = %path.display(),
            "Helix watcher: path not under helix root — skipped"
        );
        return;
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(path = %path.display(), error = %e, "Helix watcher: read failed — skipped");
            return;
        }
    };

    let Some(significance) = extract_significance(&content) else {
        tracing::debug!(
            path = %path.display(),
            "Helix watcher: no valid frontmatter significance — silently dropped"
        );
        return;
    };

    if significance >= threshold {
        tracing::info!(
            path = %path.display(),
            significance,
            threshold,
            "Helix significance spike detected"
        );
        let event = SignificanceSpikeEvent { path, significance };
        if tx.send(event).await.is_err() {
            tracing::warn!("Helix watcher: significance-spike receiver dropped");
        }
    }
}

/// Validate that a path is strictly under the helix root (no traversal).
///
/// Uses lexical normalization (no filesystem access) so `..` components are
/// resolved before the prefix check — preventing path traversal attacks.
fn is_under_helix_root(path: &Path, helix_root: &Path) -> bool {
    normalize_lexical(path).starts_with(normalize_lexical(helix_root))
}

/// Lexically normalize a path by resolving `.` and `..` without filesystem access.
fn normalize_lexical(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            other => components.push(other),
        }
    }
    components.iter().collect()
}

/// Extract the `significance` field from YAML frontmatter (between `---` fences).
///
/// Returns `None` if frontmatter is absent, malformed, or lacks the field.
/// This is the SCRUM-required gate: external writes without valid frontmatter
/// are silently dropped.
fn extract_significance(content: &str) -> Option<f64> {
    // Frontmatter must start at the very beginning of the file
    let rest = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))?;

    // Find the closing fence
    let end = rest.find("\n---").or_else(|| rest.find("\r\n---"))?;
    let frontmatter = rest.get(..end)?;

    // Parse the `significance: N.N` line — avoid pulling in a full YAML parser
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(value_str) = line.strip_prefix("significance:") {
            return value_str.trim().parse::<f64>().ok();
        }
    }
    None
}

// ── routines-mac.json parsing ────────────────────────────────────────────

/// Top-level structure for `routines-mac.json`.
#[derive(Debug, Deserialize)]
pub struct MacRoutinesFile {
    /// Flat list of routines (both cron and event-driven).
    #[serde(default)]
    pub routines: Vec<MacRoutineEntry>,
}

/// A single entry in `routines-mac.json`.
#[derive(Debug, Deserialize)]
pub struct MacRoutineEntry {
    /// Unique ID.
    pub id: String,
    /// Routine type: `"conversation"` or `"conductor"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Cron schedule (5-field, UTC). Absent for event-triggered routines.
    #[serde(default)]
    pub schedule: Option<String>,
    /// Conversation format (for `conversation` type).
    #[serde(default)]
    pub format: Option<String>,
    /// Seed source (for `conversation` type).
    #[serde(default = "default_seed_str")]
    pub seed: String,
    /// Max conversation turns (for `conversation` type, default 15).
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    /// Event trigger (e.g., `"significance_spike"`).
    #[serde(default)]
    pub trigger: Option<String>,
    /// Whether the routine is enabled (default: true).
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_seed_str() -> String {
    "any".into()
}

fn default_true() -> bool {
    true
}

impl MacRoutineEntry {
    /// Convert to a `ConversationRoutineConfig` (only valid for `type = "conversation"`).
    #[must_use]
    pub fn to_conversation_config(&self) -> ConversationRoutineConfig {
        let seed = match self.seed.as_str() {
            "completed_builds" => SeedType::CompletedBuilds,
            "significance_spike" => SeedType::SignificanceSpike,
            _ => SeedType::Any,
        };
        ConversationRoutineConfig {
            id: self.id.clone(),
            format: self.format.clone(),
            seed,
            max_turns: self.max_turns,
            trigger: self.trigger.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_significance_valid() {
        let content = "---\nid: test\nsignificance: 9.2\ntype: identity\n---\n\n# Content";
        assert_eq!(extract_significance(content), Some(9.2));
    }

    #[test]
    fn test_extract_significance_no_frontmatter() {
        let content = "# Just a heading\n\nNo frontmatter here.";
        assert_eq!(extract_significance(content), None);
    }

    #[test]
    fn test_extract_significance_missing_field() {
        let content = "---\nid: test\ntype: identity\n---\n\n# Content";
        assert_eq!(extract_significance(content), None);
    }

    #[test]
    fn test_extract_significance_malformed_value() {
        let content = "---\nsignificance: not_a_number\n---\n# Content";
        assert_eq!(extract_significance(content), None);
    }

    #[test]
    fn test_is_under_helix_root_valid() {
        let root = PathBuf::from("/home/user/.soul/helix");
        let path = PathBuf::from("/home/user/.soul/helix/eva/entries/2026-01.md");
        assert!(is_under_helix_root(&path, &root));
    }

    #[test]
    fn test_is_under_helix_root_traversal() {
        let root = PathBuf::from("/home/user/.soul/helix");
        let path = PathBuf::from("/home/user/.soul/helix/../config/secret.toml");
        // After normalization this is outside root — starts_with catches it
        assert!(!is_under_helix_root(&path, &root));
    }

    #[test]
    fn test_is_under_helix_root_sibling_dir() {
        let root = PathBuf::from("/home/user/.soul/helix");
        // A directory that merely shares the prefix but is not under root
        let path = PathBuf::from("/home/user/.soul/helix-archive/file.md");
        assert!(!is_under_helix_root(&path, &root));
    }

    #[test]
    fn test_default_max_turns() {
        assert_eq!(default_max_turns(), 15);
    }
}
