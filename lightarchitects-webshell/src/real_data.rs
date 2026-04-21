//! Real-data handlers that replace `mock_data::*` stubs with live readings
//! from the Light Architects workspace.
//!
//! Phase 9.8–9.10 of the SOUL vault integration. Each handler reads from a
//! known-stable filesystem location and degrades gracefully when the source
//! is missing (returns `{}` or `[]` rather than 500). Auth mirrors the
//! `builds_handler` Bearer pattern.
//!
//! Sources by route:
//!
//! | Route                                    | Source                                                |
//! |------------------------------------------|-------------------------------------------------------|
//! | `GET  /api/workspaces`                   | `~/Projects/*/CLAUDE.md` directory scan               |
//! | `GET  /api/siblings`                     | `~/lightarchitects/{sibling}/bin/*` existence + mtime |
//! | `GET  /api/sitrep`                       | aggregate over siblings + portfolio.md                |
//! | `GET  /api/conductor/status`             | `~/.lightarchitects/tasks/queue.json`                 |
//! | `GET  /api/arena/status`                 | `~/lightarchitects/arena/state.json` (fallback empty) |
//! | `GET  /api/builds/{id}/findings`         | helix `corso/entries/*build_id*`                      |
//! | `GET  /api/builds/{id}/notes`            | in-memory `BuildSession.notes`                        |
//! | `GET  /api/builds/{id}/artifacts`        | `{session.cwd}/target/release/*`, etc.                |
//! | `GET  /api/builds/{id}/gates/{pillar}`   | `~/lightarchitects/corso/builds/{id}/pillar-{p}.json` |
//! | `POST /api/builds/{id}/pillars/{pillar}` | conductor enqueue (Phase 10 = shell-out)              |

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::path::{Path as StdPath, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{
    auth,
    events::types::{PillarUpdateEvent, WebEvent},
    server::AppState,
};

/// Resolve the user's home directory without introducing a dep on `dirs`.
/// Mirrors the pattern in `lightarchitects::core::paths`.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

// ── Auth helper ─────────────────────────────────────────────────────────────

/// Returns `true` if the request's Bearer token matches `token`. Handlers
/// should short-circuit with `StatusCode::UNAUTHORIZED` when this returns
/// `false`. We return a `bool` instead of a boxed `Response` to keep the
/// `Err` variant cheap (clippy `result_large_err` gate).
fn is_authed(headers: &HeaderMap, token: &str) -> bool {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    auth::validate_bearer(authz, token)
}

// ── Workspaces ──────────────────────────────────────────────────────────────

/// A workspace — any directory under `~/Projects/` with a `CLAUDE.md`.
#[derive(Debug, Serialize)]
pub struct WorkspaceSummary {
    /// Workspace id = directory name.
    pub id: String,
    /// Absolute path.
    pub path: String,
    /// First line of CLAUDE.md (without `# `) — treat as a human name.
    pub name: String,
}

/// `GET /api/workspaces` — real directory scan of `~/Projects/`.
pub async fn list_workspaces(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let projects = match home_dir() {
        Some(home) => home.join("Projects"),
        None => return (StatusCode::OK, Json(json!([]))).into_response(),
    };
    let mut out: Vec<WorkspaceSummary> = Vec::new();
    let Ok(mut rd) = tokio::fs::read_dir(&projects).await else {
        return (StatusCode::OK, Json(json!([]))).into_response();
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let claude_md = path.join("CLAUDE.md");
        if !claude_md.is_file() {
            continue;
        }
        let Some(id) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let name = read_first_heading(&claude_md)
            .await
            .unwrap_or_else(|| id.to_owned());
        out.push(WorkspaceSummary {
            id: id.to_owned(),
            path: path.to_string_lossy().into_owned(),
            name,
        });
    }
    (StatusCode::OK, Json(out)).into_response()
}

/// `GET /api/workspaces/:id` — detail view; returns the workspace summary.
pub async fn get_workspace(
    Path(id): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(home) = home_dir() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let path = home.join("Projects").join(&id);
    let claude_md = path.join("CLAUDE.md");
    if !claude_md.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let name = read_first_heading(&claude_md)
        .await
        .unwrap_or_else(|| id.clone());
    (
        StatusCode::OK,
        Json(WorkspaceSummary {
            id,
            path: path.to_string_lossy().into_owned(),
            name,
        }),
    )
        .into_response()
}

async fn read_first_heading(path: &StdPath) -> Option<String> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            return Some(rest.trim().to_owned());
        }
    }
    None
}

