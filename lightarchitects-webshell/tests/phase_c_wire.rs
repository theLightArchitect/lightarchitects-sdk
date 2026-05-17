//! Phase C wire test — end-to-end round trip for the gateway→UI notify channel.
//!
//! This test exercises the exact sequence `lightarchitects-gateway`'s `ui.*`
//! tools rely on:
//!
//! 1. `POST /api/builds {cwd}` returns `{build_id, ...}` (201/200).
//! 2. `GET  /api/builds/:id` returns the same public shape without leaking
//!    the notify token.
//! 3. `GET  /api/builds/:id/events` opens a per-build SSE subscriber.
//! 4. `POST /api/builds/:id/notify` with the correct
//!    `X-LA-Notify-Token` broadcasts to that subscriber.
//! 5. The subscriber receives a `gateway_notify` event whose `payload` is the
//!    verbatim JSON posted by the gateway.
//!
//! Failure here = the gateway wire test (Phase A) and the webshell notify
//! handler have diverged on the JSON shape. Green here = the chain
//! `gateway ui.rs → webshell notify → SSE → browser` is correct at the
//! Rust level.

#![allow(clippy::unwrap_used, clippy::expect_used, unsafe_code)]

use std::{ffi::OsString, path::PathBuf, sync::Arc, time::Duration};

use futures_util::StreamExt;
use lightarchitects_webshell::{
    config::{Cli, Config},
    server::{AppState, build_app},
    session::BuildRegistry,
};
use serde_json::json;
use tokio::net::TcpListener;
use uuid::Uuid;

const TOKEN: &str = "phase-c-wire-token";

/// Spin up a real in-process webshell on `127.0.0.1:0` and hand back the
/// base URL, bearer token, and an `Arc` to the [`BuildRegistry`] so the
/// test can look up notify tokens the way the gateway would (via env var
/// in production).
async fn spawn_server() -> (String, String, Arc<BuildRegistry>) {
    // Redirect to a temp dir so no real setup.json is found — tests the
    // CLI-default path without interference from the operator's saved config.
    let tmp = std::env::temp_dir().join(format!("la-wire-test-{}", std::process::id()));
    // SAFETY: integration tests run single-threaded; no concurrent env reads.
    unsafe { std::env::set_var("LIGHTARCHITECTS_HOME", &tmp) };

    let cli = Cli {
        port: 0, // unused — TcpListener binds on its own
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    // SAFETY: restoring env immediately after config is resolved.
    unsafe { std::env::remove_var("LIGHTARCHITECTS_HOME") };
    let _ = std::fs::remove_dir_all(&tmp);
    let state = AppState::for_test(
        cfg,
        lightarchitects_webshell::container::DockerCapability::Unavailable,
    );
    let builds = Arc::clone(&state.builds);
    let app = build_app(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        // `axum::serve` takes ownership of the listener + service and loops
        // until the process exits (acceptable for a per-test server).
        let _ = axum::serve(listener, app).await;
    });
    let base_url = format!("http://{addr}");
    (base_url, TOKEN.to_owned(), builds)
}

/// Build a reqwest client with a short timeout so hangs fail fast.
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

