//! Graceful shutdown registry — SIGTERM/SIGINT cleanup hooks.

use std::sync::Mutex;

/// Registered cleanup functions to run on shutdown.
static CLEANUP_REGISTRY: Mutex<Vec<Box<dyn FnOnce() + Send>>> = Mutex::new(Vec::new());

/// Register a cleanup function to run on SIGTERM or SIGINT.
///
/// Functions run in LIFO order (most-recently-registered first).
/// Best-effort — a panicking cleanup is isolated via `catch_unwind` so the
/// remaining functions still run.
///
/// # Panics
///
/// Panics only if the global cleanup registry has been poisoned by a
/// panicking thread (extremely unlikely).
#[allow(clippy::unwrap_used)]
pub fn register_cleanup(f: impl FnOnce() + Send + 'static) {
    CLEANUP_REGISTRY.lock().unwrap().push(Box::new(f));
}

/// Returns a future that resolves when SIGTERM (Unix) or SIGINT is received.
///
/// On non-Unix platforms, only SIGINT is handled.
/// This future is suitable for passing to `axum::serve(...).with_graceful_shutdown`
/// so the HTTP layer can drain connections before the process exits.
///
/// # Panics
///
/// Panics only if Tokio signal handler registration fails (kernel-level
/// resource exhaustion — effectively impossible in practice).
#[allow(clippy::expect_used)]
pub async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm =
            signal(SignalKind::terminate()).expect("SIGTERM handler registration failed");
        let mut sigint =
            signal(SignalKind::interrupt()).expect("SIGINT handler registration failed");

        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!(target: "shutdown", "SIGTERM received");
            }
            _ = sigint.recv() => {
                tracing::info!(target: "shutdown", "SIGINT received");
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!(target: "shutdown", "CTRL-C received");
    }
}

/// Wait for SIGTERM (Unix) or SIGINT, then run all registered cleanup functions.
///
/// On non-Unix platforms, only SIGINT is handled.
/// After running cleanup, returns — the caller should exit the process.
pub async fn wait_for_shutdown() {
    shutdown_signal().await;
    run_cleanup();
}

#[doc(hidden)]
pub fn run_cleanup() {
    // Drain into a local Vec so the lock is not held during execution,
    // preventing deadlock if a cleanup calls register_cleanup.
    #[allow(clippy::unwrap_used)]
    let functions: Vec<Box<dyn FnOnce() + Send>> = {
        let mut registry = CLEANUP_REGISTRY.lock().unwrap();
        registry.drain(..).rev().collect()
    };

    for f in functions {
        if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
            tracing::error!(target: "shutdown", panic = ?e, "cleanup function panicked");
        }
    }
    tracing::info!(target: "shutdown", "cleanup complete");
}
