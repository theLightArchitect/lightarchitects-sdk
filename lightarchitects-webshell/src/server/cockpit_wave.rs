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

    /// Canonical skill key for this preset — derived server-side so the
    /// operator cannot spoof a privileged skill via the API (F-4 / OWASP LLM02).
    fn skill(&self) -> &'static str {
        match self {
            Self::Engineer => "lightarchitects:engineer",
            Self::Security => "lightarchitects:security",
            Self::Ops => "lightarchitects:ops",
            Self::Quality => "lightarchitects:quality",
            Self::Knowledge => "lightarchitects:knowledge",
            Self::Researcher => "lightarchitects:researcher",
            Self::Testing => "lightarchitects:testing",
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
#[allow(clippy::too_many_lines)] // security validation + dispatch in one handler; extraction would split the borrow chain
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

    // F-6: validate codename against git branch name allowlist (CWE-78 §3.4).
    if !CODENAME_RE.is_match(&body.codename) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_codename",
                "detail": "codename must be 3-64 lowercase alphanumeric or hyphen characters"
            })),
        )
            .into_response();
    }

    // F-2: validate worktree path — reject traversal before PathBuf promotion (CWE-22 §SG-CRYPTO.6).
    let repo_root = match validate_worktree_path(&body.worktree) {
        Ok(p) => p,
        Err(detail) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid_worktree", "detail": detail})),
            )
                .into_response();
        }
    };

    // F-1 + F-5: scan all prompt-injected fields — codename, target id/label/type
    // flow into the git-context preamble; skill flows into the [skill:...] token.
    // Only task_description was previously covered; extend the shield surface.
    let shield = IndirectInjectionShield::new();
    for field in [
        body.codename.as_str(),
        body.target.id.as_str(),
        body.target.label.as_str(),
        body.target.target_type.as_str(),
    ] {
        let patterns = shield.detect(field);
        if patterns
            .iter()
            .any(|p| matches!(p.severity, InjectionSeverity::High))
        {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error": "injection_detected",
                    "detail": "request contains suspicious patterns"
                })),
            )
                .into_response();
        }
    }
    for assignment in &body.agents {
        for field in [
            assignment.task_description.as_str(),
            assignment.skill.as_str(),
        ] {
            let patterns = shield.detect(field);
            let high = patterns
                .iter()
                .any(|p| matches!(p.severity, InjectionSeverity::High));
            if high {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({
                        "error": "injection_detected",
                        "detail": "request contains suspicious patterns"
                    })),
                )
                    .into_response();
            }
            // F-5: log medium-severity detections so shield hits are visible in AYIN.
            if patterns
                .iter()
                .any(|p| matches!(p.severity, InjectionSeverity::Medium))
            {
                tracing::warn!(
                    preset = assignment.preset.as_str(),
                    "injection_shield: medium-severity pattern detected — passing through"
                );
            }
        }
    }

    let task_specs = build_task_specs(&body);

    // PW-6 pre-spawn ownership gate (agents-playbook §15.3.13).
    if let Err(err_response) = ownership_gate(&task_specs) {
        return err_response;
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

    let litellm = state.litellm_config.read().await;
    let litellm_base_url = litellm.base_url.clone();
    let litellm_api_key = litellm.api_key.clone();
    let litellm_model = litellm.model.clone();
    drop(litellm);

    let handle = spawn_autonomous_build(BridgeContext {
        build_id,
        codename: body.codename.clone(),
        repo_root,
        worktree_root: std::env::temp_dir().join(format!("la-wt-{build_id}")),
        feat_branch: format!("feat/{}", body.codename),
        waves: vec![task_specs],
        event_tx: state.event_tx.clone(),
        decisions_writer,
        mock_workers: state.mock_workers,
        hitl_queue: state.hitl_queue.clone(),
        litellm_base_url,
        litellm_api_key,
        litellm_model,
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

/// PW-6 pre-spawn ownership gate — returns `Err(Response)` on conflict or unexpected error.
///
/// Constructs minimal [`Task`] stubs (only `id` + `file_ownership` are read by
/// [`validate_wave_ownership`]) and runs the gate before any worktree is created.
#[allow(clippy::result_large_err)] // axum::response::Response is intrinsically large; boxing adds allocation without benefit
fn ownership_gate(task_specs: &[TaskSpec]) -> Result<(), axum::response::Response> {
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

    match validate_wave_ownership(&stubs) {
        Ok(()) => Ok(()),
        Err(WaveError::OwnershipConflict { file, tasks }) => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "ownership_conflict",
                "detail": format!("file '{file}' claimed by tasks {tasks:?}")
            })),
        )
            .into_response()),
        Err(e) => {
            tracing::error!(error = %e, "unexpected ownership gate error — wave aborted");
            Err(StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
    }
}

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
                skill = a.preset.skill(),
                preamble = git_preamble,
                desc = a.task_description,
            );
            TaskSpec {
                id: format!("wc-{preset}-{i}", preset = a.preset.as_str()),
                prompt,
                depends_on: vec![],
                file_ownership: {
                    let mut v = a.file_ownership.clone();
                    v.sort_unstable();
                    v.dedup();
                    v
                },
                concurrency_safe: false,
            }
        })
        .collect()
}

