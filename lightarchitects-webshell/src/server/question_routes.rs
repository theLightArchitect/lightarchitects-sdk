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
use tracing::{info, warn};
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

/// Maximum number of questions a single `POST /api/question` body may contain.
/// Prevents amplified TTL-eviction cost and over-sized SSE payloads.
pub(crate) const MAX_QUESTIONS_PER_SUBMIT: usize = 20;

/// Maximum number of questions that may be pending simultaneously across all
/// active gateway calls. Bounds registry and metadata map memory usage.
pub(crate) const MAX_CONCURRENT_QUESTIONS: usize = 32;

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

    // F-QCOUNT: cap questions per submission to bound SSE payload and eviction cost.
    if body.questions.len() > MAX_QUESTIONS_PER_SUBMIT {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": format!(
                    "too many questions: max {MAX_QUESTIONS_PER_SUBMIT}, received {}",
                    body.questions.len()
                )
            })),
        )
            .into_response();
    }

    // F-REGCAP: reject when too many questions are already pending to bound memory.
    if state.question_registry.len() >= MAX_CONCURRENT_QUESTIONS {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": format!(
                    "question registry full: max {MAX_CONCURRENT_QUESTIONS} concurrent questions"
                )
            })),
        )
            .into_response();
    }

    // tool_use_id always minted server-side — never derived from client input
    // (prevents IDOR attacks per SA-4 pattern).
    let tool_use_id = Uuid::new_v4();
    let (tx, rx) = tokio::sync::oneshot::channel::<QuestionAnswer>();

    info!(
        tool_use_id = %tool_use_id,
        question_count = body.questions.len(),
        "HITL question submitted; awaiting operator answer (300 s TTL)"
    );
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
            info!(tool_use_id = %tool_use_id, "HITL question answered by operator");
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
            warn!(tool_use_id = %tool_use_id, "HITL question timed out (300 s); browser card will not auto-dismiss");
            state.question_registry.remove(&tool_use_id);
            state.question_metadata.remove(&tool_use_id);
            StatusCode::REQUEST_TIMEOUT.into_response()
        }
    }
}

