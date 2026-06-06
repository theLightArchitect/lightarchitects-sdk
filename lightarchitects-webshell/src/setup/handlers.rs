//! Axum handlers for the setup / backend-selection API.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    auth,
    config::{
        AgentKind, ClaudeBackend, CodexBackend, CodexConfig, Config, DEFAULT_OLLAMA_BASE_URL,
        DEFAULT_OLLAMA_MODEL, MistralVibeConfig, OllamaLaunchConfig, SetupConfig,
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
    /// Credentials present in macOS Keychain (canonical probe via SDK).
    pub has_keychain_auth: bool,
    /// `ANTHROPIC_API_KEY` is set and not a placeholder value.
    pub has_api_key: bool,
    /// Resolved login method (`keychain` / `file` / `api_key` / `none`).
    pub login_method: String,
    /// Human-readable source (e.g. `"macOS Keychain (Claude Code-credentials)"`,
    /// `"~/.claude/.credentials.json"`, `"ANTHROPIC_API_KEY env"`).
    pub login_source: String,
}

/// Auth status for Codex (`OpenAI`) backend.
#[derive(Debug, Clone, Serialize)]
pub struct CodexAuthStatus {
    /// Credentials present at canonical location (file or keyring).
    pub has_keychain_auth: bool,
    /// `OPENAI_API_KEY` is set.
    pub has_api_key: bool,
    /// Resolved login method (`file` / `keychain` / `api_key` / `none`).
    pub login_method: String,
    /// Human-readable source (e.g. `"~/.codex/auth.json"`, `"OPENAI_API_KEY env"`).
    pub login_source: String,
}

/// Connectivity status for the Ollama backend.
#[derive(Debug, Clone, Serialize)]
pub struct OllamaAuthStatus {
    /// Configured base URL.
    pub base_url: String,
    /// Whether the Ollama endpoint responded within 2 seconds.
    pub reachable: bool,
}

/// Auth status for the Mistral backend.
#[derive(Debug, Clone, Serialize)]
pub struct MistralAuthStatus {
    /// API key is stored in macOS Keychain or `MISTRAL_API_KEY` env.
    pub has_api_key: bool,
    /// Human-readable source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_source: Option<String>,
}

/// Auth status for the `OpenRouter` backend.
#[derive(Debug, Clone, Serialize)]
pub struct OpenRouterAuthStatus {
    /// API key is stored in macOS Keychain or `OPENROUTER_API_KEY` env.
    pub has_api_key: bool,
    /// Human-readable source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_source: Option<String>,
}

/// Auth status for the Ollama Cloud backend.
#[derive(Debug, Clone, Serialize)]
pub struct OllamaCloudAuthStatus {
    /// Bearer token stored in macOS Keychain.
    pub has_api_key: bool,
    /// Human-readable source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_source: Option<String>,
}

/// Auth status for the `DeepSeek` backend.
#[derive(Debug, Clone, Serialize)]
pub struct DeepSeekAuthStatus {
    /// API key stored in macOS Keychain.
    pub has_api_key: bool,
    /// Human-readable source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_source: Option<String>,
}

/// Auth status for the Google Vertex AI backend.
#[derive(Debug, Clone, Serialize)]
pub struct GoogleVertexAuthStatus {
    /// Service account JSON stored in macOS Keychain.
    pub has_service_account: bool,
    /// GCP project ID if configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
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
    /// Mistral API auth status.
    pub mistral: MistralAuthStatus,
    /// `OpenRouter` API auth status (deprecated — kept for wire-format compat).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openrouter: Option<OpenRouterAuthStatus>,
    /// Ollama Cloud Bearer token auth status.
    pub ollama_cloud: OllamaCloudAuthStatus,
    /// `DeepSeek` API key auth status.
    pub deepseek: DeepSeekAuthStatus,
    /// Google Vertex AI service account auth status.
    pub google_vertex: GoogleVertexAuthStatus,
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
    /// Session UUID pre-seeded via `--resume-session <id>`, if any.
    ///
    /// The frontend reads this on boot and forwards it as
    /// `resume_session_id` on its first `POST /api/builds` so the next
    /// copilot turn invokes `claude --resume <id>` (or
    /// `codex exec resume <id>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_session: Option<String>,
    /// Working directory the webshell was launched from.
    ///
    /// The frontend should use this as the default CWD for new builds
    /// so the copilot operates on the correct project (not `/tmp`).
    pub cwd: String,
}

