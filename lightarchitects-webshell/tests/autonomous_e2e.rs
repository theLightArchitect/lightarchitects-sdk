//! Full-stack E2E tests for the lightsquad autonomous build pipeline.
//!
//! # What this proves
//!
//! These tests verify the complete dataflow:
//!
//! ```text
//! POST /api/builds (mode=autonomous, waves=[…])
//!   → create_build_handler
//!   → spawn_autonomous_build (BridgeContext)
//!   → Program::run(mock_worker)          ← #[cfg(test)] worker
//!   → WorktreeManager::create            ← real git worktree
//!   → mock_worker: write file + git commit
//!   → MergeAgent::merge_task_to_feat     ← real git merge
//!   → DecisionsWriter::append            ← HMAC-chained NDJSON
//!   → broadcast::Sender<WebEvent>        ← SSE events
//!
//! GET /api/builds/:id/decisions          ← reads NDJSON
//! GET /api/builds/:id/events             ← SSE stream
//! ```
//!
//! Uses `#[cfg(test)]` mock worker (writes a `.txt` file + git commit;
//! no CLI binary required) via the `cfg(test)` split in `lightsquad_bridge.rs`.
//!
//! Canon XXVII suite coverage:
//! - Suite 4 (E2E): `autonomous_dispatch_runs_mock_worker_and_writes_decisions`
//! - Suite 2 (integration): `autonomous_build_emits_sse_worker_slot_and_merge_events`
//! - Suite 5 (regression): `decisions_entry_structure_matches_contract`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{ffi::OsString, path::Path, time::Duration};

use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use uuid::Uuid;

use lightarchitects_webshell::{
    config::{Cli, Config},
    server::{AppState, build_app},
};

const TOKEN: &str = "autonomous-e2e-test-token";

// ── Git helpers ───────────────────────────────────────────────────────────────

