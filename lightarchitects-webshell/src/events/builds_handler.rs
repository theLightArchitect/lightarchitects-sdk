//! `/api/builds` routes.
//!
//! - `GET /api/builds` — walks `corso/builds/*/manifest.yaml`, returns aggregate
//!   JSON array of build summaries. Supports `?status=` and `?codename=` filters.
//!   Cached by directory mtime; 503 if the vault is missing.
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
    extract::{Path, Query, State},
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

/// Cached build data: (builds-dir mtime, serialised JSON bytes).
pub type Cache = Arc<Mutex<Option<(SystemTime, Vec<u8>)>>>;

/// Shared cache instance, created once per server lifetime.
#[must_use]
pub fn build_cache() -> Cache {
    Arc::new(Mutex::new(None))
}

/// Query parameters for `GET /api/builds`.
#[derive(Debug, Deserialize, Default)]
pub struct BuildsQuery {
    /// Filter by build status (case-insensitive prefix match, e.g. `phase_2`).
    pub status: Option<String>,
    /// Return a single build matching this codename exactly.
    pub codename: Option<String>,
}

/// Parsed summary of one `manifest.yaml` file, returned by `GET /api/builds`.
#[derive(Debug, Serialize)]
pub struct BuildSummary {
    /// Build codename, from `plan_id` or parent directory name.
    pub codename: String,
    /// Human-readable build name from `plan_name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_name: Option<String>,
    /// Build status string (e.g. `PHASE_2_IN_PROGRESS`, `COMPLETE`).
    pub status: String,
    /// LASDLC tier (`SMALL`, `MEDIUM`, or `LARGE`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// ISO date the build was created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// Agent or user that owns the build.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Phase completion history from the manifest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase_status_history: Option<serde_json::Value>,
}

/// Parse a `manifest.yaml` file into a [`BuildSummary`], returning `None`
/// on any read or parse error (the walk skips unparseable manifests).
fn parse_manifest(path: &std::path::Path) -> Option<BuildSummary> {
    let content = std::fs::read_to_string(path).ok()?;
    let yaml: serde_json::Value = serde_yaml::from_str::<serde_yaml::Value>(&content)
        .ok()
        .and_then(|v| serde_json::to_value(v).ok())?;

    let codename = yaml
        .get("plan_id")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        // Fallback: derive from parent directory name.
        .or_else(|| {
            path.parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(str::to_owned)
        })?;

    let status = yaml
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_owned();

    Some(BuildSummary {
        codename,
        plan_name: yaml
            .get("plan_name")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        status,
        tier: yaml.get("tier").and_then(|v| v.as_str()).map(str::to_owned),
        created: yaml
            .get("created")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        owner: yaml
            .get("owner")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        phase_status_history: yaml.get("phase_status_history").cloned(),
    })
}

