//! Axum handlers for the setup / backend-selection API.

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    auth,
    config::{
        AgentKind, ClaudeBackend, CodexBackend, CodexConfig, Config, OllamaLaunchConfig,
        SetupConfig,
    },
    server::AppState,
};

/// Returns the user's home directory via the platform home-dir env var.
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

// ── Response types ────────────────────────────────────────────────────────────

/// Auth status for Claude Code (Anthropic) backend.
#[derive(Debug, Clone, Serialize)]
pub struct ClaudeAuthStatus {
    /// `~/.claude/` contains at least one `.json` file (OAuth tokens).
    pub has_keychain_auth: bool,
    /// `ANTHROPIC_API_KEY` is set and not a placeholder value.
    pub has_api_key: bool,
    /// Resolved login method.
    pub login_method: String,
}

/// Auth status for Codex (`OpenAI`) backend.
#[derive(Debug, Clone, Serialize)]
pub struct CodexAuthStatus {
    /// `~/.codex/auth.json` or similar auth file is present.
    pub has_keychain_auth: bool,
    /// `OPENAI_API_KEY` is set.
    pub has_api_key: bool,
    /// Resolved login method.
    pub login_method: String,
}

/// Connectivity status for the Ollama backend.
#[derive(Debug, Clone, Serialize)]
pub struct OllamaAuthStatus {
    /// Configured base URL.
    pub base_url: String,
    /// Whether the Ollama endpoint responded within 2 seconds.
    pub reachable: bool,
}

/// Full auth status snapshot returned by `GET /api/setup/info`.
#[derive(Debug, Clone, Serialize)]
pub struct AuthStatus {
    /// Claude Code / Anthropic auth status.
    pub claude: ClaudeAuthStatus,
    /// Codex / `OpenAI` auth status.
    pub codex: CodexAuthStatus,
    /// Ollama connectivity status.
    pub ollama: OllamaAuthStatus,
}

/// Response shape for `GET /api/setup/info`.
#[derive(Debug, Serialize)]
pub struct SetupInfoResponse {
    /// Whether a valid `setup.json` exists.
    pub setup_complete: bool,
    /// Persisted config, if setup is complete.
    pub config: Option<SetupConfig>,
    /// Live auth detection results.
    pub auth_status: AuthStatus,
}

/// A single model option returned by `GET /api/setup/models`.
#[derive(Debug, Serialize)]
pub struct ModelOption {
    /// Model identifier.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Capability tier: `"flagship"`, `"balanced"`, or `"fast"`.
    pub tier: String,
}

/// Response shape for `GET /api/setup/models`.
#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    /// Available models for the requested backend.
    pub models: Vec<ModelOption>,
}

/// Request body for `POST /api/setup/save`.
#[derive(Debug, Deserialize)]
pub struct SaveRequest {
    /// Agent binary to use.
    pub agent: AgentKind,
    /// Backend identifier (`"anthropic"`, `"ollama-launch"`, `"openai"`).
    pub backend: String,
    /// Model override (optional).
    pub model: Option<String>,
    /// Ollama base URL (optional — only for ollama backends).
    pub ollama_base_url: Option<String>,
    /// API key to store in keychain (optional — never written to disk).
    pub api_key: Option<String>,
}

/// Response shape for `POST /api/setup/save`.
#[derive(Debug, Serialize)]
pub struct SaveResponse {
    /// Always `true` on success.
    pub ok: bool,
}

/// Query parameters for `GET /api/setup/models`.
#[derive(Debug, Deserialize)]
pub struct ModelsQuery {
    /// Backend identifier.
    pub backend: String,
    /// Ollama base URL override.
    pub base_url: Option<String>,
}

// ── Auth detection (filesystem heuristics — no network) ──────────────────────

