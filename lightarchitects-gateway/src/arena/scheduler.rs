//! Routine scheduler — cron-based dispatch for curator and conversation routines.
//!
//! Reads `routines.json` (Khadas) or `routines-mac.json` (Mac/Docker) at startup.
//! Evaluates cron expressions every minute and dispatches matching routines.
//! Also handles event-driven dispatch from the helix significance-spike watcher.
//!
//! Routine types:
//! - `"curator"`: deterministic bulletin-board cycle (no LLM, existing behavior)
//! - `"conversation"`: multi-sibling conversation via Ollama (headless, transcript output)

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use cron::Schedule;
use serde::Deserialize;

use super::arena_config::Config;
use super::conversation_routine::{
    ConversationRoutineConfig, MacRoutineEntry, MacRoutinesFile, SignificanceSpikeEvent,
};
use super::mcp_pool::McpPool;
use super::supervisor::SupervisorHandle;
use crate::channels::Channels;

use super::compat::{JsonRpcRequestExt, JsonRpcResponseExt};
use lightarchitects::core::jsonrpc::JsonRpcRequest;

/// How often to check for due routines.
const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

// ── Legacy routines.json types (Khadas curator) ─────────────────────────

/// A scheduled routine from legacy `routines.json` (curator-only).
#[derive(Debug, Deserialize)]
struct LegacyRoutineConfig {
    name: String,
    schedule: String,
    #[allow(dead_code)]
    timezone: Option<String>,
    #[allow(dead_code)]
    agent: String,
    message: String,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Wrapper for the legacy `routines.json` file format.
#[derive(Debug, Deserialize)]
struct LegacyRoutinesFile {
    routines: Vec<LegacyRoutineConfig>,
}

// ── Unified parsed routine ────────────────────────────────────────────────

/// A parsed and validated routine entry ready for scheduling.
struct ParsedRoutine {
    /// Display name used in logs and channel messages.
    name: String,
    /// Parsed cron schedule. `None` for event-driven routines.
    schedule: Option<Schedule>,
    /// Fallback MCP message (curator routines only).
    message: String,
    /// Routine kind — determines dispatch path.
    kind: ParsedRoutineKind,
}

/// Dispatch kind for a parsed routine.
enum ParsedRoutineKind {
    /// Deterministic curator cycle.
    Curator,
    /// Multi-sibling conversation via Ollama.
    Conversation(ConversationRoutineConfig),
}

// ── Scheduler spawn ───────────────────────────────────────────────────────

/// Spawn the scheduler background task.
///
/// Loads routines from `config.routines_path` (legacy) or
/// `{data_dir}/routines-mac.json` (Mac/Docker). Runs the cron loop forever.
pub fn spawn(
    pool: Arc<McpPool>,
    supervisor: Arc<SupervisorHandle>,
    channels: Arc<Channels>,
    config: &Config,
    config_arc: Arc<Config>,
) {
    let routines_path = config.routines_path.clone();
    let mac_routines_path = config.data_dir.join("routines-mac.json");
    let data_dir = config.data_dir.clone();

    tokio::spawn(async move {
        // Prefer routines-mac.json when present (supports conversation type)
        let routines = if mac_routines_path.exists() {
            load_mac_routines(&mac_routines_path)
        } else {
            load_legacy_routines(&routines_path)
        };

        match routines {
            Ok(r) if r.is_empty() => {
                tracing::warn!("No enabled routines found — scheduler idle");
            }
            Ok(r) => {
                tracing::info!(count = r.len(), "Routines loaded");
                channels.post_telegram(&format!(
                    "Arena scheduler online — {} routines loaded",
                    r.len()
                ));
                run_loop(r, pool, supervisor, channels, data_dir, config_arc).await;
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "Failed to load routines — scheduler disabled"
                );
            }
        }
    });
}

/// Spawn the significance-spike event handler.
///
/// Receives `SignificanceSpikeEvent` from the helix watcher and fires the
/// `canon-evaluation` conversation routine inline.
pub fn spawn_spike_handler(
    mut rx: tokio::sync::mpsc::Receiver<SignificanceSpikeEvent>,
    config: Arc<Config>,
) {
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let canon_routine = ConversationRoutineConfig {
                id: "canon-evaluation".into(),
                format: Some("canon_evaluation".into()),
                seed: crate::arena::conversation_routine::SeedType::SignificanceSpike,
                max_turns: 15,
                trigger: Some("significance_spike".into()),
            };
            let seed_context = Some(format!(
                "Significance spike detected in: {}\nSignificance: {:.1}",
                event.path.display(),
                event.significance,
            ));
            tracing::info!(
                path = %event.path.display(),
                significance = event.significance,
                "Firing canon-evaluation routine"
            );
            if let Err(e) = super::conversation_routine::run_conversation_routine(
                &canon_routine,
                &config,
                seed_context,
            )
            .await
            {
                tracing::error!(error = %e, "Canon-evaluation routine failed");
            }
        }
    });
}

// ── Routine loading ───────────────────────────────────────────────────────

/// Load routines from the legacy `routines.json` format (curator-only).
fn load_legacy_routines(
    path: &Path,
) -> Result<Vec<ParsedRoutine>, Box<dyn std::error::Error + Send + Sync>> {
    if !path.exists() {
        return Err(format!("Routines file not found: {}", path.display()).into());
    }

    let content = std::fs::read_to_string(path)?;
    let file: LegacyRoutinesFile = serde_json::from_str(&content)?;
    let mut parsed = Vec::new();

    for routine in file.routines {
        if !routine.enabled {
            tracing::debug!(name = %routine.name, "Routine disabled, skipping");
            continue;
        }
        let cron_expr = format!("0 {}", routine.schedule);
        match cron_expr.parse::<Schedule>() {
            Ok(schedule) => {
                tracing::info!(name = %routine.name, schedule = %routine.schedule, "Routine registered");
                parsed.push(ParsedRoutine {
                    name: routine.name,
                    schedule: Some(schedule),
                    message: routine.message,
                    kind: ParsedRoutineKind::Curator,
                });
            }
            Err(e) => {
                tracing::error!(
                    name = %routine.name,
                    schedule = %routine.schedule,
                    error = %e,
                    "Invalid cron expression, skipping routine"
                );
            }
        }
    }

    Ok(parsed)
}