/// A single model option returned by `GET /api/setup/models`.
#[derive(Debug, Default, Serialize)]
pub struct ModelOption {
    /// Model identifier.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Capability tier: `"flagship"`, `"balanced"`, `"capable"`, or `"fast"`.
    pub tier: String,
    /// Model family (e.g. `"GLM"`, `"DeepSeek"`). Populated for `ollama-cloud` only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Whether the model supports tool use / function calling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use: Option<bool>,
    /// Whether the model supports vision (image) inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vision: Option<bool>,
    /// Context window size in thousands of tokens (e.g. `128` for 128 K).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_k: Option<u32>,
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

// ── Auth detection (canonical via SDK credentials module) ────────────────────

/// Format a detailed locator as a human-readable source string.
///
/// Safe to render in UI; the SDK's own `Debug` impl redacts these fields,
/// but rendering to user-chosen UI is explicitly allowed.
fn format_source(dl: &lightarchitects::credentials::DetailedLocator) -> String {
    use lightarchitects::credentials::DetailedLocator;
    match dl {
        DetailedLocator::Absent => "none".to_owned(),
        DetailedLocator::Keychain { service, .. } => {
            format!("macOS Keychain ({service})")
        }
        DetailedLocator::File { path } => path.display().to_string(),
        DetailedLocator::Env { var } => format!("{var} env"),
    }
}

fn login_method_from(dl: &lightarchitects::credentials::DetailedLocator) -> &'static str {
    use lightarchitects::credentials::DetailedLocator;
    match dl {
        DetailedLocator::Absent => "none",
        DetailedLocator::Keychain { .. } => "keychain",
        DetailedLocator::File { .. } => "file",
        DetailedLocator::Env { .. } => "api_key",
    }
}

/// Detect Claude Code auth state via the SDK credentials registry.
async fn detect_claude_auth() -> ClaudeAuthStatus {
    let registry = lightarchitects::credentials::default_registry();
    let dl = match registry
        .probe_detailed(lightarchitects::credentials::ANTHROPIC_CLI)
        .await
    {
        Some(Ok(dl)) => dl,
        _ => lightarchitects::credentials::DetailedLocator::Absent,
    };

    let has_api_key = std::env::var("ANTHROPIC_API_KEY")
        .map(|k| !k.is_empty() && k != "your_anthropic_key_here")
        .unwrap_or(false);
    let has_keychain_auth = matches!(
        dl,
        lightarchitects::credentials::DetailedLocator::Keychain { .. }
            | lightarchitects::credentials::DetailedLocator::File { .. }
    );

    ClaudeAuthStatus {
        has_keychain_auth,
        has_api_key,
        login_method: login_method_from(&dl).to_owned(),
        login_source: format_source(&dl),
    }
}