/// Detect Claude Code auth state from the filesystem and env.
fn detect_claude_auth() -> ClaudeAuthStatus {
    let home = home_dir();

    let has_keychain_auth = home.as_ref().is_some_and(|h| {
        let claude_dir = h.join(".claude");
        claude_dir.is_dir()
            && std::fs::read_dir(&claude_dir)
                .map(|mut d| {
                    d.any(|e| {
                        e.ok()
                            .and_then(|e| e.path().extension().map(|x| x == "json"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
    });

    let has_api_key = std::env::var("ANTHROPIC_API_KEY")
        .map(|k| !k.is_empty() && k != "your_anthropic_key_here")
        .unwrap_or(false);

    let login_method = if has_keychain_auth {
        "oauth".to_owned()
    } else if has_api_key {
        "api_key".to_owned()
    } else {
        "none".to_owned()
    };

    ClaudeAuthStatus {
        has_keychain_auth,
        has_api_key,
        login_method,
    }
}

/// Detect Codex auth state from the filesystem and env.
fn detect_codex_auth() -> CodexAuthStatus {
    let home = home_dir();

    let has_keychain_auth = home.as_ref().is_some_and(|h| {
        let codex_dir = h.join(".codex");
        codex_dir.join("auth.json").exists()
            || (codex_dir.is_dir()
                && std::fs::read_dir(&codex_dir)
                    .map(|mut d| d.any(|e| e.is_ok()))
                    .unwrap_or(false))
    });

    let has_api_key = std::env::var("OPENAI_API_KEY")
        .map(|k| !k.is_empty())
        .unwrap_or(false);

    let login_method = if has_keychain_auth {
        "chatgpt".to_owned()
    } else if has_api_key {
        "api_key".to_owned()
    } else {
        "none".to_owned()
    };

    CodexAuthStatus {
        has_keychain_auth,
        has_api_key,
        login_method,
    }
}

/// Check whether the Ollama endpoint is reachable (2s timeout).
async fn detect_ollama_status(base_url: &str) -> OllamaAuthStatus {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap_or_default();

    let reachable = client
        .get(format!("{base_url}/api/tags"))
        .send()
        .await
        .is_ok();

    OllamaAuthStatus {
        base_url: base_url.to_owned(),
        reachable,
    }
}

// ── Model lists ───────────────────────────────────────────────────────────────

fn anthropic_models() -> Vec<ModelOption> {
    vec![
        ModelOption {
            id: "claude-opus-4-7".to_owned(),
            label: "Claude Opus 4.7".to_owned(),
            tier: "flagship".to_owned(),
        },
        ModelOption {
            id: "claude-sonnet-4-6".to_owned(),
            label: "Claude Sonnet 4.6".to_owned(),
            tier: "balanced".to_owned(),
        },
        ModelOption {
            id: "claude-haiku-4-5-20251001".to_owned(),
            label: "Claude Haiku 4.5".to_owned(),
            tier: "fast".to_owned(),
        },
    ]
}

/// Read the `model` key from `~/.codex/config.toml`, returning a placeholder if absent.
fn codex_models() -> Vec<ModelOption> {
    let from_config = home_dir()
        .and_then(|h| std::fs::read_to_string(h.join(".codex").join("config.toml")).ok())
        .and_then(|s| {
            s.lines()
                .find(|l| l.trim_start().starts_with("model"))
                .and_then(|l| l.split_once('=').map(|x| x.1))
                .map(|v| v.trim().trim_matches('"').to_owned())
        });

    if let Some(model) = from_config {
        vec![ModelOption {
            id: model.clone(),
            label: format!("{model} (from ~/.codex/config.toml)"),
            tier: "balanced".to_owned(),
        }]
    } else {
        vec![ModelOption {
            id: String::new(),
            label: "(from ~/.codex/config.toml)".to_owned(),
            tier: "balanced".to_owned(),
        }]
    }
}

/// Fetch available models from the Ollama `/api/tags` endpoint.
async fn ollama_models(base_url: &str) -> Vec<ModelOption> {
    #[derive(Deserialize)]
    struct TagsResp {
        models: Vec<OllamaModel>,
    }
    #[derive(Deserialize)]
    struct OllamaModel {
        name: String,
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()
        .unwrap_or_default();

    let Ok(resp) = client.get(format!("{base_url}/api/tags")).send().await else {
        return vec![];
    };
    let Ok(tags) = resp.json::<TagsResp>().await else {
        return vec![];
    };

    tags.models
        .into_iter()
        .map(|m| ModelOption {
            label: m.name.clone(),
            id: m.name,
            tier: "balanced".to_owned(),
        })
        .collect()
}

// ── Build AgentSession from SaveRequest ──────────────────────────────────────

fn agent_session_from_save(req: &SaveRequest) -> Option<crate::config::AgentSession> {
    match req.agent {
        AgentKind::Lightarchitects => {
            let backend = match req.backend.as_str() {
                "anthropic" => ClaudeBackend::Anthropic,
                "ollama-launch" | "ollama_launch" => {
                    ClaudeBackend::OllamaLaunch(OllamaLaunchConfig {
                        model: req
                            .model
                            .clone()
                            .unwrap_or_else(|| "qwen3-coder:480b-cloud".to_owned()),
                        base_url: req
                            .ollama_base_url
                            .clone()
                            .unwrap_or_else(|| "http://localhost:11434".to_owned()),
                    })
                }
                _ => return None,
            };
            Some(crate::config::AgentSession::Lightarchitects(backend))
        }
        AgentKind::Codex => {
            let backend = match req.backend.as_str() {
                "openai" => CodexBackend::OpenAi,
                "ollama-launch" | "ollama_launch" => {
                    CodexBackend::OllamaLaunch(OllamaLaunchConfig {
                        model: req
                            .model
                            .clone()
                            .unwrap_or_else(|| "qwen3-coder:480b-cloud".to_owned()),
                        base_url: req
                            .ollama_base_url
                            .clone()
                            .unwrap_or_else(|| "http://localhost:11434".to_owned()),
                    })
                }
                _ => return None,
            };
            Some(crate::config::AgentSession::Codex(CodexConfig {
                model: req.model.clone().unwrap_or_else(|| "gpt-4o".to_owned()),
                backend,
            }))
        }
        AgentKind::LightarchitectsNative => {
            Some(crate::config::AgentSession::LightarchitectsNative(
                crate::config::LightarchitectsNativeConfig {
                    binary: "laex0".to_owned(),
                },
            ))
        }
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /api/setup/info` — returns setup state + auth detection (unauthenticated).
pub async fn setup_info(State(_state): State<AppState>) -> impl IntoResponse {
    let setup_complete = Config::is_setup_complete();
    let config = Config::load_setup();

    let claude = detect_claude_auth();
    let codex = detect_codex_auth();
    let ollama_url = config
        .as_ref()
        .and_then(|c| c.ollama_base_url.clone())
        .unwrap_or_else(|| "http://localhost:11434".to_owned());
    let ollama = detect_ollama_status(&ollama_url).await;

    let auth_status = AuthStatus {
        claude,
        codex,
        ollama,
    };

    Json(SetupInfoResponse {
        setup_complete,
        config,
        auth_status,
    })
    .into_response()
}

/// `GET /api/setup/models` — returns available models for the requested backend.
pub async fn setup_models(Query(q): Query<ModelsQuery>) -> impl IntoResponse {
    let models = match q.backend.as_str() {
        "anthropic" => anthropic_models(),
        "openai" | "codex" => codex_models(),
        "ollama-launch" | "ollama_launch" | "ollama" => {
            let url = q.base_url.as_deref().unwrap_or("http://localhost:11434");
            ollama_models(url).await
        }
        _ => vec![],
    };
    Json(ModelsResponse { models }).into_response()
}

/// `POST /api/setup/save` — persist config + hot-reload the active agent (authenticated).
pub async fn setup_save(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<SaveRequest>,
) -> impl IntoResponse {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !auth::validate_bearer(authz, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Store API key in OS keychain if provided.
    if let Some(ref key) = req.api_key {
        if !key.is_empty() {
            if let Ok(entry) = keyring::Entry::new("lightarchitects-webshell-setup", &req.backend) {
                let _ = entry.set_password(key);
            }
        }
    }

    let setup_cfg = SetupConfig {
        agent: req.agent,
        backend: req.backend.clone(),
        model: req.model.clone(),
        ollama_base_url: req.ollama_base_url.clone(),
        api_key_stored: req.api_key.as_ref().is_some_and(|k| !k.is_empty()),
    };

    if let Err(e) = Config::save_setup(&setup_cfg) {
        tracing::error!(target: "setup", "Failed to persist setup config: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if let Some(new_agent) = agent_session_from_save(&req) {
        *state.active_agent.write().await = new_agent;
        info!(target: "setup", backend = %req.backend, "Active agent updated live");
    }

    Json(SaveResponse { ok: true }).into_response()
}

/// `DELETE /api/setup/reset` — wipe setup config, frontend returns to splash (authenticated).
pub async fn setup_reset(headers: HeaderMap, State(state): State<AppState>) -> impl IntoResponse {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !auth::validate_bearer(authz, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    if let Err(e) = Config::delete_setup() {
        tracing::error!(target: "setup", "Failed to delete setup config: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    info!(target: "setup", "Setup config reset — frontend will re-enter setup flow");
    StatusCode::NO_CONTENT.into_response()
}
