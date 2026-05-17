//! Supervisor SSE endpoint and `WAVE_COMPLETE` event watcher.
//!
//! Two HTTP routes:
//! - `GET /api/builds/:id/supervisor/events` — SSE stream of [`NorthstarEvaluationEvent`]s.
//! - `POST /api/builds/:id/supervisor/acknowledge` — operator acknowledgement
//!   of a drift proposal card.
//!
//! A background watcher task (spawned by [`spawn_supervisor_watcher`])
//! subscribes to `AgentSessionHost.event_tx`, fires [`evaluate_wave`] on
//! every `AgentEvent::WaveComplete`, and broadcasts
//! [`WebEvent::SupervisorUpdate`] on `BuildSession.event_tx`.
//!
//! ## Auth
//!
//! - SSE endpoint: requires `Authorization: Bearer <token>` (global webshell bearer).
//! - Acknowledge endpoint: requires `X-LA-Notify-Token` (per-build notify token).
//!   This matches the trust domain of `/api/builds/:id/notify` — the gateway
//!   acknowledges proposals on behalf of the operator via the same secret
//!   delivered through `LA_NOTIFY_TOKEN` env var.
//!
//! ## Error map
//!
//! - `404 Not Found` — `build_id` unknown or no supervisor entry for that build.
//! - `401 Unauthorized` — bearer or notify token missing/invalid.
//! - `204 No Content` — successful acknowledgement (no body).

use std::{convert::Infallible, sync::Arc};

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::stream;
use serde::Serialize;
use tokio::sync::{Mutex, broadcast};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    auth,
    events::{WebEvent, notify::NOTIFY_TOKEN_HEADER, types::NorthstarEvaluationEvent},
    server::AppState,
    supervisor::{EvaluationStatus, SupervisorConfig, SupervisorState, WaveContext, evaluate_wave},
};

// ── SupervisorEntry ───────────────────────────────────────────────────────────

/// Per-build supervisor state, held in `AppState::supervisor_states`.
///
/// Cheap to hold behind `Arc` — the only mutable field lives behind a `Mutex`.
pub struct SupervisorEntry {
    /// Supervisor state machine (drift detection + proposal gating).
    pub state: Arc<Mutex<SupervisorState>>,
    /// Northstar text for this build, used as the evaluation prompt context.
    ///
    /// `None` means the operator did not supply a northstar at build-creation
    /// time; wave events are ignored by the watcher in that case.
    pub northstar_text: Option<String>,
    /// Most-recent wave evaluation result for `GET /supervisor/state`.
    pub last_evaluation: Arc<Mutex<Option<NorthstarEvaluationEvent>>>,
    /// Cancellation token — triggers watcher task shutdown on session teardown.
    pub watcher_token: CancellationToken,
}

/// JSON response for `GET /api/builds/:id/supervisor/state`.
#[derive(Debug, Serialize)]
pub struct SupervisorStateResponse {
    /// Operator's declared northstar for this build.
    pub northstar_text: Option<String>,
    /// Number of consecutive drifting wave evaluations.
    pub consecutive_drifts: u32,
    /// Drift count at which a proposal card is triggered.
    pub drift_threshold: u32,
    /// Whether a proposal is currently awaiting operator acknowledgement.
    pub proposal_pending: bool,
    /// Last completed wave evaluation, or `null` if no waves yet.
    pub last_evaluation: Option<NorthstarEvaluationEvent>,
}

impl SupervisorEntry {
    /// Create a new entry, seeding the northstar text into the supervisor state.
    #[must_use]
    pub fn new(northstar_text: Option<String>, config: SupervisorConfig) -> Arc<Self> {
        let state = if let Some(ref text) = northstar_text {
            SupervisorState::new(config).with_northstar(text.clone())
        } else {
            SupervisorState::new(config)
        };
        Arc::new(Self {
            state: Arc::new(Mutex::new(state)),
            northstar_text,
            last_evaluation: Arc::new(Mutex::new(None)),
            watcher_token: CancellationToken::new(),
        })
    }
}

// ── HTTP handlers ─────────────────────────────────────────────────────────────