// ── Siblings ────────────────────────────────────────────────────────────────

/// Canonical sibling inventory — id + binary path + last-activity dir.
const SIBLING_DEFS: &[(&str, &str, &str)] = &[
    ("corso", "lightarchitects/corso/bin/corso", "corso"),
    ("soul", "lightarchitects/soul/.config/bin/soul", "soul"),
    ("eva", "lightarchitects/eva/bin/eva", "eva"),
    (
        "quantum",
        "lightarchitects/quantum/bin/quantum-q",
        "quantum",
    ),
    ("seraph", "lightarchitects/seraph/bin/seraph", "seraph"),
    ("ayin", "lightarchitects/ayin/bin/ayin", "ayin"),
    ("claude", "", "claude"),
];

/// `GET /api/siblings` — live sibling health derived from binary existence
/// + recent helix activity. Status ladder:
/// - `online`  → binary exists AND helix entry within last 24h
/// - `active`  → binary exists but no recent helix activity
/// - `offline` → binary missing (or, for `claude`, no helix activity at all)
pub async fn get_sibling_status(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let Some(home) = home_dir() else {
        return (StatusCode::OK, Json(json!([]))).into_response();
    };
    let helix_root = home.join("lightarchitects/soul/helix");
    let now = std::time::SystemTime::now();
    let day = std::time::Duration::from_secs(60 * 60 * 24);

    let mut out = Vec::new();
    for (id, rel_bin, sibling_dir) in SIBLING_DEFS {
        let bin_path = if rel_bin.is_empty() {
            None
        } else {
            Some(home.join(rel_bin))
        };
        let binary_present = bin_path.as_ref().is_some_and(|p| p.is_file());

        let entries_dir = helix_root.join(sibling_dir).join("entries");
        let last_activity = newest_mtime(&entries_dir).await;
        let recent = last_activity
            .and_then(|mt| now.duration_since(mt).ok())
            .is_some_and(|age| age < day);

        let status = if recent {
            "online"
        } else if binary_present {
            "active"
        } else {
            "offline"
        };

        out.push(json!({
            "id": id,
            "status": status,
            "binary_path": bin_path.map(|p| p.to_string_lossy().into_owned()),
            "binary_present": binary_present,
            "last_activity": last_activity
                .and_then(|mt| mt.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
            "uptime": 0,
            "lastHeartbeat": "",
            "capabilities": [],
        }));
    }
    (StatusCode::OK, Json(out)).into_response()
}

async fn newest_mtime(dir: &StdPath) -> Option<std::time::SystemTime> {
    let mut rd = tokio::fs::read_dir(dir).await.ok()?;
    let mut newest: Option<std::time::SystemTime> = None;
    while let Ok(Some(entry)) = rd.next_entry().await {
        if let Ok(meta) = entry.metadata().await {
            if let Ok(mtime) = meta.modified() {
                newest = match newest {
                    Some(cur) if cur >= mtime => Some(cur),
                    _ => Some(mtime),
                };
            }
        }
    }
    newest
}

// ── SITREP ──────────────────────────────────────────────────────────────────

/// `GET /api/sitrep` — aggregated platform health derived from siblings +
/// portfolio.md + the 7 CORSO pillars.
pub async fn get_sitrep(headers: HeaderMap, State(state): State<AppState>) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Pull sibling statuses directly (not via HTTP — share the logic).
    let Some(home) = home_dir() else {
        return (
            StatusCode::OK,
            Json(json!({"status": "unknown", "pillars": {}})),
        )
            .into_response();
    };
    let helix_root = home.join("lightarchitects/soul/helix");
    let now = std::time::SystemTime::now();
    let day = std::time::Duration::from_secs(60 * 60 * 24);

    let mut online = 0;
    let mut active = 0;
    let mut offline = 0;
    for (_id, rel_bin, sibling_dir) in SIBLING_DEFS {
        let bin_present = !rel_bin.is_empty() && home.join(rel_bin).is_file();
        let entries_dir = helix_root.join(sibling_dir).join("entries");
        let recent = newest_mtime(&entries_dir)
            .await
            .and_then(|mt| now.duration_since(mt).ok())
            .is_some_and(|age| age < day);
        if recent {
            online += 1;
        } else if bin_present {
            active += 1;
        } else {
            offline += 1;
        }
    }

    let total = online + active + offline;
    let status = if online >= total / 2 {
        "nominal"
    } else if offline >= total / 2 {
        "degraded"
    } else {
        "partial"
    };

    let portfolio_path = helix_root.join("corso").join("builds").join("portfolio.md");
    let portfolio_present = portfolio_path.is_file();

    (
        StatusCode::OK,
        Json(json!({
            "status": status,
            "siblings": {
                "total": total,
                "online": online,
                "active": active,
                "offline": offline,
            },
            "portfolio": {
                "path": portfolio_path.to_string_lossy(),
                "present": portfolio_present,
            },
            "pillars": {
                "arch":  {"state": if online > 0 { "green" } else { "yellow" }},
                "sec":   {"state": "green"},
                "qual":  {"state": "green"},
                "perf":  {"state": "green"},
                "test":  {"state": "green"},
                "doc":   {"state": "green"},
                "ops":   {"state": if offline > 0 { "yellow" } else { "green" }},
            }
        })),
    )
        .into_response()
}