/// Detect Codex auth state via the SDK credentials registry.
async fn detect_codex_auth() -> CodexAuthStatus {
    let registry = lightarchitects::credentials::default_registry();
    let dl = match registry
        .probe_detailed(lightarchitects::credentials::OPENAI_CLI)
        .await
    {
        Some(Ok(dl)) => dl,
        _ => lightarchitects::credentials::DetailedLocator::Absent,
    };

    let has_api_key = std::env::var("OPENAI_API_KEY")
        .map(|k| !k.is_empty())
        .unwrap_or(false);
    let has_keychain_auth = matches!(
        dl,
        lightarchitects::credentials::DetailedLocator::Keychain { .. }
            | lightarchitects::credentials::DetailedLocator::File { .. }
    );

    CodexAuthStatus {
        has_keychain_auth,
        has_api_key,
        login_method: login_method_from(&dl).to_owned(),
        login_source: format_source(&dl),
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

/// Read a stored API key from the webshell setup keychain service, using
/// `security(1)` CLI to avoid GUI dialogs with ad-hoc-signed binaries.
fn keychain_stored_key(account: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                "lightarchitects-webshell-setup",
                "-a",
                account,
                "-w",
            ])
            .output()
            .ok()?;
        if out.status.success() {
            let s = String::from_utf8(out.stdout).ok()?;
            let t = s.trim().to_owned();
            if !t.is_empty() {
                return Some(t);
            }
        }
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = account;
        None
    }
}

/// Detect whether a Mistral API key is available (keychain or env var).
fn detect_mistral_auth() -> MistralAuthStatus {
    if keychain_stored_key("mistral-vibe").is_some() {
        return MistralAuthStatus {
            has_api_key: true,
            login_source: Some("macOS Keychain".to_owned()),
        };
    }
    if std::env::var("MISTRAL_API_KEY")
        .map(|k| !k.is_empty())
        .unwrap_or(false)
    {
        return MistralAuthStatus {
            has_api_key: true,
            login_source: Some("MISTRAL_API_KEY env".to_owned()),
        };
    }
    MistralAuthStatus {
        has_api_key: false,
        login_source: None,
    }
}

/// Detect whether an Ollama Cloud Bearer token is available (Keychain only).
fn detect_ollama_cloud_auth() -> OllamaCloudAuthStatus {
    use crate::auth::credential::ollama_cloud;
    match crate::auth::credential::keychain::keychain_get(ollama_cloud::KEYCHAIN_SERVICE) {
        Ok(Some(_)) => OllamaCloudAuthStatus {
            has_api_key: true,
            login_source: Some("macOS Keychain".to_owned()),
        },
        _ => OllamaCloudAuthStatus {
            has_api_key: false,
            login_source: None,
        },
    }
}

/// Detect whether a `DeepSeek` API key is available (Keychain or env var).
fn detect_deepseek_auth() -> DeepSeekAuthStatus {
    use crate::auth::credential::deepseek;
    if crate::auth::credential::keychain::keychain_get(deepseek::KEYCHAIN_SERVICE)
        .ok()
        .flatten()
        .is_some()
    {
        return DeepSeekAuthStatus {
            has_api_key: true,
            login_source: Some("macOS Keychain".to_owned()),
        };
    }
    if std::env::var("DEEPSEEK_API_KEY")
        .map(|k| !k.is_empty())
        .unwrap_or(false)
    {
        return DeepSeekAuthStatus {
            has_api_key: true,
            login_source: Some("DEEPSEEK_API_KEY env".to_owned()),
        };
    }
    DeepSeekAuthStatus {
        has_api_key: false,
        login_source: None,
    }
}

/// Detect whether Google Vertex AI credentials are configured (Keychain only).
fn detect_google_vertex_auth() -> GoogleVertexAuthStatus {
    use crate::auth::credential::vertex;
    let has_service_account =
        crate::auth::credential::keychain::keychain_get(vertex::KEYCHAIN_SERVICE)
            .ok()
            .flatten()
            .is_some();
    let project_id = vertex::load_project_id().ok().flatten();
    GoogleVertexAuthStatus {
        has_service_account,
        project_id,
    }
}

// ── Model lists ───────────────────────────────────────────────────────────────