/// Load routines from `routines-mac.json` (supports conversation type).
fn load_mac_routines(
    path: &Path,
) -> Result<Vec<ParsedRoutine>, Box<dyn std::error::Error + Send + Sync>> {
    let content = std::fs::read_to_string(path)?;
    let file: MacRoutinesFile = serde_json::from_str(&content)?;
    let mut parsed = Vec::new();

    for entry in file.routines {
        if !entry.enabled {
            tracing::debug!(id = %entry.id, "Routine disabled, skipping");
            continue;
        }

        // Event-driven routines (trigger = "significance_spike") have no cron schedule
        if entry.trigger.as_deref() == Some("significance_spike") {
            tracing::info!(id = %entry.id, trigger = "significance_spike", "Event routine registered");
            // These are handled by spawn_spike_handler; skip cron scheduling.
            continue;
        }

        let Some(ref schedule_str) = entry.schedule else {
            tracing::warn!(id = %entry.id, "Routine has no schedule and no trigger — skipping");
            continue;
        };

        let cron_expr = format!("0 {schedule_str}");
        let Ok(schedule) = cron_expr.parse::<Schedule>() else {
            tracing::error!(id = %entry.id, schedule = %schedule_str, "Invalid cron expression — skipping");
            continue;
        };

        let kind = build_routine_kind(&entry);
        tracing::info!(id = %entry.id, kind = %entry.kind, schedule = %schedule_str, "Routine registered");
        parsed.push(ParsedRoutine {
            name: entry.id,
            schedule: Some(schedule),
            message: String::new(), // not used for conversation routines
            kind,
        });
    }

    Ok(parsed)
}

/// Determine the `ParsedRoutineKind` for a Mac routine entry.
fn build_routine_kind(entry: &MacRoutineEntry) -> ParsedRoutineKind {
    if entry.kind == "conversation" {
        ParsedRoutineKind::Conversation(entry.to_conversation_config())
    } else {
        ParsedRoutineKind::Curator
    }
}

// ── Scheduler loop ────────────────────────────────────────────────────────

/// Main scheduler loop — ticks every minute and dispatches due routines.
async fn run_loop(
    routines: Vec<ParsedRoutine>,
    pool: Arc<McpPool>,
    supervisor: Arc<SupervisorHandle>,
    channels: Arc<Channels>,
    data_dir: std::path::PathBuf,
    config: Arc<Config>,
) {
    loop {
        tokio::time::sleep(TICK_INTERVAL).await;

        let now = Utc::now();

        for routine in &routines {
            let Some(ref schedule) = routine.schedule else {
                continue; // event-driven — skip cron check
            };
            if let Some(next) = schedule
                .after(&(now - chrono::Duration::seconds(60)))
                .next()
            {
                if next <= now {
                    dispatch_routine(routine, &pool, &supervisor, &channels, &data_dir, &config)
                        .await;
                }
            }
        }
    }
}

/// Dispatch a single routine — routes to curator or conversation handler.
async fn dispatch_routine(
    routine: &ParsedRoutine,
    pool: &McpPool,
    supervisor: &SupervisorHandle,
    channels: &Channels,
    data_dir: &Path,
    config: &Arc<Config>,
) {
    tracing::info!(routine = %routine.name, "Dispatching routine");

    match &routine.kind {
        ParsedRoutineKind::Curator => {
            dispatch_curator(routine, pool, supervisor, channels, data_dir).await;
        }
        ParsedRoutineKind::Conversation(conv_config) => {
            dispatch_conversation(routine, conv_config, config).await;
        }
    }
}

/// Dispatch a curator cycle (no LLM, deterministic).
async fn dispatch_curator(
    routine: &ParsedRoutine,
    pool: &McpPool,
    supervisor: &SupervisorHandle,
    channels: &Channels,
    data_dir: &Path,
) {
    match super::curator::run_cycle(data_dir, channels) {
        Ok(()) => {
            tracing::info!(routine = %routine.name, "Curator cycle complete");
        }
        Err(e) => {
            tracing::error!(routine = %routine.name, error = %e, "Curator cycle failed");
            channels.post_telegram(&format!("Curator {} failed: {}", routine.name, e));

            // Fallback: MCP dispatch to SOUL if curator fails
            if supervisor.is_healthy("soul").await {
                let request = JsonRpcRequest::tools_call(
                    0,
                    "soulTools",
                    serde_json::json!({
                        "action": "chat",
                        "params": {
                            "sub_action": "chat_inject",
                            "message": routine.message,
                            "source": format!("routine:{}", routine.name),
                        }
                    }),
                );
                if let Ok(resp) = pool.call("soul", &request).await {
                    if !resp.is_error() {
                        tracing::info!(routine = %routine.name, "Fallback MCP dispatch OK");
                    }
                }
            }
        }
    }
}

/// Dispatch a conversation routine (Ollama-backed, headless).
async fn dispatch_conversation(
    routine: &ParsedRoutine,
    conv_config: &ConversationRoutineConfig,
    config: &Arc<Config>,
) {
    if let Err(e) =
        super::conversation_routine::run_conversation_routine(conv_config, config, None).await
    {
        tracing::error!(routine = %routine.name, error = %e, "Conversation routine failed");
    }
}
