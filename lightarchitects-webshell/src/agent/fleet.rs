//! `FleetBroadcaster` — owns the `FleetTracker` + `ClaudeJsonlTailer` + ticker task.
//!
//! Spawned lazily (per build session) on first SSE connect or snapshot request.
//! Broadcasts `FleetEvent` variants to all subscribed SSE handlers.
//!
//! # Cold-start resilience (XEA-3)
//!
//! If [`find_jsonl_for_session`] returns `None` at start time, the broadcaster
//! retries every 500 ms up to 10 times (5 s total). After 10 retries it logs a
//! warning and proceeds with an empty tracker — fleet will show no agents until
//! the next SSE reconnect triggers a fresh [`FleetBroadcaster::start`].

use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::{info, warn};

use lightarchitects::fleet::{
    ClaudeJsonlTailer, ExitPath, FleetNode, FleetSnapshot, FleetStatus, FleetTracker,
    find_jsonl_for_session,
};

/// Broadcast channel capacity for fleet events.
///
/// 256 slots is sufficient for a burst of agent spawns + 500 ms tick events
/// before a slow subscriber can drain. Ring-buffer semantics mean lagged
/// subscribers receive a synthetic lag signal rather than blocking.
const FLEET_EVENT_BUF: usize = 256;

/// Maximum number of retries waiting for the JSONL file to appear.
const JSONL_RETRY_MAX: u8 = 10;

/// Retry interval when the JSONL file is not yet present.
const JSONL_RETRY_MS: u64 = 500;

/// Ticker interval — elapsed-time update + progress events (SCR1-F6).
const TICK_INTERVAL_MS: u64 = 500;

// ── FleetEvent ────────────────────────────────────────────────────────────────

/// Events broadcast to subscribed SSE handlers.
///
/// `#[serde(tag = "type", rename_all = "snake_case")]` ensures every variant
/// serialises with a `"type"` discriminant matching the wire format defined in
/// `fleet-api-contract.md`.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FleetEvent {
    /// Full point-in-time snapshot — emitted as the first event on SSE connect.
    Snapshot {
        /// All known agent nodes at capture time.
        nodes: Vec<FleetNode>,
        /// RFC 3339 UTC timestamp.
        captured_at: String,
    },
    /// A new agent was detected in the JSONL stream.
    AgentSpawned {
        /// Full `FleetNode` for the newly-spawned agent.
        node: FleetNode,
    },
    /// Timer-driven elapsed-time tick for a running agent (every 500 ms).
    AgentProgress {
        /// Agent unique identifier.
        agent_id: String,
        /// Current elapsed time in milliseconds.
        elapsed_ms: u64,
    },
    /// An agent exited (`tool_result` detected in JSONL).
    AgentCompleted {
        /// Agent unique identifier.
        agent_id: String,
        /// How the agent exited.
        exit_path: ExitPath,
        /// Total turn count (always 0 in V1 — OQ4).
        turns: u64,
        /// Total wall-clock duration in milliseconds.
        duration_ms: u64,
    },
}

// ── FleetBroadcaster ─────────────────────────────────────────────────────────

/// Owns the `FleetTracker`, JSONL tailer task, and ticker task for one build.
///
/// Cheaply shareable via `Arc`. Drop aborts both background tasks.
pub struct FleetBroadcaster {
    tracker: FleetTracker,
    tx: broadcast::Sender<FleetEvent>,
    _tailer_task: tokio::task::JoinHandle<()>,
    _ticker_task: tokio::task::JoinHandle<()>,
}

impl FleetBroadcaster {
    /// Start the broadcaster for the given `session_id`.
    ///
    /// Resolves the JSONL file path (with retry on cold-start), creates a
    /// `FleetTracker`, and spawns the tailer and ticker background tasks.
    pub async fn start(session_id: String) -> Arc<Self> {
        let tracker = FleetTracker::new();
        let (tx, _) = broadcast::channel(FLEET_EVENT_BUF);

        // ── Resolve JSONL path with cold-start retry ──────────────────────
        let jsonl_path = resolve_jsonl_with_retry(&session_id).await;

        // ── Tailer task ──────────────────────────────────────────────────
        let tailer_task = {
            let tracker_clone = tracker.clone();
            let tx_clone = tx.clone();
            tokio::spawn(async move {
                run_tailer(jsonl_path, tracker_clone, tx_clone).await;
            })
        };

        // ── Ticker task (SCR1-F6) ────────────────────────────────────────
        let ticker_task = {
            let tracker_clone = tracker.clone();
            let tx_clone = tx.clone();
            tokio::spawn(async move {
                run_ticker(tracker_clone, tx_clone).await;
            })
        };

        info!(session_id = %session_id, "FleetBroadcaster started");

        Arc::new(Self {
            tracker,
            tx,
            _tailer_task: tailer_task,
            _ticker_task: ticker_task,
        })
    }

