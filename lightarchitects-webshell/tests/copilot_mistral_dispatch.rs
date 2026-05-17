//! Wiring confirmation — `POST /api/builds/:id/copilot` routes to `run_vibe_turn`
//! for `MistralVibe` sessions.
//!
//! Canon XXVII §50.3: a wiring confirmation test calls the PUBLIC production entry
//! point and verifies the outcome proves the component was consulted.
//!
//! Two opaque error codes prove the `MistralVibe` dispatch arm fired:
//! - `vibe_spawn_failed` — binary not found in PATH or known locations (CI environment)
//! - `vibe_subprocess_error` — binary found but exited non-zero (dev environment with
//!   `vibe` installed but no valid `MISTRAL_API_KEY` in the test context)
//!
//! Either code can only appear inside `run_vibe_turn`. Any wrong dispatch arm
//! produces a different error entirely. Accepting both makes the test robust
//! across CI (no vibe) and dev (vibe installed) environments.
//!
//! # Environment isolation
//!
//! `HOME` is overridden to an empty temp directory so that `resolve_binary("vibe")`
//! returns the bare name (no binary at `{HOME}/.local/bin/vibe`) and
//! `augmented_path()` (which also reads `HOME`) computes a PATH that does not
//! include the developer's real `~/.local/bin`. Without this, the real vibe binary
//! is found via full path, has a valid `MISTRAL_API_KEY` injected from macOS
//! Keychain, and may contact the Mistral API — exceeding the 5s test timeout.
//!
//! This file contains a single test, so there is no parallel race from the env
//! override.
//!
//! The happy-path (binary present, successful response) scenario is covered by the
//! `resolve_binary_with_home` unit tests in `copilot/mod.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, unsafe_code)]

use std::{ffi::OsString, path::PathBuf, sync::Arc, time::Duration};

use lightarchitects_webshell::{
    config::{AgentSession, Cli, Config, MistralVibeConfig},
    container::DockerCapability,
    server::{AppState, build_app},
    session::{BuildRegistry, BuildSession},
};
use serde_json::json;
use tokio::net::TcpListener;

const TOKEN: &str = "vibe-dispatch-test-token";

async fn spawn_server() -> (String, String, Arc<BuildRegistry>) {
    let tmp = std::env::temp_dir().join(format!("la-vibe-dispatch-{}", std::process::id()));
    // SAFETY: single-threaded integration test binary; env restored before any
    // concurrent test code runs.
    unsafe { std::env::set_var("LIGHTARCHITECTS_HOME", &tmp) };
    let cli = Cli {
        port: 0,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    unsafe { std::env::remove_var("LIGHTARCHITECTS_HOME") };
    let _ = std::fs::remove_dir_all(&tmp);

    let state = AppState::for_test(cfg, DockerCapability::Unavailable);
    let builds = Arc::clone(&state.builds);
    let app = build_app(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), TOKEN.to_owned(), builds)
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

/// Wiring confirmation: `POST /api/builds/:id/copilot` with a `MistralVibe` session
/// returns a `vibe_spawn_failed` or `vibe_subprocess_error` reason in the 500 body.
///
/// Both codes are produced exclusively inside `run_vibe_turn`, which is only
/// reached via the `AgentSession::MistralVibe` arm of `call_subprocess`. Any
/// incorrect dispatch arm would produce a different error entirely.
///
/// - CI (vibe absent): `vibe_spawn_failed` — binary not found
/// - Dev (vibe installed, no valid API key in test context): `vibe_subprocess_error`
#[tokio::test]
async fn mistral_vibe_copilot_dispatch_returns_run_vibe_turn_error_code() {
    // Isolate HOME so resolve_binary + augmented_path find no dev-installed vibe
    // and do not inject a Mistral API key from the real Keychain into the subprocess.
    // SAFETY: single test in this binary; no parallel env mutation in this file.
    let clean_home =
        std::env::temp_dir().join(format!("la-vibe-dispatch-home-{}", std::process::id()));
    std::fs::create_dir_all(&clean_home).unwrap();
    unsafe { std::env::set_var("HOME", &clean_home) };

    let (base, token, builds) = spawn_server().await;

    // Register a MistralVibe session directly into the shared BuildRegistry.
    let session = Arc::new(BuildSession::new(
        PathBuf::from("/tmp"),
        AgentSession::MistralVibe(MistralVibeConfig::default()),
    ));
    let build_id = session.build_id;
    builds.insert(Arc::clone(&session));

    let resp = http_client()
        .post(format!("{base}/api/builds/{build_id}/copilot"))
        .bearer_auth(&token)
        .json(&json!({ "message": "hello vibe" }))
        .send()
        .await
        .expect("request must complete");

    // Restore HOME before assertions to keep teardown clean.
    unsafe { std::env::remove_var("HOME") };
    let _ = std::fs::remove_dir_all(&clean_home);

    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = resp.json().await.expect("body must be JSON");
    assert_eq!(
        body["error"], "provider_error",
        "outer error key must be provider_error: {body}"
    );
    let reason = body["reason"].as_str().unwrap_or("");
    assert!(
        reason == "vibe_spawn_failed" || reason == "vibe_subprocess_error",
        "reason must be vibe_spawn_failed or vibe_subprocess_error — proves MistralVibe dispatch arm fired: {body}"
    );
}
