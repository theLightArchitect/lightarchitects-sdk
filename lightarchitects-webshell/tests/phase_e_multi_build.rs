//! Phase E isolation tests — two concurrent builds must not cross-contaminate.
//!
//! What this file proves (and what breaks if it fails):
//!
//! 1. Notify events sent to build A only reach A's SSE subscribers —
//!    B's subscriber stays silent. A regression would show up as the wrong
//!    build's pillar lighting up in the browser when Claude issues a
//!    `ui_focus_pillar` from build A's PTY.
//! 2. Each build's notify token is a key that ONLY unlocks that build.
//!    A's token `POSTed` against B's `/notify` must 401 — otherwise a
//!    compromised gateway could fan events across every open build.
//! 3. Build-scoped GETs (details, events) are not affected by sibling
//!    builds being present.
//!
//! Uses a real ephemeral TCP listener per test to avoid test interleaving.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{ffi::OsString, path::PathBuf, sync::Arc, time::Duration};

use futures_util::StreamExt;
use lightarchitects_webshell::{
    config::{Cli, Config},
    server::{AppState, build_app},
    session::BuildRegistry,
};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use uuid::Uuid;

const TOKEN: &str = "phase-e-multi-build-token";

async fn spawn_server() -> (String, String, Arc<BuildRegistry>) {
    let cli = Cli {
        port: 0,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    let state = AppState::for_test(cfg);
    let builds = Arc::clone(&state.builds);
    let app = build_app(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), TOKEN.to_owned(), builds)
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

/// Create a build with the given cwd. Returns (`build_id`, `notify_hex`).
async fn create_build(
    base: &str,
    token: &str,
    cwd: &str,
    builds: &BuildRegistry,
) -> (Uuid, String) {
    let resp: Value = http()
        .post(format!("{base}/api/builds"))
        .bearer_auth(token)
        .json(&json!({ "cwd": cwd }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let build_id: Uuid = resp["build_id"].as_str().unwrap().parse().unwrap();
    let session = builds.get(build_id).unwrap();
    (build_id, session.notify_token_hex())
}

/// Open an SSE stream on a build's `/events` route with Bearer auth.
/// Returns a `Response` whose `.bytes_stream()` the caller polls.
async fn open_sse(base: &str, token: &str, build_id: Uuid) -> reqwest::Response {
    http()
        .get(format!("{base}/api/builds/{build_id}/events"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
}

// ── Event isolation ─────────────────────────────────────────────────────────

/// Poll an SSE stream until a frame containing `needle` arrives or `dur` elapses.
macro_rules! wait_sse {
    ($stream:expr, $needle:expr, $dur:expr) => {{
        tokio::time::timeout($dur, async {
            let mut stream = $stream;
            while let Some(frame) = stream.next().await {
                let bytes = frame.unwrap();
                let text = std::str::from_utf8(&bytes).unwrap_or("").to_owned();
                if text.contains($needle) {
                    return Some(text);
                }
            }
            None
        })
        .await
        .unwrap_or(None)
    }};
}

/// Collect an SSE stream for a fixed duration and return concatenated text.
macro_rules! collect_sse {
    ($stream:expr, $dur:expr) => {{
        let mut buf = String::new();
        let _ = tokio::time::timeout($dur, async {
            let mut stream = $stream;
            while let Some(frame) = stream.next().await {
                if let Ok(bytes) = frame {
                    buf.push_str(std::str::from_utf8(&bytes).unwrap_or(""));
                }
            }
        })
        .await;
        buf
    }};
}

#[tokio::test]
async fn notify_to_a_does_not_reach_b() {
    let (base, token, builds) = spawn_server().await;
    let (a_id, a_tok) = create_build(&base, &token, "/tmp/build-a", &builds).await;
    let (b_id, _b_tok) = create_build(&base, &token, "/tmp/build-b", &builds).await;

    // Subscribe to both SSE streams before posting.
    let sse_a = open_sse(&base, &token, a_id).await.bytes_stream();
    let sse_b = open_sse(&base, &token, b_id).await.bytes_stream();

    // Let the Receivers register on their respective broadcast channels.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Fire a single notify at A — B must remain silent.
    let status = http()
        .post(format!("{base}/api/builds/{a_id}/notify"))
        .header("x-la-notify-token", &a_tok)
        .json(&json!({ "type": "focus_pillar", "pillar": "ARCH" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status.as_u16(), 200);

    // A must see it.
    let a_frame = wait_sse!(sse_a, "gateway_notify", Duration::from_secs(2))
        .expect("A should receive the event");
    assert!(a_frame.contains("focus_pillar"));
    assert!(a_frame.contains("ARCH"));

    // B: listen for 300 ms and confirm no gateway_notify shows up.
    let b_buf = collect_sse!(sse_b, Duration::from_millis(300));
    assert!(
        !b_buf.contains("gateway_notify"),
        "B's SSE stream must NOT see A's notify: {b_buf}"
    );
}

#[tokio::test]
async fn notify_to_b_does_not_reach_a() {
    // Mirror of the above — same property verified from B's angle so the
    // isolation is bi-directional.
    let (base, token, builds) = spawn_server().await;
    let (a_id, _a_tok) = create_build(&base, &token, "/tmp/build-a", &builds).await;
    let (b_id, b_tok) = create_build(&base, &token, "/tmp/build-b", &builds).await;

    let sse_a = open_sse(&base, &token, a_id).await.bytes_stream();
    let sse_b = open_sse(&base, &token, b_id).await.bytes_stream();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let status = http()
        .post(format!("{base}/api/builds/{b_id}/notify"))
        .header("x-la-notify-token", &b_tok)
        .json(&json!({ "type": "refresh_sitrep" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status.as_u16(), 200);

    let b_frame = wait_sse!(sse_b, "refresh_sitrep", Duration::from_secs(2))
        .expect("B should receive the event");
    assert!(b_frame.contains("gateway_notify"));

    let a_buf = collect_sse!(sse_a, Duration::from_millis(300));
    assert!(
        !a_buf.contains("refresh_sitrep"),
        "A's SSE stream must NOT see B's notify: {a_buf}"
    );
}

// ── Token cross-use rejection ───────────────────────────────────────────────

#[tokio::test]
async fn as_token_rejected_when_posted_against_b() {
    let (base, token, builds) = spawn_server().await;
    let (_a_id, a_tok) = create_build(&base, &token, "/tmp/build-a", &builds).await;
    let (b_id, _b_tok) = create_build(&base, &token, "/tmp/build-b", &builds).await;

    let resp = http()
        .post(format!("{base}/api/builds/{b_id}/notify"))
        .header("x-la-notify-token", &a_tok) // wrong token for this build
        .json(&json!({ "type": "flag_finding" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status().as_u16(),
        401,
        "cross-build token must not unlock a sibling build"
    );
}

#[tokio::test]
async fn bs_token_rejected_when_posted_against_a() {
    let (base, token, builds) = spawn_server().await;
    let (a_id, _a_tok) = create_build(&base, &token, "/tmp/build-a", &builds).await;
    let (_b_id, b_tok) = create_build(&base, &token, "/tmp/build-b", &builds).await;

    let resp = http()
        .post(format!("{base}/api/builds/{a_id}/notify"))
        .header("x-la-notify-token", &b_tok) // wrong direction
        .json(&json!({ "type": "flag_finding" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 401);
}

#[tokio::test]
async fn global_bearer_token_is_rejected_on_notify() {
    // Deliberate contract — the global Bearer is valid for every OTHER route,
    // but MUST NOT be accepted as a notify token. A regression here would
    // widen the gateway's trust domain to include browser-held credentials.
    let (base, token, builds) = spawn_server().await;
    let (a_id, _a_tok) = create_build(&base, &token, "/tmp/build-a", &builds).await;

    let resp = http()
        .post(format!("{base}/api/builds/{a_id}/notify"))
        // No notify header — only Authorization Bearer.
        .bearer_auth(&token)
        .json(&json!({ "type": "notify", "message": "hi" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status().as_u16(),
        401,
        "Bearer token must not satisfy notify-endpoint auth"
    );
}

// ── Registry independence ───────────────────────────────────────────────────

#[tokio::test]
async fn independent_build_details_returned_for_each() {
    let (base, token, builds) = spawn_server().await;
    let (a_id, _) = create_build(&base, &token, "/tmp/build-a", &builds).await;
    let (b_id, _) = create_build(&base, &token, "/tmp/build-b", &builds).await;

    let a: Value = http()
        .get(format!("{base}/api/builds/{a_id}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let b: Value = http()
        .get(format!("{base}/api/builds/{b_id}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(a["cwd"], "/tmp/build-a");
    assert_eq!(b["cwd"], "/tmp/build-b");
    assert_ne!(a["build_id"], b["build_id"]);
    assert!(a.get("notify_token").is_none());
    assert!(b.get("notify_token").is_none());
}