    /// Take a point-in-time snapshot of the current fleet state.
    #[must_use]
    pub fn snapshot(&self) -> FleetSnapshot {
        self.tracker.snapshot()
    }

    /// Subscribe to fleet events.
    ///
    /// Returns a `broadcast::Receiver` that will receive all subsequent events.
    /// Callers should emit the current snapshot as the first SSE event before
    /// polling this receiver.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<FleetEvent> {
        self.tx.subscribe()
    }
}

#[allow(clippy::used_underscore_binding)]
impl Drop for FleetBroadcaster {
    fn drop(&mut self) {
        self._tailer_task.abort();
        self._ticker_task.abort();
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Resolve the JSONL path for `session_id`, retrying up to `JSONL_RETRY_MAX`
/// times with `JSONL_RETRY_MS` between attempts (XEA-3 cold-start resilience).
///
/// Returns `None` after all retries are exhausted — the broadcaster proceeds
/// with an empty tracker.
async fn resolve_jsonl_with_retry(session_id: &str) -> Option<std::path::PathBuf> {
    for attempt in 1..=JSONL_RETRY_MAX {
        match find_jsonl_for_session(session_id) {
            Ok(Some(path)) => {
                info!(
                    session_id = %session_id,
                    attempt,
                    path = %path.display(),
                    "JSONL file located"
                );
                return Some(path);
            }
            Ok(None) => {
                if attempt < JSONL_RETRY_MAX {
                    tokio::time::sleep(tokio::time::Duration::from_millis(JSONL_RETRY_MS)).await;
                }
            }
            Err(e) => {
                warn!(session_id = %session_id, error = %e, "JSONL path resolution error");
                break;
            }
        }
    }
    warn!(
        session_id = %session_id,
        retries = JSONL_RETRY_MAX,
        "JSONL file not found after retries — fleet will show no agents"
    );
    None
}

/// Tailer task: start the JSONL tailer and drain spawn/completion events into
/// `tracker`, broadcasting `FleetEvent` variants to `tx`.
///
/// If `jsonl_path` is `None` (cold-start failed), this task exits immediately.
async fn run_tailer(
    jsonl_path: Option<std::path::PathBuf>,
    tracker: FleetTracker,
    tx: broadcast::Sender<FleetEvent>,
) {
    let Some(path) = jsonl_path else {
        return;
    };

    // Wrap tracker in a channel-bridged variant that emits broadcast events.
    // We use an inner relay approach: a watch channel signals when the tracker
    // changes, then the loop re-polls for new events.
    //
    // For V1 we use a simpler approach: spawn a second ticker that polls
    // the tracker for spans that appeared since last check.
    //
    // Real approach: wrap FleetTracker in an event-emitting adapter.
    // Since FleetTracker doesn't expose callbacks, we use a broadcast-adapting
    // wrapper tracker that snaps before/after each tailer poll.

    let _tailer = match ClaudeJsonlTailer::start(path.clone(), tracker.clone()).await {
        Ok(t) => t,
        Err(e) => {
            warn!(path = %path.display(), error = %e, "FleetBroadcaster: tailer failed to start");
            return;
        }
    };

    // Poll loop: every JSONL_RETRY_MS, snapshot the tracker and broadcast
    // any new or completed agents since the previous snapshot.
    let mut known_ids: std::collections::HashMap<String, lightarchitects::fleet::FleetStatus> =
        std::collections::HashMap::new();

    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(JSONL_RETRY_MS)).await;

        let snapshot = tracker.snapshot();
        let mut new_known = std::collections::HashMap::new();

        for node in snapshot.nodes {
            let prev_status = known_ids.get(&node.agent_id).cloned();
            let curr_status = node.status.clone();

            match prev_status {
                None => {
                    // New agent — broadcast AgentSpawned.
                    let _ = tx.send(FleetEvent::AgentSpawned { node: node.clone() });
                }
                Some(ref prev) => {
                    // Check for completion transition.
                    if *prev == FleetStatus::Running
                        && matches!(
                            curr_status,
                            FleetStatus::Completed | FleetStatus::Failed | FleetStatus::Stalled
                        )
                    {
                        let exit_path = match curr_status {
                            FleetStatus::Completed => ExitPath::Completed,
                            FleetStatus::Failed => ExitPath::Error,
                            _ => ExitPath::WatchdogStall,
                        };
                        let _ = tx.send(FleetEvent::AgentCompleted {
                            agent_id: node.agent_id.clone(),
                            exit_path,
                            turns: node.turns,
                            duration_ms: node.elapsed_ms,
                        });
                    }
                }
            }

            new_known.insert(node.agent_id, curr_status);
        }

        known_ids = new_known;

        // Check if tx is closed (no more subscribers + broadcaster dropped).
        if tx.receiver_count() == 0 {
            // Keep task alive — broadcaster may gain subscribers later.
            // Only exit if the send fails (receiver_count check is advisory).
        }
    }

