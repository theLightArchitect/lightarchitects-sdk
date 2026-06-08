//! HTTP routes for Lightspace canvas operations.
//!
//! - `GET  /api/lightspace/:session_id/snapshot` — current [`CanvasState`] JSON
//! - `GET  /api/lightspace/:session_id/replay`   — ordered event log for replay
//! - `POST /api/lightspace/:session_id/event`    — apply a [`CanvasEvent`] to the reducer
//!
//! All routes require `Authorization: Bearer <token>`.

use std::sync::atomic::Ordering;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use lightarchitects_lightspace::CanvasEvent;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

use crate::{auth, server::AppState};

/// `GET /api/lightspace/:session_id/snapshot`
///
/// Returns the current [`CanvasState`] as JSON.  Creates a fresh empty canvas
/// if no session exists yet.
#[instrument(name = "lightspace.snapshot", skip_all, fields(session_id = %session_id))]
pub async fn snapshot_handler(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if !bearer_ok(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let slot = state.lightspace_registry.get_or_create(session_id);
    let engine = slot.engine.read().await;
    let snapshot = engine.snapshot();
    Json(snapshot.into_state()).into_response()
}

/// `GET /api/lightspace/:session_id/replay`
///
/// Returns the ordered event log for client-side replay and integrity checks.
/// Each entry carries the sequence number and the raw event JSON value.
#[derive(Debug, Serialize)]
pub struct ReplayEntry {
    /// Monotonic sequence number of this event in the session log.
    pub seq: u64,
    /// Raw event payload as serialised JSON.
    pub event: serde_json::Value,
}

/// `GET /api/lightspace/:session_id/replay`
///
/// Returns the ordered event log for client-side replay and chain verification.
#[instrument(name = "lightspace.replay", skip_all, fields(session_id = %session_id))]
pub async fn replay_handler(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if !bearer_ok(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match crate::lightspace::persist::read_events(session_id) {
        Ok(entries) => {
            let body: Vec<ReplayEntry> = entries
                .into_iter()
                .map(|(seq, event)| ReplayEntry { seq, event })
                .collect();
            Json(body).into_response()
        }
        Err(e) => {
            tracing::warn!(session_id = %session_id, error = %e, "lightspace replay: read failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `POST /api/lightspace/:session_id/event`
///
/// Apply a [`CanvasEvent`] to the reducer and broadcast the result as a
/// [`WebEventV2`] on the per-session channel.
///
/// The body is a JSON-serialised [`CanvasEvent`].
#[derive(Debug, Deserialize)]
pub struct ApplyEventRequest {
    /// The canvas event to apply to the reducer.
    pub event: CanvasEvent,
}

/// `POST /api/lightspace/:session_id/event`
///
/// Applies a [`CanvasEvent`] to the session's reducer.  Returns 204 on success,
/// 422 when the reducer rejects the event.
#[instrument(name = "lightspace.apply_event", skip_all, fields(session_id = %session_id))]
pub async fn apply_event_handler(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<ApplyEventRequest>,
) -> impl IntoResponse {
    if !bearer_ok(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let slot = state.lightspace_registry.get_or_create(session_id);

    // Apply the event to the reducer (exclusive write lock).
    let event_json = match serde_json::to_value(&body.event) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(session_id = %session_id, error = %e, "lightspace apply: serialize event");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    {
        let engine = slot.engine.read().await;
        let snapshot = engine.snapshot();
        drop(engine);

        let new_engine = {
            let e = lightarchitects_lightspace::Lightspace::restore(snapshot);
            match e.reduce(body.event) {
                Ok(updated) => updated,
                Err(err) => {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %err,
                        "lightspace apply: reducer rejected event"
                    );
                    return StatusCode::UNPROCESSABLE_ENTITY.into_response();
                }
            }
        };

        *slot.engine.write().await = new_engine;
    }

    // Persist + broadcast (best-effort; does not fail the 204 response).
    let seq = slot.event_counter.fetch_add(1, Ordering::SeqCst);
    {
        let mut prev = slot.prev_chain.lock().await;
        match crate::lightspace::persist::append(
            session_id,
            &slot.hmac_seed,
            seq,
            &prev,
            &event_json,
        ) {
            Ok(new_chain) => {
                *prev = new_chain;
            }
            Err(e) => {
                tracing::warn!(session_id = %session_id, seq, error = %e, "lightspace persist: append failed");
            }
        }
    }

    StatusCode::NO_CONTENT.into_response()
}

fn bearer_ok(headers: &axum::http::HeaderMap, expected: &str) -> bool {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|h| auth::validate_bearer(h, expected))
}
