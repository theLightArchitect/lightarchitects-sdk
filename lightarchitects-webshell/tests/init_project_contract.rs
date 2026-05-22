//! Contract tests for `POST /api/projects/init` (Part XXI §XXI.3).
//!
//! Uses the REAL `~/Projects/` directory so the test validates against
//! projects that actually exist on disk:
//!   - `lightarchitects-sdk`  — git repo; verifies remote detection
//!   - `lightarchitects-cli`  — git repo; verifies remote detection
//!
//! Each test uses a RAII cleanup guard so `.lightarchitects/` is removed
//! on drop even if the test panics.
//!
//! 4 cases:
//! - C1: `success_returns_201` — full happy path (sdk)
//! - C2: `emits_project_update_event` — broadcast channel receives the event (cli)
//! - C3: `appends_decisions_row` — decisions.md written with correct fields (sdk)
//! - C4: `writes_helix_entry_significance_7` — index.md contains significance 7.0 (cli)

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::missing_panics_doc,
    unsafe_code
)]

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use lightarchitects_webshell::{
    config::{Cli, Config},
    events::{WebEventV2, types::WebEvent},
    projects::init::{InitBody, init_project},
    server::AppState,
};
use tokio::sync::broadcast;

// Serialize tests that mutate HOME.
static HOME_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
fn home_lock() -> &'static tokio::sync::Mutex<()> {
    HOME_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

const TOKEN: &str = "test-token-init-contract";