/// Models available for the lightarchitects native CLI agent.
/// Mirrors the CLI catalogue (src/llm/catalogue.rs).
fn lightarchitects_models() -> Vec<ModelOption> {
    vec![
        ModelOption {
            id: "nemotron-3-super:120b-cloud".to_owned(),
            label: "Nemotron 3 Super 120B".to_owned(),
            tier: "capable".to_owned(),
            ..Default::default()
        },
        ModelOption {
            id: "nemotron-3-super:cloud".to_owned(),
            label: "Nemotron 3 Super Cloud".to_owned(),
            tier: "capable".to_owned(),
            ..Default::default()
        },
        ModelOption {
            id: "qwen3-coder:480b-cloud".to_owned(),
            label: "Qwen3 Coder 480B Cloud".to_owned(),
            tier: "balanced".to_owned(),
            ..Default::default()
        },
        ModelOption {
            id: "claude-opus-4-7".to_owned(),
            label: "Claude Opus 4.7".to_owned(),
            tier: "flagship".to_owned(),
            ..Default::default()
        },
        ModelOption {
            id: "claude-sonnet-4-6".to_owned(),
            label: "Claude Sonnet 4.6".to_owned(),
            tier: "balanced".to_owned(),
            ..Default::default()
        },
        ModelOption {
            id: "claude-haiku-4-5-20251001".to_owned(),
            label: "Claude Haiku 4.5".to_owned(),
            tier: "fast".to_owned(),
            ..Default::default()
        },
    ]
}

fn anthropic_models() -> Vec<ModelOption> {
    vec![
        ModelOption {
            id: "claude-opus-4-7".to_owned(),
            label: "Claude Opus 4.7".to_owned(),
            tier: "flagship".to_owned(),
            ..Default::default()
        },
        ModelOption {
            id: "claude-sonnet-4-6".to_owned(),
            label: "Claude Sonnet 4.6".to_owned(),
            tier: "balanced".to_owned(),
            ..Default::default()
        },
        ModelOption {
            id: "claude-haiku-4-5-20251001".to_owned(),
            label: "Claude Haiku 4.5".to_owned(),
            tier: "fast".to_owned(),
            ..Default::default()
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
            ..Default::default()
        }]
    } else {
        vec![ModelOption {
            id: String::new(),
            label: "(from ~/.codex/config.toml)".to_owned(),
            tier: "balanced".to_owned(),
            ..Default::default()
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
            ..Default::default()
        })
        .collect()
}

/// Returns all Ollama Cloud models from the compiled `CLOUD_MODEL_REGISTRY`.
fn ollama_cloud_models() -> Vec<ModelOption> {
    use lightarchitects::agent::{CLOUD_MODEL_REGISTRY, CostTier};
    CLOUD_MODEL_REGISTRY
        .iter()
        .map(|m| ModelOption {
            id: m.slug.to_owned(),
            label: format!("{} — {}", m.display_name, m.provider_org),
            tier: match m.cost_tier {
                CostTier::Low => "fast".to_owned(),
                CostTier::Medium => "balanced".to_owned(),
                CostTier::High => "capable".to_owned(),
                CostTier::Premium => "flagship".to_owned(),
            },
            family: Some(m.family.to_owned()),
            tool_use: Some(m.tool_use),
            vision: Some(m.vision),
            context_k: Some(m.context_length / 1_000),
        })
        .collect()
}

/// A curated set of popular `OpenRouter` models for initial selection.
fn openrouter_models() -> Vec<ModelOption> {
    vec![
        ModelOption {
            id: "openai/gpt-4o".to_owned(),
            label: "GPT-4o (OpenAI via OpenRouter)".to_owned(),
            tier: "balanced".to_owned(),
            ..Default::default()
        },
        ModelOption {
            id: "anthropic/claude-sonnet-4-6".to_owned(),
            label: "Claude Sonnet 4.6 (via OpenRouter)".to_owned(),
            tier: "balanced".to_owned(),
            ..Default::default()
        },
        ModelOption {
            id: "meta-llama/llama-3.3-70b-instruct".to_owned(),
            label: "Llama 3.3 70B (via OpenRouter)".to_owned(),
            tier: "fast".to_owned(),
            ..Default::default()
        },
        ModelOption {
            id: "google/gemini-2.0-flash-001".to_owned(),
            label: "Gemini 2.0 Flash (via OpenRouter)".to_owned(),
            tier: "fast".to_owned(),
            ..Default::default()
        },
        ModelOption {
            id: "deepseek/deepseek-r1".to_owned(),
            label: "DeepSeek R1 (via OpenRouter)".to_owned(),
            tier: "capable".to_owned(),
            ..Default::default()
        },
    ]
}