    // _tailer goes out of scope here (unreachable in practice — task is
    // aborted by FleetBroadcaster::drop via JoinHandle::abort()).
    #[allow(unreachable_code, clippy::used_underscore_binding)]
    let _ = _tailer;
}

/// Ticker task: every `TICK_INTERVAL_MS` milliseconds, advance elapsed timers
/// for running agents and broadcast `AgentProgress` events.
///
/// Exits when the broadcast channel is closed.
async fn run_ticker(tracker: FleetTracker, tx: broadcast::Sender<FleetEvent>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(TICK_INTERVAL_MS));
    // First tick fires immediately — skip it so we don't emit progress at t=0.
    interval.tick().await;

    loop {
        interval.tick().await;

        tracker.tick_elapsed(TICK_INTERVAL_MS);

        // Emit AgentProgress for every running agent.
        let snapshot = tracker.snapshot();
        let running: Vec<_> = snapshot
            .nodes
            .into_iter()
            .filter(|n| n.status == FleetStatus::Running)
            .collect();

        if running.is_empty() {
            continue;
        }

        for node in running {
            // Best-effort — if all receivers dropped the channel may return Err.
            let _ = tx.send(FleetEvent::AgentProgress {
                agent_id: node.agent_id,
                elapsed_ms: node.elapsed_ms,
            });
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Fleet events serialise with the correct `"type"` discriminant.
    #[test]
    fn fleet_event_snapshot_type_tag() {
        let ev = FleetEvent::Snapshot {
            nodes: vec![],
            captured_at: "2026-05-18T00:00:00Z".to_owned(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains(r#""type":"snapshot""#), "{json}");
    }

    #[test]
    fn fleet_event_agent_spawned_type_tag() {
        use lightarchitects::fleet::{FleetNode, FleetStatus};
        let node = FleetNode {
            agent_id: "a1".to_owned(),
            agent_type: "engineer".to_owned(),
            description: "test".to_owned(),
            parent_agent_id: None,
            worktree_path: None,
            run_in_background: false,
            status: FleetStatus::Running,
            turns: 0,
            elapsed_ms: 0,
            exit_path: None,
        };
        let ev = FleetEvent::AgentSpawned { node };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains(r#""type":"agent_spawned""#), "{json}");
    }

    #[test]
    fn fleet_event_agent_progress_type_tag() {
        let ev = FleetEvent::AgentProgress {
            agent_id: "a1".to_owned(),
            elapsed_ms: 1500,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains(r#""type":"agent_progress""#), "{json}");
        assert!(json.contains("1500"), "{json}");
    }

    #[test]
    fn fleet_event_agent_completed_type_tag() {
        let ev = FleetEvent::AgentCompleted {
            agent_id: "a1".to_owned(),
            exit_path: ExitPath::Completed,
            turns: 0,
            duration_ms: 5000,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains(r#""type":"agent_completed""#), "{json}");
        assert!(json.contains(r#""exit_path":"completed""#), "{json}");
    }

    /// S1 invariant: no `prompt` field in any fleet event serialization.
    #[test]
    fn s1_no_prompt_in_fleet_event_serialization() {
        use lightarchitects::fleet::{FleetNode, FleetStatus};
        let node = FleetNode {
            agent_id: "a1".to_owned(),
            agent_type: "engineer".to_owned(),
            description: "analyze".to_owned(),
            parent_agent_id: None,
            worktree_path: None,
            run_in_background: false,
            status: FleetStatus::Running,
            turns: 0,
            elapsed_ms: 0,
            exit_path: None,
        };
        let events = vec![
            serde_json::to_string(&FleetEvent::Snapshot {
                nodes: vec![node.clone()],
                captured_at: "t".to_owned(),
            })
            .unwrap(),
            serde_json::to_string(&FleetEvent::AgentSpawned { node }).unwrap(),
            serde_json::to_string(&FleetEvent::AgentProgress {
                agent_id: "a1".to_owned(),
                elapsed_ms: 500,
            })
            .unwrap(),
            serde_json::to_string(&FleetEvent::AgentCompleted {
                agent_id: "a1".to_owned(),
                exit_path: ExitPath::Completed,
                turns: 0,
                duration_ms: 1000,
            })
            .unwrap(),
        ];
        for json in &events {
            assert!(
                !json.contains("prompt"),
                "S1 violation: 'prompt' found in fleet event: {json}"
            );
        }
    }
}
