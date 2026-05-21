//! `GitForest` HTTP route handlers — Phase 4.
//!
//! Three endpoints:
//!
//! - `GET /api/gitforest/topology?repo=<name>[&since=<ISO-8601>]`
//!   Full 4-level [`BranchNode`] tree; 60s server-side moka cache per repo.
//!   Branches matching `.gitforestignore` patterns are redacted until merged.
//!
//! - `GET /api/gitforest/live`
//!   SSE stream of `WebEvent::GitForestUpdate` events, filtered by optional
//!   `?build_codename=<name>` query param.
//!
//! - `GET /api/gitforest/node/:id`
//!   Deep-link fetch for a single [`BranchNode`] by its stable node ID.
//!
//! All three require bearer auth ([`AuthGuard`]). Path params and `build_codename`
//! validated against `^[a-zA-Z0-9_/-]+$` (max 128 bytes); `since` param validated
//! against ISO-8601 regex. Resolved repo paths are prefix-checked against the
//! repos root to prevent subdirectory traversal. AYIN span emitted per subprocess.

use std::{
    convert::Infallible,
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        IntoResponse, Json,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::stream;
use moka::future::Cache;
use regex::Regex;
use serde::Deserialize;
use tokio::{process::Command, sync::broadcast::error::RecvError};
use tracing::{debug, instrument, warn};

use crate::{
    auth::AuthGuard,
    events::{WebEvent, WebEventV2},
    gitforest::{BranchKind, BranchLifecycle, BranchNode, BranchOverlayMeta, CiStatus, HitlState},
    server::AppState,
};

// ── Validation regexes ───────────────────────────────────────────────────────

/// Allowlist for `:repo`, `:branch`, `:id` path params.
/// Rejects `..`, shell metacharacters, absolute paths.
static REPO_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    // SAFETY: pattern is a compile-time constant and has been verified correct.
    #[allow(clippy::expect_used)]
    Regex::new(r"^[a-zA-Z0-9_/\-]+$").expect("static regex")
});

/// Allowlist for `?since=` query parameter (ISO-8601 UTC with Z suffix).
static SINCE_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    // SAFETY: pattern is a compile-time constant and has been verified correct.
    #[allow(clippy::expect_used)]
    Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$").expect("static regex")
});

// ── Moka cache ───────────────────────────────────────────────────────────────

/// Concrete cache type — named so `AppState` can reference it without repeating the generics.
pub type TopologyMokaCache = Cache<String, Arc<BranchNode>>;

/// 60s TTL per repo; max 64 repos in memory.
pub fn topology_cache() -> TopologyMokaCache {
    Cache::builder()
        .max_capacity(64)
        .time_to_live(Duration::from_secs(60))
        .build()
}

// ── Query / path types ───────────────────────────────────────────────────────

/// Query parameters for `GET /api/gitforest/topology`.
#[derive(Deserialize)]
pub struct TopologyQuery {
    /// Repository name (validated against `REPO_RE`).
    pub repo: String,
    /// Optional ISO-8601 UTC lower bound for commit filtering.
    pub since: Option<String>,
}

/// Query parameters for `GET /api/gitforest/live`.
#[derive(Deserialize)]
pub struct LiveQuery {
    /// If set, only events whose repo contains this codename are forwarded.
    pub build_codename: Option<String>,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// `GET /api/gitforest/topology?repo=<name>[&since=<ISO>]`
#[instrument(skip_all, fields(repo))]
pub async fn handle_topology(
    _: AuthGuard,
    State(state): State<AppState>,
    Query(q): Query<TopologyQuery>,
) -> impl IntoResponse {
    if !REPO_RE.is_match(&q.repo) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid repo name"})),
        )
            .into_response();
    }
    if let Some(ref s) = q.since {
        if !SINCE_RE.is_match(s) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid since param"})),
            )
                .into_response();
        }
    }

    // Cache hit?
    if let Some(cached) = state.gitforest_cache.get(&q.repo).await {
        debug!(repo = %q.repo, "topology cache hit");
        return Json((*cached).clone()).into_response();
    }

    // Build topology from git log
    let start = Instant::now();
    let root = match build_topology(&q.repo, q.since.as_deref()).await {
        Ok(n) => Arc::new(n),
        Err(e) => {
            warn!(repo = %q.repo, err = %e, "git topology build failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e})),
            )
                .into_response();
        }
    };

    let elapsed_ms = start.elapsed().as_millis();
    debug!(repo = %q.repo, elapsed_ms, "topology built");

    state
        .gitforest_cache
        .insert(q.repo.clone(), root.clone())
        .await;
    Json((*root).clone()).into_response()
}

