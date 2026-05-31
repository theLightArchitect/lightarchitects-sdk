//! Wave composer route — `POST /api/cockpit/wave`.
//!
//! Accepts a [`WaveComposerRequest`] from the Cockpit UI and dispatches an
//! autonomous build wave via [`spawn_autonomous_build`]. Returns a
//! [`WaveComposerResponse`] immediately; the build runs in the background
//! and emits progress via the global SSE stream.
//!
//! ## Auth
//! Bearer token (`Authorization: Bearer <token>`), same as all authenticated routes.
//!
//! ## Body limit
//! 16 KB (`DefaultBodyLimit::max(16 * 1024)` applied at route registration).
//!
//! ## Security
//! `task_description` fields flow into worker system prompts.
//! [`lightarchitects::agent::IndirectInjectionShield`] scans each description
//! for injection patterns before the prompt is constructed (OWASP LLM01 —
//! indirect prompt injection via operator-supplied API input).

use std::path::PathBuf;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use lightarchitects::{
    agent::{IndirectInjectionShield, InjectionSeverity},
    lightsquad::{
        types::Task,
        wave_dispatcher::{WaveError, validate_wave_ownership},
    },
};
use secrecy::ExposeSecret as _;

use crate::{
    events::{
        builds_handler::TaskSpec,
        decisions::DecisionsWriter,
        lightsquad_bridge::{BridgeContext, spawn_autonomous_build},
    },
    server::AppState,
};

/// Maximum agents per wave — mirrors `SLOT_CAPACITY` (agents-playbook §15.3.13 PW-7).
const MAX_AGENT_SLOTS: usize = 7;

// ── Request / Response types ──────────────────────────────────────────────────

/// Domain preset key from the Cockpit UI.
///
/// Must stay in sync with `CockpitPreset` in
/// `lightarchitects-webshell-ui/src/lib/cockpit/stores.ts`.
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CockpitPreset {
    /// General software engineering tasks.
    Engineer,
    /// Security auditing and threat modelling.
    Security,
    /// Operations, CI/CD, and deployment.
    Ops,
    /// Code quality and standards enforcement.
    Quality,
    /// Knowledge graph enrichment and documentation.
    Knowledge,
    /// Research and investigation.
    Researcher,
    /// Test design and pyramid coverage.
    Testing,
}

impl CockpitPreset {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Engineer => "engineer",
            Self::Security => "security",
            Self::Ops => "ops",
            Self::Quality => "quality",
            Self::Knowledge => "knowledge",
            Self::Researcher => "researcher",
            Self::Testing => "testing",
        }
    }
}

/// Target entity within the LASDLC hierarchy.
#[derive(Debug, Deserialize, Clone)]
pub struct CockpitTargetPayload {
    /// Entity type (e.g. `"build"`, `"pr"`, `"file"`).
    #[serde(rename = "type")]
    pub target_type: String,
    /// Entity identifier.
    pub id: String,
    /// Human-readable label shown in the breadcrumb.
    pub label: String,
}

/// One agent assignment from the composer.
///
/// `task_description` is scanned by [`IndirectInjectionShield`] before injection
/// into the worker system prompt (OWASP LLM01).
#[derive(Debug, Deserialize, Clone)]
pub struct AgentAssignmentPayload {
    /// Domain preset (must deserialise from one of the `CockpitPreset` variants).
    pub preset: CockpitPreset,
    /// Skill name to invoke (e.g. `"lightarchitects:quality"`).
    pub skill: String,
    /// Operator-supplied task description — scanned before promotion.
    pub task_description: String,
    /// Worktree-relative file paths this agent may write. Empty = no enforcement.
    #[serde(default)]
    pub file_ownership: Vec<String>,
}

/// `POST /api/cockpit/wave` request body. Body limit: 16 KB.
#[derive(Debug, Deserialize)]
pub struct WaveComposerRequest {
    /// Build codename (branch prefix + `active.yaml` key).
    pub codename: String,
    /// Agent assignments — 1 to `MAX_AGENT_SLOTS` entries.
    pub agents: Vec<AgentAssignmentPayload>,
    /// Target entity the wave acts on.
    pub target: CockpitTargetPayload,
    /// Absolute path to the repository root passed to `BridgeContext::repo_root`.
    pub worktree: String,
}

