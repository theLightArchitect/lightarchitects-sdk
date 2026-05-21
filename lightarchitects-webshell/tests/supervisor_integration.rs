//! Supervisor integration tests — §Q mechanical check closure.
//!
//! Covered §Q checks:
//! - **Check 1** (`test_northstar_text_stored_in_sqlite`): `northstar_text` is
//!   persisted to `SQLite` via `SessionStore::set_northstar_text` and survives a
//!   read-back from `SessionStore::list`.
//! - **Check 3** (`test_neutral_stub_when_no_backend`): `evaluate_wave` returns
//!   `status: neutral, confidence: 0.5` when `ollama_base` is `None`.
//! - **Check 4** (`test_wave_complete_broadcasts_supervisor_update`): sending a
//!   synthetic `AgentEvent::WaveComplete` on the agent event bus causes
//!   `spawn_supervisor_watcher` to fire `evaluate_wave` and broadcast a
//!   `WebEvent::SupervisorUpdate` on `BuildSession.event_tx` within 500 ms.
//! - **Check 6** (`test_drift_detection_configurable_threshold`): the
//!   `SupervisorState` drift counter triggers a proposal at exactly N
//!   consecutive drifting waves, where N is configurable per-build.
//! - **Check 8** (`test_supervisor_state_endpoint_returns_json`): `GET
//!   /api/builds/:id/supervisor/state` returns HTTP 200 with correct JSON
//!   when a supervisor entry exists.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{path::PathBuf, sync::Arc, time::Duration};

use lightarchitects_webshell::{
    agent::{ensure_agent_host, protocol::AgentEvent},
    config::{AgentSession, Cli, Config},
    container::DockerCapability,
    events::{
        WebEvent, WebEventV2,
        supervisor_handler::{SupervisorEntry, spawn_supervisor_watcher},
    },
    server::{AppState, build_app},
    session::{BuildRegistry, BuildSession},
    session_store::SessionStore,
    supervisor::{
        EvaluationStatus, SupervisorConfig, SupervisorState,
        evaluation::{NorthstarEvaluation, WaveContext, evaluate_wave},
    },
};
use serde_json::Value;
use tokio::net::TcpListener;

// ── Helpers ───────────────────────────────────────────────────────────────────

const TOKEN: &str = "supervisor-integration-test-token";

