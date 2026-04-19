//! Phase E auth-profile tests — verify backend-specific env injection.
//!
//! The flow under test:
//!
//! 1. Webshell starts with `Config.agent = Lightarchitects(<backend>)`.
//! 2. A `POST /api/builds` creates a `BuildSession` that inherits the
//!    Config's backend.
//! 3. When the PTY spawns, `run_session` calls `BuildSession::build_spawn_env`
//!    to build the env-var list injected into the child process.
//!
//! These tests cover step 3's output — the exact `(key, value)` list that
//! `portable_pty::CommandBuilder::env` would forward to the child. We do
//! NOT spawn a real PTY: that is Phase F+CHASE manual territory. We DO
//! verify that the Config's backend choice reaches `build_spawn_env` via
//! the HTTP + registry pathway, which is the architectural seam a
//! regression is most likely to hit.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{collections::HashMap, ffi::OsString, path::PathBuf, sync::Arc, time::Duration};

use lightarchitects_webshell::{
    config::{AgentSession, ClaudeBackend, Cli, Config, OllamaConfig},
    server::{AppState, build_app},
    session::BuildRegistry,
};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use uuid::Uuid;

const TOKEN: &str = "phase-e-auth-profile-token";

fn cfg_with_agent(agent: AgentSession) -> Config {
    let cli = Cli {
        port: 0,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let mut cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    cfg.agent = agent;
    cfg
}

async fn spawn_server_with(cfg: Config) -> (String, Arc<BuildRegistry>) {
    let state = AppState::for_test(cfg);
    let builds = Arc::clone(&state.builds);
    let app = build_app(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), builds)
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

async fn post_build(base: &str, cwd: &str) -> Uuid {
    let resp: Value = http()
        .post(format!("{base}/api/builds"))
        .bearer_auth(TOKEN)
        .json(&json!({ "cwd": cwd }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    resp["build_id"].as_str().unwrap().parse().unwrap()
}

/// Indexable env view — `build_spawn_env` returns a `Vec<(String, String)>`
/// because ordered insertion into `CommandBuilder` is what `portable-pty`
/// wants. For assertion, a `HashMap` is more ergonomic.
fn env_map(pairs: &[(String, String)]) -> HashMap<&str, &str> {
    pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}

// ── Anthropic profile ───────────────────────────────────────────────────────

#[tokio::test]
async fn anthropic_build_injects_la_vars_only() {
    let cfg = cfg_with_agent(AgentSession::Lightarchitects(ClaudeBackend::Anthropic));
    let (base, builds) = spawn_server_with(cfg).await;
    let id = post_build(&base, "/tmp/anth").await;

    let session = builds.get(id).expect("session registered");
    let env = session.build_spawn_env(&base);
    let map = env_map(&env);

    // LA_* triad is always present.
    assert!(
        map.contains_key("LA_BUILD_ID"),
        "LA_BUILD_ID missing: {env:?}"
    );
    assert_eq!(map["LA_BUILD_ID"], session.build_id.to_string());
    assert_eq!(map["LA_NOTIFY_TOKEN"], session.notify_token_hex());
    assert_eq!(map["LA_GUI_URL"], base);

    // Anthropic → NO ANTHROPIC_* overrides (Claude uses its native auth).
    assert!(
        !map.keys().any(|k| k.starts_with("ANTHROPIC_")),
        "Anthropic profile must not set ANTHROPIC_* env: {env:?}"
    );
}

// ── Ollama profile ──────────────────────────────────────────────────────────

#[tokio::test]
async fn ollama_build_injects_anthropic_overrides() {
    let oc = OllamaConfig {
        base_url: "http://localhost:11434".to_owned(),
        model: "qwen3-coder:480b-cloud".to_owned(),
        auth_token: "sk-test-ollama".to_owned(),
    };
    let cfg = cfg_with_agent(AgentSession::Lightarchitects(ClaudeBackend::Ollama(
        oc.clone(),
    )));
    let (base, builds) = spawn_server_with(cfg).await;
    let id = post_build(&base, "/tmp/ollama").await;

    let session = builds.get(id).expect("session registered");
    let env = session.build_spawn_env(&base);
    let map = env_map(&env);

    // LA_* triad still present.
    assert_eq!(map["LA_BUILD_ID"], session.build_id.to_string());
    assert_eq!(map["LA_NOTIFY_TOKEN"], session.notify_token_hex());
    assert_eq!(map["LA_GUI_URL"], base);

    // ANTHROPIC_* triad reflects the OllamaConfig values — Claude will hit
    // the Ollama Anthropic-compat endpoint instead of api.anthropic.com.
    assert_eq!(map["ANTHROPIC_BASE_URL"], oc.base_url);
    assert_eq!(map["ANTHROPIC_MODEL"], oc.model);
    assert_eq!(map["ANTHROPIC_AUTH_TOKEN"], oc.auth_token);
}

// ── Build details do NOT leak Ollama secrets ────────────────────────────────

#[tokio::test]
async fn build_details_endpoint_redacts_ollama_auth_token() {
    // The HTTP API `AgentDescriptor` strips `auth_token` and `base_url` so a
    // browser fetching `/api/builds/:id` never sees the Ollama key. This is
    // separate from the env-injection path (where the child process DOES
    // need these values to reach the backend).
    let oc = OllamaConfig {
        base_url: "http://localhost:11434".to_owned(),
        model: "qwen3-coder:480b-cloud".to_owned(),
        auth_token: "sk-MUST-NOT-LEAK".to_owned(),
    };
    let cfg = cfg_with_agent(AgentSession::Lightarchitects(ClaudeBackend::Ollama(oc)));
    let (base, _builds) = spawn_server_with(cfg).await;
    let id = post_build(&base, "/tmp/ollama-leak-test").await;

    let details: Value = http()
        .get(format!("{base}/api/builds/{id}"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let body_str = details.to_string();
    assert!(
        !body_str.contains("sk-MUST-NOT-LEAK"),
        "auth_token leaked in /api/builds/:id response: {body_str}"
    );
    assert!(
        !body_str.contains("11434"),
        "base_url leaked in /api/builds/:id response: {body_str}"
    );
    assert_eq!(details["agent"]["backend"], "ollama");
}

// ── Argv reflects per-build overrides ───────────────────────────────────────

#[tokio::test]
async fn per_build_overrides_show_up_in_argv() {
    let cfg = cfg_with_agent(AgentSession::Lightarchitects(ClaudeBackend::Anthropic));
    let (base, builds) = spawn_server_with(cfg).await;

    // Create with all optional overrides — this exercises the full
    // `CreateBuildRequest` → `BuildSession` field-copy path.
    let resp: Value = http()
        .post(format!("{base}/api/builds"))
        .bearer_auth(TOKEN)
        .json(&json!({
            "cwd": "/tmp/argv-test",
            "claude_agent_template": "corso",
            "model": "opus",
            "system_prompt": "You are CORSO.",
            "allowed_tools": "Read Grep",
            "disallowed_tools": "Bash"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id: Uuid = resp["build_id"].as_str().unwrap().parse().unwrap();
    let session = builds.get(id).expect("registered");

    let argv = session.build_argv();
    assert!(argv.windows(2).any(|w| w == ["--agent", "corso"]));
    assert!(argv.windows(2).any(|w| w == ["--model", "opus"]));
    assert!(
        argv.windows(2)
            .any(|w| w == ["--system-prompt", "You are CORSO."])
    );
    assert!(
        argv.windows(2)
            .any(|w| w == ["--allowedTools", "Read Grep"])
    );
    assert!(argv.windows(2).any(|w| w == ["--disallowedTools", "Bash"]));
    // Baseline always present.
    assert!(argv.iter().any(|a| a == "--add-dir"));
    assert!(argv.iter().any(|a| a == "/tmp/argv-test"));
}