/// `POST /api/question/:id/answer`
///
/// Browser submits the operator's answer for a pending question. The handler
/// validates answers against the declared option set (F4 — OWASP LLM01 prompt
/// injection guard), then atomically removes the oneshot sender from
/// [`AppState::question_registry`] and fires the answer, unblocking the
/// gateway long-poll in [`question_submit_handler`].
///
/// Returns:
/// - **200** — answer delivered to the gateway.
/// - **404** — `id` not registered (already answered, TTL-expired, or receiver
///   dropped). Returns `404` rather than `410` to avoid disclosing whether the
///   ID existed (F1 — timing oracle prevention).
/// - **422** — one or more answer labels are not in the declared option set,
///   there are more answer vectors than declared questions, or question metadata
///   is absent while the registry entry is still live (F4 — prompt injection guard).
///
/// # Partial-answer behaviour
///
/// Validation passes if all *provided* answer vectors are valid; the handler
/// does not require that every declared question slot receives an answer. If
/// exhaustive coverage is required, callers should assert `answers.len() ==
/// questions.len()` before submitting.
///
/// Requires `Authorization: Bearer <token>`.
pub(crate) async fn question_answer_handler(
    _: auth::AuthGuard,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<QuestionAnswer>,
) -> impl IntoResponse {
    // F4 — OWASP LLM01: Validate answer labels against the declared option set
    // before the answer re-enters the LLM context as a trusted tool_result.
    // Clone the questions out of the metadata map to release the shard lock
    // before the subsequent registry remove (prevents DashMap shard contention).
    let declared_questions = state
        .question_metadata
        .get(&id)
        .map(|entry| entry.questions.clone());

    // F4 guard: if metadata is absent but a live registry entry exists (TTL race:
    // eviction removed metadata before registry), reject rather than bypass the
    // allowlist. "Metadata absent + registry live" is an inconsistent state that
    // must not silently pass validation (SERAPH SA-F4-1).
    let Some(questions) = declared_questions else {
        if state.question_registry.contains_key(&id) {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                axum::Json(serde_json::json!({
                    "error": "question metadata unavailable; answer rejected"
                })),
            )
                .into_response();
        }
        return StatusCode::NOT_FOUND.into_response();
    };

    // F4 — every declared question must have exactly one answer vector.
    // The loop only iterates body.answers, so fewer vectors than questions
    // would silently accept an incomplete answer; catch that here.
    if body.answers.len() != questions.len() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            axum::Json(serde_json::json!({
                "error": format!(
                    "expected {} answer vector(s), received {}",
                    questions.len(),
                    body.answers.len()
                )
            })),
        )
            .into_response();
    }

    for (i, answers_i) in body.answers.iter().enumerate() {
        // questions.get(i) is guaranteed Some here because lengths were verified above,
        // but the guard is kept as a defensive belt-and-suspenders for future refactors.
        let Some(q) = questions.get(i) else {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                axum::Json(serde_json::json!({
                    "error": "more answer vectors than questions declared"
                })),
            )
                .into_response();
        };
        // F-MULTI: single-select questions must receive exactly one label.
        if !q.multi_select && answers_i.len() != 1 {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                axum::Json(serde_json::json!({
                    "error": format!(
                        "question {i} is single-select (multi_select: false) but received {} label(s)",
                        answers_i.len()
                    )
                })),
            )
                .into_response();
        }

        for label in answers_i {
            if !q.options.iter().any(|opt| &opt.label == label) {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    axum::Json(serde_json::json!({
                        "error": format!(
                            "answer '{label}' not in declared options for question {i}"
                        )
                    })),
                )
                    .into_response();
            }
        }
    }

    let Some((_, tx)) = state.question_registry.remove(&id) else {
        // debug! not warn! — this fires on every TTL-expired retry; warn! would
        // flood logs in normal operation. auth-gated so no adversarial amplification.
        tracing::debug!(tool_use_id = %id, "answer for unknown or expired question (F1: 404)");
        return StatusCode::NOT_FOUND.into_response();
    };

    if tx.send(body).is_err() {
        // F1: return 404 rather than 410 — avoids disclosing that the ID existed
        // but its receiver timed out (timing oracle).
        warn!(tool_use_id = %id, "answer delivered but receiver already dropped (timeout race)");
        return StatusCode::NOT_FOUND.into_response();
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
                resume_session_id: None,
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
        // Populate metadata so F4 allowlist validation runs against declared options.
        state.question_metadata.insert(id, make_pending(id));

        let app = crate::server::build_app(state);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/question/{id}/answer"))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(json!({"answers": [["Yes"]]}).to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let answer = rx.await.unwrap();
        assert_eq!(answer.answers[0], vec!["Yes".to_owned()]);
    }

    // ── F4 + F-MULTI property tests ─────────────────────────────────────────
    // Canon XXVII T-suite gap (documented at Phase 6): allowlist monotonicity and
    // single-select cardinality enforcement.

    proptest::proptest! {
        /// For any non-empty subset of the declared option labels, a single-select
        /// answer containing exactly one of them must return 200.
        #[test]
        fn prop_valid_label_always_passes(
            idx in 0usize..2usize,   // "Yes" (0) or "No" (1)
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let state = test_state();
                let (tx, _rx) = tokio::sync::oneshot::channel::<QuestionAnswer>();
                let id = Uuid::new_v4();
                state.question_registry.insert(id, tx);
                state.question_metadata.insert(id, make_pending(id));
                let label = if idx == 0 { "Yes" } else { "No" };
                let app = crate::server::build_app(state);
                let req = Request::builder()
                    .method("POST")
                    .uri(format!("/api/question/{id}/answer"))
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {TEST_TOKEN}"))
                    .body(Body::from(
                        serde_json::json!({"answers": [[label]]}).to_string(),
                    ))
                    .unwrap();
                let resp = app.oneshot(req).await.unwrap();
                proptest::prop_assert_eq!(resp.status(), StatusCode::OK);
                Ok(())
            })?;
        }

        /// Any label that is NOT in the declared option set must return 422.
        #[test]
        fn prop_invalid_label_always_rejects(
            bad_label in "[A-Za-z]{5,20}",
        ) {
            proptest::prop_assume!(bad_label != "Yes" && bad_label != "No");
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let state = test_state();
                let (tx, _rx) = tokio::sync::oneshot::channel::<QuestionAnswer>();
                let id = Uuid::new_v4();
                state.question_registry.insert(id, tx);
                state.question_metadata.insert(id, make_pending(id));
                let app = crate::server::build_app(state);
                let req = Request::builder()
                    .method("POST")
                    .uri(format!("/api/question/{id}/answer"))
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {TEST_TOKEN}"))
                    .body(Body::from(
                        serde_json::json!({"answers": [[bad_label]]}).to_string(),
                    ))
                    .unwrap();
                let resp = app.oneshot(req).await.unwrap();
                proptest::prop_assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
                Ok(())
            })?;
        }

        /// F-MULTI: single-select question with >1 label must return 422.
        #[test]
        fn prop_single_select_multi_label_rejects(
            extra in 1usize..=4usize,
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let state = test_state();
                let (tx, _rx) = tokio::sync::oneshot::channel::<QuestionAnswer>();
                let id = Uuid::new_v4();
                state.question_registry.insert(id, tx);
                state.question_metadata.insert(id, make_pending(id));
                // Submit 1 + extra labels for a single-select question.
                let labels: Vec<&str> = std::iter::repeat_n("Yes", 1 + extra).collect();
                let app = crate::server::build_app(state);
                let req = Request::builder()
                    .method("POST")
                    .uri(format!("/api/question/{id}/answer"))
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {TEST_TOKEN}"))
                    .body(Body::from(
                        serde_json::json!({"answers": [labels]}).to_string(),
                    ))
                    .unwrap();
                let resp = app.oneshot(req).await.unwrap();
                proptest::prop_assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
                Ok(())
            })?;
        }
    }

    /// Builds a minimal [`QuestionPending`] with options `["Yes", "No"]` for
    /// F4 allowlist tests. Each test should create a fresh state (via
    /// `test_state()`) to avoid cross-test contamination via `DashMap`.
    fn make_pending(id: Uuid) -> crate::events::types::QuestionPending {
        use crate::events::types::{QuestionItem, QuestionOptionItem};
        crate::events::types::QuestionPending {
            tool_use_id: id,
            questions: vec![QuestionItem {
                question: "Pick one".to_owned(),
                header: "Test".to_owned(),
                multi_select: false,
                options: vec![
                    QuestionOptionItem {
                        label: "Yes".to_owned(),
                        description: String::new(),
                    },
                    QuestionOptionItem {
                        label: "No".to_owned(),
                        description: String::new(),
                    },
                ],
            }],
            headless_policy: None,
            inserted_at: chrono::Utc::now(),
        }
    }

    /// F-QCOUNT: questions array exceeds `MAX_QUESTIONS_PER_SUBMIT` → 422.
    #[tokio::test]
    async fn submit_too_many_questions_returns_422() {
        let questions: Vec<serde_json::Value> = (0..=MAX_QUESTIONS_PER_SUBMIT)
            .map(|i| {
                serde_json::json!({
                    "question": format!("Q{i}"),
                    "header": format!("H{i}"),
                    "multiSelect": false,
                    "options": [{"label": "Yes", "description": ""}]
                })
            })
            .collect();
        let app = crate::server::build_app(test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/api/question")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(
                serde_json::json!({"questions": questions}).to_string(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// F-REGCAP: registry at `MAX_CONCURRENT_QUESTIONS` → 503 Service Unavailable.
    #[tokio::test]
    async fn submit_when_registry_full_returns_503() {
        let state = test_state();
        // Fill the registry to capacity with phantom senders (never consumed).
        for _ in 0..MAX_CONCURRENT_QUESTIONS {
            let (tx, _rx) = tokio::sync::oneshot::channel::<QuestionAnswer>();
            state.question_registry.insert(Uuid::new_v4(), tx);
        }
        let app = crate::server::build_app(state);
        let req = Request::builder()
            .method("POST")
            .uri("/api/question")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(
                serde_json::json!({"questions": [{
                    "question": "Q", "header": "H", "multiSelect": false,
                    "options": [{"label": "Yes", "description": ""}]
                }]})
                .to_string(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// F-MULTI: single-select question with two labels → 422.
    #[tokio::test]
    async fn answer_single_select_multi_label_returns_422() {
        let state = test_state();
        let (tx, _rx) = tokio::sync::oneshot::channel::<QuestionAnswer>();
        let id = Uuid::new_v4();
        state.question_registry.insert(id, tx);
        state.question_metadata.insert(id, make_pending(id));

        let app = crate::server::build_app(state);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/question/{id}/answer"))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(
                serde_json::json!({"answers": [["Yes", "No"]]}).to_string(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// F4: fewer answer vectors than declared questions → 422 (partial-answer bypass closed).
    #[tokio::test]
    async fn answer_too_few_vectors_returns_422() {
        let state = test_state();
        let (tx, _rx) = tokio::sync::oneshot::channel::<QuestionAnswer>();
        let id = Uuid::new_v4();
        state.question_registry.insert(id, tx);
        // make_pending declares 1 question; submitting 0 answer vectors must be rejected.
        state.question_metadata.insert(id, make_pending(id));

        let app = crate::server::build_app(state);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/question/{id}/answer"))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(json!({"answers": []}).to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// F4: answer label not in declared options → 422 Unprocessable Content.
    #[tokio::test]
    async fn answer_invalid_label_returns_422() {
        let state = test_state();
        let (tx, _rx) = tokio::sync::oneshot::channel::<QuestionAnswer>();
        let id = Uuid::new_v4();
        state.question_registry.insert(id, tx);
        state.question_metadata.insert(id, make_pending(id));

        let app = crate::server::build_app(state);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/question/{id}/answer"))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(
                json!({"answers": [["INJECT IGNORED"]]}).to_string(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// F4 + F1: valid label with metadata → 200; label in options passes allowlist.
    #[tokio::test]
    async fn answer_valid_label_with_metadata_returns_200() {
        let state = test_state();
        let (tx, rx) = tokio::sync::oneshot::channel::<QuestionAnswer>();
        let id = Uuid::new_v4();
        state.question_registry.insert(id, tx);
        state.question_metadata.insert(id, make_pending(id));

        let app = crate::server::build_app(state);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/question/{id}/answer"))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(json!({"answers": [["Yes"]]}).to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let answer = rx.await.unwrap();
        assert_eq!(answer.answers[0], vec!["Yes"]);
    }
}