fn deepseek_models() -> Vec<ModelOption> {
    vec![
        ModelOption {
            id: "deepseek/deepseek-chat".to_owned(),
            label: "DeepSeek V3 (Chat)".to_owned(),
            tier: "balanced".to_owned(),
            tool_use: Some(true),
            ..Default::default()
        },
        ModelOption {
            id: "deepseek/deepseek-reasoner".to_owned(),
            label: "DeepSeek R1 (Reasoning)".to_owned(),
            tier: "capable".to_owned(),
            tool_use: Some(false),
            ..Default::default()
        },
        ModelOption {
            id: "deepseek/deepseek-coder".to_owned(),
            label: "DeepSeek Coder V2".to_owned(),
            tier: "balanced".to_owned(),
            tool_use: Some(true),
            ..Default::default()
        },
    ]
}

fn mistral_models() -> Vec<ModelOption> {
    vec![
        ModelOption {
            id: "mistral/mistral-large-latest".to_owned(),
            label: "Mistral Large".to_owned(),
            tier: "capable".to_owned(),
            tool_use: Some(true),
            ..Default::default()
        },
        ModelOption {
            id: "mistral/mistral-small-latest".to_owned(),
            label: "Mistral Small".to_owned(),
            tier: "fast".to_owned(),
            tool_use: Some(true),
            ..Default::default()
        },
        ModelOption {
            id: "mistral/codestral-latest".to_owned(),
            label: "Codestral (Code)".to_owned(),
            tier: "balanced".to_owned(),
            tool_use: Some(false),
            ..Default::default()
        },
    ]
}