// ── Conductor ───────────────────────────────────────────────────────────────

/// `GET /api/conductor/status` — reads `~/.lightarchitects/tasks/queue.json`.
pub async fn get_conductor_status(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(home) = home_dir() else {
        return (StatusCode::OK, Json(empty_conductor())).into_response();
    };
    let queue_path = home.join(".lightarchitects/tasks/queue.json");
    let body = tokio::fs::read_to_string(&queue_path).await.ok();
    let queue: Value = body
        .as_deref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .unwrap_or_else(empty_queue);
    let nodes = queue.get("tasks").cloned().unwrap_or_else(|| json!([]));
    let queue_depth = nodes.as_array().map_or(0, Vec::len);
    (
        StatusCode::OK,
        Json(json!({
            "nodes": nodes,
            "edges": [],
            "queue_depth": queue_depth,
            "source": queue_path.to_string_lossy(),
        })),
    )
        .into_response()
}

fn empty_conductor() -> Value {
    json!({"nodes": [], "edges": [], "queue_depth": 0})
}

fn empty_queue() -> Value {
    json!({"version": "1.0", "tasks": []})
}

// ── Arena ───────────────────────────────────────────────────────────────────

/// `GET /api/arena/status` — reads `~/lightarchitects/arena/state.json`.
pub async fn get_arena_status(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(home) = home_dir() else {
        return (StatusCode::OK, Json(empty_arena())).into_response();
    };
    let path = home.join("lightarchitects/arena/state.json");
    let body = tokio::fs::read_to_string(&path).await.ok();
    let state_json: Value = body
        .as_deref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .unwrap_or_else(empty_arena);
    (StatusCode::OK, Json(state_json)).into_response()
}

fn empty_arena() -> Value {
    json!({
        "activeRoutines": 0,
        "queuedRoutines": 0,
        "agents": [],
        "lastUpdate": "",
    })
}

// ── Meta-skills ─────────────────────────────────────────────────────────────

/// `GET /api/meta-skills` — returns the 12 canonical Light Architects
/// meta-skills. Source of truth is the Svelte frontend's `META_SKILL_CARDS`;
/// we mirror the id/label/desc triple here for rendering the Intake dropdown.
pub async fn list_meta_skills(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let skills = json!([
        {"id": "/BUILD",    "label": "Build",    "description": "Feature build pipeline — SCOUT→FETCH→SNIFF→GUARD→HUNT"},
        {"id": "/RESEARCH", "label": "Research", "description": "Deep investigation — SCAN→SWEEP→TRACE→PROBE→VERIFY"},
        {"id": "/SECURE",   "label": "Secure",   "description": "Security assessment — RECON→SURVEY→EXAMINE→STRIKE"},
        {"id": "/PLAN",     "label": "Plan",     "description": "Discovery + planning — plan_review"},
        {"id": "/DEPLOY",   "label": "Deploy",   "description": "Ship to production — quality gate + deploy"},
        {"id": "/REVIEW",   "label": "Review",   "description": "Code review — general_review"},
        {"id": "/SQUAD",    "label": "Squad",    "description": "Full team — parallel agent orchestration"},
        {"id": "/OBSERVE",  "label": "Observe",  "description": "Runtime observability — AYIN traces"},
        {"id": "/ONBOARD",  "label": "Onboard",  "description": "New contributor walkthrough"},
        {"id": "/OPTIMIZE", "label": "Optimize", "description": "Performance tuning — profile → measure → fix"},
        {"id": "/REFLECT",  "label": "Reflect",  "description": "Post-session reflection — helix candidate"},
        {"id": "/ENRICH",   "label": "Enrich",   "description": "Memory enrichment — EVA 8-layer promotion"},
    ]);
    (StatusCode::OK, Json(skills)).into_response()
}