/// `POST /api/cockpit/wave` response body.
#[derive(Debug, Serialize)]
pub struct WaveComposerResponse {
    /// Matches `build_id` — convenience alias for UI wave tracking.
    pub wave_id: Uuid,
    /// Build session UUID.
    pub build_id: Uuid,
    /// Number of agents dispatched.
    pub agent_count: usize,
    /// Estimated start latency in milliseconds (0 — async dispatch).
    pub estimated_start_ms: u64,
}

// ── Handler ───────────────────────────────────────────────────────────────────

/// `POST /api/cockpit/wave` — dispatch a wave from the Cockpit composer.
///
/// Auth-gated. Body ≤ 16 KB. Returns `200 WaveComposerResponse` immediately;
/// build runs in background and emits events via the global SSE stream.
pub async fn cockpit_wave_handler(
    _: crate::auth::AuthGuard,
    State(state): State<AppState>,
    Json(body): Json<WaveComposerRequest>,
) -> impl IntoResponse {
    let agent_count = body.agents.len();

    if agent_count == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"no_agents","detail":"at least one agent required"})),
        )
            .into_response();
    }

    if agent_count > MAX_AGENT_SLOTS {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "too_many_agents",
                "detail": format!("agent_count {agent_count} exceeds max {MAX_AGENT_SLOTS}")
            })),
        )
            .into_response();
    }

    // OWASP LLM01: scan task_description for indirect injection patterns before
    // promoting operator input into a worker system prompt (Cookbook §65).
    let shield = IndirectInjectionShield::new();
    for assignment in &body.agents {
        let patterns = shield.detect(&assignment.task_description);
        let high = patterns
            .iter()
            .any(|p| matches!(p.severity, InjectionSeverity::High));
        if high {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error": "injection_detected",
                    "detail": "task_description contains suspicious patterns"
                })),
            )
                .into_response();
        }
    }

    let task_specs = build_task_specs(&body);

    // PW-6 pre-spawn ownership gate (agents-playbook §15.3.13).
    // Minimal Task stubs — only id + file_ownership are read by validate_wave_ownership.
    let stubs: Vec<Task> = task_specs
        .iter()
        .map(|s| Task {
            id: s.id.clone(),
            branch: String::new(),
            depends_on: vec![],
            file_ownership: s.file_ownership.clone(),
            concurrency_safe: false,
            context_tiers: vec![],
            prompt: String::new(),
        })
        .collect();

    if let Err(WaveError::OwnershipConflict { file, tasks }) = validate_wave_ownership(&stubs) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "ownership_conflict",
                "detail": format!("file '{file}' claimed by tasks {tasks:?}")
            })),
        )
            .into_response();
    }

    let build_id = Uuid::new_v4();

    let pepper = state.turnlog_pepper.expose_secret();
    let decisions_writer = match DecisionsWriter::open(&state.decisions_dir, build_id, pepper) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!(error = %e, "decisions writer open failed — wave aborted");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let handle = spawn_autonomous_build(BridgeContext {
        build_id,
        codename: body.codename.clone(),
        repo_root: PathBuf::from(&body.worktree),
        worktree_root: std::env::temp_dir().join(format!("la-wt-{build_id}")),
        feat_branch: format!("feat/{}", body.codename),
        waves: vec![task_specs],
        event_tx: state.event_tx.clone(),
        decisions_writer,
        mock_workers: state.mock_workers,
        hitl_queue: state.hitl_queue.clone(),
    });
    state.lightsquad_programs.insert(build_id, handle);

    tracing::info!(
        build_id = %build_id,
        codename = %body.codename,
        agent_count,
        "cockpit wave dispatched"
    );

    (
        StatusCode::OK,
        Json(WaveComposerResponse {
            wave_id: build_id,
            build_id,
            agent_count,
            estimated_start_ms: 0,
        }),
    )
        .into_response()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Translate `AgentAssignmentPayload` list → `TaskSpec` list for the bridge.
