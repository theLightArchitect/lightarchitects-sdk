//! Integration tests for `GET /api/helix/nodes` — Phase helix-viz-remap.
//!
//! Verifies auth gate, empty response, entry round-trip, and limit pagination.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{ffi::OsString, path::PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use lightarchitects_webshell::{
    config::{Cli, Config},
    container::DockerCapability,
    events::{
        GlobalEventStore,
        types::{AyinStatus, EventSource, HelixEntrySummary, HelixEventKind, WebEvent},
    },
    server::{AppState, build_app},
};
use tower::ServiceExt;

const TOKEN: &str = "test-token-helix-nodes";

fn make_app_with_store(store: GlobalEventStore) -> axum::Router {
    let cli = Cli {
        port: 8733,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    let mut state = AppState::for_test(cfg, DockerCapability::Unavailable);
    state.global_event_store = store;
    build_app(state)
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn helix_entry(path: &str) -> WebEvent {
    WebEvent::HelixEntry(HelixEntrySummary::minimal(
        path.to_owned(),
        HelixEventKind::Created,
    ))
}

fn test_source() -> EventSource {
    EventSource::GateRunner {
        gate_id: "test-gate".to_owned(),
    }
}

// --- 401 without auth -------------------------------------------------------

#[tokio::test]
async fn helix_nodes_rejects_unauthenticated() {
    let store = GlobalEventStore::noop();
    let resp = make_app_with_store(store)
        .oneshot(
            Request::get("/api/helix/nodes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// --- 200 with empty store ---------------------------------------------------

#[tokio::test]
async fn helix_nodes_returns_empty_on_cold_store() {
    let store = GlobalEventStore::noop();
    let resp = make_app_with_store(store)
        .oneshot(
            Request::get("/api/helix/nodes")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["nodes"].as_array().unwrap().len(), 0);
    assert_eq!(json["total"], 0);
}

// --- happy path: pushed entry appears in response --------------------------

#[tokio::test]
async fn helix_nodes_returns_pushed_helix_entry() {
    let store = GlobalEventStore::noop();
    store.push(test_source(), helix_entry("eva/entries/day-42.md"));

    let resp = make_app_with_store(store)
        .oneshot(
            Request::get("/api/helix/nodes")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    let nodes = json["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["path"], "eva/entries/day-42.md");
    assert_eq!(json["total"], 1);
}

// --- limit pagination: total reflects full count, nodes is capped ----------

#[tokio::test]
async fn helix_nodes_paginates_by_limit() {
    let store = GlobalEventStore::noop();
    store.push(test_source(), helix_entry("eva/a.md"));
    store.push(test_source(), helix_entry("eva/b.md"));
    store.push(test_source(), helix_entry("eva/c.md"));

    let resp = make_app_with_store(store)
        .oneshot(
            Request::get("/api/helix/nodes?limit=2")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(json["total"], 3);
}

// --- non-helix events are excluded -----------------------------------------

#[tokio::test]
async fn helix_nodes_excludes_non_helix_events() {
    let store = GlobalEventStore::noop();
    store.push(test_source(), WebEvent::AyinStatus(AyinStatus::Connected));
    store.push(test_source(), helix_entry("soul/standards.md"));

    let resp = make_app_with_store(store)
        .oneshot(
            Request::get("/api/helix/nodes")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(json["nodes"][0]["path"], "soul/standards.md");
    assert_eq!(json["total"], 1);
}
