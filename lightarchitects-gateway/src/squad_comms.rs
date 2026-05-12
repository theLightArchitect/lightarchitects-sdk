//! Squad Comms MCP actions — thin HTTP wrappers delegating to the webshell
//! `/api/coordination/*` endpoints.
//!
//! Each action reads the webshell token from the standard token file, then
//! makes an authenticated HTTP request to the webshell process running on
//! `http://localhost:8733`.  Responses are forwarded as-is to the MCP caller.
//!
//! # Actions
//!
//! - `session_start` — `POST /api/coordination/sessions/start` (UUID minted here)
//! - `session_end`   — `POST /api/coordination/sessions/end`
//! - `list_tasks`    — `GET  /api/coordination/tasks`
//! - `add_task`      — `POST /api/coordination/tasks/add`
//! - `claim_task`    — `POST /api/coordination/tasks/claim/:id`
//! - `task_logs`     — `GET  /api/coordination/tasks/:id/logs`
//! - `chat_inject`   — `POST /api/coordination/chat/inject`
//!
//! # Error handling
//!
//! If the webshell is not running, a clear error message is returned to the
//! MCP caller — no panic, no unwrap (BC-1).

use serde_json::{Value, json};
use uuid::Uuid;

use crate::config::GatewayConfig;
use crate::error::GatewayError;

/// Base URL for the local webshell server.
const WEBSHELL_BASE: &str = "http://localhost:8733";

/// Read the webshell bearer token from the canonical token file.
///
/// Returns an empty string if the token file is missing or unreadable —
/// callers will receive a 401 from the webshell, which is surfaced clearly.
fn read_webshell_token() -> String {
    let path = dirs_next::home_dir()
        .map(|h| h.join(".lightarchitects").join("webshell").join(".token"))
        .unwrap_or_default();
    std::fs::read_to_string(&path)
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

/// Make a GET request to the webshell and return the JSON body.
async fn webshell_get(path: &str, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let _ = config; // reserved for future per-agent URL config
    let token = read_webshell_token();
    let url = format!("{WEBSHELL_BASE}{path}");

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| {
            GatewayError::InvalidRequest(format!(
                "webshell unreachable at {url} — is lightarchitects-webshell running? ({e})"
            ))
        })?;

    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);

    if !status.is_success() {
        return Err(GatewayError::InvalidRequest(format!(
            "webshell returned {status} for {url}: {body}"
        )));
    }

    Ok(body)
}

/// Make a POST request to the webshell and return the JSON body.
async fn webshell_post(
    path: &str,
    payload: Value,
    config: &GatewayConfig,
) -> Result<Value, GatewayError> {
    let _ = config;
    let token = read_webshell_token();
    let url = format!("{WEBSHELL_BASE}{path}");

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            GatewayError::InvalidRequest(format!(
                "webshell unreachable at {url} — is lightarchitects-webshell running? ({e})"
            ))
        })?;

    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);

    if !status.is_success() {
        return Err(GatewayError::InvalidRequest(format!(
            "webshell returned {status} for {url}: {body}"
        )));
    }

    Ok(body)
}

// ── Actions ───────────────────────────────────────────────────────────────────

/// `lightarchitects_squad_comms_session_start` — open a per-build soul-chat session.
///
/// Mints a UUID v4 session ID in the gateway (the session authority), then
/// POSTs to the webshell to materialize the soul-chat session. The returned
/// `session_id` must be stored in every task's `build_session_id` field so
/// workers can join the session without a second round-trip.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] if `build_codename` is absent.
/// Returns [`GatewayError::InvalidRequest`] if the webshell is unreachable.
pub async fn session_start(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let build_codename = params["build_codename"]
        .as_str()
        .ok_or(GatewayError::MissingParam("build_codename"))?
        .to_owned();
    let session_id = Uuid::new_v4().to_string();
    let payload = json!({
        "build_codename": build_codename,
        "session_id": session_id,
    });
    webshell_post("/api/coordination/sessions/start", payload, config).await
}

/// `lightarchitects_squad_comms_session_end` — close a per-build soul-chat session.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] if `session_id` is absent.
/// Returns [`GatewayError::InvalidRequest`] if the webshell is unreachable.
pub async fn session_end(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let session_id = params["session_id"]
        .as_str()
        .ok_or(GatewayError::MissingParam("session_id"))?
        .to_owned();
    let payload = json!({ "session_id": session_id });
    webshell_post("/api/coordination/sessions/end", payload, config).await
}

/// `lightarchitects_squad_comms_list_tasks` — list task queue snapshot.
///
/// # Errors
///
/// Returns [`GatewayError::InvalidRequest`] if the webshell is unreachable.
pub async fn list_tasks(_params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    webshell_get("/api/coordination/tasks", config).await
}

