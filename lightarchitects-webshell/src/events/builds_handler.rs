//! `/api/builds` routes.
//!
//! - `GET /api/builds` — returns the parsed `active.yaml` build tracking data
//!   (cached by mtime; 503 if the vault is missing).
//! - `POST /api/builds` — creates a new live build session (Phase C):
//!   mints a UUID + random 32-byte notify token, inserts an
//!   `Arc<BuildSession>` into the registry, returns public metadata.
//!   The notify token is *never* returned — it lives server-side and is
//!   injected into the PTY child's env on spawn.
//! - `GET /api/builds/:id` — returns public metadata for one live build.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    auth,
    config::{AgentSession, ClaudeBackend},
    server::AppState,
    session::BuildSession,
};

/// Cached build data: (mtime, serialised JSON bytes).
pub type Cache = Arc<Mutex<Option<(SystemTime, Vec<u8>)>>>;

/// Shared cache instance, created once per server lifetime.
#[must_use]
pub fn build_cache() -> Cache {
    Arc::new(Mutex::new(None))
}

/// `GET /api/builds` — returns build tracking data as JSON.
///
/// Auth-gated (same Bearer token as `/api/events`).
/// Returns 503 if the vault is not configured or the file doesn't exist.
#[allow(clippy::missing_panics_doc)]
pub async fn builds_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Validate bearer token.
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !auth::validate_bearer(authz, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Resolve the active.yaml path.
    let Some(helix_root) = lightarchitects::core::paths::helix_root() else {
        warn!("helix_root unavailable — cannot serve /api/builds");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let path = helix_root.join("corso").join("builds").join("active.yaml");

    let metadata = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, path = %path.display(), "active.yaml not found");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };

    let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    // Check the cache — if mtime matches, return cached bytes.
    // Mutex lock is held briefly; a poisoned lock from a panic is acceptable
    // because the server would be in an inconsistent state anyway.
    #[allow(clippy::unwrap_used)]
    let cache_hit = {
        let cache = state.builds_cache.lock().unwrap();
        cache.as_ref().and_then(|(cached_mtime, cached_bytes)| {
            if *cached_mtime == mtime {
                Some(cached_bytes.clone())
            } else {
                None
            }
        })
    };

    if let Some(cached_bytes) = cache_hit {
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            cached_bytes,
        )
            .into_response();
    }

    // Read and parse the YAML file.
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, path = %path.display(), "failed to read active.yaml");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let yaml_value: serde_yaml::Value = match serde_yaml::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "failed to parse active.yaml");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Convert YAML → JSON for the browser.
    let json_value = serde_json::to_value(&yaml_value).unwrap_or_else(|e| {
        warn!(error = %e, "failed to convert YAML to JSON");
        serde_json::Value::Null
    });

    let json_bytes = match serde_json::to_vec_pretty(&json_value) {
        Ok(b) => b,
        Err(e) => {
            warn!(error = %e, "failed to serialise builds JSON");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    info!(path = %path.display(), "served /api/builds");

    // Update cache.
    #[allow(clippy::unwrap_used)]
    {
        *state.builds_cache.lock().unwrap() = Some((mtime, json_bytes.clone()));
    }

    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        json_bytes,
    )
        .into_response()
}

// ── POST /api/builds ─────────────────────────────────────────────────────────

/// Request body for `POST /api/builds`.
///
/// `cwd` is required — the PTY child will run with this as its working
/// directory and the project-scoped `.mcp.json` will be written here on
/// spawn (Phase C-2, follow-up). The remaining fields are optional
/// per-build overrides of the corresponding [`BuildSession`] flags.
#[derive(Debug, Deserialize)]
pub struct CreateBuildRequest {
    /// Working directory for the PTY child process.
    pub cwd: PathBuf,
    /// Claude agent template name (`claude --agent <name>`). Falls back
    /// to [`crate::config::Config::claude_agent_template`] when absent.
    #[serde(default)]
    pub claude_agent_template: Option<String>,
    /// Override for `claude --model`.
    #[serde(default)]
    pub model: Option<String>,
    /// Override for `claude --system-prompt`.
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Override for `claude --append-system-prompt`.
    #[serde(default)]
    pub append_system_prompt: Option<String>,
    /// Override for `claude --allowedTools`.
    #[serde(default)]
    pub allowed_tools: Option<String>,
    /// Override for `claude --disallowedTools`.
    #[serde(default)]
    pub disallowed_tools: Option<String>,
}

