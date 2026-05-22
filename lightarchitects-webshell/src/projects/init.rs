//! `POST /api/projects/init` — atomic project initialisation.
//!
//! **Security contract** (Part XXI §XXI.3):
//!
//! 1. Slug validated before any filesystem access (`SlugInvalid` → 400).
//! 2. `project_dir` resolved via [`canonicalize_and_check`] against `~/Projects/`.
//!    Symlink-escape and traversal attacks → `PathTraversal` → 400.
//! 3. TOML written via `O_CREAT|O_EXCL` (`create_new(true)`) — race-free duplicate
//!    guard → `AlreadyExists` → 409.
//! 4. Audit trail appended after successful write — any I/O error propagates as 500.

use std::{
    io::Write as _,
    os::unix::fs::OpenOptionsExt as _,
    path::{Path, PathBuf},
};

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{
    auth::AuthGuard,
    events::{
        WebEventV2,
        types::{ProjectUpdateKind, ProjectUpdatePayload, WebEvent},
    },
    projects::{
        ArchPathError, canonicalize_and_check,
        types::{Project, ProjectAgents, ProjectGit, ProjectKind, ProjectMeta, Slug, SlugError},
    },
    server::AppState,
};

use super::audit;

// ── Request / response types ──────────────────────────────────────────────────

/// JSON body for `POST /api/projects/init`.
#[derive(Debug, Deserialize)]
pub struct InitBody {
    /// Project slug — validated to `^[a-z0-9][a-z0-9-]{0,62}$` before use.
    pub slug: String,
    /// Human-readable name. Defaults to the slug when absent.
    pub name: Option<String>,
    /// Project classification. Auto-detected from `.git/` presence when absent.
    pub kind: Option<ProjectKind>,
    /// Agent role assignments. Defaults to empty.
    #[serde(default)]
    pub agents: ProjectAgents,
}

/// Response body on 201 Created.
#[derive(Debug, Serialize)]
pub struct InitResponse {
    /// Stable project UUID v7.
    pub project_id: String,
    /// Validated slug.
    pub slug: String,
    /// Absolute path to the created `project.toml`.
    pub toml_path: String,
    /// Absolute path to the helix marker directory.
    pub helix_link: String,
    /// Non-fatal warning emitted when the helix marker write failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub helix_link_warning: Option<String>,
}

// ── Error type ─────────────────────────────────────────────────────────────────

/// All failure modes for [`init_project`].
#[derive(Debug, thiserror::Error)]
pub enum InitError {
    /// The supplied slug fails RFC 1035 validation.
    #[error("slug invalid: {0}")]
    SlugInvalid(#[from] SlugError),

    /// `HOME` environment variable not set.
    #[error("HOME environment variable not set")]
    HomeMissing,

    /// Path resolution detected a traversal or symlink escape.
    #[error("path traversal detected: {0}")]
    PathTraversal(#[from] ArchPathError),

    /// The `~/Projects/{slug}` directory does not exist.
    #[error("project root does not exist: {0}")]
    ProjectRootMissing(PathBuf),

    /// A `project.toml` already exists — use PUT to update.
    #[error("project already initialised")]
    AlreadyExists,

    /// An I/O error during directory creation or TOML write.
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),

    /// TOML serialisation failure (should be unreachable in practice).
    #[error("TOML serialisation failed: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    /// Audit trail append failed after a successful TOML write.
    #[error("audit trail append failed: {0}")]
    AuditAppend(std::io::Error),
}

impl IntoResponse for InitError {
    fn into_response(self) -> axum::response::Response {
        let (status, code): (StatusCode, &str) = match &self {
            InitError::SlugInvalid(_) => (StatusCode::BAD_REQUEST, "SLUG_INVALID"),
            InitError::HomeMissing => (StatusCode::INTERNAL_SERVER_ERROR, "HOME_MISSING"),
            InitError::PathTraversal(_) => (StatusCode::BAD_REQUEST, "PATH_TRAVERSAL"),
            InitError::ProjectRootMissing(_) => (StatusCode::NOT_FOUND, "PROJECT_ROOT_MISSING"),
            InitError::AlreadyExists => (StatusCode::CONFLICT, "ALREADY_EXISTS"),
            InitError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "IO_ERROR"),
            InitError::TomlSerialize(_) => (StatusCode::INTERNAL_SERVER_ERROR, "TOML_ERROR"),
            InitError::AuditAppend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "AUDIT_APPEND"),
        };
        (
            status,
            Json(json!({"code": code, "message": self.to_string()})),
        )
            .into_response()
    }
}