/// `GET /api/builds` — returns aggregate build portfolio as a JSON array.
///
/// Walks `corso/builds/*/manifest.yaml`, parses each, applies optional
/// `?status=` and `?codename=` filters, returns the result sorted by
/// `created` descending.  Auth-gated (same Bearer token as `/api/events`).
/// Returns 503 if the vault is not configured.  Results are cached by the
/// `corso/builds/` directory mtime.
#[allow(clippy::missing_panics_doc)]
pub async fn builds_handler(
    headers: axum::http::HeaderMap,
    Query(query): Query<BuildsQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !auth::validate_bearer(authz, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let Some(helix_root) = lightarchitects::core::paths::helix_root() else {
        warn!("helix_root unavailable — cannot serve /api/builds");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let builds_dir = helix_root.join("corso").join("builds");

    // Use directory mtime as cache key: any manifest write updates dir mtime.
    let dir_mtime = std::fs::metadata(&builds_dir)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);

    // Fast-path: serve cached bytes when directory is unchanged AND no filters
    // are active (filtered responses are not cached to keep the cache simple).
    let no_filters = query.status.is_none() && query.codename.is_none();
    #[allow(clippy::unwrap_used)]
    if no_filters {
        let cache_hit = {
            let cache = state.builds_cache.lock().unwrap();
            cache.as_ref().and_then(|(cached_mtime, cached_bytes)| {
                if *cached_mtime == dir_mtime {
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
    }

    // Walk builds_dir/*/manifest.yaml, parse each, collect summaries.
    let mut summaries: Vec<BuildSummary> = match std::fs::read_dir(&builds_dir) {
        Ok(entries) => entries
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if !entry.file_type().ok()?.is_dir() {
                    return None;
                }
                let manifest_path = entry.path().join("manifest.yaml");
                parse_manifest(&manifest_path)
            })
            .collect(),
        Err(e) => {
            warn!(error = %e, path = %builds_dir.display(), "cannot read builds directory");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };

    // Apply filters.
    if let Some(ref codename_filter) = query.codename {
        summaries.retain(|s| s.codename == *codename_filter);
    }
    if let Some(ref status_filter) = query.status {
        let filter_lower = status_filter.to_lowercase();
        summaries.retain(|s| s.status.to_lowercase().starts_with(&filter_lower));
    }

    // Sort by created descending (most recent first); unknown dates sort last.
    summaries.sort_by(|a, b| b.created.cmp(&a.created));

    let json_bytes = match serde_json::to_vec_pretty(&summaries) {
        Ok(b) => b,
        Err(e) => {
            warn!(error = %e, "failed to serialise builds JSON");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    info!(count = summaries.len(), "served /api/builds");

    // Cache only the unfiltered response.
    #[allow(clippy::unwrap_used)]
    if no_filters {
        *state.builds_cache.lock().unwrap() = Some((dir_mtime, json_bytes.clone()));
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
    /// Whether this build will spawn in a container (true) or native PTY (false).
    pub containerized: bool,
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

    session.containerized = state.docker_capable == crate::container::DockerCapability::Ready
        && state.config.container_mode != crate::container::ContainerMode::ForceDisable;

    let resp = BuildResponse {
        build_id: session.build_id,
        cwd: session.cwd.clone(),
        agent: AgentDescriptor::from_session(&session.agent),
        claude_agent_template: session.claude_agent_template.clone(),
        model: session.model.clone(),
        containerized: session.containerized,
    };

    if let Ok(store) = state.session_store.lock() {
        if let Err(e) = store.insert(
            &session.build_id.to_string(),
            session.cwd.to_string_lossy().as_ref(),
            match session.agent.kind() {
                crate::config::AgentKind::Lightarchitects => "lightarchitects",
                crate::config::AgentKind::Codex => "codex",
                crate::config::AgentKind::LightarchitectsNative => "lightarchitects_native",
            },
            None,
            session.model.as_deref(),
            session.containerized,
        ) {
            tracing::error!(error = %e, "session_store insert failed");
        }
    }

    let session = Arc::new(session);
    let _prev = state.builds.insert(Arc::clone(&session));
    state
        .telemetry
        .build_created(&session.build_id, &session.cwd);
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
        containerized: session.containerized,
    };

    (StatusCode::OK, Json(resp)).into_response()
}

// ── Plan CRUD (Phase 25 — build plan lifecycle) ──────────────────────────────

/// Request body for `POST /api/builds/plan` — creates a tracked build plan.
///
/// Writes an entry to `active.yaml` and scaffolds a per-build manifest
/// directory under `helix/corso/builds/{codename}/`.
#[derive(Debug, Deserialize)]
pub struct CreatePlanRequest {
    /// Human-readable build plan name.
    pub name: String,
    /// Adjective-gerund-noun codename (auto-generated if absent).
    #[serde(default)]
    pub codename: Option<String>,
    /// Semver target version (e.g., "0.3.0").
    pub version: String,
    /// Repository path for this build.
    pub path: String,
    /// Free-form build description.
    pub description: String,
    /// Meta-skill: "/BUILD", "/RESEARCH", "/SECURE", etc.
    pub meta_skill: String,
    /// Priority: "high", "medium", or "low".
    pub priority: String,
    /// Intake source: "manual", "github", "audit", or "discovery".
    pub source: String,
    /// Primary language (defaults to "rust+typescript").
    #[serde(default)]
    pub language: Option<String>,
    /// Assigned sibling IDs.
    #[serde(default)]
    pub siblings: Vec<String>,
    /// Codenames of builds that block this plan.
    #[serde(default)]
    pub blocked_by: Option<Vec<String>>,
    /// Codenames of builds that this plan blocks.
    #[serde(default)]
    pub blocks: Option<Vec<String>>,
    /// Phase detail with mandatory exit gates (raw JSON → YAML).
    #[serde(default)]
    pub phase_detail: Vec<serde_json::Value>,
    /// Pre-flight checks (Section 0 of template v2).
    #[serde(default)]
    pub pre_flight: Vec<serde_json::Value>,
    /// Close-out steps (Section 5 of template v2).
    #[serde(default)]
    pub close_out: Vec<serde_json::Value>,
    /// Active domain gate categories for this build.
    #[serde(default)]
    pub domain_gates: Vec<String>,
    /// Agentic SDLC configuration (Section 6 of template v2).
    #[serde(default)]
    pub agentic: Option<serde_json::Value>,
    /// Build tier (1=production through 5=planned).
    #[serde(default)]
    pub tier: Option<u8>,
}

/// `POST /api/builds/plan` — create a tracked build plan entry.
///
/// Appends to `active.yaml` atomically (write-to-temp + rename). Invalidates
/// the builds cache so the next `GET /api/builds` reflects the new entry.
#[allow(clippy::missing_panics_doc, clippy::too_many_lines)]
pub async fn create_plan_handler(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<CreatePlanRequest>,
) -> impl IntoResponse {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !auth::validate_bearer(authz, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Soft-validate LASDLC phase names (warn, don't reject)
    let valid_phase_prefixes = [
        "Plan",
        "Research",
        "Implement",
        "Harden",
        "Verify",
        "Ship",
        "Learn",
    ];
    for phase in &body.phase_detail {
        if let Some(title) = phase.get("title").and_then(|v| v.as_str()) {
            let has_valid_prefix = valid_phase_prefixes.iter().any(|p| title.starts_with(p));
            if !has_valid_prefix {
                tracing::debug!(title = %title, "phase title does not match LASDLC naming — allowed but non-standard");
            }
        }
    }

    // Resolve helix root and active.yaml path.
    let Some(helix_root) = lightarchitects::core::paths::helix_root() else {
        warn!("helix_root unavailable — cannot create plan");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let active_path = helix_root.join("corso").join("builds").join("active.yaml");

    // Read current active.yaml.
    let current = match std::fs::read_to_string(&active_path) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "failed to read active.yaml for plan creation");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Generate codename if not provided.
    let codename = body.codename.unwrap_or_else(|| {
        use std::time::SystemTime;
        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        // Simple deterministic codename from timestamp hash
        let adjectives = [
            "keen", "swift", "bold", "bright", "steady", "fierce", "noble", "radiant",
        ];
        let gerunds = [
            "forging", "weaving", "tracking", "mining", "bridging", "sealing", "nesting",
            "scribing",
        ];
        let nouns = [
            "hawk", "eagle", "wolf", "phoenix", "raven", "spider", "falcon", "viper",
        ];
        let a = adjectives[(seed as usize) % adjectives.len()];
        let g = gerunds[((seed >> 8) as usize) % gerunds.len()];
        let n = nouns[((seed >> 16) as usize) % nouns.len()];
        format!("{a}-{g}-{n}")
    });

    // Build the YAML entry for active.yaml.
    let mut entry = serde_yaml::Mapping::new();
    entry.insert("name".into(), body.name.clone().into());
    entry.insert("codename".into(), codename.clone().into());
    entry.insert("version".into(), body.version.into());
    entry.insert(
        "tier".into(),
        serde_yaml::Value::Number(body.tier.unwrap_or(3).into()),
    );
    entry.insert("status".into(), "planned".into());
    entry.insert("path".into(), body.path.into());
    entry.insert(
        "binary".into(),
        "~/.lightarchitects/webshell/bin/lightarchitects-webshell".into(),
    );
    entry.insert("deploy".into(), "make deploy".into());
    entry.insert(
        "language".into(),
        body.language
            .unwrap_or_else(|| "rust+typescript".to_owned())
            .into(),
    );
    entry.insert("description".into(), body.description.into());
    entry.insert("meta_skill".into(), body.meta_skill.into());
    entry.insert("priority".into(), body.priority.into());

    if !body.siblings.is_empty() {
        let siblings: Vec<serde_yaml::Value> = body.siblings.into_iter().map(Into::into).collect();
        entry.insert("siblings".into(), serde_yaml::Value::Sequence(siblings));
    }
    if let Some(blocked) = body.blocked_by {
        if !blocked.is_empty() {
            let blocked_vals: Vec<serde_yaml::Value> =
                blocked.into_iter().map(Into::into).collect();
            entry.insert(
                "blocked_by".into(),
                serde_yaml::Value::Sequence(blocked_vals),
            );
        }
    }
    if let Some(blocks) = body.blocks {
        if !blocks.is_empty() {
            let block_vals: Vec<serde_yaml::Value> = blocks.into_iter().map(Into::into).collect();
            entry.insert("blocks".into(), serde_yaml::Value::Sequence(block_vals));
        }
    }

    // Phase detail — store as raw YAML values
    let phases_count = body.phase_detail.len();
    if !body.phase_detail.is_empty() {
        entry.insert(
            "phases".into(),
            serde_yaml::Value::Number(serde_yaml::Number::from(phases_count as u64)),
        );
        entry.insert(
            "current_phase".into(),
            serde_yaml::Value::Number(0u64.into()),
        );
        entry.insert("phase_status".into(), "PLANNED".into());

        // Convert JSON phase_detail to YAML
        let phase_yaml: Vec<serde_yaml::Value> = body
            .phase_detail
            .iter()
            .filter_map(|v| serde_yaml::to_value(v).ok())
            .collect();
        if !phase_yaml.is_empty() {
            entry.insert(
                "phase_detail".into(),
                serde_yaml::Value::Sequence(phase_yaml),
            );
        }
    }

    // Append to active.yaml — read existing YAML, add entry, write atomically.
    let mut yaml_value: serde_yaml::Value = match serde_yaml::from_str(&current) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "failed to parse active.yaml");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if let Some(builds) = yaml_value
        .get_mut("builds")
        .and_then(|v| v.as_sequence_mut())
    {
        builds.push(serde_yaml::Value::Mapping(entry));
    } else {
        warn!("active.yaml missing 'builds' sequence");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    // Write atomically: temp file → rename.
    let tmp_path = active_path.with_extension("yaml.tmp");
    let yaml_str = match serde_yaml::to_string(&yaml_value) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to serialize updated active.yaml");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if let Err(e) = std::fs::write(&tmp_path, &yaml_str) {
        warn!(error = %e, "failed to write temp active.yaml");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if let Err(e) = std::fs::rename(&tmp_path, &active_path) {
        warn!(error = %e, "failed to rename temp active.yaml");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    // Invalidate builds cache so next GET reflects the new entry.
    #[allow(clippy::unwrap_used)]
    {
        *state.builds_cache.lock().unwrap() = None;
    }

    // Scaffold per-build manifest directory.
    let manifest_dir = helix_root.join("corso").join("builds").join(&codename);
    if let Err(e) = std::fs::create_dir_all(&manifest_dir) {
        warn!(error = %e, "failed to create manifest dir");
    }
    let manifest_path = manifest_dir.join("manifest.yaml");
    if !manifest_path.exists() {
        let scaffold = format!(
            "schema_version: \"1.1\"\nplan_id: \"{codename}\"\nstatus: planned\ntier: PLANNED\ncreated: \"{now}\"\nupdated: \"{now}\"\n\ngates:\n  triage: {{ passed: false }}\n  requirements: {{ passed: false }}\n  context: {{ passed: false }}\n  plan: {{ passed: false }}\n  scrum: {{ passed: false }}\n\nphases: []\n",
            codename = codename,
            now = chrono::Utc::now().to_rfc3339(),
        );
        if let Err(e) = std::fs::write(&manifest_path, scaffold) {
            warn!(error = %e, "failed to write manifest scaffold");
        }
    }

    info!(codename = %codename, "plan created in active.yaml");

    let resp = serde_json::json!({
        "codename": codename,
        "build_id": codename,
        "phases": phases_count,
    });
    (StatusCode::OK, Json(resp)).into_response()
}

/// `PUT /api/builds/plan/{codename}` — update plan status, phase, or gate results.
///
/// Partial update — only provided fields are merged into the active.yaml entry.
#[allow(clippy::missing_panics_doc)]
pub async fn update_plan_handler(
    Path(codename): Path<String>,
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !auth::validate_bearer(authz, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let Some(helix_root) = lightarchitects::core::paths::helix_root() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let active_path = helix_root.join("corso").join("builds").join("active.yaml");

    let current = match std::fs::read_to_string(&active_path) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "failed to read active.yaml for plan update");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let mut yaml_value: serde_yaml::Value = match serde_yaml::from_str(&current) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "failed to parse active.yaml");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Find the entry by codename and merge updates.
    let mut found = false;
    if let Some(builds) = yaml_value
        .get_mut("builds")
        .and_then(|v| v.as_sequence_mut())
    {
        for build in builds.iter_mut() {
            if let Some(cn) = build.get("codename").and_then(|v| v.as_str()) {
                if cn == codename {
                    found = true;
                    // Merge provided fields
                    if let (Some(mapping), Ok(updates)) =
                        (build.as_mapping_mut(), serde_yaml::to_value(&body))
                    {
                        if let Some(update_map) = updates.as_mapping() {
                            for (k, v) in update_map {
                                mapping.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    if !found {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Write atomically.
    let tmp_path = active_path.with_extension("yaml.tmp");
    let yaml_str = match serde_yaml::to_string(&yaml_value) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to serialize updated active.yaml");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if let Err(e) = std::fs::write(&tmp_path, &yaml_str) {
        warn!(error = %e, "failed to write temp active.yaml");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if let Err(e) = std::fs::rename(&tmp_path, &active_path) {
        warn!(error = %e, "failed to rename temp active.yaml");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    // Invalidate cache.
    #[allow(clippy::unwrap_used)]
    {
        *state.builds_cache.lock().unwrap() = None;
    }

    info!(codename = %codename, "plan updated in active.yaml");
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

/// `GET /api/lasdlc` — returns LASDLC framework metadata (phases, tiers, dimensions).
///
/// Public (no auth required) — serves static framework configuration for the UI.
pub async fn lasdlc_meta_handler() -> impl IntoResponse {
    let meta = serde_json::json!({
        "framework": "LASDLC",
        "version": "1.0.0",
        "phases": ["Plan", "Research", "Implement", "Harden", "Verify", "Ship", "Learn"],
        "tiers": {
            "SMALL": ["Plan", "Implement", "Verify", "Ship"],
            "MEDIUM": ["Plan", "Research", "Implement", "Verify", "Ship", "Learn"],
            "LARGE": ["Plan", "Research", "Implement", "Harden", "Verify", "Ship", "Learn"]
        },
        "quality_dimensions": ["Architecture", "Security", "Quality", "Performance", "Testing", "Documentation", "Operations"],
        "template": "LASDLC-TEMPLATE-v1.yaml",
        "spec": "helix/user/standards/canon/lasdlc-spec.md"
    });
    (StatusCode::OK, Json(meta))
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
            containerized: false,
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
        assert!(
            !json.contains("lightarchitects-cli"),
            "binary path must not leak: {json}"
        );
    }
}