// ── Happy path ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn wire_create_then_details_then_notify_then_sse_roundtrip() {
    let (base, token, builds) = spawn_server().await;
    let http = client();

    // 1. POST /api/builds → build_id (no notify_token in response).
    let create: serde_json::Value = http
        .post(format!("{base}/api/builds"))
        .bearer_auth(&token)
        .json(&json!({ "cwd": "/tmp" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let build_id_str = create["build_id"].as_str().expect("build_id is a string");
    let build_id: Uuid = build_id_str.parse().expect("build_id parses as UUID");
    assert!(
        create.get("notify_token").is_none(),
        "notify_token must not appear in POST /api/builds response: {create}"
    );

    // 2. GET /api/builds/:id — also redacted.
    let details: serde_json::Value = http
        .get(format!("{base}/api/builds/{build_id}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(details["build_id"], json!(build_id_str));
    assert_eq!(details["cwd"], json!("/tmp"));
    assert_eq!(details["agent"]["kind"], "lightarchitects");
    assert_eq!(details["agent"]["backend"], "anthropic");
    assert!(details.get("notify_token").is_none());

    // 3. Look up the notify token via the shared registry handle (this is
    //    equivalent to the gateway reading `LA_NOTIFY_TOKEN` from its env).
    let session = builds.get(build_id).expect("session present in registry");
    let notify_hex = session.notify_token_hex();

    // 4. Open the SSE stream *before* the notify POST so our subscriber
    //    exists when the broadcaster fires.
    let mut sse = http
        .get(format!("{base}/api/builds/{build_id}/events"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .bytes_stream();

    // Give the SSE handler a tick to register its broadcast::Receiver
    // before we POST; without this the send could race ahead of subscribe.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 5. POST the notify. Gateway would do this from its `ui.focus_pillar`
    //    handler after the UI tool call.
    let notify_status = http
        .post(format!("{base}/api/builds/{build_id}/notify"))
        .header("x-la-notify-token", &notify_hex)
        .json(&json!({ "type": "focus_pillar", "pillar": "ARCH" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(notify_status.as_u16(), 200);

    // 6. Read SSE frames until we see the gateway_notify payload or time out.
    let received = tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(frame) = sse.next().await {
            let bytes = frame.unwrap();
            let text = std::str::from_utf8(&bytes).unwrap_or("").to_owned();
            if text.contains("gateway_notify") {
                return text;
            }
        }
        String::new()
    })
    .await
    .expect("SSE receive timed out");

    // SSE frame looks like `data: {"type":"gateway_notify","payload":{...}}\n\n`.
    // Parse the first `data: ...` line and assert the payload round-trips.
    let data_line = received
        .lines()
        .find(|l| l.starts_with("data: "))
        .expect("at least one data: line");
    let data_json: serde_json::Value =
        serde_json::from_str(data_line.trim_start_matches("data: ")).unwrap();
    assert_eq!(data_json["type"], "gateway_notify");
    assert_eq!(data_json["payload"]["type"], "focus_pillar");
    assert_eq!(data_json["payload"]["pillar"], "ARCH");
}

// ── Negative paths ───────────────────────────────────────────────────────────

#[tokio::test]
async fn notify_with_wrong_token_is_401() {
    let (base, token, _builds) = spawn_server().await;
    let http = client();

    let create: serde_json::Value = http
        .post(format!("{base}/api/builds"))
        .bearer_auth(&token)
        .json(&json!({ "cwd": "/tmp" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let build_id = create["build_id"].as_str().unwrap();

    // 64-char hex that happens to be all zeros — definitely not the real token.
    let fake = "0".repeat(64);
    let resp = http
        .post(format!("{base}/api/builds/{build_id}/notify"))
        .header("x-la-notify-token", &fake)
        .json(&json!({ "type": "focus_pillar", "pillar": "ARCH" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 401);
}

#[tokio::test]
async fn notify_without_token_header_is_401() {
    let (base, token, _builds) = spawn_server().await;
    let http = client();

    let create: serde_json::Value = http
        .post(format!("{base}/api/builds"))
        .bearer_auth(&token)
        .json(&json!({ "cwd": "/tmp" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let build_id = create["build_id"].as_str().unwrap();

    // No `x-la-notify-token` header at all.
    let resp = http
        .post(format!("{base}/api/builds/{build_id}/notify"))
        .json(&json!({ "type": "focus_pillar", "pillar": "ARCH" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 401);
}

#[tokio::test]
async fn notify_unknown_build_id_is_404() {
    let (base, _token, _builds) = spawn_server().await;
    let http = client();

    let nonexistent = Uuid::new_v4();
    // We don't even need the right token — the handler returns 404 for
    // unknown builds before inspecting the token (build IDs are v4 UUIDs,
    // so enumeration is computationally infeasible).
    let resp = http
        .post(format!("{base}/api/builds/{nonexistent}/notify"))
        .header("x-la-notify-token", "0".repeat(64))
        .json(&json!({ "type": "refresh_sitrep" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 404);
}

#[tokio::test]
async fn create_build_requires_bearer() {
    let (base, _token, _builds) = spawn_server().await;
    let http = client();

    let resp = http
        .post(format!("{base}/api/builds"))
        .json(&json!({ "cwd": "/tmp" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 401);
}

#[tokio::test]
async fn build_details_unknown_id_is_404() {
    let (base, token, _builds) = spawn_server().await;
    let http = client();

    let nonexistent = Uuid::new_v4();
    let resp = http
        .get(format!("{base}/api/builds/{nonexistent}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 404);
}
