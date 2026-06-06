//! Session persistence end-to-end tests.
//!
//! Covers `SQLite` roundtrip, concurrent writes, touch semantics, and the noop store.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use lightarchitects_webshell::session_store::SessionStore;

#[test]
fn insert_and_read_roundtrip() {
    let store = SessionStore::noop();

    store
        .insert(
            "build-001",
            "/tmp/test",
            "lightarchitects",
            Some("anthropic"),
            Some("claude-sonnet-4-6"),
            false,
        )
        .expect("insert should succeed");

    let count = store.count().expect("count should succeed");
    assert_eq!(count, 1, "count should be 1 after single insert");

    let list = store.list().expect("list should succeed");
    assert_eq!(list.len(), 1, "list should return exactly one row");

    let row = &list[0];
    assert_eq!(row.build_id, "build-001");
    assert_eq!(row.cwd, "/tmp/test");
    assert_eq!(row.agent_kind, "lightarchitects");
    assert_eq!(row.backend.as_deref(), Some("anthropic"));
    assert_eq!(row.model.as_deref(), Some("claude-sonnet-4-6"));
    assert!(!row.containerized, "containerized should be false");
}

#[test]
fn concurrent_writes_no_corruption() {
    let store = Arc::new(Mutex::new(SessionStore::noop()));
    let mut handles = Vec::new();

    for i in 0..10 {
        let s = Arc::clone(&store);
        handles.push(thread::spawn(move || {
            s.lock()
                .unwrap()
                .insert(
                    &format!("build-{i}"),
                    "/tmp/concurrent",
                    "lightarchitects",
                    None,
                    None,
                    false,
                )
                .expect("concurrent insert should succeed");
        }));
    }

    for h in handles {
        h.join().expect("thread should not panic");
    }

    let count = store.lock().unwrap().count().expect("count should succeed");
    assert_eq!(count, 10, "all 10 concurrent inserts should be present");

    let list = store.lock().unwrap().list().expect("list should succeed");
    assert_eq!(list.len(), 10, "list should return 10 rows");
}

#[test]
fn touch_updates_timestamp() {
    let store = SessionStore::noop();

    store
        .insert(
            "build-touch",
            "/tmp/touch",
            "codex",
            Some("openai"),
            Some("gpt-4.1"),
            false,
        )
        .expect("insert should succeed");

    let before = store
        .list()
        .expect("list should succeed")
        .into_iter()
        .find(|r| r.build_id == "build-touch")
        .expect("row should exist");

    // Sleep to ensure a measurable timestamp difference (SQLite stores seconds).
    thread::sleep(Duration::from_secs(2));

    store
        .touch("build-touch", Some("ollama"), Some("qwen3-coder"))
        .expect("touch should succeed");

    let after = store
        .list()
        .expect("list should succeed")
        .into_iter()
        .find(|r| r.build_id == "build-touch")
        .expect("row should still exist");

    assert!(
        after.updated_at > before.updated_at,
        "touch should increase updated_at: before={}, after={}",
        before.updated_at,
        after.updated_at
    );

    // Optional fields should be updated when provided.
    assert_eq!(after.backend.as_deref(), Some("ollama"));
    assert_eq!(after.model.as_deref(), Some("qwen3-coder"));
}

#[test]
fn remove_deletes_session() {
    let store = SessionStore::noop();

    store
        .insert("build-rm", "/tmp/rm", "light-architect", None, None, false)
        .expect("insert should succeed");

    assert_eq!(store.count().expect("count"), 1);

    store.remove("build-rm").expect("remove should succeed");

    assert_eq!(
        store.count().expect("count"),
        0,
        "count should be 0 after remove"
    );

    let list = store.list().expect("list should succeed");
    assert!(list.is_empty(), "list should be empty after remove");
}

#[test]
fn containerized_flag_roundtrip() {
    let store = SessionStore::noop();

    store
        .insert(
            "build-container",
            "/tmp/container",
            "lightarchitects",
            None,
            None,
            true,
        )
        .expect("insert should succeed");

    let list = store.list().expect("list should succeed");
    assert_eq!(list.len(), 1);
    assert!(
        list[0].containerized,
        "containerized=true should survive roundtrip"
    );
}

#[test]
fn noop_store_uses_in_memory_database() {
    let store = SessionStore::noop();

    // Insert, verify count, then drop and recreate — count should be 0 again
    // because each noop() opens a new :memory: connection.
    store
        .insert(
            "build-ephemeral",
            "/tmp",
            "lightarchitects",
            None,
            None,
            false,
        )
        .expect("insert should succeed");

    assert_eq!(store.count().expect("count"), 1);

    let store2 = SessionStore::noop();
    assert_eq!(
        store2.count().expect("count"),
        0,
        "new noop store should be empty (in-memory database is isolated)"
    );
}

#[test]
fn list_orders_by_updated_at_descending() {
    let store = SessionStore::noop();

    for i in 0..5 {
        store
            .insert(
                &format!("build-{i}"),
                "/tmp/order",
                "lightarchitects",
                None,
                None,
                false,
            )
            .expect("insert should succeed");
        thread::sleep(Duration::from_secs(1));
    }

    // Touch build-2 after another second so it becomes most recent.
    thread::sleep(Duration::from_secs(1));
    store
        .touch("build-2", None, None)
        .expect("touch should succeed");

    let list = store.list().expect("list should succeed");
    assert_eq!(list[0].build_id, "build-2", "touched build should be first");

    // Verify descending order of updated_at.
    for i in 1..list.len() {
        assert!(
            list[i - 1].updated_at >= list[i].updated_at,
            "list should be ordered by updated_at DESC"
        );
    }
}