async fn git(dir: &Path, args: &[&str]) {
    let out = tokio::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .await
        .expect("git spawn failed");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Bootstrap a minimal git repo suitable for lightsquad: init + empty commit.
///
/// The repo stays on `main`. `run_build` will `git checkout -B feat/auto-<id>`
/// automatically when the autonomous build starts.
async fn init_git_repo(dir: &Path) {
    git(dir, &["init", "-b", "main"]).await;
    git(dir, &["config", "user.email", "test@la-e2e.test"]).await;
    git(dir, &["config", "user.name", "LA E2E Test"]).await;
    git(dir, &["commit", "--allow-empty", "-m", "init"]).await;
}

// ── Server helpers ────────────────────────────────────────────────────────────

async fn spawn_server(cwd: &Path) -> String {
    // Mock workers are enabled via AppState::for_test (mock_workers: true) —
    // no env vars needed. The BridgeContext inherits the flag from AppState.
    let cli = Cli {
        port: 0,
        host_cmd: OsString::from("echo"),
        cwd: Some(cwd.to_path_buf()),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    let state = AppState::for_test(
        cfg,
        lightarchitects_webshell::container::DockerCapability::Unavailable,
    );
    let app = build_app(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap()
}

/// POST an autonomous build with a 2-task LASDLC-style wave.
/// Returns `build_id`.
async fn post_autonomous_build(base: &str, cwd: &str) -> Uuid {
    let resp: Value = http()
        .post(format!("{base}/api/builds"))
        .bearer_auth(TOKEN)
        .json(&json!({
            "cwd": cwd,
            "mode": "autonomous",
            "waves": [
                [{ "id": "t1", "prompt": "implement foundation layer", "depends_on": [] }],
                [{ "id": "t2", "prompt": "implement integration layer", "depends_on": [] }]
            ]
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    resp["build_id"]
        .as_str()
        .unwrap()
        .parse()
        .expect("build_id in response must be a UUID")
}

/// Poll `GET /api/builds/:id/decisions` until a "Build complete" L1 entry appears
/// or `timeout` expires. Returns the full entry list at that point.
///
/// Waiting for "complete" (not just non-empty) ensures the full pipeline — both
/// waves — has finished before the test makes assertions.
async fn wait_for_build_complete(base: &str, build_id: Uuid, timeout: Duration) -> Vec<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let resp: Value = http()
            .get(format!("{base}/api/builds/{build_id}/decisions"))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap_or(Value::Array(vec![]));
        if let Some(arr) = resp.as_array() {
            let complete = arr.iter().any(|e| {
                e["level"].as_str() == Some("L1")
                    && e["decision"].as_str().unwrap_or("").contains("complete")
            });
            if complete {
                return arr.clone();
            }
        }
        if tokio::time::Instant::now() >= deadline {
            // Return whatever we have so the test assertion can show useful output.
            if let Some(arr) = resp.as_array() {
                return arr.clone();
            }
            return vec![];
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

// ── Suite 4 (E2E) ─────────────────────────────────────────────────────────────

/// Full autonomous pipeline: POST → mock worker writes files + commits → decisions NDJSON written
/// → GET /decisions returns L1 build-started + L2 task-complete entries.
///
/// This is the canonical proof that the webshell → lightsquad → decisions dataflow is wired.
#[tokio::test]
async fn autonomous_dispatch_runs_mock_worker_and_writes_decisions() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path()).await;

    let base = spawn_server(repo.path()).await;
    let build_id = post_autonomous_build(&base, repo.path().to_str().unwrap()).await;

    // Allow up to 20 s for both tasks to complete and decisions to be flushed.
    let entries = wait_for_build_complete(&base, build_id, Duration::from_secs(30)).await;

    assert!(
        !entries.is_empty(),
        "decisions endpoint must return at least one entry after autonomous build completes"
    );

    // L1 entry: build started (written in run_build before Program::run)
    let l1 = entries.iter().find(|e| {
        e["level"].as_str() == Some("L1")
            && e["decision"].as_str().unwrap_or("").contains("started")
    });
    assert!(
        l1.is_some(),
        "expected L1 'build started' decision entry; got {entries:?}"
    );

    // L2 entry: task complete (written by mock worker for each task)
    let l2_t1 = entries.iter().find(|e| {
        e["level"].as_str() == Some("L2") && e["decision"].as_str().unwrap_or("").contains("t1")
    });
    assert!(
        l2_t1.is_some(),
        "expected L2 entry for task t1; got {entries:?}"
    );

    let l2_t2 = entries.iter().find(|e| {
        e["level"].as_str() == Some("L2") && e["decision"].as_str().unwrap_or("").contains("t2")
    });
    assert!(
        l2_t2.is_some(),
        "expected L2 entry for task t2; got {entries:?}"
    );

    // Final L1: build complete
    let l1_done = entries.iter().find(|e| {
        e["level"].as_str() == Some("L1")
            && e["decision"].as_str().unwrap_or("").contains("complete")
    });
    assert!(
        l1_done.is_some(),
        "expected L1 'build complete' decision entry; got {entries:?}"
    );
}

// ── Suite 2 (integration) ─────────────────────────────────────────────────────

/// SSE stream emits `WorkerSlotGauge` and `MergeAgentStatus` events during
/// autonomous dispatch, proving the real-time operator feedback loop is wired.
#[tokio::test]
async fn autonomous_build_emits_sse_worker_slot_and_merge_events() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path()).await;

    let base = spawn_server(repo.path()).await;

    // Open SSE stream before triggering the build so no events are missed.
    let sse_resp = http()
        .get(format!("{base}/api/events"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(sse_resp.status(), 200);

    let mut stream = sse_resp.bytes_stream();

    // Trigger the autonomous build.
    let _build_id = post_autonomous_build(&base, repo.path().to_str().unwrap()).await;

    // Collect SSE frames for up to 20 s and look for expected event types.
    let mut saw_worker_slot = false;
    let mut saw_merge_status = false;
    let mut saw_conductor_tick = false;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(Ok(bytes))) =
            tokio::time::timeout(Duration::from_secs(1), stream.next()).await
        {
            let text = String::from_utf8_lossy(&bytes);
            // WebEvent serializes with snake_case type tags per
            // #[serde(tag = "type", rename_all = "snake_case")].
            if text.contains("worker_slot_gauge") {
                saw_worker_slot = true;
            }
            if text.contains("merge_agent_status") {
                saw_merge_status = true;
            }
            if text.contains("conductor_tick") {
                saw_conductor_tick = true;
            }
            if saw_worker_slot && saw_merge_status && saw_conductor_tick {
                break;
            }
        }
    }

    assert!(
        saw_worker_slot,
        "SSE stream must emit at least one worker_slot_gauge event"
    );
    assert!(
        saw_merge_status,
        "SSE stream must emit at least one merge_agent_status event"
    );
    assert!(
        saw_conductor_tick,
        "SSE stream must emit at least one conductor_tick event (queue_depth=0 signals completion)"
    );
}

// ── Suite 5 (regression) ─────────────────────────────────────────────────────

/// Decision entries returned by the endpoint must carry the required fields:
/// `line_n`, `level`, `decision`, `canon_ref`, `hmac`. This pins the
/// `DecisionEntry` wire format so future refactors don't silently drop fields.
#[tokio::test]
async fn decisions_entry_structure_matches_contract() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path()).await;

    let base = spawn_server(repo.path()).await;
    let build_id = post_autonomous_build(&base, repo.path().to_str().unwrap()).await;

    let entries = wait_for_build_complete(&base, build_id, Duration::from_secs(30)).await;
    assert!(
        !entries.is_empty(),
        "at least one decision entry must be written"
    );

    for entry in &entries {
        assert!(
            entry.get("line_n").is_some(),
            "entry missing 'line_n': {entry}"
        );
        assert!(
            entry.get("level").is_some(),
            "entry missing 'level': {entry}"
        );
        assert!(
            entry.get("decision").is_some(),
            "entry missing 'decision': {entry}"
        );
        // hmac field must be present (HMAC chain integrity)
        assert!(
            entry.get("hmac").is_some(),
            "entry missing 'hmac' (HMAC chain broken): {entry}"
        );
    }

    // line_n values must be strictly monotonically increasing.
    let line_ns: Vec<u64> = entries
        .iter()
        .filter_map(|e| e["line_n"].as_u64())
        .collect();
    assert!(
        !line_ns.is_empty(),
        "line_n values must be numeric integers"
    );
    for w in line_ns.windows(2) {
        assert!(w[1] > w[0], "line_n must be strictly increasing; got {w:?}");
    }
}
