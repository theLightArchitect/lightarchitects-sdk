//! `spawn_with_span_context` propagation tests.
//!
//! Verifies that task-local span context crosses `tokio::spawn` boundaries
//! when using `spawn_with_span_context`, and that raw `tokio::spawn` correctly
//! loses the context (demonstrating why the CI denylist exists).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects_gateway::span_context::{
    GatewaySpanContext, current_span_ctx, spawn_with_span_context, with_span_context,
};
use uuid::Uuid;

/// `spawn_with_span_context` forwards both session_id and parent_id into the
/// spawned task's task-local storage.
#[tokio::test]
async fn spawn_with_span_context_preserves_parent_id() {
    let parent = Uuid::new_v4();
    let sid = "spawn-safety-session".to_owned();
    let ctx = GatewaySpanContext {
        session_id: Some(sid.clone()),
        parent_id: Some(parent),
    };

    let seen = with_span_context(ctx, async {
        spawn_with_span_context(async { current_span_ctx() })
            .await
            .expect("task join")
    })
    .await;

    assert_eq!(
        seen.parent_id,
        Some(parent),
        "parent_id must survive tokio::spawn boundary"
    );
    assert_eq!(
        seen.session_id.as_deref(),
        Some("spawn-safety-session"),
        "session_id must survive tokio::spawn boundary"
    );
}

/// Raw `tokio::spawn` does NOT inherit the caller's task-local context.
///
/// This is the failure mode the CI denylist (`ci-denylist.sh`) prevents:
/// raw spawns on the request path silently produce orphan spans with no
/// parent_id or session_id.
#[tokio::test]
async fn raw_tokio_spawn_loses_span_context() {
    let parent = Uuid::new_v4();
    let ctx = GatewaySpanContext {
        session_id: Some("should-not-cross".to_owned()),
        parent_id: Some(parent),
    };

    let seen = with_span_context(ctx, async {
        tokio::spawn(async { current_span_ctx() })
            .await
            .expect("task join")
    })
    .await;

    // Raw spawn creates a new task with default (empty) context.
    assert!(
        seen.parent_id.is_none(),
        "raw tokio::spawn must NOT inherit parent_id — use spawn_with_span_context instead"
    );
    assert!(
        seen.session_id.is_none(),
        "raw tokio::spawn must NOT inherit session_id"
    );
}

/// The CI denylist script exists and is executable.
///
/// This ensures the gate enforcing `spawn_with_span_context` usage is
/// actually wired into the build — a missing script would silently allow
/// raw spawns to accumulate on the request path.
#[test]
fn ci_denylist_script_is_executable() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cargo")
        .join("ci-denylist.sh");

    assert!(script.exists(), "ci-denylist.sh missing at {:?}", script);

    // Verify execute permission is set.
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(&script).expect("metadata");
    let mode = meta.permissions().mode();
    assert!(
        mode & 0o111 != 0,
        "ci-denylist.sh is not executable (mode {:o})",
        mode
    );
}