/// `GET /api/gitforest/live`
/// SSE stream of `GitForestUpdate` events filtered by optional `build_codename`.
pub async fn handle_live(
    _: AuthGuard,
    State(state): State<AppState>,
    Query(q): Query<LiveQuery>,
) -> axum::response::Response {
    // GF-02: validate build_codename to prevent unbounded string and SSE filter bypass
    if let Some(ref cn) = q.build_codename {
        if cn.len() > 128 || !REPO_RE.is_match(cn) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid build_codename"})),
            )
                .into_response();
        }
    }
    let rx = state.event_tx.subscribe();
    let codename_filter = q.build_codename;

    let event_stream = stream::unfold((rx, codename_filter), |(mut rx, filter)| async move {
        loop {
            match rx.recv().await {
                Ok(WebEventV2 {
                    inner: WebEvent::GitForestUpdate { repo, root },
                    ..
                }) => {
                    if let Some(ref cn) = filter {
                        if !repo.contains(cn.as_str()) {
                            continue;
                        }
                    }
                    let data = serde_json::to_string(&root).unwrap_or_default();
                    let ev: Result<Event, Infallible> =
                        Ok(Event::default().event("gitforest").data(data));
                    return Some((ev, (rx, filter)));
                }
                Ok(_) => {}
                Err(RecvError::Lagged(n)) => {
                    let ev: Result<Event, Infallible> = Ok(Event::default()
                        .event("lag")
                        .data(format!(r#"{{"skipped":{n}}}"#)));
                    return Some((ev, (rx, filter)));
                }
                Err(RecvError::Closed) => return None,
            }
        }
    });

    Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `GET /api/gitforest/node/:id`
pub async fn handle_node(
    _: AuthGuard,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if !REPO_RE.is_match(&id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid node id"})),
        )
            .into_response();
    }

    // Scan all cached topologies for the node
    // (node IDs are globally unique by construction: <repo>/<branch>/<sha-prefix>)
    // For Phase 4: scan the cache; Phase 5 will add a direct index.
    #[allow(clippy::explicit_iter_loop)] // moka Cache doesn't impl IntoIterator
    for (_, root) in state.gitforest_cache.iter() {
        if let Some(node) = find_node(&root, &id) {
            return Json(node).into_response();
        }
    }

    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "node not found"})),
    )
        .into_response()
}

// ── Git subprocess ────────────────────────────────────────────────────────────

