//! Conductor HITL endpoints — blocked-task surfacing and resolution.
//!
//! ## Auth
//!
//! Both routes accept `Authorization: Bearer <token>` or a valid `la_session`
//! cookie, matching the pattern used by all other Cockpit API handlers.
//!
//! ## Routes
//!
//! - `GET /api/conductor/hitl` — lists all tasks with status
//!   `awaiting_operator_resolution`.
//! - `POST /api/conductor/hitl/:task_id/resolve` — transitions the task:
//!   `approve` → `pending` (re-queued), `reject` → `failed`.
//!
//! ## Queue access
//!
//! Reads and mutations go through `coordination::handlers::{queue_lock,
//! queue_path, read_queue_async, write_queue_async}`. The process-global
//! `queue_lock()` [`std::sync::OnceLock`] mutex is shared with the coordination handlers so
//! concurrent writes from different code paths cannot corrupt `queue.json`.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::{
    coordination::handlers::{queue_lock, queue_path, read_queue_async, write_queue_async},
    real_data::is_authed_pub,
    server::AppState,
};

const AWAITING: &str = "awaiting_operator_resolution";

/// Compact view of a single HITL-blocked task, returned by
/// `GET /api/conductor/hitl`.
#[derive(Debug, Serialize)]
pub struct HitlTask {
    /// Unique task identifier.
    pub id: String,
    /// Human-readable task title.
    pub title: String,
    /// Project path relative to `~/Projects/`.
    pub project: String,
    /// Build codename the task belongs to, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_codename: Option<String>,
    /// HITL assertion ID — reason the conductor paused the task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub awaiting_assertion_id: Option<String>,
    /// ISO-8601 UTC deadline by which the task must be resolved or it fails.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_deadline: Option<String>,
    /// Priority label: `high`, `medium`, or `low`.
    pub priority: String,
    /// ISO-8601 UTC timestamp the task was enqueued.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<String>,
}

/// Request body for `POST /api/conductor/hitl/:task_id/resolve`.
#[derive(Debug, Deserialize)]
pub struct ResolveRequest {
    /// `"approve"` transitions the task to `pending`; `"reject"` to `failed`.
    pub action: String,
}

/// Response body for the resolve endpoint.
#[derive(Debug, Serialize)]
pub struct ResolveResponse {
    /// Whether the resolution succeeded.
    pub ok: bool,
    /// The status the task was transitioned to (`pending` or `failed`).
    pub new_status: String,
}

/// `GET /api/conductor/hitl` — return all `awaiting_operator_resolution` tasks.
pub async fn list_hitl_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authed_pub(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let tasks = match read_queue_async(queue_path()).await {
        Ok(q) => q
            .tasks
            .into_iter()
            .filter(|t| t.status == AWAITING)
            .map(|t| HitlTask {
                id: t.id,
                title: t.title,
                project: t.project,
                build_codename: t.build_codename,
                awaiting_assertion_id: t.awaiting_assertion_id,
                resolution_deadline: t.resolution_deadline,
                priority: t.priority,
                added: t.added,
            })
            .collect::<Vec<_>>(),
        Err(_) => vec![],
    };

    Json(tasks).into_response()
}

/// `POST /api/conductor/hitl/:task_id/resolve` — approve or reject a blocked task.
pub async fn resolve_hitl_handler(
    Path(task_id): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<ResolveRequest>,
) -> impl IntoResponse {
    if !is_authed_pub(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let new_status = match body.action.as_str() {
        "approve" => "pending",
        "reject" => "failed",
        _ => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({"error": "action must be 'approve' or 'reject'"})),
            )
                .into_response();
        }
    };

    let _guard = queue_lock().lock().await;
    let path = queue_path();

    let Ok(mut queue) = read_queue_async(path.clone()).await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    let task = queue.tasks.iter_mut().find(|t| t.id == task_id);
    let Some(task) = task else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if task.status != AWAITING {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "task is not awaiting operator resolution"})),
        )
            .into_response();
    }

    task.status = String::from(new_status);
    task.awaiting_assertion_id = None;
    task.resolution_deadline = None;

    if let Err(e) = write_queue_async(path, queue).await {
        tracing::error!(task_id = %task_id, error = %e, "failed to write queue after resolve");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    tracing::info!(task_id = %task_id, action = %body.action, new_status = %new_status, "HITL task resolved");
    Json(ResolveResponse {
        ok: true,
        new_status: new_status.to_owned(),
    })
    .into_response()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::ResolveRequest;

    #[test]
    fn resolve_request_deserializes_approve() {
        let r: ResolveRequest = serde_json::from_str(r#"{"action":"approve"}"#).unwrap();
        assert_eq!(r.action, "approve");
    }

    #[test]
    fn resolve_request_deserializes_reject() {
        let r: ResolveRequest = serde_json::from_str(r#"{"action":"reject"}"#).unwrap();
        assert_eq!(r.action, "reject");
    }
}