/// Public response shape for `POST /api/builds` and `GET /api/builds/:id`.
///
/// Deliberately excludes `notify_token` — that secret lives only in the
/// registry and is delivered to the gateway via the PTY child's
/// `LA_NOTIFY_TOKEN` env var.
#[derive(Debug, Serialize)]
pub struct BuildResponse {
    /// The fresh `Uuid` minted on creation.
    pub build_id: Uuid,
    /// Working directory for this build's PTY child.
    pub cwd: PathBuf,
    /// Redacted agent descriptor — kind + backend name only, no secrets.
    pub agent: AgentDescriptor,
    /// Echo of the resolved Claude agent template, if any.
    pub claude_agent_template: Option<String>,
    /// Echo of the model override, if any.
    pub model: Option<String>,
}

/// Sanitised view of [`AgentSession`] — omits Ollama `auth_token`.
#[derive(Debug, Serialize)]
pub struct AgentDescriptor {
    /// Agent binary family, e.g. `"lightarchitects"`, `"codex"`.
    pub kind: &'static str,
    /// Backend routing (e.g. `"anthropic"`, `"ollama"`).
    pub backend: &'static str,
}

impl AgentDescriptor {
    /// Derive a descriptor from an [`AgentSession`] without touching
    /// sensitive fields (auth tokens, base URLs).
    #[must_use]
    pub fn from_session(agent: &AgentSession) -> Self {
        match agent {
            AgentSession::Lightarchitects(ClaudeBackend::Anthropic) => Self {
                kind: "lightarchitects",
                backend: "anthropic",
            },
            AgentSession::Lightarchitects(ClaudeBackend::OllamaLaunch(_)) => Self {
                kind: "lightarchitects",
                backend: "ollama_launch",
            },
            AgentSession::Lightarchitects(ClaudeBackend::Ollama(_)) => Self {
                kind: "lightarchitects",
                backend: "ollama",
            },
            AgentSession::Codex(cfg) => Self {
                kind: "codex",
                backend: match &cfg.backend {
                    crate::config::CodexBackend::OpenAi => "openai",
                    crate::config::CodexBackend::OllamaLaunch(_) => "ollama_launch",
                },
            },
            AgentSession::LightarchitectsNative(_) => Self {
                kind: "lightarchitects_native",
                backend: "native",
            },
        }
    }
}