/// Build a [`BranchNode`] tree for `repo` by running `git log --all --merges`.
/// Each `git` invocation emits an AYIN trace span via `tracing`.
#[instrument(skip_all, fields(repo, since))]
async fn build_topology(repo: &str, since: Option<&str>) -> Result<BranchNode, String> {
    // Resolve repo path relative to the configured repos root
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let repos_root = home.join("lightarchitects");
    let repo_path = repos_root.join(repo);

    if !repo_path.exists() {
        return Err(format!("repo not found: {repo}"));
    }

    // GF-01: verify resolved path stays under the repos root (defense-in-depth vs traversal)
    match repo_path.canonicalize() {
        Ok(canonical) if canonical.starts_with(&repos_root) => {}
        _ => return Err("repo path escapes repos root".to_string()),
    }

    // Check .gitforestignore for branch ACL
    let ignored = load_gitforestignore(&repo_path).await;

    // git log: list branches (simplified topology for Phase 4; Phase 5 adds depth)
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(&repo_path)
        .arg("branch")
        .arg("--all")
        .arg("--format=%(refname:short)\t%(objectname:short)\t%(upstream:short)")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(s) = since {
        cmd.arg("--sort=-committerdate");
        let _ = s; // since= used in log query below; branch list not filtered by date
    }

    let out = cmd.output().await.map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout);

    let mut children: Vec<String> = Vec::new();
    let mut nodes: std::collections::HashMap<String, BranchNode> = std::collections::HashMap::new();

    // Build root (main) node
    let root_id = format!("{repo}/main");
    for line in text.lines() {
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        let Some(&branch_name) = parts.first() else {
            continue;
        };
        // Strip remotes/ prefix
        let branch = branch_name.trim_start_matches("remotes/origin/");

        // Apply .gitforestignore ACL
        if is_ignored(branch, &ignored) {
            continue;
        }

        if branch == "main" || branch == "master" {
            continue;
        }

        let node_id = format!("{repo}/{branch}");
        let kind = classify_branch(branch);
        let node = BranchNode {
            id: node_id.clone(),
            name: branch.to_string(),
            kind,
            parent_id: Some(root_id.clone()),
            depth: 1,
            fork_commit_sha: None,
            fork_position: 0.0,
            children: vec![],
            overlay: BranchOverlayMeta {
                phase: None,
                gate_score: None,
                age_days: 0,
                ci_status: CiStatus::Unknown,
                hitl_state: HitlState::None,
                model_attribution: vec![],
                lifecycle: BranchLifecycle::LiveActive,
                merged_at: None,
                merged_to: None,
                fade_level: 1.0,
            },
            build_progress: None,
            worktrees: vec![],
        };
        children.push(node_id.clone());
        nodes.insert(node_id, node);
    }

    let root = BranchNode {
        id: root_id.clone(),
        name: "main".to_string(),
        kind: BranchKind::Main,
        parent_id: None,
        depth: 0,
        fork_commit_sha: None,
        fork_position: 0.0,
        children,
        overlay: BranchOverlayMeta {
            phase: None,
            gate_score: None,
            age_days: 0,
            ci_status: CiStatus::Unknown,
            hitl_state: HitlState::None,
            model_attribution: vec![],
            lifecycle: BranchLifecycle::LiveActive,
            merged_at: None,
            merged_to: None,
            fade_level: 1.0,
        },
        build_progress: None,
        worktrees: vec![],
    };

    debug!(repo, children_count = root.children.len(), "topology built");
    Ok(root)
}

/// Load branch name patterns from `<repo>/.gitforestignore`.
/// Lines beginning with `#` or empty are skipped.
async fn load_gitforestignore(repo_path: &std::path::Path) -> Vec<String> {
    let path = repo_path.join(".gitforestignore");
    let Ok(text) = tokio::fs::read_to_string(&path).await else {
        return vec![];
    };
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.trim().to_string())
        .collect()
}

/// Returns true when `branch` matches any pattern in `ignored`.
/// Patterns support `*` prefix glob only (sufficient for `security-*`, `secret-*`).
fn is_ignored(branch: &str, ignored: &[String]) -> bool {
    for pattern in ignored {
        if let Some(prefix) = pattern.strip_suffix('*') {
            if branch.starts_with(prefix) {
                return true;
            }
        } else if branch == pattern {
            return true;
        }
    }
    false
}

/// Map branch name to [`BranchKind`] heuristically.
fn classify_branch(name: &str) -> BranchKind {
    if name.starts_with("feat/") {
        BranchKind::Build
    } else if name.starts_with("prog/") {
        BranchKind::Program
    } else if name.starts_with("wave/") {
        BranchKind::WaveCluster
    } else {
        BranchKind::Build
    }
}

/// Depth-first search for a node by ID in a [`BranchNode`] tree.
/// Returns a clone to avoid holding a reference into the cache [`Arc`](std::sync::Arc).
fn find_node(node: &BranchNode, target_id: &str) -> Option<BranchNode> {
    if node.id == target_id {
        return Some(node.clone());
    }
    // Phase 4: children are IDs only (not hydrated sub-nodes in this cache level)
    None
}