/// `lightarchitects_squad_comms_add_task` — append a task to the queue.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] for missing required fields.
/// Returns [`GatewayError::InvalidRequest`] if the webshell is unreachable.
pub async fn add_task(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let title = params["title"]
        .as_str()
        .ok_or(GatewayError::MissingParam("title"))?
        .to_owned();
    let project = params["project"]
        .as_str()
        .ok_or(GatewayError::MissingParam("project"))?
        .to_owned();
    let prompt = params["prompt"]
        .as_str()
        .ok_or(GatewayError::MissingParam("prompt"))?
        .to_owned();
    let priority = params["priority"].as_str().unwrap_or("medium").to_owned();
    let build_codename = params["build_codename"].as_str().map(str::to_owned);
    let assignee = params["assignee"].as_str().map(str::to_owned);
    let build_session_id = params["build_session_id"].as_str().map(str::to_owned);

    let payload = json!({
        "title": title,
        "project": project,
        "prompt": prompt,
        "priority": priority,
        "build_codename": build_codename,
        "assignee": assignee,
        "build_session_id": build_session_id,
    });

    webshell_post("/api/coordination/tasks/add", payload, config).await
}

/// `lightarchitects_squad_comms_claim_task` — soft-claim a task.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] for missing `id`.
/// Returns [`GatewayError::InvalidRequest`] if the webshell is unreachable.
pub async fn claim_task(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let id = params["id"]
        .as_str()
        .ok_or(GatewayError::MissingParam("id"))?;
    // `source` is the claiming agent identifier; webshell ClaimRequest expects `claimant`.
    let claimant = params["source"].as_str().unwrap_or("gateway").to_owned();
    let payload = json!({ "claimant": claimant });
    webshell_post(
        &format!("/api/coordination/tasks/claim/{id}"),
        payload,
        config,
    )
    .await
}

/// `lightarchitects_squad_comms_task_logs` — fetch last 200 lines of a task log.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] for missing `id`.
/// Returns [`GatewayError::InvalidRequest`] if the webshell is unreachable.
pub async fn task_logs(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let id = params["id"]
        .as_str()
        .ok_or(GatewayError::MissingParam("id"))?;
    webshell_get(&format!("/api/coordination/tasks/{id}/logs"), config).await
}

/// `lightarchitects_squad_comms_chat_inject` — inject a chat message.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] for missing required fields.
/// Returns [`GatewayError::InvalidRequest`] if the webshell is unreachable.
pub async fn chat_inject(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let session_id = params["session_id"]
        .as_str()
        .ok_or(GatewayError::MissingParam("session_id"))?
        .to_owned();
    let message = params["message"]
        .as_str()
        .ok_or(GatewayError::MissingParam("message"))?
        .to_owned();
    let sender = params["sender"].as_str().unwrap_or("gateway").to_owned();

    let payload = json!({
        "session_id": session_id,
        "message": message,
        "sender": sender
    });

    webshell_post("/api/coordination/chat/inject", payload, config).await
}

// ── Operator assertion-resolve actions (Wave 3.2) ─────────────────────────────

/// `lightarchitects_squad_comms_resolve_assertion_gate` — submit an operator decision
/// for a blocked assertion gate.
///
/// Expects params: `request_id`, `assertion_id`, `build_id`, `operator_id`,
/// `action_type` (`provide_citation` | escalate | `accept_unvalidated` | dispute),
/// and optionally `citation` (the citation data for `provide_citation` actions).
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] for missing required fields.
/// Returns [`GatewayError::InvalidRequest`] if the webshell is unreachable.
pub async fn resolve_assertion_gate(
    params: Value,
    config: &GatewayConfig,
) -> Result<Value, GatewayError> {
    let request_id = params["request_id"]
        .as_str()
        .ok_or(GatewayError::MissingParam("request_id"))?
        .to_owned();
    let assertion_id = params["assertion_id"]
        .as_str()
        .ok_or(GatewayError::MissingParam("assertion_id"))?
        .to_owned();
    let build_id = params["build_id"]
        .as_str()
        .ok_or(GatewayError::MissingParam("build_id"))?
        .to_owned();
    let operator_id = params["operator_id"]
        .as_str()
        .ok_or(GatewayError::MissingParam("operator_id"))?
        .to_owned();
    let action_type = params["action_type"]
        .as_str()
        .ok_or(GatewayError::MissingParam("action_type"))?
        .to_owned();

    let payload = json!({
        "request_id": request_id,
        "assertion_id": assertion_id,
        "build_id": build_id,
        "operator_id": operator_id,
        "action_type": action_type,
        "citation": params.get("citation"),
    });

    webshell_post("/api/coordination/assertions/resolve", payload, config).await
}

/// `lightarchitects_squad_comms_query_blocked_flow` — query currently blocked
/// assertion gates for a build.
///
/// Expects params: `build_id`, and optionally `assertion_id` to filter results.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] for missing `build_id`.
/// Returns [`GatewayError::InvalidRequest`] if the webshell is unreachable.
pub async fn query_blocked_flow(
    params: Value,
    config: &GatewayConfig,
) -> Result<Value, GatewayError> {
    let build_id = params["build_id"]
        .as_str()
        .ok_or(GatewayError::MissingParam("build_id"))?;
    let path = if let Some(assertion_id) = params["assertion_id"].as_str() {
        format!(
            "/api/coordination/assertions/blocked?build_id={build_id}&assertion_id={assertion_id}"
        )
    } else {
        format!("/api/coordination/assertions/blocked?build_id={build_id}")
    };
    webshell_get(&path, config).await
}