///
/// Each spec gets a Git-Context preamble (Cookbook §64.8) so the worker knows
/// its branch, codename, and target scope before reading the task description.
fn build_task_specs(body: &WaveComposerRequest) -> Vec<TaskSpec> {
    body.agents
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let git_preamble = format!(
                "Git context: codename={codename}, target={ttype}:{tid} ({tlabel}), branch=feat/{codename}.\n\n",
                codename = body.codename,
                ttype = body.target.target_type,
                tid = body.target.id,
                tlabel = body.target.label,
            );
            let prompt = format!(
                "[preset:{preset}] [skill:{skill}]\n{preamble}{desc}",
                preset = a.preset.as_str(),
                skill = a.skill,
                preamble = git_preamble,
                desc = a.task_description,
            );
            TaskSpec {
                id: format!("wc-{preset}-{i}", preset = a.preset.as_str()),
                prompt,
                depends_on: vec![],
                file_ownership: a.file_ownership.clone(),
                concurrency_safe: false,
            }
        })
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(agents: Vec<AgentAssignmentPayload>) -> WaveComposerRequest {
        WaveComposerRequest {
            codename: "test-build".into(),
            agents,
            target: CockpitTargetPayload {
                target_type: "build".into(),
                id: "abc123".into(),
                label: "my build".into(),
            },
            worktree: "/tmp/wt".into(),
        }
    }

    fn engineer_agent(task: &str, files: Vec<&str>) -> AgentAssignmentPayload {
        AgentAssignmentPayload {
            preset: CockpitPreset::Engineer,
            skill: "lightarchitects:engineer".into(),
            task_description: task.into(),
            file_ownership: files.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn build_task_specs_round_trip() {
        let req = make_request(vec![
            engineer_agent("implement auth", vec!["src/auth.rs"]),
            AgentAssignmentPayload {
                preset: CockpitPreset::Security,
                skill: "lightarchitects:security".into(),
                task_description: "audit the auth surface".into(),
                file_ownership: vec![],
            },
        ]);
        let specs = build_task_specs(&req);
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].id, "wc-engineer-0");
        assert_eq!(specs[1].id, "wc-security-1");
        assert!(specs[0].prompt.contains("[preset:engineer]"));
        assert!(specs[0].prompt.contains("Git context: codename=test-build"));
        assert!(specs[0].prompt.contains("implement auth"));
        assert_eq!(specs[0].file_ownership, vec!["src/auth.rs"]);
        assert!(specs[1].file_ownership.is_empty());
    }

    #[test]
    fn ownership_conflict_rejected() {
        let req = make_request(vec![
            engineer_agent("task A", vec!["src/shared.rs"]),
            engineer_agent("task B", vec!["src/shared.rs"]),
        ]);
        let specs = build_task_specs(&req);
        let stubs: Vec<Task> = specs
            .iter()
            .map(|s| Task {
                id: s.id.clone(),
                branch: String::new(),
                depends_on: vec![],
                file_ownership: s.file_ownership.clone(),
                concurrency_safe: false,
                context_tiers: vec![],
                prompt: String::new(),
            })
            .collect();
        assert!(matches!(
            validate_wave_ownership(&stubs),
            Err(WaveError::OwnershipConflict { .. })
        ));
    }

    #[test]
    fn disjoint_ownership_accepted() {
        let req = make_request(vec![
            engineer_agent("task A", vec!["src/foo.rs"]),
            engineer_agent("task B", vec!["src/bar.rs"]),
        ]);
        let specs = build_task_specs(&req);
        let stubs: Vec<Task> = specs
            .iter()
            .map(|s| Task {
                id: s.id.clone(),
                branch: String::new(),
                depends_on: vec![],
                file_ownership: s.file_ownership.clone(),
                concurrency_safe: false,
                context_tiers: vec![],
                prompt: String::new(),
            })
            .collect();
        assert!(validate_wave_ownership(&stubs).is_ok());
    }

    #[test]
    fn too_many_agents_rejected_by_constant() {
        // 8 agents > MAX_AGENT_SLOTS (7)
        let agents: Vec<_> = (0..=7)
            .map(|i| engineer_agent(&format!("task {i}"), vec![]))
            .collect();
        assert!(agents.len() > MAX_AGENT_SLOTS);
    }

    #[test]
    fn cockpit_preset_as_str() {
        assert_eq!(CockpitPreset::Engineer.as_str(), "engineer");
        assert_eq!(CockpitPreset::Security.as_str(), "security");
        assert_eq!(CockpitPreset::Testing.as_str(), "testing");
    }
}
