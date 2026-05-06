//! Chaos test suite — Canon XXVII §50.
//!
//! Exercises failure and edge cases in the broadcast channel:
//! - Zero-subscriber sends (should not panic)
//! - Multiple subscribers all receive events
//! - A lagged receiver gets `RecvError::Lagged` (not a panic or hang)
//! - `AppState` can be dropped cleanly
//! - Health endpoint stays live regardless of broadcast channel state

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{ffi::OsString, path::PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use lightarchitects_webshell::{
    config::{Cli, Config},
    events::WebEvent,
    events::types::AyinStatus,
    server::{AppState, build_app},
};
use tokio::sync::broadcast;
use tower::ServiceExt;

const TOKEN: &str = "test-token-chaos-suite";

fn make_app() -> axum::Router {
    let cli = Cli {
        port: 8733,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    build_app(AppState::for_test(
        cfg,
        lightarchitects_webshell::container::DockerCapability::Unavailable,
    ))
}

// --- Zero-subscriber send does not panic ------------------------------------

#[test]
fn send_to_channel_with_no_receivers_does_not_panic() {
    let (tx, initial_rx) = broadcast::channel::<WebEvent>(16);
    // Drop the initial receiver — channel now has zero subscribers.
    drop(initial_rx);
    // Sending to a channel with no subscribers returns Err but must not panic.
    let _ = tx.send(WebEvent::AyinStatus(AyinStatus::Connected));
    let _ = tx.send(WebEvent::AyinStatus(AyinStatus::Disconnected));
}

// --- Multiple subscribers all receive the same event -----------------------

#[tokio::test]
async fn two_subscribers_both_receive_sent_event() {
    let (tx, _) = broadcast::channel::<WebEvent>(16);
    let mut rx1 = tx.subscribe();
    let mut rx2 = tx.subscribe();

    let event = WebEvent::AyinStatus(AyinStatus::Connected);
    tx.send(event.clone()).unwrap();

    let got1 = rx1.recv().await.unwrap();
    let got2 = rx2.recv().await.unwrap();

    assert_eq!(
        serde_json::to_string(&got1).unwrap(),
        serde_json::to_string(&event).unwrap(),
    );
    assert_eq!(
        serde_json::to_string(&got2).unwrap(),
        serde_json::to_string(&event).unwrap(),
    );
}

// --- Lagged receiver gets RecvError::Lagged, not a panic -------------------

#[tokio::test]
async fn lagged_subscriber_gets_lagged_error_not_panic() {
    // Buffer of 4 — send 6 events before the subscriber reads any.
    let (tx, _) = broadcast::channel::<WebEvent>(4);
    let mut rx = tx.subscribe();

    for i in 0u32..6 {
        let _ = tx.send(WebEvent::AyinStatus(AyinStatus::Reconnecting {
            attempt: i,
        }));
    }

    // The subscriber must get RecvError::Lagged (skipped N events), not panic.
    let result = rx.recv().await;
    assert!(
        matches!(
            result,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
        ),
        "expected Lagged, got: {result:?}",
    );
}

// --- AppState drop is clean (no panic / resource leak) ---------------------

#[test]
fn appstate_can_be_created_and_dropped() {
    let cli = Cli {
        port: 8733,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    // for_test does not spawn background tasks so no Tokio runtime is required.
    let state = AppState::for_test(
        cfg,
        lightarchitects_webshell::container::DockerCapability::Unavailable,
    );
    // Simply dropping state must not panic.
    drop(state);
}

// --- Health endpoint stays live even when broadcast channel is empty --------

#[tokio::test]
async fn health_live_with_empty_broadcast_channel() {
    let resp = make_app()
        .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// --- Channel closed event propagates without hang ---------------------------

#[tokio::test]
async fn channel_closed_produces_recv_error_closed() {
    let (tx, _) = broadcast::channel::<WebEvent>(4);
    let mut rx = tx.subscribe();

    // Drop the sender — channel is now closed.
    drop(tx);

    let result = rx.recv().await;
    assert!(
        matches!(
            result,
            Err(tokio::sync::broadcast::error::RecvError::Closed)
        ),
        "expected Closed, got: {result:?}",
    );
}