async fn spawn_server() -> (String, String, Arc<BuildRegistry>, AppState) {
    let cli = Cli {
        port: 0,
        host_cmd: std::ffi::OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    let state = AppState::for_test(cfg, DockerCapability::Unavailable);
    let builds = Arc::clone(&state.builds);
    let state_clone = state.clone();
    let app = build_app(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (
        format!("http://{addr}"),
        TOKEN.to_owned(),
        builds,
        state_clone,
    )
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

// ── §Q Check 1 — northstar_text persisted to `SQLite` ──────────────────────────

/// §Q check 1: `northstar_text` is stored durably in `SQLite`.
///
/// Uses `SessionStore::noop()` (in-memory `SQLite` with real schema) to verify
/// the full round-trip: `insert → set_northstar_text → list`.
#[test]
fn test_northstar_text_stored_in_sqlite() {
    let store = SessionStore::noop();

    store
        .insert(
            "build-northstar-1",
            "/tmp",
            "claude_code",
            None,
            None,
            false,
        )
        .unwrap();
    assert!(
        store.list().unwrap()[0].northstar_text.is_none(),
        "northstar_text must be NULL on insert",
    );

    store
        .set_northstar_text(
            "build-northstar-1",
            "Ship E2E webshell without terminal fallback",
        )
        .unwrap();

    let rows = store.list().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].northstar_text.as_deref(),
        Some("Ship E2E webshell without terminal fallback"),
        "§Q check 1: northstar_text must round-trip via `SQLite`",
    );
}

// ── §Q Check 3 — neutral stub when no Ollama backend ─────────────────────────

/// §Q check 3: `evaluate_wave` with `ollama_base: None` returns
/// `status: Neutral, confidence: 0.5` without any network calls.
#[tokio::test]
async fn test_neutral_stub_when_no_backend() {
    let client = reqwest::Client::new();
    let ctx = WaveContext {
        northstar_text: "Ship E2E webshell without terminal fallback",
        wave_num: 2,
        wave_summary: "Implemented supervisor.rs, evaluation.rs, and routing.rs.",
    };

    let eval = evaluate_wave(&ctx, &client, None, "llama3")
        .await
        .expect("neutral stub must not error");

    assert_eq!(
        eval.status,
        EvaluationStatus::Neutral,
        "§Q check 3: no-backend evaluation must be Neutral",
    );
    assert!(
        (eval.confidence - 0.5).abs() < 1e-6,
        "§Q check 3: no-backend confidence must be 0.5, got {}",
        eval.confidence,
    );
    assert_eq!(eval.wave_num, 2, "wave_num must be echoed back");
}

// ── §Q Check 4 — WAVE_COMPLETE → WebEvent::SupervisorUpdate ──────────────────

/// §Q check 4: a synthetic `AgentEvent::WaveComplete` published on the agent
/// event bus causes `spawn_supervisor_watcher` to emit a
/// `WebEvent::SupervisorUpdate` on `BuildSession.event_tx` within 500 ms.
///
/// Uses `ollama_base: None` so `evaluate_wave` returns a neutral stub without
/// hitting any real network endpoints — evaluation still fires and the result
/// is still broadcast.
#[tokio::test]
async fn test_wave_complete_broadcasts_supervisor_update() {
    let session = Arc::new(BuildSession::new(
        PathBuf::from("/tmp"),
        AgentSession::default(),
    ));

    let entry = SupervisorEntry::new(
        Some("Ship E2E webshell without terminal fallback".to_owned()),
        SupervisorConfig::default(),
    );

    // Subscribe to web events BEFORE spawning the watcher so we don't miss the update.
    let mut web_rx = session.event_tx.subscribe();

    spawn_supervisor_watcher(
        Arc::clone(&session),
        Arc::clone(&entry),
        reqwest::Client::new(),
        None, // no Ollama backend — uses neutral stub
        "llama3".to_owned(),
    );

    // Give the watcher a moment to subscribe to the agent event bus.
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Inject a synthetic WaveComplete event.
    let (agent_event_tx, _ctrl) = ensure_agent_host(&session).await;
    agent_event_tx
        .send(AgentEvent::WaveComplete {
            wave_num: 1,
            summary: "Implemented supervisor routes and ProposalCard component.".to_owned(),
        })
        .expect("agent event channel must be open");

    // Wait up to 500 ms for the SupervisorUpdate to appear.
    let timeout = Duration::from_millis(500);
    let received = tokio::time::timeout(timeout, async {
        loop {
            match web_rx.recv().await {
                Ok(WebEventV2 {
                    inner: WebEvent::SupervisorUpdate(ev),
                    ..
                }) => return Some(ev),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    })
    .await
    .expect("§Q check 4: SupervisorUpdate must arrive within 500 ms")
    .expect("channel must not close before event arrives");

    assert_eq!(
        received.build_id,
        session.build_id.to_string(),
        "build_id must match session",
    );
    assert_eq!(received.wave_num, 1, "wave_num must match WaveComplete");
    assert_eq!(
        received.status, "neutral",
        "status must be neutral (no Ollama backend)",
    );
}

// ── §Q Check 6 — configurable-N drift detection ───────────────────────────────

/// §Q check 6: `SupervisorState.record_evaluation` triggers a proposal at
/// exactly N consecutive drifting waves, where N is `drift_threshold_waves`.
///
/// Verifies both that a low threshold (N=2) fires correctly and that a
/// non-default threshold (N=5) does not fire early.
#[test]
fn test_drift_detection_configurable_threshold() {
    fn drifting() -> NorthstarEvaluation {
        NorthstarEvaluation {
            status: EvaluationStatus::Drifting,
            confidence: 0.8,
            recommended_next: "Refocus.".to_owned(),
            wave_num: 0,
        }
    }

    // Threshold N=2: proposal fires on 2nd consecutive drift.
    let mut state = SupervisorState::new(SupervisorConfig {
        drift_threshold_waves: 2,
        ..Default::default()
    });
    assert!(
        !state.record_evaluation(&drifting()),
        "drift 1 of 2 must not trigger"
    );
    let fired = state.record_evaluation(&drifting());
    assert!(
        fired,
        "§Q check 6: drift 2 of 2 (N=2) must trigger proposal"
    );
    assert!(
        state.proposal_pending,
        "proposal_pending must be true after trigger"
    );

    // Threshold N=5: proposal must NOT fire before 5 consecutive drifts.
    let mut state5 = SupervisorState::new(SupervisorConfig {
        drift_threshold_waves: 5,
        ..Default::default()
    });
    for i in 0..4u32 {
        let fired = state5.record_evaluation(&drifting());
        assert!(
            !fired,
            "§Q check 6: drift {}/{} must not trigger at N=5",
            i + 1,
            5
        );
    }
    let fired = state5.record_evaluation(&drifting());
    assert!(
        fired,
        "§Q check 6: 5th consecutive drift must trigger proposal at N=5"
    );
}

// ── §Q Check 8 — supervisor state API ────────────────────────────────────────

/// §Q check 8: `GET /api/builds/:id/supervisor/state` returns HTTP 200 with
/// a JSON body containing `northstar_text`, `consecutive_drifts`,
/// `drift_threshold`, `proposal_pending`, and `last_evaluation`.
#[tokio::test]
async fn test_supervisor_state_endpoint_returns_json() {
    let (base, token, builds, state) = spawn_server().await;

    // Register a BuildSession with a supervisor entry.
    let session = Arc::new(BuildSession::new(
        PathBuf::from("/tmp"),
        AgentSession::default(),
    ));
    let build_id = session.build_id;
    builds.insert(Arc::clone(&session));

    let entry = SupervisorEntry::new(
        Some("Ship E2E webshell P1".to_owned()),
        SupervisorConfig {
            drift_threshold_waves: 4,
            ..Default::default()
        },
    );
    state.supervisor_states.insert(build_id, entry);

    let resp = http()
        .get(format!("{base}/api/builds/{build_id}/supervisor/state"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("request must succeed");

    assert_eq!(
        resp.status(),
        200,
        "§Q check 8: supervisor state endpoint must return 200",
    );

    let body: Value = resp.json().await.expect("body must be JSON");
    assert_eq!(
        body["northstar_text"], "Ship E2E webshell P1",
        "northstar_text must match",
    );
    assert_eq!(body["consecutive_drifts"], 0, "no drifts yet");
    assert_eq!(
        body["drift_threshold"], 4,
        "drift_threshold must reflect SupervisorConfig",
    );
    assert_eq!(body["proposal_pending"], false, "no proposal pending yet");
    assert!(
        body["last_evaluation"].is_null(),
        "last_evaluation must be null when no waves evaluated",
    );
}

/// §Q check 8 (error path): `GET /api/builds/:id/supervisor/state` returns 404
/// when no supervisor entry exists for the build (northstar not set at creation).
#[tokio::test]
async fn test_supervisor_state_endpoint_returns_404_without_northstar() {
    let (base, token, builds, _state) = spawn_server().await;

    let session = Arc::new(BuildSession::new(
        PathBuf::from("/tmp"),
        AgentSession::default(),
    ));
    let build_id = session.build_id;
    builds.insert(Arc::clone(&session));
    // No supervisor_states entry — simulates a build created without northstar_text.

    let resp = http()
        .get(format!("{base}/api/builds/{build_id}/supervisor/state"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("request must succeed");

    assert_eq!(
        resp.status(),
        404,
        "§Q check 8: supervisor state must return 404 when no northstar was set",
    );
}
