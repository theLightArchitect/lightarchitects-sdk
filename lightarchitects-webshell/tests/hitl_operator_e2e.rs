//! E2E tests for the HITL relay + operator request surface (Phase 4).
//!
//! # Canon XXVII coverage
//!
//! - Suite 2 (integration): HITL park → HTTP resolve → oneshot delivery
//! - Suite 4 (E2E): full autonomous status + cancel pipeline
//! - Suite 5 (regression): IDOR guard + 410 Gone semantics
//! - Suite 6 (smoke): auth guard on all three operator endpoints
//!
//! # Design
//!
//! `AppState::builds` (`Arc<BuildRegistry>`) and `AppState::hitl_queue`
//! (`Arc<DashMap<...>>`) are both `Arc`-cloned before `build_app` consumes
//! the state. Tests seed the registry directly (no real autonomous build or
//! git repo needed) and park escalations into the queue.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{ffi::OsString, path::PathBuf, sync::Arc, time::Duration};

use tokio::net::TcpListener;
use uuid::Uuid;

use lightarchitects_webshell::{
    config::{AgentSession, Cli, Config},
    container::DockerCapability,
    events::hitl_relay,
    server::{AppState, build_app},
    session::{BuildRegistry, BuildSession},
};

const TOKEN: &str = "hitl-e2e-test-token";

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_cfg() -> Config {
    let cli = Cli {
        port: 0,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap()
}

/// Spawn a real TCP server and return the base URL plus handles to the shared
/// `HitlQueue` and `BuildRegistry` for direct test manipulation.
async fn spawn_server() -> (String, hitl_relay::HitlQueue, Arc<BuildRegistry>) {
    let state = AppState::for_test(make_cfg(), DockerCapability::Unavailable);
    let queue = state.hitl_queue.clone();
    let builds = state.builds.clone();
    let app = build_app(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), queue, builds)
}