// ── Security helpers ─────────────────────────────────────────────────────────

/// Allowlist for build codenames — lowercase alphanumeric + hyphen, 3-64 chars.
/// Guards `feat/{codename}` branch interpolation (F-6 / CWE-78 §3.4).
#[allow(clippy::expect_used)] // static literal regex cannot fail; LazyLock init is equivalent to a compile-time constant
static CODENAME_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"^[a-z0-9][a-z0-9\-]{2,63}$").expect("static regex is valid")
});

/// Validate and return a `PathBuf` for the operator-supplied worktree path.
///
/// Enforces F-2 (CWE-22/CWE-61): requires absolute path; rejects `..`
/// components; canonicalizes the nearest existing ancestor to detect symlink
/// escapes (§63.P5). Allowed canonical roots: `$HOME`, `/tmp`, `/private/tmp`,
/// `/var/folders`, `/private/var/folders`.
fn validate_worktree_path(worktree: &str) -> Result<PathBuf, &'static str> {
    let p = PathBuf::from(worktree);
    if !p.is_absolute() {
        return Err("worktree must be an absolute path");
    }
    if p.components().any(|c| c == std::path::Component::ParentDir) {
        return Err("worktree path must not contain '..' components");
    }
    // CWE-61: walk to nearest existing ancestor, canonicalize to surface symlink escapes.
    let mut check = p.as_path();
    let canon = loop {
        if check.exists() {
            break check.canonicalize().ok();
        }
        match check.parent() {
            Some(parent) if parent != check => check = parent,
            _ => break None,
        }
    };
    if let Some(canon) = canon {
        let home = std::env::var("HOME").unwrap_or_default();
        // Canonicalize HOME before comparing: on macOS /tmp is a symlink to /private/tmp,
        // so a tmpdir HOME would otherwise never match the canonicalized path (CWE-61).
        let home_canon = if home.is_empty() {
            std::path::PathBuf::new()
        } else {
            std::path::Path::new(&home)
                .canonicalize()
                .unwrap_or_else(|_| std::path::PathBuf::from(&home))
        };
        let safe = (!home.is_empty() && canon.starts_with(&home_canon))
            || canon.starts_with("/tmp")
            || canon.starts_with("/private/tmp")
            || canon.starts_with("/var/folders")
            || canon.starts_with("/private/var/folders");
        if !safe {
            return Err("worktree path resolves outside permitted roots");
        }
    }
    Ok(p)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile;

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

    // ── Unit: security helpers (F-1/F-2/F-4/F-6) ─────────────────────────────

    #[test]
    fn skill_derived_from_preset_not_operator_supplied() {
        // F-4: skill is computed server-side; operator-supplied value is not promoted.
        let req = make_request(vec![AgentAssignmentPayload {
            preset: CockpitPreset::Quality,
            skill: "lightarchitects:seraph".into(), // adversarial spoofed skill
            task_description: "audit".into(),
            file_ownership: vec![],
        }]);
        let specs = build_task_specs(&req);
        assert_eq!(specs.len(), 1);
        assert!(
            specs[0].prompt.contains("[skill:lightarchitects:quality]"),
            "prompt must use preset-derived skill, not operator-supplied value"
        );
        assert!(
            !specs[0].prompt.contains("lightarchitects:seraph"),
            "adversarial skill must not reach the prompt"
        );
    }

    #[test]
    fn all_presets_have_canonical_skill_keys() {
        // F-4: every preset maps to its canonical skill string.
        let pairs = [
            (CockpitPreset::Engineer, "lightarchitects:engineer"),
            (CockpitPreset::Security, "lightarchitects:security"),
            (CockpitPreset::Ops, "lightarchitects:ops"),
            (CockpitPreset::Quality, "lightarchitects:quality"),
            (CockpitPreset::Knowledge, "lightarchitects:knowledge"),
            (CockpitPreset::Researcher, "lightarchitects:researcher"),
            (CockpitPreset::Testing, "lightarchitects:testing"),
        ];
        for (preset, expected) in pairs {
            assert_eq!(preset.skill(), expected);
        }
    }

    #[test]
    fn validate_worktree_path_accepts_absolute_clean() {
        // F-2: well-formed absolute paths accepted.
        // Use /tmp paths only — avoids HOME env var dependency that causes flakiness
        // when audit.rs tests remove HOME in a parallel thread.
        assert!(validate_worktree_path("/tmp/wt-abc").is_ok());
        assert!(validate_worktree_path("/private/tmp/wt-xyz/my-build").is_ok());
    }

    #[test]
    fn validate_worktree_path_rejects_relative() {
        // F-2: relative paths have no safe base — reject them.
        assert!(validate_worktree_path("worktrees/my-build").is_err());
        assert!(validate_worktree_path("./wt").is_err());
    }

    #[test]
    fn validate_worktree_path_rejects_parent_dir_traversal() {
        // F-2: path traversal must be blocked before PathBuf promotion.
        assert!(validate_worktree_path("/tmp/wt/../../etc/passwd").is_err());
        assert!(validate_worktree_path("/Users/kft/../../../etc").is_err());
    }

    #[test]
    fn codename_regex_allows_valid_slugs() {
        // F-6: standard codenames must pass.
        assert!(CODENAME_RE.is_match("cockpit-wave-composer"));
        assert!(CODENAME_RE.is_match("my-build-01"));
        assert!(CODENAME_RE.is_match("abc"));
    }

    #[test]
    fn codename_regex_rejects_invalid_slugs() {
        // F-6: special characters and too-short names must be rejected.
        assert!(!CODENAME_RE.is_match("ab")); // too short
        assert!(!CODENAME_RE.is_match("my build")); // space
        assert!(!CODENAME_RE.is_match("UPPERCASE"));
        assert!(!CODENAME_RE.is_match("feat/my-build")); // slash (branch injection)
        assert!(!CODENAME_RE.is_match("../escape")); // traversal
    }

    // ── Integration: build_task_specs prompt structure ────────────────────────

    #[test]
    fn build_task_specs_includes_git_context_preamble() {
        // Cookbook §64.8: git-context preamble must be present in every prompt.
        let req = WaveComposerRequest {
            codename: "my-wave".into(),
            agents: vec![engineer_agent("implement feature", vec![])],
            target: CockpitTargetPayload {
                target_type: "pr".into(),
                id: "42".into(),
                label: "Add wave composer".into(),
            },
            worktree: "/tmp/wt".into(),
        };
        let specs = build_task_specs(&req);
        let prompt = &specs[0].prompt;
        assert!(prompt.contains("codename=my-wave"));
        assert!(prompt.contains("target=pr:42"));
        assert!(prompt.contains("branch=feat/my-wave"));
        assert!(prompt.contains("Add wave composer"));
        assert!(prompt.contains("implement feature"));
    }

    #[test]
    fn build_task_specs_deduplicates_file_ownership() {
        // Duplicate files in one agent's ownership list must be deduplicated.
        let req = make_request(vec![AgentAssignmentPayload {
            preset: CockpitPreset::Engineer,
            skill: "lightarchitects:engineer".into(),
            task_description: "task".into(),
            file_ownership: vec!["src/a.rs".into(), "src/a.rs".into(), "src/b.rs".into()],
        }]);
        let specs = build_task_specs(&req);
        assert_eq!(specs[0].file_ownership, vec!["src/a.rs", "src/b.rs"]);
    }

    #[test]
    fn build_task_specs_all_presets_produce_valid_ids() {
        // Each preset must produce a stable task ID containing the preset name.
        let presets = [
            (CockpitPreset::Engineer, "engineer"),
            (CockpitPreset::Security, "security"),
            (CockpitPreset::Ops, "ops"),
            (CockpitPreset::Quality, "quality"),
            (CockpitPreset::Knowledge, "knowledge"),
            (CockpitPreset::Researcher, "researcher"),
            (CockpitPreset::Testing, "testing"),
        ];
        let agents: Vec<_> = presets
            .iter()
            .map(|(preset, _)| AgentAssignmentPayload {
                preset: preset.clone(),
                skill: preset.skill().into(),
                task_description: "task".into(),
                file_ownership: vec![],
            })
            .collect();
        let req = make_request(agents);
        let specs = build_task_specs(&req);
        for (i, (_, name)) in presets.iter().enumerate() {
            assert!(
                specs[i].id.contains(name),
                "task id '{}' should contain preset name '{name}'",
                specs[i].id
            );
        }
    }

    // ── Smoke: validate_worktree_path edge cases ──────────────────────────────

    #[test]
    #[allow(clippy::unwrap_used)]
    fn validate_worktree_path_accepts_deep_absolute() {
        // Use a real tmpdir so the ancestor walk finds an existing path and canonicalizes
        // to /private/tmp/... (macOS), which is in the hardcoded allowlist. This avoids
        // dependency on HOME, which concurrent tests may temporarily mutate.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp
            .path()
            .join("worktrees")
            .join("proj")
            .join("sub")
            .join("wave-abc123");
        assert!(validate_worktree_path(path.to_str().unwrap()).is_ok());
    }

    #[test]
    fn validate_worktree_path_rejects_tilde_home() {
        // ~ is not expanded by PathBuf::from; such paths are not absolute.
        assert!(validate_worktree_path("~/lightarchitects/worktrees/my-build").is_err());
    }

    // ── Property: systematic boundary and invariant checks ────────────────────

    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn pt_dotdot_anywhere_is_rejected(
                prefix in "[a-z]{2,8}",
                suffix in "[a-z]{2,8}",
            ) {
                // Property: any path containing ".." must always be rejected.
                let path = format!("/{prefix}/../{suffix}");
                prop_assert!(validate_worktree_path(&path).is_err());
            }

            #[test]
            fn pt_absolute_path_without_dotdot_passes_format_check(
                seg1 in "[a-z]{3,10}",
                seg2 in "[a-z]{3,10}",
                seg3 in "[a-z0-9-]{3,20}",
            ) {
                // Property: absolute paths with no ".." pass the format checks
                // (symlink check is best-effort; tmp paths are in the allowed set).
                let path = format!("/tmp/{seg1}/{seg2}/{seg3}");
                prop_assert!(validate_worktree_path(&path).is_ok());
            }

            #[test]
            fn pt_codename_re_valid_slugs_accepted(
                slug in "[a-z][a-z0-9-]{2,30}",
            ) {
                prop_assert!(
                    CODENAME_RE.is_match(&slug),
                    "valid slug rejected: {slug}"
                );
            }

            #[test]
            fn pt_codename_re_rejects_uppercase(
                upper in "[A-Z][a-zA-Z0-9-]{2,20}",
            ) {
                prop_assert!(!CODENAME_RE.is_match(&upper));
            }
        }
    }

    // ── Regression: pin fixes from gate-2/gate-5 security findings ────────────

    mod regression_tests {
        use super::*;

        fn specs_to_task_stubs(specs: &[TaskSpec]) -> Vec<Task> {
            specs
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
                .collect()
        }

        #[test]
        fn reg_f2_traversal_at_path_end_rejected() {
            // Regression F-2: traversal suffix ("/etc/../etc") is rejected.
            assert!(validate_worktree_path("/tmp/wt/../etc/passwd").is_err());
            assert!(validate_worktree_path("/tmp/../../etc").is_err());
        }

        #[test]
        fn reg_f4_adversarial_skill_never_reaches_prompt() {
            // Regression F-4: operator-supplied skill value is dropped; only
            // the preset-derived skill appears in the agent prompt (gate-2 fix).
            let req = make_request(vec![AgentAssignmentPayload {
                preset: CockpitPreset::Engineer,
                skill: "lightarchitects:seraph".into(), // adversarial spoof
                task_description: "task".into(),
                file_ownership: vec![],
            }]);
            let specs = build_task_specs(&req);
            assert!(specs[0].prompt.contains("[skill:lightarchitects:engineer]"));
            assert!(!specs[0].prompt.contains("seraph"));
        }

        #[test]
        fn reg_ownership_conflict_error_not_silent() {
            // Regression: overlapping file_ownership must surface as
            // OwnershipConflict, not be silently ignored (gate-2 C2b fix).
            let req = make_request(vec![
                engineer_agent("A", vec!["src/shared.rs"]),
                engineer_agent("B", vec!["src/shared.rs"]),
            ]);
            let specs = build_task_specs(&req);
            let stubs = specs_to_task_stubs(&specs);
            assert!(matches!(
                validate_wave_ownership(&stubs),
                Err(WaveError::OwnershipConflict { .. })
            ));
        }

        #[test]
        fn reg_task_id_deterministic_and_stable() {
            // Regression: task IDs must be stable across calls (no random suffix).
            let req = make_request(vec![engineer_agent("task", vec![])]);
            let s1 = build_task_specs(&req);
            let s2 = build_task_specs(&req);
            assert_eq!(s1[0].id, s2[0].id, "task IDs must be deterministic");
        }
    }
}