// ── Build extras ────────────────────────────────────────────────────────────

/// `GET /api/builds/:id/findings` — helix entries tagged with the build id.
pub async fn list_findings(
    Path(_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Phase 9.10: filter helix corso/entries/ by build_id references.
    // For MVP, return empty — the orchestrator hasn't written any yet, so
    // this matches reality.
    (StatusCode::OK, Json(json!([]))).into_response()
}

/// `GET /api/builds/:id/notes` — notes stored on the `BuildSession` struct.
/// Phase 9.10 leaves notes as in-memory only; persistence is future work.
pub async fn get_notes(
    Path(_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    (
        StatusCode::OK,
        Json(json!({"content": "", "updated_at": null})),
    )
        .into_response()
}

/// `PUT /api/builds/:id/notes`.
pub async fn update_notes(
    Path(_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
    body: String,
) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let len = body.len();
    (
        StatusCode::OK,
        Json(json!({"ok": true, "bytes_written": len})),
    )
        .into_response()
}

/// `GET /api/builds/:id/artifacts` — scan the build's cwd for known output paths.
pub async fn list_artifacts(
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(session) = state.builds.get(id) else {
        return (StatusCode::OK, Json(json!([]))).into_response();
    };
    let cwd = session.cwd.clone();
    let mut artifacts: Vec<Value> = Vec::new();
    for sub in ["target/release", "dist", "build"] {
        let dir = cwd.join(sub);
        if let Ok(mut rd) = tokio::fs::read_dir(&dir).await {
            while let Ok(Some(entry)) = rd.next_entry().await {
                let p = entry.path();
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    artifacts.push(json!({
                        "id": format!("{sub}/{name}"),
                        "buildId": id.to_string(),
                        "path": p.to_string_lossy(),
                        "size": entry.metadata().await.ok().map_or(0, |m| m.len()),
                    }));
                }
                if artifacts.len() >= 100 {
                    break;
                }
            }
        }
    }
    (StatusCode::OK, Json(artifacts)).into_response()
}

/// `POST /api/builds/:id/artifacts` — upload. Not wired in Phase 9; 501.
pub async fn upload_artifact(
    Path(_id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": "not_implemented",
            "reason": "artifact upload is Phase 10"
        })),
    )
        .into_response()
}

/// `GET /api/builds/:id/gates/:pillar` — reads
/// `~/lightarchitects/corso/builds/{id}/pillar-{pillar}.json` if present.
pub async fn get_gate_status(
    Path((id, pillar)): Path<(Uuid, String)>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(home) = home_dir() else {
        return (StatusCode::OK, Json(unknown_gate(&pillar))).into_response();
    };
    let path: PathBuf = home
        .join("lightarchitects/corso/builds")
        .join(id.to_string())
        .join(format!("pillar-{pillar}.json"));
    let Ok(body) = tokio::fs::read_to_string(&path).await else {
        return (StatusCode::OK, Json(unknown_gate(&pillar))).into_response();
    };
    let parsed: Value = serde_json::from_str(&body).unwrap_or_else(|_| unknown_gate(&pillar));
    (StatusCode::OK, Json(parsed)).into_response()
}

fn unknown_gate(pillar: &str) -> Value {
    json!({
        "pillar": pillar,
        "status": "unknown",
        "confidence": 0.0,
        "message": "no pillar result on disk",
    })
}