fn make_event_tx() -> broadcast::Sender<WebEventV2> {
    let cli = Cli {
        port: 8733,
        host_cmd: OsString::from("echo"),
        cwd: None,
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    AppState::for_test(
        cfg,
        lightarchitects_webshell::container::DockerCapability::Unavailable,
    )
    .event_tx
    .clone()
}

// ── RAII cleanup ──────────────────────────────────────────────────────────────

/// Removes `<project_dir>/.lightarchitects/` on drop.
struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let dot_la = self.0.join(".lightarchitects");
        if dot_la.exists() {
            let _ = std::fs::remove_dir_all(&dot_la);
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Resolves `~/Projects/<slug>` using the real HOME.
/// Returns `None` if the directory does not exist (test skip).
fn real_project(slug: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let p = home.join("Projects").join(slug);
    if p.is_dir() { Some(p) } else { None }
}

fn skip_if_missing(slug: &str) -> PathBuf {
    if let Some(p) = real_project(slug) {
        p
    } else {
        eprintln!("SKIP: ~/Projects/{slug} not found");
        PathBuf::new()
    }
}

fn already_inited(project_dir: &Path) -> bool {
    project_dir
        .join(".lightarchitects")
        .join("project.toml")
        .exists()
}

// ── C1: success_returns_201 ───────────────────────────────────────────────────

/// Full happy path against the real `lightarchitects-sdk` directory.
/// Verifies: 201, `project_id` present, `toml_path` written, slug in response.
#[tokio::test]
async fn success_returns_201_sdk() {
    let _guard = home_lock().lock().await;
    let project_dir = skip_if_missing("lightarchitects-sdk");
    if project_dir == PathBuf::new() || already_inited(&project_dir) {
        return;
    }
    let _cleanup = Cleanup(project_dir.clone());

    let tx = make_event_tx();
    let resp = init_project(
        InitBody {
            slug: "lightarchitects-sdk".to_owned(),
            name: Some("Light Architects SDK".to_owned()),
            kind: None,
            agents: lightarchitects_webshell::projects::types::ProjectAgents::default(),
        },
        &tx,
    )
    .await
    .expect("init must succeed");

    assert_eq!(resp.slug, "lightarchitects-sdk");
    assert!(!resp.project_id.is_empty(), "project_id must be set");
    assert!(
        std::path::Path::new(&resp.toml_path).exists(),
        "project.toml must be on disk at {}",
        resp.toml_path
    );
    // Git remote must be detected from .git/config (sdk is a git repo).
    let content = std::fs::read_to_string(&resp.toml_path).unwrap();
    assert!(
        content.contains("github.com"),
        "expected git remote in TOML, got: {content}"
    );
}

// ── C2: emits_project_update_event ───────────────────────────────────────────

/// Verifies the broadcast channel receives a `ProjectUpdate` event for the cli.
#[tokio::test]
async fn emits_project_update_event_cli() {
    let _guard = home_lock().lock().await;
    let project_dir = skip_if_missing("lightarchitects-cli");
    if project_dir == PathBuf::new() || already_inited(&project_dir) {
        return;
    }
    let _cleanup = Cleanup(project_dir.clone());

    let tx = make_event_tx();
    let mut rx = tx.subscribe();

    init_project(
        InitBody {
            slug: "lightarchitects-cli".to_owned(),
            name: None,
            kind: None,
            agents: lightarchitects_webshell::projects::types::ProjectAgents::default(),
        },
        &tx,
    )
    .await
    .expect("init must succeed");

    // The event must be in the channel (no await needed — send is sync).
    let event_v2 = rx
        .try_recv()
        .expect("ProjectUpdate event must be in channel");
    assert!(
        event_v2.topic.starts_with("v1.project"),
        "expected v1.project.* topic, got {}",
        event_v2.topic
    );
    assert!(
        matches!(event_v2.inner, WebEvent::ProjectUpdate(_)),
        "inner must be WebEvent::ProjectUpdate"
    );
}

// ── C3: appends_decisions_row ─────────────────────────────────────────────────

/// Verifies decisions.md is written with the correct fields for sdk.
#[tokio::test]
async fn appends_decisions_row_sdk() {
    let _guard = home_lock().lock().await;
    let project_dir = skip_if_missing("lightarchitects-sdk");
    if project_dir == PathBuf::new() || already_inited(&project_dir) {
        return;
    }
    let _cleanup = Cleanup(project_dir.clone());

    let tx = make_event_tx();
    let resp = init_project(
        InitBody {
            slug: "lightarchitects-sdk".to_owned(),
            name: None,
            kind: None,
            agents: lightarchitects_webshell::projects::types::ProjectAgents::default(),
        },
        &tx,
    )
    .await
    .expect("init must succeed");

    // decisions.md should live alongside the helix index.
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap();
    let decisions_path = home
        .join("lightarchitects")
        .join("soul")
        .join("helix")
        .join("corso")
        .join("projects")
        .join("lightarchitects-sdk")
        .join("decisions.md");

    let content =
        std::fs::read_to_string(&decisions_path).expect("decisions.md must exist after init");
    assert!(content.contains("## Project init events"), "header missing");
    assert!(
        content.contains(&resp.project_id),
        "project_id missing from decisions.md"
    );
    assert!(content.contains("operator"), "actor field missing");
    assert!(
        content.contains("lightarchitects-sdk"),
        "slug missing from decisions.md"
    );
}

// ── C4: writes_helix_entry_significance_7 ────────────────────────────────────

/// Verifies the helix index.md contains `significance: 7.0` for cli.
#[tokio::test]
async fn writes_helix_entry_significance_7_cli() {
    let _guard = home_lock().lock().await;
    let project_dir = skip_if_missing("lightarchitects-cli");
    if project_dir == PathBuf::new() || already_inited(&project_dir) {
        return;
    }
    let _cleanup = Cleanup(project_dir.clone());

    let tx = make_event_tx();
    let resp = init_project(
        InitBody {
            slug: "lightarchitects-cli".to_owned(),
            name: None,
            kind: None,
            agents: lightarchitects_webshell::projects::types::ProjectAgents::default(),
        },
        &tx,
    )
    .await
    .expect("init must succeed");

    let index_path = PathBuf::from(&resp.helix_link).join("index.md");
    let content =
        std::fs::read_to_string(&index_path).expect("helix index.md must exist after init");

    assert!(
        content.contains("significance: 7.0"),
        "helix entry must declare significance: 7.0, got: {content}"
    );
    assert!(
        content.contains("lightarchitects-cli"),
        "helix entry must reference the slug"
    );
    assert!(
        content.contains("type: project-marker"),
        "helix entry must declare type: project-marker"
    );
}
