//! HTTP endpoints for the native LA `question` tool (webshell-hitl-bridge).
//!
//! Two routes form the long-poll bridge between the gateway tool call and the
//! browser [`QuestionCard`] component:
//!
//! - [`question_submit_handler`] — `POST /api/question`: gateway long-polls
//!   here; webshell mints a `tool_use_id`, emits an SSE prompt, and awaits
//!   the operator's answer for up to 300 s.
//! - [`question_answer_handler`] — `POST /api/question/:id/answer`: browser
//!   submits the operator's answer; unblocks the gateway long-poll.

use std::time::Duration;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Utc;
use serde::Deserialize;
use tokio::time::timeout;
use uuid::Uuid;

use crate::{
    auth,
    events::{
        WebEvent,
        envelope::WebEventV2,
        types::{
            QuestionAnswer, QuestionAnsweredEvent, QuestionHeadlessPolicy, QuestionItem,
            QuestionPending, QuestionPromptEvent,
        },
    },
    server::AppState,
};

/// Webshell long-poll budget in seconds (gateway allows 310 s, so 300 s here
/// ensures the webshell returns a clean 408 before the gateway's reqwest
/// client timeout fires).
const QUESTION_LONG_POLL_SECS: u64 = 300;

/// Incoming body shape — mirrors the gateway `QuestionInput` camelCase wire format.
///
/// Defined locally because the webshell crate does not depend on the gateway crate.
/// The field names and serde behaviour match verbatim.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuestionRequest {
    questions: Vec<QuestionItem>,
    #[serde(default)]
    headless_policy: Option<QuestionHeadlessPolicy>,
}

/// `POST /api/question`
///
/// Receives a serialised [`QuestionRequest`] from the gateway and long-polls
/// for the operator's answer for up to [`QUESTION_LONG_POLL_SECS`] seconds.
///
/// # Protocol
///
/// 1. Validates the payload has at least one question.
/// 2. Mints a server-side `tool_use_id` (`Uuid::new_v4()`; never client-supplied).
/// 3. Inserts a `oneshot::Sender<QuestionAnswer>` into
///    [`AppState::question_registry`].
/// 4. Inserts a [`QuestionPending`] entry into [`AppState::question_metadata`]
///    (used by the 300 s TTL eviction loop).
/// 5. Emits [`WebEvent::QuestionPrompt`] over the broadcast channel so the
///    browser renders `QuestionCard.svelte`.
/// 6. Awaits the oneshot receiver:
///    - **Answer received** → emits [`WebEvent::QuestionAnswered`], cleans up
///      metadata, returns 200 + [`QuestionAnswer`] JSON.
///    - **Timeout (300 s)** → cleans up both registries, returns 408.
///    - **Sender dropped** (TTL eviction or answer handler panic) → returns 410.
///
/// Requires `Authorization: Bearer <token>`.
pub(crate) async fn question_submit_handler(
    _: auth::AuthGuard,
    State(state): State<AppState>,
    Json(body): Json<QuestionRequest>,
) -> impl IntoResponse {
    if body.questions.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "questions must not be empty"})),
        )
            .into_response();
    }

    // tool_use_id always minted server-side — never derived from client input
    // (prevents IDOR attacks per SA-4 pattern).
    let tool_use_id = Uuid::new_v4();
    let (tx, rx) = tokio::sync::oneshot::channel::<QuestionAnswer>();

    state.question_registry.insert(tool_use_id, tx);
    state.question_metadata.insert(
        tool_use_id,
        QuestionPending {
            tool_use_id,
            questions: body.questions.clone(),
            headless_policy: body.headless_policy,
            inserted_at: Utc::now(),
        },
    );

    // Fire-and-forget SSE emission — ignore SendError when no subscriber is
    // connected (no browser tab open is a valid operational state).
    let _ = state.event_tx.send(WebEventV2::from_event(
        WebEvent::QuestionPrompt(QuestionPromptEvent {
            tool_use_id,
            questions: body.questions,
            headless_policy: body.headless_policy,
        }),
        None,
    ));

    match timeout(Duration::from_secs(QUESTION_LONG_POLL_SECS), rx).await {
        Ok(Ok(answer)) => {
            // Emit QuestionAnswered so the browser clears the pending card.
            let _ = state.event_tx.send(WebEventV2::from_event(
                WebEvent::QuestionAnswered(QuestionAnsweredEvent {
                    tool_use_id,
                    answers: answer.answers.clone(),
                }),
                None,
            ));
            state.question_metadata.remove(&tool_use_id);
            (StatusCode::OK, Json(answer)).into_response()
        }
        Ok(Err(_)) => {
            // Sender dropped (TTL eviction or double-answer race).
            state.question_metadata.remove(&tool_use_id);
            StatusCode::GONE.into_response()
        }
        Err(_elapsed) => {
            // 300 s budget exhausted; gateway will handle via headless fallback.
            state.question_registry.remove(&tool_use_id);
            state.question_metadata.remove(&tool_use_id);
            StatusCode::REQUEST_TIMEOUT.into_response()
        }
    }
}

