//! `POST /api/builds/:id/attestation` — worker `IMPLEMENTATION_COMPLETE` attestation.
//!
//! Worker agents POST a §3.5 `IMPLEMENTATION_COMPLETE` attestation when they
//! complete a wave task. The handler:
//!
//! 1. Validates Bearer auth (same credential the browser uses).
//! 2. Assembles an [`ImplCompleteEvent`] from the request body + path `build_id`.
//! 3. Appends to a per-build ring buffer (cap 50; oldest dropped on overflow).
//! 4. Broadcasts via the build's SSE channel so the frontend receives it live.
//!
//! `GET /api/builds/:id/attestation` returns the ring buffer as JSON for
//! browser reconnects and initial load.
//!
//! ## Security invariants
//!
//! - `file_content_span_id` is a UUID reference only. Absolute file paths are
//!   NEVER embedded in attestation payloads (CWE-200 boundary). The handler
//!   validates this: if the field contains a `/`, it is rejected with 400.
//! - `trust_boundary` is forwarded verbatim; the handler never writes `"signed"`.
//!   The frontend must render any `trust_boundary` starting with `"unverified"`
//!   as an amber badge per Northstar §T3.
//! - `ayin_spans_dropped_total > 0` is a P gate signal; it is forwarded to the
//!   frontend without modification.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth,
    events::types::ImplCompleteEvent,
    events::{WebEvent, WebEventV2},
    server::AppState,
};

/// Maximum attestation events retained per build.
pub const RING_CAP: usize = 50;

/// Request body for `POST /api/builds/:id/attestation`.
///
/// Maps directly to [`ImplCompleteEvent`] minus `build_id` (taken from path)
/// and `timestamp` (set server-side for anti-replay integrity).
#[derive(Debug, Deserialize)]
pub struct PostAttestationBody {
    /// Wave index within the build.
    pub wave: u32,
    /// Unique task identifier from the `AgentRunner` dispatcher.
    pub task_id: String,
    /// Agent identifier that produced this attestation.
    pub agent_id: String,
    /// Git commit SHA of the wave's deliverable.
    pub commit_sha: String,
    /// Gate labels that passed (e.g. `"Q1_fmt"`, `"Q2_clippy"`).
    #[serde(default)]
    pub gates_passed: Vec<String>,
    /// Gate labels intentionally skipped with rationale recorded in the commit.
    #[serde(default)]
    pub gates_skipped: Vec<String>,
    /// UUID reference into the AYIN span store — NOT a file path.
    /// Rejected with 400 if it contains a `/` or `\`.
    #[serde(default)]
    pub file_content_span_id: Option<String>,
    /// AYIN spans dropped by the collector since the last attestation.
    /// A non-zero value triggers a P gate warning badge in the frontend.
    #[serde(default)]
    pub ayin_spans_dropped_total: u64,
    /// Trust boundary tag. Forwarded verbatim; handler never writes `"signed"`.
    pub trust_boundary: String,
    /// Optional Agents Playbook §3.5 spec-compliance claim string.
    #[serde(default)]
    pub spec_compliance_claim: Option<String>,
    /// Agent self-reported confidence in the implementation (0.0–1.0).
    pub confidence: f32,
}

/// Response body returned by both `POST` (echo) and `GET` (ring buffer).
#[derive(Debug, Serialize)]
pub struct AttestationResponse {
    /// Attestation events — up to [`RING_CAP`] entries, newest last.
    pub attestations: Vec<ImplCompleteEvent>,
}

/// `POST /api/builds/{id}/attestation`
///
/// Accepts a worker's `IMPLEMENTATION_COMPLETE` attestation (Agents Playbook §3.5),
/// appends to the ring buffer, and broadcasts on the build's SSE channel.
///
/// # Panics
///
/// Panics if the ring-buffer `Mutex` is poisoned — which indicates unrecoverable
/// process state corruption and is the correct failure mode.
pub async fn post_attestation(
    Path(build_id): Path<Uuid>,
    _auth: auth::AuthGuard,
    State(state): State<AppState>,
    Json(body): Json<PostAttestationBody>,
) -> impl IntoResponse {
    // CWE-200: reject any file_content_span_id that looks like a path.
    if body
        .file_content_span_id
        .as_deref()
        .is_some_and(|s| s.contains('/') || s.contains('\\'))
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "file_content_span_id must be a UUID reference, not a file path"
            })),
        )
            .into_response();
    }

    let Some(session) = state.builds.get(build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let ev = ImplCompleteEvent {
        build_id,
        wave: body.wave,
        task_id: body.task_id,
        agent_id: body.agent_id,
        commit_sha: body.commit_sha,
        gates_passed: body.gates_passed,
        gates_skipped: body.gates_skipped,
        file_content_span_id: body.file_content_span_id,
        ayin_spans_dropped_total: body.ayin_spans_dropped_total,
        trust_boundary: body.trust_boundary,
        spec_compliance_claim: body.spec_compliance_claim,
        confidence: body.confidence,
        timestamp: Utc::now(),
    };

    // Append to ring buffer (cap RING_CAP; oldest dropped on overflow).
    let ring = state
        .attestation_log
        .entry(build_id)
        .or_insert_with(|| Arc::new(Mutex::new(VecDeque::with_capacity(RING_CAP))));
    {
        #[allow(clippy::unwrap_used)] // poisoned mutex → process state corrupt; panic is correct
        let mut q = ring.lock().unwrap();
        if q.len() >= RING_CAP {
            q.pop_front();
        }
        q.push_back(ev.clone());
    }

    // Broadcast on the per-build SSE channel.
    let envelope = WebEventV2::from_event(WebEvent::ImplComplete(ev.clone()), Some(build_id));
    match session.event_tx.send(envelope) {
        Ok(n) => {
            tracing::debug!(build_id = %build_id, subscribers = n, "attestation broadcast");
        }
        Err(_) => {
            tracing::debug!(build_id = %build_id, "attestation broadcast: no SSE subscribers");
        }
    }

    (
        StatusCode::ACCEPTED,
        Json(AttestationResponse {
            attestations: vec![ev],
        }),
    )
        .into_response()
}

