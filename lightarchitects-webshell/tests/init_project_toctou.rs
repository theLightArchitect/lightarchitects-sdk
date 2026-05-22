//! TOCTOU and atomicity tests for `POST /api/projects/init` (Part XXI §XXI.3).
//!
//! Integration-level cases (unit-level T1/T3 live in src/projects/init.rs):
//! - T2: concurrent init calls — exactly one wins 201, one gets 409
//! - T4: a poisoned path (directory at toml location) does not leave a corrupt file

#![allow(clippy::unwrap_used, clippy::expect_used, unsafe_code)]

use std::{ffi::OsString, sync::OnceLock};

use lightarchitects_webshell::{
    config::{Cli, Config},
    projects::init::{InitBody, init_project},
    server::AppState,
};
use tokio::sync::broadcast;

// Serialize tests that mutate HOME.
static HOME_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
fn home_lock() -> &'static tokio::sync::Mutex<()> {
    HOME_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

const TOKEN: &str = "test-token-init-toctou";

fn make_event_tx() -> broadcast::Sender<lightarchitects_webshell::events::WebEventV2> {
    let cli = Cli {
        port: 8733,
        host_cmd: OsString::from("echo"),
        cwd: None,
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    let state = AppState::for_test(
        cfg,
        lightarchitects_webshell::container::DockerCapability::Unavailable,
    );
    state.event_tx.clone()
}

// T2 — concurrent init: exactly one 201, one 409 (no data corruption).
// (T1/T3 write_atomic unit tests live in src/projects/init.rs#[cfg(test)])
#[tokio::test]
async fn concurrent_init_one_wins_one_409() {
    let _guard = home_lock().lock().await;
    let tmp = tempfile::TempDir::new().unwrap();
    // SAFETY: HOME_LOCK serializes HOME mutations across tests.
    unsafe { std::env::set_var("HOME", tmp.path()) };

    // Create ~/Projects/race-slug so the handler finds the root.
    let project_dir = tmp.path().join("Projects").join("race-slug");
    std::fs::create_dir_all(&project_dir).unwrap();

    let tx = make_event_tx();
    let body = || InitBody {
        slug: "race-slug".to_owned(),
        name: None,
        kind: None,
        agents: lightarchitects_webshell::projects::types::ProjectAgents::default(),
    };

    let (r1, r2) = tokio::join!(init_project(body(), &tx), init_project(body(), &tx),);

    let statuses: Vec<bool> = [r1, r2].into_iter().map(|r| r.is_ok()).collect();

    let wins = statuses.iter().filter(|&&ok| ok).count();
    let conflicts = statuses.iter().filter(|&&ok| !ok).count();

    assert_eq!(wins, 1, "exactly one init must succeed");
    assert_eq!(conflicts, 1, "exactly one init must get a conflict");

    unsafe { std::env::remove_var("HOME") };
}

// T4 — directory planted at toml_path causes AlreadyExists; no corrupt file created.
//
// Simulates a filesystem state where `project.toml` is a directory (poisoned
// state). `create_new(true)` must fail cleanly and not leave any partial file.
#[tokio::test]
async fn poisoned_toml_path_does_not_corrupt() {
    let _guard = home_lock().lock().await;
    let tmp = tempfile::TempDir::new().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let project_dir = tmp.path().join("Projects").join("poison-slug");
    // Pre-create .lightarchitects/project.toml/ as a *directory* (the poison).
    let toml_as_dir = project_dir.join(".lightarchitects").join("project.toml");
    std::fs::create_dir_all(&toml_as_dir).unwrap();

    let tx = make_event_tx();
    let result = init_project(
        InitBody {
            slug: "poison-slug".to_owned(),
            name: None,
            kind: None,
            agents: lightarchitects_webshell::projects::types::ProjectAgents::default(),
        },
        &tx,
    )
    .await;

    // Must fail — either AlreadyExists (EEXIST) or Io — never succeed.
    assert!(
        result.is_err(),
        "init must fail when toml path is a directory"
    );
    // The poisoned directory must still be a directory (not replaced by a file).
    assert!(
        toml_as_dir.is_dir(),
        "poisoned directory must remain untouched"
    );

    unsafe { std::env::remove_var("HOME") };
}