/// `POST /api/question/:id/answer`
///
/// Browser submits the operator's answer for a pending question. The handler
/// atomically removes the oneshot sender from [`AppState::question_registry`]
/// and fires the answer, unblocking the gateway long-poll in
/// [`question_submit_handler`].
///
/// Returns:
/// - **200** — answer delivered to the gateway.
/// - **404** — `id` not registered (already answered or TTL-expired).
/// - **410** — gateway long-poll already timed out (receiver dropped).
///
/// Requires `Authorization: Bearer <token>`.
pub(crate) async fn question_answer_handler(
    _: auth::AuthGuard,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<QuestionAnswer>,
) -> impl IntoResponse {
    let Some((_, tx)) = state.question_registry.remove(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if tx.send(body).is_err() {
        // Gateway receiver already dropped (long-poll timed out).
        return StatusCode::GONE.into_response();
    }

    StatusCode::OK.into_response()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use serde_json::json;
    use tower::ServiceExt;

    use std::ffi::OsString;
    use std::path::PathBuf;

    const TEST_TOKEN: &str = "test-bearer-abc";

    fn test_state() -> crate::server::AppState {
        crate::server::AppState::for_test(
            crate::config::Config {
                port: 0,
                host_cmd: OsString::from("bash"),
                cwd: PathBuf::from("/tmp"),
                token: TEST_TOKEN.to_owned(),
                token_source: crate::config::TokenSource::EnvVar,
                agent: crate::config::AgentSession::default(),
                claude_agent_template: None,
                container_mode: crate::container::ContainerMode::Auto,
                dev_mode: false,
                max_context_prompts: 50,
                litellm: crate::config::LiteLLMConfig::default(),
                hermes_mcp: crate::config::HermesMcpConfig::default(),
            },
            crate::container::DockerCapability::Unavailable,
        )
    }

    #[tokio::test]
    async fn submit_empty_questions_returns_400() {
        let app = crate::server::build_app(test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/api/question")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(json!({"questions": []}).to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn answer_unknown_id_returns_404() {
        let app = crate::server::build_app(test_state());
        let id = Uuid::new_v4();
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/question/{id}/answer"))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(json!({"answers": [["Yes"]]}).to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn answer_resolves_pending_question() {
        let state = test_state();
        let (tx, rx) = tokio::sync::oneshot::channel::<QuestionAnswer>();
        let id = Uuid::new_v4();
        state.question_registry.insert(id, tx);

        let app = crate::server::build_app(state);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/question/{id}/answer"))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(
                json!({"answers": [["Proceed"], ["Read", "Write"]]}).to_string(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let answer = rx.await.unwrap();
        assert_eq!(answer.answers[0], vec!["Proceed".to_owned()]);
        assert_eq!(
            answer.answers[1],
            vec!["Read".to_owned(), "Write".to_owned()]
        );
    }
}
