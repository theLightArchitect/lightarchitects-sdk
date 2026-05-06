//! Graceful shutdown registry — SIGTERM/SIGINT cleanup hooks.

use std::sync::Mutex;

/// Registered cleanup functions to run on shutdown.
static CLEANUP_REGISTRY: Mutex<Vec<Box<dyn FnOnce() + Send>>> = Mutex::new(Vec::new());

/// Register a cleanup function to run on SIGTERM or SIGINT.
///
/// Functions run in LIFO order (most-recently-registered first).
/// Best-effort — if a function panics, the remaining functions still run.
///
/// # Panics
///
/// Panics only if the global cleanup registry has been poisoned by a
/// panicking thread (extremely unlikely).
#[allow(clippy::unwrap_used)]
pub fn register_cleanup(f: impl FnOnce() + Send + 'static) {
    CLEANUP_REGISTRY.lock().unwrap().push(Box::new(f));
}

/// Wait for SIGTERM (Unix) or SIGINT, then run all registered cleanup functions.
///
/// On non-Unix platforms, only SIGINT is handled.
/// After running cleanup, returns — the caller should exit the process.
///
/// # Panics
///
/// Panics only if Tokio signal handler registration fails (kernel-level
/// resource exhaustion — effectively impossible in practice).
#[allow(clippy::expect_used)]
pub async fn wait_for_shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm =
            signal(SignalKind::terminate()).expect("SIGTERM handler registration failed");
        let mut sigint =
            signal(SignalKind::interrupt()).expect("SIGINT handler registration failed");

        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!(target: "shutdown", "SIGTERM received — running cleanup");
            }
            _ = sigint.recv() => {
                tracing::info!(target: "shutdown", "SIGINT received — running cleanup");
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!(target: "shutdown", "CTRL-C received — running cleanup");
    }

    run_cleanup();
}

fn run_cleanup() {
    #[allow(clippy::unwrap_used)]
    let mut registry = CLEANUP_REGISTRY.lock().unwrap();
    for f in registry.drain(..).rev() {
        f();
    }
    tracing::info!(target: "shutdown", "cleanup complete");
}
