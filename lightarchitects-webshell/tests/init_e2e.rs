//! Init pipeline end-to-end tests.
//!
//! Covers profiler checkpoint events, telemetry hashing, and shutdown registry.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use tracing_subscriber::fmt::format::FmtSpan;

/// Tracing writer that forwards formatted log lines into an MPSC channel.
/// Defined once at the top level so it is not repeated inline in every test.
struct ChanWriter(std::sync::mpsc::Sender<String>);

impl std::io::Write for ChanWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(s) = std::str::from_utf8(buf) {
            let _ = self.0.send(s.to_owned());
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// ── Profiler tests ─────────────────────────────────────────────────────────

#[test]
fn profiler_emits_checkpoint_event() {
    let (tx, rx) = std::sync::mpsc::channel();

    let subscriber = tracing_subscriber::fmt()
        .with_target(true)
        .with_span_events(FmtSpan::NONE)
        .with_writer(move || ChanWriter(tx.clone()))
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        lightarchitects_webshell::profile_checkpoint!("test_phase");
    });

    // Drain messages and look for the checkpoint.
    let mut found = false;
    while let Ok(line) = rx.recv_timeout(Duration::from_millis(500)) {
        if line.contains("startup") && line.contains("test_phase") {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "profiler checkpoint should emit a tracing event with target 'startup' and phase name"
    );
}

// ── Telemetry tests ────────────────────────────────────────────────────────

#[test]
fn telemetry_hashes_uuid() {
    let (tx, rx) = std::sync::mpsc::channel();

    let subscriber = tracing_subscriber::fmt()
        .with_target(true)
        .with_writer(move || ChanWriter(tx.clone()))
        .finish();

    let build_id = uuid::Uuid::new_v4();
    let build_id_str = build_id.to_string();

    tracing::subscriber::with_default(subscriber, || {
        let telem = lightarchitects_webshell::init::telemetry::TelemetryHandle::new();
        telem.session_start(&build_id, "lightarchitects", "anthropic", false);
    });

    let mut found = false;
    while let Ok(line) = rx.recv_timeout(Duration::from_millis(500)) {
        if line.contains("la_telemetry")
            && line.contains("session_start")
            && !line.contains(&build_id_str)
        {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "telemetry session_start should emit event with hashed build_id (raw UUID must NOT appear)"
    );
}

#[test]
fn telemetry_build_created_strips_cwd_to_basename() {
    let (tx, rx) = std::sync::mpsc::channel();

    let subscriber = tracing_subscriber::fmt()
        .with_target(true)
        .with_writer(move || ChanWriter(tx.clone()))
        .finish();

    let build_id = uuid::Uuid::new_v4();
    let cwd = std::path::Path::new("/Users/kft/Projects/lightarchitects-sdk/some-build");

    tracing::subscriber::with_default(subscriber, || {
        let telem = lightarchitects_webshell::init::telemetry::TelemetryHandle::new();
        telem.build_created(&build_id, cwd);
    });

    let mut found = false;
    while let Ok(line) = rx.recv_timeout(Duration::from_millis(500)) {
        if line.contains("la_telemetry")
            && line.contains("build_created")
            && line.contains("some-build")
            && !line.contains("/Users/kft/Projects")
        {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "telemetry build_created should strip cwd to basename only (no full path)"
    );
}

#[test]
fn telemetry_model_switch_event() {
    let (tx, rx) = std::sync::mpsc::channel();

    let subscriber = tracing_subscriber::fmt()
        .with_target(true)
        .with_writer(move || ChanWriter(tx.clone()))
        .finish();

    let build_id = uuid::Uuid::new_v4();

    tracing::subscriber::with_default(subscriber, || {
        let telem = lightarchitects_webshell::init::telemetry::TelemetryHandle::new();
        telem.model_switch(&build_id, "claude-sonnet-4-6", "claude-opus-4-7");
    });

    let mut found = false;
    while let Ok(line) = rx.recv_timeout(Duration::from_millis(500)) {
        if line.contains("la_telemetry")
            && line.contains("model_switch")
            && line.contains("claude-sonnet-4-6")
            && line.contains("claude-opus-4-7")
        {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "telemetry model_switch should emit event with old and new model names"
    );
}

// ── Shutdown registry tests ────────────────────────────────────────────────
//
// The global CLEANUP_REGISTRY static is shared across all test threads.
// These tests must be serialized and must drain stale state before registering
// their own handlers so concurrent runs don't cross-contaminate.
static SHUTDOWN_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn shutdown_registry_runs_all_handlers_in_lifo_order() {
    let _guard = SHUTDOWN_TEST_LOCK.lock().unwrap();
    lightarchitects_webshell::init::shutdown::drain_for_test();

    let order = Arc::new(std::sync::Mutex::new(Vec::new()));

    {
        let o1 = Arc::clone(&order);
        lightarchitects_webshell::init::shutdown::register_cleanup(move || {
            o1.lock().unwrap().push(1);
        });
    }
    {
        let o2 = Arc::clone(&order);
        lightarchitects_webshell::init::shutdown::register_cleanup(move || {
            o2.lock().unwrap().push(2);
        });
    }
    {
        let o3 = Arc::clone(&order);
        lightarchitects_webshell::init::shutdown::register_cleanup(move || {
            o3.lock().unwrap().push(3);
        });
    }

    // Run cleanup manually (exposed as pub(crate) for tests).
    lightarchitects_webshell::init::shutdown::run_cleanup();

    let final_order = order.lock().unwrap();
    assert_eq!(
        *final_order,
        vec![3, 2, 1],
        "cleanup should run in LIFO (most-recently-registered first) order"
    );
}

#[test]
fn shutdown_registry_isolates_panicking_cleanup() {
    let _guard = SHUTDOWN_TEST_LOCK.lock().unwrap();
    lightarchitects_webshell::init::shutdown::drain_for_test();

    let count = Arc::new(std::sync::Mutex::new(0));

    {
        let c1 = Arc::clone(&count);
        lightarchitects_webshell::init::shutdown::register_cleanup(move || {
            *c1.lock().unwrap() += 1;
        });
    }
    {
        lightarchitects_webshell::init::shutdown::register_cleanup(move || {
            panic!("intentional panic for test isolation");
        });
    }
    {
        let c2 = Arc::clone(&count);
        lightarchitects_webshell::init::shutdown::register_cleanup(move || {
            *c2.lock().unwrap() += 1;
        });
    }

    lightarchitects_webshell::init::shutdown::run_cleanup();

    let final_count = count.lock().unwrap();
    assert_eq!(
        *final_count, 2,
        "non-panicking cleanups should still run when one panics"
    );
}