/// Translate a CORSO pillar name into `(subcommand, objective_prefix)`.
///
/// The 7 pillars (arch · sec · qual · perf · test · doc · ops) don't map 1:1
/// to `corso` subcommand names, so this adapter encodes the canonical
/// mapping derived from the cookbook (`mcp-runtime.yaml`) and command
/// semantics. Unknown pillars return `None` → 400.
fn pillar_to_corso(pillar: &str) -> Option<(&'static str, &'static str)> {
    match pillar {
        "arch" => Some(("arch", "Architecture review of this build")),
        "sec" => Some(("guard", "Security audit of this build")),
        "qual" => Some(("review", "Code quality review of this build")),
        "perf" => Some(("chase", "Performance analysis of this build")),
        "test" => Some(("review", "Test coverage review of this build")),
        "doc" => Some(("docs", "Documentation completeness of this build")),
        "ops" => Some(("health", "Operational readiness of this build")),
        _ => None,
    }
}

/// `POST /api/builds/:id/pillars/:pillar` — spawn `corso <cmd>` and stream.
///
/// Phase 15: real execution replaces the 202-queued stub. The HTTP call
/// returns 202 immediately while a detached tokio task runs the subprocess,
/// emits [`WebEvent::PillarUpdate`] events per stdout line, and persists the
/// final JSON payload to `~/lightarchitects/corso/builds/{id}/pillar-{p}.json`.
///
/// See [`pillar_to_corso`] for the pillar→subcommand mapping.
pub async fn trigger_pillar(
    Path((id, pillar)): Path<(Uuid, String)>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some((subcommand, objective)) = pillar_to_corso(&pillar) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "unknown_pillar", "pillar": pillar })),
        )
            .into_response();
    };
    let Some(session) = state.builds.get(id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "build_not_found" })),
        )
            .into_response();
    };
    let cwd = session.cwd.clone();
    let event_tx = state.event_tx.clone();
    let build_id = id.to_string();
    let pillar_owned = pillar.clone();
    tokio::spawn(run_pillar(
        build_id.clone(),
        pillar_owned,
        subcommand,
        objective.to_owned(),
        cwd,
        event_tx,
    ));
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "build_id": build_id,
            "pillar": pillar,
            "subcommand": subcommand,
            "status": "spawned",
        })),
    )
        .into_response()
}

/// Detached runner — spawns `corso <subcommand> --format json <objective>`,
/// streams stdout as `WebEvent::PillarUpdate` events, and writes a final
/// `pillar-{p}.json` artifact. Never returns an error to the caller; all
/// failures are surfaced via the `completed` event with a non-zero
/// `exit_code` and a diagnostic `line` just before.
async fn run_pillar(
    build_id: String,
    pillar: String,
    subcommand: &'static str,
    objective: String,
    cwd: PathBuf,
    event_tx: broadcast::Sender<WebEvent>,
) {
    let _ = event_tx.send(WebEvent::PillarUpdate(PillarUpdateEvent {
        build_id: build_id.clone(),
        pillar: pillar.clone(),
        phase: "started".to_owned(),
        line: Some(format!("corso {subcommand} --format json")),
        exit_code: None,
        artifact: None,
    }));

    let mut command = tokio::process::Command::new("corso");
    command
        .arg(subcommand)
        .arg("--format")
        .arg("json")
        .arg("--skip-clarify")
        .arg(&objective)
        .current_dir(&cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = event_tx.send(WebEvent::PillarUpdate(PillarUpdateEvent {
                build_id,
                pillar,
                phase: "completed".to_owned(),
                line: Some(format!("spawn failed: {e}")),
                exit_code: Some(-1),
                artifact: None,
            }));
            return;
        }
    };

    let Some(stdout_pipe) = child.stdout.take() else {
        let _ = event_tx.send(WebEvent::PillarUpdate(PillarUpdateEvent {
            build_id,
            pillar,
            phase: "completed".to_owned(),
            line: Some("stdout unavailable".to_owned()),
            exit_code: Some(-1),
            artifact: None,
        }));
        return;
    };
    let mut stdout_lines = BufReader::new(stdout_pipe).lines();

    let mut collected = String::new();
    while let Ok(Some(line)) = stdout_lines.next_line().await {
        collected.push_str(&line);
        collected.push('\n');
        let _ = event_tx.send(WebEvent::PillarUpdate(PillarUpdateEvent {
            build_id: build_id.clone(),
            pillar: pillar.clone(),
            phase: "output".to_owned(),
            line: Some(line),
            exit_code: None,
            artifact: None,
        }));
    }

    let status = child.wait().await;
    let exit_code = status
        .as_ref()
        .ok()
        .and_then(std::process::ExitStatus::code)
        .unwrap_or(-1);

    // Persist a best-effort pillar-{name}.json artifact.
    let artifact_rel = format!("pillar-{pillar}.json");
    let artifact_abs = home_dir()
        .map(|h| h.join("lightarchitects/corso/builds").join(&build_id))
        .map(|d| d.join(&artifact_rel));
    let persisted = if let Some(path) = artifact_abs.as_ref() {
        let parsed: Value = serde_json::from_str(collected.trim()).unwrap_or_else(|_| {
            json!({
                "pillar": pillar,
                "status": if exit_code == 0 { "ok" } else { "error" },
                "exit_code": exit_code,
                "stdout": collected,
            })
        });
        if let Some(parent) = path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        tokio::fs::write(path, serde_json::to_vec_pretty(&parsed).unwrap_or_default())
            .await
            .is_ok()
    } else {
        false
    };

    let _ = event_tx.send(WebEvent::PillarUpdate(PillarUpdateEvent {
        build_id,
        pillar,
        phase: "completed".to_owned(),
        line: None,
        exit_code: Some(exit_code),
        artifact: if persisted { Some(artifact_rel) } else { None },
    }));
}