/// `POST /api/builds` — create a new live build session.
///
/// Auth-gated (global Bearer token). The request body is the
/// [`CreateBuildRequest`] shape; optional fields fall back to `Config`
/// defaults. Returns a [`BuildResponse`] JSON with the minted UUID.
///
/// The per-build 32-byte notify token is *not* returned — it lives in the
/// server-side registry and is injected into the PTY child's env var on
/// spawn (see [`BuildSession::build_spawn_env`]).
#[allow(clippy::missing_panics_doc)]
pub async fn create_build_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<CreateBuildRequest>,
) -> impl IntoResponse {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !auth::validate_bearer(authz, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Use the active agent session (updated live by /api/setup/save).
    let agent = state.active_agent.read().await.clone();
    let mut session = BuildSession::new(body.cwd.clone(), agent);
    session.claude_agent_template = body
        .claude_agent_template
        .or_else(|| state.config.claude_agent_template.clone());
    session.model = body.model;
    session.system_prompt = body.system_prompt;
    session.append_system_prompt = body.append_system_prompt;
    session.allowed_tools = body.allowed_tools;
    session.disallowed_tools = body.disallowed_tools;

    let resp = BuildResponse {
        build_id: session.build_id,
        cwd: session.cwd.clone(),
        agent: AgentDescriptor::from_session(&session.agent),
        claude_agent_template: session.claude_agent_template.clone(),
        model: session.model.clone(),
    };

    let session = Arc::new(session);
    let _prev = state.builds.insert(Arc::clone(&session));
    info!(build_id = %resp.build_id, cwd = %body.cwd.display(), "build session created");

    (StatusCode::OK, Json(resp)).into_response()
}

/// `GET /api/builds/:id` — return public metadata for a live build.
///
/// Auth-gated (global Bearer token). Returns 404 if the build is not in
/// the registry. The response never contains the notify token.
pub async fn build_details_handler(
    Path(build_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !auth::validate_bearer(authz, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let Some(session) = state.builds.get(build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let resp = BuildResponse {
        build_id: session.build_id,
        cwd: session.cwd.clone(),
        agent: AgentDescriptor::from_session(&session.agent),
        claude_agent_template: session.claude_agent_template.clone(),
        model: session.model.clone(),
    };

    (StatusCode::OK, Json(resp)).into_response()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn build_cache_initialises_empty() {
        let cache = build_cache();
        assert!(cache.lock().unwrap().is_none());
    }

    #[test]
    fn agent_descriptor_redacts_anthropic() {
        let d =
            AgentDescriptor::from_session(&AgentSession::Lightarchitects(ClaudeBackend::Anthropic));
        assert_eq!(d.kind, "lightarchitects");
        assert_eq!(d.backend, "anthropic");
    }

    #[test]
    fn agent_descriptor_redacts_ollama_auth_token() {
        use crate::config::OllamaConfig;
        let oc = OllamaConfig {
            base_url: "http://localhost:11434".to_owned(),
            model: "qwen3-coder:480b-cloud".to_owned(),
            auth_token: "sk-super-secret".to_owned(),
        };
        let sess = AgentSession::Lightarchitects(ClaudeBackend::Ollama(oc));
        let d = AgentDescriptor::from_session(&sess);
        let json = serde_json::to_string(&d).unwrap();
        assert!(
            !json.contains("sk-super-secret"),
            "auth_token must not appear in AgentDescriptor output: {json}"
        );
        assert!(
            !json.contains("11434"),
            "base_url must not appear either: {json}"
        );
        assert_eq!(d.backend, "ollama");
    }

    #[test]
    fn build_response_omits_notify_token_field() {
        use crate::config::OllamaConfig;
        let _ = OllamaConfig {
            base_url: String::new(),
            model: String::new(),
            auth_token: String::new(),
        };
        let resp = BuildResponse {
            build_id: Uuid::new_v4(),
            cwd: PathBuf::from("/tmp"),
            agent: AgentDescriptor::from_session(&AgentSession::Lightarchitects(
                ClaudeBackend::Anthropic,
            )),
            claude_agent_template: None,
            model: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(
            !json.contains("notify_token"),
            "public response must never include notify_token: {json}"
        );
    }

    #[test]
    fn create_build_request_accepts_minimal_body() {
        let body = r#"{"cwd":"/tmp/build-1"}"#;
        let req: CreateBuildRequest = serde_json::from_str(body).unwrap();
        assert_eq!(req.cwd, PathBuf::from("/tmp/build-1"));
        assert!(req.claude_agent_template.is_none());
    }

    #[test]
    fn create_build_request_accepts_full_body() {
        let body = r#"{
            "cwd":"/tmp/build-2",
            "claude_agent_template":"corso",
            "model":"opus",
            "allowed_tools":"Read Grep"
        }"#;
        let req: CreateBuildRequest = serde_json::from_str(body).unwrap();
        assert_eq!(req.claude_agent_template.as_deref(), Some("corso"));
        assert_eq!(req.model.as_deref(), Some("opus"));
        assert_eq!(req.allowed_tools.as_deref(), Some("Read Grep"));
    }

    #[test]
    fn agent_descriptor_lightarchitects_native() {
        use crate::config::LightarchitectsNativeConfig;
        let sess = AgentSession::LightarchitectsNative(LightarchitectsNativeConfig::default());
        let d = AgentDescriptor::from_session(&sess);
        assert_eq!(d.kind, "lightarchitects_native");
        assert_eq!(d.backend, "native");
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("laex0"), "binary path must not leak: {json}");
    }
}