/// `GET /api/builds/{id}/attestation`
///
/// Returns the in-memory ring buffer of attestations for a build.
/// Used by the browser on reconnect / initial load to populate `AttestationCard`.
///
/// # Panics
///
/// Panics if the ring-buffer `Mutex` is poisoned — which indicates unrecoverable
/// process state corruption and is the correct failure mode.
pub async fn list_attestations(
    Path(build_id): Path<Uuid>,
    _auth: auth::AuthGuard,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let attestations = state
        .attestation_log
        .get(&build_id)
        .map(|ring| {
            #[allow(clippy::unwrap_used)]
            ring.lock().unwrap().iter().cloned().collect::<Vec<_>>()
        })
        .unwrap_or_default();

    (StatusCode::OK, Json(AttestationResponse { attestations })).into_response()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{
        config::{AgentSession, Config, TokenSource},
        container::{ContainerMode, DockerCapability},
        server::AppState,
    };
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use std::{ffi::OsString, path::PathBuf};
    use tower::ServiceExt;

    fn test_state() -> AppState {
        let config = Config {
            port: 0,
            host_cmd: OsString::from("bash"),
            cwd: PathBuf::from("/tmp"),
            token: "test-token".to_owned(),
            token_source: TokenSource::EnvVar,
            agent: AgentSession::default(),
            claude_agent_template: None,
            container_mode: ContainerMode::Auto,
            dev_mode: false,
            max_context_prompts: 50,
            litellm: crate::config::LiteLLMConfig::default(),
            hermes_mcp: crate::config::HermesMcpConfig::default(),
        };
        AppState::for_test(config, DockerCapability::Unavailable)
    }

    fn attestation_body(_build_id: Uuid) -> serde_json::Value {
        serde_json::json!({
            "wave": 1,
            "task_id": "task-001",
            "agent_id": "claude-code",
            "commit_sha": "abc1234",
            "gates_passed": ["Q1_fmt", "Q2_clippy"],
            "gates_skipped": [],
            "file_content_span_id": null,
            "ayin_spans_dropped_total": 0,
            "trust_boundary": "unverified_pre_2.10",
            "spec_compliance_claim": null,
            "confidence": 0.97
        })
    }

    #[tokio::test]
    async fn rejects_file_path_in_span_id() {
        let state = test_state();
        let app = crate::server::build_app(state);
        let build_id = Uuid::new_v4();
        let body = serde_json::json!({
            "wave": 1,
            "task_id": "t",
            "agent_id": "a",
            "commit_sha": "abc",
            "trust_boundary": "unverified_pre_2.10",
            "confidence": 0.9,
            "file_content_span_id": "/etc/passwd",
        });
        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/builds/{build_id}/attestation"))
            .header("content-type", "application/json")
            .header("authorization", "Bearer test-token")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn returns_404_for_missing_build() {
        let state = test_state();
        let app = crate::server::build_app(state);
        let build_id = Uuid::new_v4();
        let body = attestation_body(build_id);
        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/builds/{build_id}/attestation"))
            .header("content-type", "application/json")
            .header("authorization", "Bearer test-token")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn ring_buffer_caps_at_ring_cap() {
        use std::collections::VecDeque;
        use std::sync::{Arc, Mutex};
        let log: Arc<dashmap::DashMap<Uuid, Arc<Mutex<VecDeque<ImplCompleteEvent>>>>> =
            Arc::new(dashmap::DashMap::new());
        let build_id = Uuid::new_v4();
        let ring = log
            .entry(build_id)
            .or_insert_with(|| Arc::new(Mutex::new(VecDeque::with_capacity(RING_CAP))));
        for i in 0..(RING_CAP + 5) {
            let ev = ImplCompleteEvent {
                build_id,
                wave: u32::try_from(i).unwrap_or(0),
                task_id: format!("t-{i}"),
                agent_id: "agent".into(),
                commit_sha: "abc".into(),
                gates_passed: vec![],
                gates_skipped: vec![],
                file_content_span_id: None,
                ayin_spans_dropped_total: 0,
                trust_boundary: "unverified_pre_2.10".into(),
                spec_compliance_claim: None,
                confidence: 1.0,
                timestamp: Utc::now(),
            };
            let mut q = ring.lock().unwrap();
            if q.len() >= RING_CAP {
                q.pop_front();
            }
            q.push_back(ev);
        }
        assert_eq!(ring.lock().unwrap().len(), RING_CAP);
    }

    #[tokio::test]
    async fn list_returns_empty_for_unknown_build() {
        let state = test_state();
        let app = crate::server::build_app(state);
        let build_id = Uuid::new_v4();
        let request = Request::builder()
            .method("GET")
            .uri(format!("/api/builds/{build_id}/attestation"))
            .header("authorization", "Bearer test-token")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["attestations"], serde_json::json!([]));
    }
}