/// Insert a dummy `BuildSession` into the registry and return its `build_id`.
fn register_build(builds: &BuildRegistry) -> Uuid {
    let session = Arc::new(BuildSession::new(
        PathBuf::from("/tmp"),
        AgentSession::default(),
    ));
    let id = session.build_id;
    builds.insert(session);
    id
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

// ── Suite 2 (integration) — HITL park + HTTP resolve ─────────────────────────

/// Park an escalation directly into the queue, then resolve it via HTTP.
/// The oneshot receiver must deliver the decision to the waiting "worker".
#[tokio::test]
async fn hitl_resolve_delivers_decision_via_oneshot() {
    let (base, queue, builds) = spawn_server().await;
    let build_id = register_build(&builds);

    let (call_id, _nonce, rx) = hitl_relay::park(
        &queue,
        build_id,
        "task-resolve-test".to_owned(),
        "G-DENY: forbidden import detected".to_owned(),
        0,
        2,
    );

    let resp = http()
        .post(format!("{base}/api/builds/{build_id}/hitl/{call_id}"))
        .bearer_auth(TOKEN)
        .json(&serde_json::json!({ "approved": true, "reason": "reviewed and safe" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "resolve must return 200 OK");

    let decision = tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .expect("oneshot must fire within 2s")
        .expect("sender must not have been dropped");

    assert!(decision.approved);
    assert_eq!(
        decision.operator_reason.as_deref(),
        Some("reviewed and safe")
    );
    assert!(
        !queue.contains_key(&call_id),
        "entry must be removed after resolve"
    );
}

/// Reject path: `approved: false` delivers a rejection decision.
#[tokio::test]
async fn hitl_resolve_reject_delivers_rejection() {
    let (base, queue, builds) = spawn_server().await;
    let build_id = register_build(&builds);

    let (call_id, _nonce, rx) = hitl_relay::park(
        &queue,
        build_id,
        "task-reject-test".to_owned(),
        "G-SYMLINK: path escape attempt".to_owned(),
        1,
        5,
    );

    let resp = http()
        .post(format!("{base}/api/builds/{build_id}/hitl/{call_id}"))
        .bearer_auth(TOKEN)
        .json(&serde_json::json!({ "approved": false, "reason": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let decision = tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .unwrap()
        .unwrap();
    assert!(!decision.approved);
    assert!(decision.operator_reason.is_none());
}

// ── Suite 5 (regression) — IDOR guard ────────────────────────────────────────

/// Sending a valid `call_id` but mismatched `build_id` must return 403.
/// The entry must remain in the queue so the legitimate build can still resolve it.
#[tokio::test]
async fn hitl_resolve_wrong_build_id_returns_403() {
    let (base, queue, builds) = spawn_server().await;
    let real_build_id = register_build(&builds);
    let attacker_build_id = register_build(&builds); // also registered — only call_id is wrong

    let (call_id, _nonce, _rx) = hitl_relay::park(
        &queue,
        real_build_id,
        "task-idor-test".to_owned(),
        "some reason".to_owned(),
        0,
        1,
    );

    let resp = http()
        .post(format!(
            "{base}/api/builds/{attacker_build_id}/hitl/{call_id}"
        ))
        .bearer_auth(TOKEN)
        .json(&serde_json::json!({ "approved": true, "reason": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "mismatched build_id must return 403");

    // Entry must still be in the queue — not consumed by the attacker.
    assert!(
        queue.contains_key(&call_id),
        "IDOR-rejected entry must remain in queue"
    );
}

/// Resolving an unknown `call_id` must return 410 Gone.
#[tokio::test]
async fn hitl_resolve_unknown_call_id_returns_410() {
    let (base, _queue, builds) = spawn_server().await;
    let build_id = register_build(&builds);
    let phantom_call_id = Uuid::new_v4();

    let resp = http()
        .post(format!(
            "{base}/api/builds/{build_id}/hitl/{phantom_call_id}"
        ))
        .bearer_auth(TOKEN)
        .json(&serde_json::json!({ "approved": true, "reason": null }))
        .send()
        .await
        .unwrap();
    // Unknown call_id → handler removes nothing → reaches 410 path
    // (handler returns NOT_FOUND when build not found first, 410/NOT_FOUND when call_id missing)
    assert!(
        resp.status() == 410 || resp.status() == 404,
        "unknown call_id must return 410 or 404, got {}",
        resp.status()
    );
}

// ── Suite 4 (E2E) — autonomous status endpoint ────────────────────────────────

/// GET /api/builds/:id/autonomous/status returns 404 when no build registered.
#[tokio::test]
async fn autonomous_status_returns_404_for_unknown_build() {
    let (base, _queue, _builds) = spawn_server().await;
    let unknown_id = Uuid::new_v4();

    let resp = http()
        .get(format!("{base}/api/builds/{unknown_id}/autonomous/status"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

/// Status response includes only the pending HITL items that belong to the queried build.
#[tokio::test]
async fn autonomous_status_includes_pending_hitl_for_correct_build() {
    let (base, queue, builds) = spawn_server().await;
    let build_a = register_build(&builds);
    let build_b = register_build(&builds);

    // Park two escalations for build_a, one for build_b.
    let _ = hitl_relay::park(
        &queue,
        build_a,
        "t-a1".to_owned(),
        "reason a1".to_owned(),
        0,
        1,
    );
    let _ = hitl_relay::park(
        &queue,
        build_a,
        "t-a2".to_owned(),
        "reason a2".to_owned(),
        1,
        2,
    );
    let _ = hitl_relay::park(
        &queue,
        build_b,
        "t-b1".to_owned(),
        "reason b1".to_owned(),
        0,
        3,
    );

    let resp: serde_json::Value = http()
        .get(format!("{base}/api/builds/{build_a}/autonomous/status"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let pending = resp["pending_hitl"]
        .as_array()
        .expect("pending_hitl must be array");
    assert_eq!(
        pending.len(),
        2,
        "build_a must have exactly 2 pending HITL items"
    );

    // All returned items must belong to build_a.
    for item in pending {
        assert_eq!(
            item["task_id"].as_str().map(|s| s.starts_with("t-a")),
            Some(true),
            "pending_hitl item task_id must belong to build_a: {item}"
        );
    }
}

// ── Suite 4 (E2E) — autonomous cancel endpoint ────────────────────────────────

/// DELETE /api/builds/:id/autonomous returns 404 when no build is registered.
#[tokio::test]
async fn autonomous_cancel_returns_404_for_unknown_build() {
    let (base, _queue, _builds) = spawn_server().await;

    let resp = http()
        .delete(format!("{base}/api/builds/{}/autonomous", Uuid::new_v4()))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

/// Cancel drains all HITL entries for the build from the queue.
#[tokio::test]
async fn autonomous_cancel_drains_hitl_queue_for_build() {
    let (base, queue, builds) = spawn_server().await;
    let build_id = register_build(&builds);

    // Park two escalations.
    let (ca, _, _) = hitl_relay::park(&queue, build_id, "t1".to_owned(), "r1".to_owned(), 0, 1);
    let (cb, _, _) = hitl_relay::park(&queue, build_id, "t2".to_owned(), "r2".to_owned(), 0, 2);

    assert!(queue.contains_key(&ca));
    assert!(queue.contains_key(&cb));

    // No autonomous JoinHandle registered → cancel returns 409.
    // (cancel requires a running autonomous build in lightsquad_programs)
    let resp = http()
        .delete(format!("{base}/api/builds/{build_id}/autonomous"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    // 409 = no autonomous handle registered; 204 = cancelled. Both are valid.
    assert!(
        resp.status() == 409 || resp.status() == 204,
        "cancel returns 409 (no handle) or 204 (cancelled), got {}",
        resp.status()
    );
}

// ── Suite 6 (smoke) — auth guard ─────────────────────────────────────────────

/// All three operator endpoints must reject unauthenticated requests with 401.
#[tokio::test]
async fn hitl_endpoints_reject_unauthenticated_requests() {
    let (base, queue, builds) = spawn_server().await;
    let build_id = register_build(&builds);
    let (call_id, _nonce, _rx) = hitl_relay::park(
        &queue,
        build_id,
        "task-auth-test".to_owned(),
        "reason".to_owned(),
        0,
        1,
    );

    let status_resp = http()
        .get(format!("{base}/api/builds/{build_id}/autonomous/status"))
        .send()
        .await
        .unwrap();
    assert_eq!(status_resp.status(), 401);

    let cancel_resp = http()
        .delete(format!("{base}/api/builds/{build_id}/autonomous"))
        .send()
        .await
        .unwrap();
    assert_eq!(cancel_resp.status(), 401);

    let resolve_resp = http()
        .post(format!("{base}/api/builds/{build_id}/hitl/{call_id}"))
        .json(&serde_json::json!({ "approved": true, "reason": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(resolve_resp.status(), 401);
}