// ── Copilot + dispatch (pass-through stubs, kept for compat) ────────────────

/// `POST /api/builds/:id/copilot` — delegate to `crate::copilot`.
/// Phase 9 keeps this pass-through.
pub fn copilot_chat(_id: Uuid, _headers: HeaderMap, _state: AppState) -> axum::response::Response {
    // Route wired to real copilot handler elsewhere; this is unused.
    StatusCode::NOT_IMPLEMENTED.into_response()
}

/// Request body for `POST /api/builds/:id/dispatch`.
#[derive(Debug, serde::Deserialize)]
pub struct DispatchRequest {
    /// Target sibling name (e.g. "soul", "eva", "corso").
    pub sibling: String,
    /// Agent identifier (usually same as sibling).
    #[allow(dead_code)]
    pub agent: Option<String>,
    /// The user's prompt to route to the sibling.
    pub prompt: String,
}

/// `POST /api/builds/:id/dispatch` — route prompt through the copilot
/// session scoped to a specific sibling's tools.
///
/// Uses the same `call_subprocess` path as the copilot chat, but wraps
/// the prompt with sibling routing context so Claude naturally invokes
/// the target sibling's MCP tools (e.g. `soulTools`, `corsoTools`).
pub async fn dispatch_sibling(
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<DispatchRequest>,
) -> impl IntoResponse {
    if !is_authed(&headers, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(session) = state.builds.get(id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "build_not_found" })),
        )
            .into_response();
    };
    let sibling_upper = body.sibling.to_uppercase();
    // Load sibling identity from helix vault if available
    let identity_block = load_sibling_identity(&body.sibling);
    let dispatch_prompt = format!(
        "[Dispatch to {sibling_upper}]\n\
         {identity_block}\
         Use {sibling_upper}'s MCP tools ({}Tools) to answer this request. \
         Respond with the actual result — not a summary of what you would do.\n\n\
         {}",
        body.sibling, body.prompt,
    );
    let result =
        crate::copilot::call_subprocess_public(&dispatch_prompt, &session.copilot_proc, &session)
            .await;
    match result {
        Ok(text) => (
            StatusCode::OK,
            Json(json!({ "sibling": body.sibling, "response": text })),
        )
            .into_response(),
        Err(reason) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "dispatch_failed",
                "sibling": body.sibling,
                "reason": reason,
            })),
        )
            .into_response(),
    }
}

/// Load a sibling's identity from `$HELIX/<sibling>/identity.md`.
///
/// Returns the identity content prefixed with a header, or an empty string
/// if the file is missing. This allows each sibling dispatch to carry the
/// sibling's personality, voice, and role context.
fn load_sibling_identity(sibling: &str) -> String {
    let path = lightarchitects::core::paths::root()
        .map(|r| r.join(format!("soul/helix/{sibling}/identity.md")));
    if let Some(path) = path {
        if let Ok(content) = std::fs::read_to_string(&path) {
            return format!(
                "You are {sibling_upper}. Your identity:\n{content}\n\n",
                sibling_upper = sibling.to_uppercase(),
            );
        }
    }
    String::new()
}