fn google_vertex_models() -> Vec<ModelOption> {
    vec![
        ModelOption {
            id: "vertex_ai/gemini-1.5-pro".to_owned(),
            label: "Gemini 1.5 Pro".to_owned(),
            tier: "capable".to_owned(),
            family: None,
            tool_use: Some(true),
            vision: Some(true),
            context_k: Some(1_000),
        },
        ModelOption {
            id: "vertex_ai/gemini-2.0-flash".to_owned(),
            label: "Gemini 2.0 Flash".to_owned(),
            tier: "fast".to_owned(),
            family: None,
            tool_use: Some(true),
            vision: Some(true),
            context_k: Some(1_000),
        },
        ModelOption {
            id: "vertex_ai/claude-sonnet-4-5@20251001".to_owned(),
            label: "Claude Sonnet 4.5 (via Vertex)".to_owned(),
            tier: "balanced".to_owned(),
            family: None,
            tool_use: Some(true),
            vision: Some(true),
            context_k: Some(200),
        },
    ]
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
                            .unwrap_or_else(|| DEFAULT_OLLAMA_MODEL.to_owned()),
                        base_url: req
                            .ollama_base_url
                            .clone()
                            .unwrap_or_else(|| DEFAULT_OLLAMA_BASE_URL.to_owned()),
                    })
                }
                // BYOK cloud providers routed via LiteLLM proxy.
                "openai" => ClaudeBackend::LiteLlm(crate::config::LiteLlmBackendConfig {
                    model: req
                        .model
                        .clone()
                        .unwrap_or_else(|| "openai/gpt-4o".to_owned()),
                }),
                "openrouter" => ClaudeBackend::LiteLlm(crate::config::LiteLlmBackendConfig {
                    model: req
                        .model
                        .clone()
                        .unwrap_or_else(|| "openrouter/openai/gpt-4o".to_owned()),
                }),
                "mistral" | "mistral-vibe" | "mistral_vibe" => {
                    ClaudeBackend::LiteLlm(crate::config::LiteLlmBackendConfig {
                        model: req
                            .model
                            .clone()
                            .unwrap_or_else(|| "mistral/mistral-large-latest".to_owned()),
                    })
                }
                "ollama-cloud" => ClaudeBackend::LiteLlm(crate::config::LiteLlmBackendConfig {
                    model: req
                        .model
                        .clone()
                        .unwrap_or_else(|| "ollama_chat/llama3.2".to_owned()),
                }),
                "deepseek" => ClaudeBackend::LiteLlm(crate::config::LiteLlmBackendConfig {
                    model: req
                        .model
                        .clone()
                        .unwrap_or_else(|| "deepseek/deepseek-chat".to_owned()),
                }),
                "google-vertex" => ClaudeBackend::LiteLlm(crate::config::LiteLlmBackendConfig {
                    model: req
                        .model
                        .clone()
                        .unwrap_or_else(|| "vertex_ai/gemini-1.5-pro".to_owned()),
                }),
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
                            .unwrap_or_else(|| DEFAULT_OLLAMA_MODEL.to_owned()),
                        base_url: req
                            .ollama_base_url
                            .clone()
                            .unwrap_or_else(|| DEFAULT_OLLAMA_BASE_URL.to_owned()),
                    })
                }
                _ => return None,
            };
            Some(crate::config::AgentSession::Codex(CodexConfig {
                model: req.model.clone().unwrap_or_else(|| "gpt-4o".to_owned()),
                backend,
            }))
        }
        AgentKind::LightArchitect => Some(crate::config::AgentSession::LightArchitect(
            crate::config::LightArchitectConfig {
                binary: "lightarchitects".to_owned(),
                model: req.model.clone(),
            },
        )),
        AgentKind::MistralVibe => Some(crate::config::AgentSession::MistralVibe(
            MistralVibeConfig {
                model: req.model.clone(),
            },
        )),
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// Read the `[llm] model` key from the CLI's TOML config, if it exists.
fn lightarchitects_cli_model_from_toml() -> Option<String> {
    let path = crate::config::lightarchitects_cli_config_path()?;
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(target: "setup", path = %path.display(), "Failed to read CLI TOML config: {e}");
            return None;
        }
    };
    let doc: toml::Table = match content.parse() {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(target: "setup", path = %path.display(), "Failed to parse CLI TOML config: {e}");
            return None;
        }
    };
    doc.get("llm")
        .and_then(|v| v.as_table())
        .and_then(|t| t.get("model"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Write `model` to `[llm] model` in the CLI's TOML config.
///
/// Uses atomic rename via a tempfile to avoid TOCTOU races (CWE-59) and
/// sets `0o600` permissions to match the rest of the codebase (CWE-732).
fn lightarchitects_cli_model_to_toml(model: &str) -> Result<(), std::io::Error> {
    let Some(path) = crate::config::lightarchitects_cli_config_path() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "lightarchitects-cli config path unavailable",
        ));
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => Some(c),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let mut doc: toml::Table = match content {
        Some(c) => c.parse().map_err(|e: toml::de::Error| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?,
        None => toml::Table::new(),
    };
    if doc.get_mut("llm").is_none() {
        let _ = doc.insert("llm".to_owned(), toml::Value::Table(toml::Table::new()));
    }
    if let Some(llm) = doc.get_mut("llm").and_then(|v| v.as_table_mut()) {
        let _ = llm.insert("model".to_owned(), toml::Value::String(model.to_owned()));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let out = toml::to_string_pretty(&doc)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let tmp = path.with_extension(format!("toml.tmp.{}", std::process::id()));
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(out.as_bytes())?;
        file.sync_all()?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// `GET /api/setup/info` — returns setup state + auth detection (unauthenticated).
pub async fn setup_info(State(state): State<AppState>) -> impl IntoResponse {
    let mut setup_complete = Config::is_setup_complete();
    let mut config = Config::load_setup();

    // Auto-complete for native CLI when no explicit setup exists.
    if !setup_complete {
        let is_native_default = matches!(state.config.agent.kind(), AgentKind::LightArchitect);
        if is_native_default {
            setup_complete = true;
            config = Some(SetupConfig {
                agent: AgentKind::LightArchitect,
                backend: "lightarchitects".to_owned(),
                model: lightarchitects_cli_model_from_toml(),
                ollama_base_url: None,
                api_key_stored: false,
            });
        }
    }

    let claude = detect_claude_auth().await;
    let codex = detect_codex_auth().await;
    let ollama_url = config
        .as_ref()
        .and_then(|c| c.ollama_base_url.clone())
        .unwrap_or_else(|| "http://localhost:11434".to_owned());
    let ollama = detect_ollama_status(&ollama_url).await;
    let mistral = detect_mistral_auth();
    let ollama_cloud = detect_ollama_cloud_auth();
    let deepseek = detect_deepseek_auth();
    let google_vertex = detect_google_vertex_auth();

    let auth_status = AuthStatus {
        claude,
        codex,
        ollama,
        mistral,
        openrouter: None,
        ollama_cloud,
        deepseek,
        google_vertex,
    };

    Json(SetupInfoResponse {
        setup_complete,
        config,
        auth_status,
        resume_session: None,
        cwd: state.config.cwd.to_string_lossy().into_owned(),
    })
    .into_response()
}

/// `GET /api/setup/models` — returns available models for the requested backend.
pub async fn setup_models(Query(q): Query<ModelsQuery>) -> impl IntoResponse {
    let models = match q.backend.as_str() {
        "anthropic" => anthropic_models(),
        "lightarchitects" | "light_architect" => lightarchitects_models(),
        "openai" | "codex" => codex_models(),
        "ollama-launch" | "ollama_launch" | "ollama" => {
            let url = q.base_url.as_deref().unwrap_or("http://localhost:11434");
            ollama_models(url).await
        }
        "ollama-cloud" => ollama_cloud_models(),
        "openrouter" => openrouter_models(),
        "deepseek" => deepseek_models(),
        "mistral" | "mistral-vibe" | "mistral_vibe" => mistral_models(),
        "google-vertex" => google_vertex_models(),
        _ => vec![],
    };
    Json(ModelsResponse { models }).into_response()
}

/// `POST /api/setup/save` — persist config + hot-reload the active agent.
///
/// Authenticated via [`auth::AuthGuard`] (Bearer header **or** `la_session` cookie).
pub async fn setup_save(
    _: auth::AuthGuard,
    State(state): State<AppState>,
    Json(req): Json<SaveRequest>,
) -> impl IntoResponse {
    // For native CLI, write the selected model to the CLI's TOML config first.
    // This must succeed before we claim the setup is saved.
    if req.agent == AgentKind::LightArchitect {
        if let Some(ref model) = req.model {
            if model.trim().is_empty() {
                tracing::warn!(target: "setup", "Rejecting empty model for native CLI");
                return StatusCode::BAD_REQUEST.into_response();
            }
            if let Err(e) = lightarchitects_cli_model_to_toml(model) {
                tracing::error!(target: "setup", "Failed to write CLI TOML config: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
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

/// `DELETE /api/setup/reset` — wipe setup config, frontend returns to splash.
///
/// Authenticated via [`auth::AuthGuard`] (Bearer header **or** `la_session` cookie).
pub async fn setup_reset(_: auth::AuthGuard) -> impl IntoResponse {
    if let Err(e) = Config::delete_setup() {
        tracing::error!(target: "setup", "Failed to delete setup config: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    info!(target: "setup", "Setup config reset — frontend will re-enter setup flow");
    StatusCode::NO_CONTENT.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentSession;

    fn save_req(agent: AgentKind, backend: &str, model: Option<&str>) -> SaveRequest {
        SaveRequest {
            agent,
            backend: backend.to_owned(),
            model: model.map(ToOwned::to_owned),
            ollama_base_url: None,
            api_key: None,
        }
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn agent_session_from_save_mistral_vibe_explicit_model() {
        let req = save_req(
            AgentKind::MistralVibe,
            "mistral",
            Some("mistral-small-latest"),
        );
        let sess = agent_session_from_save(&req).unwrap();
        let AgentSession::MistralVibe(cfg) = sess else {
            panic!("expected MistralVibe session");
        };
        assert_eq!(cfg.model.as_deref(), Some("mistral-small-latest"));
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn agent_session_from_save_mistral_vibe_model_defaults_when_absent() {
        let req = save_req(AgentKind::MistralVibe, "mistral", None);
        let sess = agent_session_from_save(&req).unwrap();
        let AgentSession::MistralVibe(cfg) = sess else {
            panic!("expected MistralVibe session");
        };
        assert_eq!(
            cfg.model, None,
            "absent model passes through as None — vibe uses its own config"
        );
    }

    // ── Phase 4 unit tests: LiteLLM BYOK routing ────────────────────────────

    #[allow(clippy::panic)]
    fn assert_litellm_model(sess: crate::config::AgentSession, expected_model: &str) {
        let crate::config::AgentSession::Lightarchitects(backend) = sess else {
            panic!("expected Lightarchitects session");
        };
        let crate::config::ClaudeBackend::LiteLlm(cfg) = backend else {
            panic!("expected LiteLlm backend");
        };
        assert_eq!(cfg.model, expected_model);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn litellm_deepseek_default_model() {
        let req = save_req(AgentKind::Lightarchitects, "deepseek", None);
        let sess = agent_session_from_save(&req).unwrap();
        assert_litellm_model(sess, "deepseek/deepseek-chat");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn litellm_deepseek_explicit_model() {
        let req = save_req(
            AgentKind::Lightarchitects,
            "deepseek",
            Some("deepseek/deepseek-r1"),
        );
        let sess = agent_session_from_save(&req).unwrap();
        assert_litellm_model(sess, "deepseek/deepseek-r1");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn litellm_google_vertex_default_model() {
        let req = save_req(AgentKind::Lightarchitects, "google-vertex", None);
        let sess = agent_session_from_save(&req).unwrap();
        assert_litellm_model(sess, "vertex_ai/gemini-1.5-pro");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn litellm_google_vertex_explicit_model() {
        let req = save_req(
            AgentKind::Lightarchitects,
            "google-vertex",
            Some("vertex_ai/gemini-2.0-flash"),
        );
        let sess = agent_session_from_save(&req).unwrap();
        assert_litellm_model(sess, "vertex_ai/gemini-2.0-flash");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn litellm_ollama_cloud_default_model() {
        let req = save_req(AgentKind::Lightarchitects, "ollama-cloud", None);
        let sess = agent_session_from_save(&req).unwrap();
        assert_litellm_model(sess, "ollama_chat/llama3.2");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn litellm_mistral_backend_default_model() {
        let req = save_req(AgentKind::Lightarchitects, "mistral", None);
        let sess = agent_session_from_save(&req).unwrap();
        assert_litellm_model(sess, "mistral/mistral-large-latest");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn litellm_openai_backend_default_model() {
        let req = save_req(AgentKind::Lightarchitects, "openai", None);
        let sess = agent_session_from_save(&req).unwrap();
        assert_litellm_model(sess, "openai/gpt-4o");
    }

    #[test]
    fn litellm_unknown_backend_returns_none() {
        let req = save_req(AgentKind::Lightarchitects, "unknown-provider-xyz", None);
        assert!(agent_session_from_save(&req).is_none());
    }
}