// ── Handler ────────────────────────────────────────────────────────────────────

/// `POST /api/projects/init` — create `.lightarchitects/project.toml` atomically.
///
/// Authenticated via [`AuthGuard`] (Bearer header or `la_session` cookie).
/// Returns 201 on success, or an appropriate 4xx/5xx with `{"code","message"}` body.
#[tracing::instrument(
    name = "webshell.project.init",
    skip(_auth, state, body),
    fields(
        project.slug = tracing::field::Empty,
        outcome = tracing::field::Empty,
        latency_ms = tracing::field::Empty,
    )
)]
pub async fn init_project_handler(
    _auth: AuthGuard,
    State(state): State<AppState>,
    Json(body): Json<InitBody>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let result = init_project(body, &state.event_tx).await;
    let elapsed = start.elapsed().as_millis();
    let span = tracing::Span::current();
    span.record("latency_ms", elapsed);
    match result {
        Ok(resp) => {
            span.record("outcome", "created");
            (StatusCode::CREATED, Json(resp)).into_response()
        }
        Err(e) => {
            span.record("outcome", e.to_string().as_str());
            e.into_response()
        }
    }
}

// ── Core logic (public for integration tests) ─────────────────────────────────

/// Core initialisation logic — separated from the axum extractor layer.
///
/// # Errors
///
/// See [`InitError`] variants for all failure modes and corresponding HTTP codes.
pub async fn init_project(
    body: InitBody,
    event_tx: &broadcast::Sender<WebEventV2>,
) -> Result<InitResponse, InitError> {
    // Step 1 — slug validation (parse-don't-validate; no FS access yet).
    let slug = Slug::validate(&body.slug)?;
    tracing::Span::current().record("project.slug", slug.as_str());

    // Step 2 — home dir.
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(InitError::HomeMissing)?;
    let projects_root = home.join("Projects");

    // Step 3 — assert project root exists before canonicalize (clean 404 path).
    let raw_project_dir = projects_root.join(slug.as_str());
    if !raw_project_dir.exists() {
        return Err(InitError::ProjectRootMissing(raw_project_dir));
    }

    // Step 4 — TOCTOU-safe resolution (two-pass: per-segment symlink + post-canonicalize).
    let project_dir = canonicalize_and_check(&raw_project_dir, &[projects_root])?;

    // Step 5 — detect git remote (no subprocess — reads `.git/config` directly).
    let git = detect_git_remote(&project_dir).await;

    // Step 6 — derive ProjectKind from git presence when not explicitly supplied.
    let kind = body.kind.unwrap_or_else(|| {
        if git.is_some() {
            ProjectKind::GitRepo
        } else {
            ProjectKind::Folder
        }
    });

    // Step 7 — mint identifiers.
    let project_id = Uuid::now_v7();
    let created_at = Utc::now();

    // Step 8 — derive helix link path.
    let helix_link = home
        .join("lightarchitects")
        .join("soul")
        .join("helix")
        .join("corso")
        .join("projects")
        .join(slug.as_str());

    // Step 9 — build ProjectMeta.
    let name = body.name.unwrap_or_else(|| slug.as_str().to_owned());
    let meta = ProjectMeta {
        project: Project {
            id: project_id,
            slug: slug.clone(),
            name,
            kind,
            created_at,
            helix_link: helix_link.clone(),
        },
        git: git.clone(),
        agents: body.agents,
    };

    // Step 10 — atomic TOML write (O_CREAT | O_EXCL | mode 0600).
    let dot_la_dir = project_dir.join(".lightarchitects");
    tokio::fs::create_dir_all(&dot_la_dir).await?;
    let toml_path = dot_la_dir.join("project.toml");
    let toml_content = toml::to_string_pretty(&meta)?;
    write_atomic(&toml_path, &toml_content)?;

    // Step 11 — audit trail (fatal: must complete after atomic write succeeds).
    audit::append_decisions_row(project_id, slug.as_str(), &project_dir, git.as_ref())
        .map_err(InitError::AuditAppend)?;

    // Step 12 — helix marker (best-effort: surfaced as warning, not error).
    let helix_link_warning = audit::write_helix_entry(&helix_link, &meta)
        .await
        .err()
        .map(|e| e.to_string());

    // Step 13 — broadcast SSE event (no receivers is fine — discard SendError).
    let _ = event_tx.send(WebEventV2::from_event(
        WebEvent::ProjectUpdate(ProjectUpdatePayload {
            project_id,
            slug: slug.as_str().to_owned(),
            kind: ProjectUpdateKind::Created,
        }),
        None,
    ));

    Ok(InitResponse {
        project_id: project_id.to_string(),
        slug: slug.as_str().to_owned(),
        toml_path: toml_path.display().to_string(),
        helix_link: helix_link.display().to_string(),
        helix_link_warning,
    })
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Atomic file write using `O_CREAT | O_EXCL | mode 0600`.
///
/// Returns [`InitError::AlreadyExists`] when the file is already present —
/// mapping the kernel-level race guard directly to the 409 Conflict path.
pub(crate) fn write_atomic(path: &Path, content: &str) -> Result<(), InitError> {
    let result = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path);

    match result {
        Ok(mut f) => {
            f.write_all(content.as_bytes())?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(InitError::AlreadyExists),
        Err(e) => Err(InitError::Io(e)),
    }
}

/// Detect a git `origin` remote by reading `.git/config` directly.
///
/// Returns `None` when the directory is not a git repo or has no `origin` remote.
/// Avoids subprocess invocation to eliminate any injection surface.
async fn detect_git_remote(project_dir: &Path) -> Option<ProjectGit> {
    let config_path = project_dir.join(".git").join("config");
    let content = tokio::fs::read_to_string(&config_path).await.ok()?;
    parse_git_config_origin(&content)
}

/// Parse the `[remote "origin"]` section from a git config file.
///
/// Handles the minimal subset needed for project init — does not attempt
/// full git-config parsing (multi-value, include directives, etc.).
fn parse_git_config_origin(config: &str) -> Option<ProjectGit> {
    let mut in_origin = false;
    let mut remote_url: Option<String> = None;

    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed == r#"[remote "origin"]"# {
            in_origin = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_origin = false;
        }
        if in_origin {
            if let Some(url) = trimmed.strip_prefix("url = ") {
                remote_url = Some(url.trim().to_owned());
            }
        }
    }

    remote_url.map(|remote| ProjectGit {
        remote,
        branch: "main".to_owned(),
    })
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn parse_git_config_extracts_origin_url() {
        let config = concat!(
            "[core]\n    repositoryformatversion = 0\n",
            r#"[remote "origin"]"#,
            "\n    url = https://github.com/TheLightArchitects/lightarchitects-sdk\n",
            "    fetch = +refs/heads/*:refs/remotes/origin/*\n",
            "[branch \"main\"]\n    remote = origin\n",
        );
        let git = parse_git_config_origin(config).unwrap();
        assert_eq!(
            git.remote,
            "https://github.com/TheLightArchitects/lightarchitects-sdk"
        );
        assert_eq!(git.branch, "main");
    }

    #[test]
    fn parse_git_config_no_origin_returns_none() {
        assert!(parse_git_config_origin("[core]\n    repositoryformatversion = 0\n").is_none());
    }

    #[test]
    fn parse_git_config_empty_returns_none() {
        assert!(parse_git_config_origin("").is_none());
    }

    #[test]
    fn write_atomic_sets_mode_0600() {
        use std::os::unix::fs::PermissionsExt as _;
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("project.toml");
        write_atomic(&path, "content = true").unwrap();
        let perms = std::fs::metadata(&path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600, "expected mode 0600");
    }

    #[test]
    fn write_atomic_rejects_duplicate() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("project.toml");
        write_atomic(&path, "first = true").unwrap();
        let err = write_atomic(&path, "second = true").unwrap_err();
        assert!(
            matches!(err, InitError::AlreadyExists),
            "expected AlreadyExists, got {err:?}"
        );
    }
}