/// `GET /api/builds/:id/supervisor/events` — SSE stream of supervisor updates.
///
/// Subscribes to `BuildSession.event_tx` and filters for
/// `WebEvent::SupervisorUpdate` events whose `build_id` matches the path
/// parameter. Returns `404` if the build is unknown.
pub async fn supervisor_sse_handler(
    _: auth::AuthGuard,
    Path(build_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Response {
    let Some(session) = state.builds.get(build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let rx = session.event_tx.subscribe();
    let build_id_str = build_id.to_string();

    let event_stream = stream::unfold(rx, move |mut rx| {
        let id = build_id_str.clone();
        async move {
            loop {
                match rx.recv().await {
                    Ok(WebEvent::SupervisorUpdate(ev)) if ev.build_id == id => {
                        let Ok(json) = serde_json::to_string(&ev) else {
                            continue;
                        };
                        let sse_event = Event::default().data(json).event("supervisor_update");
                        return Some((Ok::<_, Infallible>(sse_event), rx));
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(build_id = %id, lagged = n, "supervisor SSE lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        }
    });

    Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `POST /api/builds/:id/supervisor/acknowledge` — operator acknowledges a drift proposal.
///
/// Validates `X-LA-Notify-Token`, resets the supervisor's drift counter and
/// `proposal_pending` flag, then broadcasts a `SupervisorUpdate` so the
/// frontend can clear the proposal card immediately.
pub async fn supervisor_acknowledge_handler(
    Path(build_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let Some(session) = state.builds.get(build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(provided) = headers
        .get(NOTIFY_TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
    else {
        warn!(
            target: "auth",
            event = "supervisor_ack_failure",
            reason = "missing_header",
            build_id = %build_id,
        );
        return StatusCode::UNAUTHORIZED.into_response();
    };

    if !auth::validate_notify_token(provided, &session.notify_token) {
        warn!(
            target: "auth",
            event = "supervisor_ack_failure",
            reason = "invalid_token",
            build_id = %build_id,
            header_length = provided.len(),
        );
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let Some(entry) = state.supervisor_states.get(&build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mut supervisor = entry.state.lock().await;
    supervisor.acknowledge_proposal();
    drop(supervisor);

    // Broadcast a synthetic update so the frontend clears the proposal card.
    let ev = NorthstarEvaluationEvent {
        build_id: build_id.to_string(),
        wave_num: 0,
        status: "neutral".to_owned(),
        confidence: 0.5,
        recommended_next: "Proposal acknowledged — supervision resumed.".to_owned(),
        proposal_pending: false,
    };
    let _ = session.event_tx.send(WebEvent::SupervisorUpdate(ev));

    StatusCode::NO_CONTENT.into_response()
}

/// `GET /api/builds/:id/supervisor/state` — point-in-time supervisor snapshot.
///
/// Returns the current drift count, proposal flag, and last wave evaluation.
/// Returns `404` if no supervisor entry exists for the build (northstar not set).
pub async fn supervisor_state_handler(
    _: auth::AuthGuard,
    Path(build_id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let Some(entry) = state.supervisor_states.get(&build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let supervisor = entry.state.lock().await;
    let consecutive_drifts = supervisor.consecutive_drifts();
    let drift_threshold = supervisor.config.drift_threshold_waves;
    let proposal_pending = supervisor.proposal_pending;
    drop(supervisor);
    let last_evaluation = entry.last_evaluation.lock().await.clone();
    let resp = SupervisorStateResponse {
        northstar_text: entry.northstar_text.clone(),
        consecutive_drifts,
        drift_threshold,
        proposal_pending,
        last_evaluation,
    };
    (StatusCode::OK, Json(resp)).into_response()
}

// ── Background watcher ────────────────────────────────────────────────────────

/// Spawn the background supervisor watcher for a build.
///
/// Subscribes to `AgentSessionHost.event_tx` via [`crate::agent::ensure_agent_host`]
/// and, on every `AgentEvent::WaveComplete`, calls [`evaluate_wave`] and
/// broadcasts [`WebEvent::SupervisorUpdate`] on `BuildSession.event_tx`.
///
/// The watcher exits when:
/// - `entry.watcher_token` is cancelled (session teardown), or
/// - the agent event broadcast channel closes.
pub fn spawn_supervisor_watcher(
    session: Arc<crate::session::BuildSession>,
    entry: Arc<SupervisorEntry>,
    client: reqwest::Client,
    ollama_base: Option<String>,
    model: String,
) {
    use crate::agent::{ensure_agent_host, protocol::AgentEvent};

    tokio::spawn(async move {
        let (agent_event_tx, _control_tx) = ensure_agent_host(&session).await;
        let mut rx = agent_event_tx.subscribe();
        let token = entry.watcher_token.clone();

        loop {
            tokio::select! {
                () = token.cancelled() => {
                    info!(build_id = %session.build_id, "supervisor watcher cancelled");
                    break;
                }
                result = rx.recv() => {
                    match result {
                        Ok(AgentEvent::WaveComplete { wave_num, summary }) => {
                            let northstar_text = match entry.northstar_text.as_deref() {
                                Some(t) => t.to_owned(),
                                None => continue,
                            };
                            let ctx = WaveContext {
                                northstar_text: &northstar_text,
                                wave_num,
                                wave_summary: &summary,
                            };
                            match evaluate_wave(&ctx, &client, ollama_base.as_deref(), &model).await {
                                Ok(eval) => {
                                    let mut supervisor = entry.state.lock().await;
                                    let proposal_triggered = supervisor.record_evaluation(&eval);
                                    let pending = supervisor.proposal_pending;
                                    drop(supervisor);

                                    let status = match eval.status {
                                        EvaluationStatus::Advancing => "advancing",
                                        EvaluationStatus::Neutral   => "neutral",
                                        EvaluationStatus::Drifting  => "drifting",
                                    };
                                    let ev = NorthstarEvaluationEvent {
                                        build_id: session.build_id.to_string(),
                                        wave_num: eval.wave_num,
                                        status: status.to_owned(),
                                        confidence: eval.confidence,
                                        recommended_next: eval.recommended_next,
                                        proposal_pending: pending,
                                    };

                                    if proposal_triggered {
                                        info!(
                                            build_id = %session.build_id,
                                            wave_num,
                                            recommended_next = %ev.recommended_next,
                                            "supervisor drift threshold reached — proposal pending",
                                        );
                                    }

                                    // Cache for GET /supervisor/state.
                                    *entry.last_evaluation.lock().await = Some(ev.clone());
                                    let _ = session.event_tx.send(WebEvent::SupervisorUpdate(ev));
                                }
                                Err(e) => {
                                    warn!(
                                        build_id = %session.build_id,
                                        wave_num,
                                        error = %e,
                                        "supervisor wave evaluation failed",
                                    );
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(build_id = %session.build_id, lagged = n, "supervisor watcher lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            info!(build_id = %session.build_id, "agent event channel closed — supervisor watcher exiting");
                            break;
                        }
                    }
                }
            }
        }
    });
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::supervisor::SupervisorConfig;

    #[test]
    fn supervisor_entry_with_northstar_seeds_state() {
        let entry = SupervisorEntry::new(
            Some("Ship E2E webshell".to_owned()),
            SupervisorConfig::default(),
        );
        let state = entry.state.blocking_lock();
        assert_eq!(
            state.northstar_text.as_deref(),
            Some("Ship E2E webshell"),
            "northstar_text must be seeded from SupervisorEntry",
        );
    }

    #[test]
    fn supervisor_entry_without_northstar_state_is_none() {
        let entry = SupervisorEntry::new(None, SupervisorConfig::default());
        let state = entry.state.blocking_lock();
        assert!(state.northstar_text.is_none());
    }

    #[test]
    fn supervisor_entry_watcher_token_starts_uncancelled() {
        let entry = SupervisorEntry::new(None, SupervisorConfig::default());
        assert!(!entry.watcher_token.is_cancelled());
        entry.watcher_token.cancel();
        assert!(entry.watcher_token.is_cancelled());
    }
}
