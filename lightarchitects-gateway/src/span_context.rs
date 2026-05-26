//! Gateway-wide AYIN span context — task-local propagation + atomic disk write.
//!
//! [`GatewaySpanContext`] is propagated through async call chains via
//! `tokio::task_local!`. Use [`with_span_context`] to seed a new context
//! and [`spawn_with_span_context`] to forward it across `tokio::spawn`.
//!
//! [`write_span_to_disk`] performs an atomic tmp→rename write with an EXDEV
//! fallback (cross-filesystem copy + unlink) and macOS `F_FULLFSYNC` for
//! true power-loss durability (R21 requirement).

use lightarchitects::ayin::span::TraceSpan;
use std::future::Future;
use std::path::{Path, PathBuf};
use uuid::Uuid;

// ── Span context ──────────────────────────────────────────────────────────────

/// Span propagation metadata carried through an async task chain.
#[derive(Clone, Debug, Default)]
pub struct GatewaySpanContext {
    /// Stable session identifier for the gateway interaction.
    pub session_id: Option<String>,
    /// Parent span UUID — links child spans to their logical parent.
    pub parent_id: Option<Uuid>,
}

tokio::task_local! {
    /// Task-local span context, accessed via [`with_span_context`] and
    /// [`current_span_ctx`].
    pub static SPAN_CTX: GatewaySpanContext;
}

/// Run `f` with `ctx` seeded into the current task's local storage.
///
/// Nested calls push a new scope; the prior value is restored on exit.
pub async fn with_span_context<F, T>(ctx: GatewaySpanContext, f: F) -> T
where
    F: Future<Output = T>,
{
    SPAN_CTX.scope(ctx, f).await
}

/// Spawn a tokio task that inherits the caller's span context.
///
/// Without this wrapper, `tokio::spawn` creates a new task whose
/// `task_local!` storage is unset. This wrapper captures the caller's
/// context (or `Default` when called outside a span scope) and re-seeds
/// it in the spawned task.
pub fn spawn_with_span_context<F>(f: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let ctx = SPAN_CTX
        .try_with(GatewaySpanContext::clone)
        .unwrap_or_default();
    tokio::spawn(SPAN_CTX.scope(ctx, f))
}

/// Return a clone of the current task-local span context.
///
/// Returns [`Default`] when called outside a [`with_span_context`] scope.
pub fn current_span_ctx() -> GatewaySpanContext {
    SPAN_CTX
        .try_with(GatewaySpanContext::clone)
        .unwrap_or_default()
}

// ── Atomic disk write ─────────────────────────────────────────────────────────

/// Maximum span payload in bytes — 64 KB cap prevents R11 eviction attacks.
const SPAN_BUDGET_BYTES: usize = 64 * 1024;

/// Span sub-directory layout: `<base>/<actor>/<YYYY-MM-DD>/`.
pub fn span_dir(base: &Path, actor: &str, timestamp: &chrono::DateTime<chrono::Utc>) -> PathBuf {
    base.join(actor)
        .join(timestamp.format("%Y-%m-%d").to_string())
}

/// Write `span` to `dir` atomically via tmp→rename with EXDEV fallback.
///
/// The filename is derived from the span timestamp, action, and UUID prefix
/// so traces sort naturally by time without an index file.
///
/// On macOS, `F_FULLFSYNC` is called after the write to ensure power-loss
/// durability (APFS `F_BARRIERFSYNC` only provides an ordering barrier,
/// not a flush to stable storage).
///
/// # Errors
/// Returns an error string when the span cannot be written to disk.
/// Oversized spans (> 64 KB) are silently dropped with a counter log.
pub async fn write_span_to_disk(span: &TraceSpan, dir: &PathBuf) -> Result<(), String> {
    let bytes = serde_json::to_vec(span).map_err(|e| format!("span serialize: {e}"))?;

    if bytes.len() > SPAN_BUDGET_BYTES {
        tracing::warn!(
            span_id = %span.id,
            size = bytes.len(),
            counter = "ayin.span.dropped.oversize",
            "AYIN span dropped: payload exceeds 64 KB budget (R11)"
        );
        return Ok(());
    }

    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| format!("ayin trace dir: {e}"))?;

    let safe_action = span.action.replace('/', "_");
    let id_prefix = &span.id.to_string()[..8];
    let filename = format!(
        "{}-{safe_action}-{id_prefix}.json",
        span.timestamp.format("%H-%M-%S")
    );

    let dest = dir.join(&filename);
    let tmp = dir.join(format!("{filename}.tmp"));

    tokio::fs::write(&tmp, &bytes)
        .await
        .map_err(|e| format!("ayin tmp write: {e}"))?;

    match tokio::fs::rename(&tmp, &dest).await {
        Ok(()) => {
            fullfsync_path(&dest);
            Ok(())
        }
        Err(ref e) if is_exdev(e) => {
            // Cross-filesystem (EXDEV): fall back to direct write + fsync.
            let _ = tokio::fs::remove_file(&tmp).await;
            tokio::fs::write(&dest, &bytes)
                .await
                .map_err(|e| format!("ayin EXDEV write: {e}"))?;
            fullfsync_path(&dest);
            Ok(())
        }
        Err(e) => Err(format!("ayin span rename: {e}")),
    }
}

fn is_exdev(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(libc::EXDEV)
}

/// Call `F_FULLFSYNC` on macOS for true power-loss durability.
///
/// On other platforms this is a no-op — standard `fsync` / page-cache
/// semantics apply.
#[cfg(target_os = "macos")]
fn fullfsync_path(path: &Path) {
    use std::os::unix::io::AsRawFd;
    if let Ok(f) = std::fs::File::open(path) {
        let fd = f.as_raw_fd();
        // SAFETY: `fd` is valid (owned by `f`). `F_FULLFSYNC` is a well-defined
        // macOS fcntl command (since 10.5). Return value intentionally ignored —
        // failure is non-fatal; the OS page cache flush from close() still occurs.
        #[allow(unsafe_code)]
        unsafe {
            libc::fcntl(fd, libc::F_FULLFSYNC);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn fullfsync_path(_path: &Path) {}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn span_ctx_propagates_through_scope() {
        let ctx = GatewaySpanContext {
            session_id: Some("sess-123".to_owned()),
            parent_id: Some(Uuid::new_v4()),
        };
        let seen = with_span_context(ctx.clone(), async { current_span_ctx() }).await;
        assert_eq!(seen.session_id, ctx.session_id);
        assert_eq!(seen.parent_id, ctx.parent_id);
    }

    #[tokio::test]
    async fn current_span_ctx_returns_default_outside_scope() {
        let ctx = current_span_ctx();
        assert!(ctx.session_id.is_none());
        assert!(ctx.parent_id.is_none());
    }

    #[tokio::test]
    async fn spawn_with_span_context_forwards_ctx() {
        let expected_session = "forward-test".to_owned();
        let ctx = GatewaySpanContext {
            session_id: Some(expected_session.clone()),
            parent_id: None,
        };
        let handle = with_span_context(ctx, async {
            spawn_with_span_context(async { current_span_ctx().session_id })
        })
        .await;
        let got = handle.await.expect("join");
        assert_eq!(got, Some(expected_session));
    }

    #[tokio::test]
    async fn write_span_to_disk_creates_file() {
        use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};
        let tmp_dir = tempfile::tempdir().expect("tempdir");
        let dir = PathBuf::from(tmp_dir.path());

        let span = TraceContext::new(Actor::new("gateway"), "test.action")
            .outcome(TraceOutcome::Continue)
            .finish()
            .expect("span");

        write_span_to_disk(&span, &dir).await.expect("write");

        let entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("readdir")
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1, "expected exactly one span file");
        let content = std::fs::read(&entries[0].path()).expect("read span");
        let parsed: serde_json::Value = serde_json::from_slice(&content).expect("parse json");
        assert_eq!(parsed["action"], "test.action");
    }

    #[tokio::test]
    async fn oversized_span_is_dropped_not_errored() {
        use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};
        use serde_json::json;
        let tmp_dir = tempfile::tempdir().expect("tempdir");
        let dir = PathBuf::from(tmp_dir.path());

        // Build a span with a very large metadata payload
        let big_meta = json!({ "data": "x".repeat(SPAN_BUDGET_BYTES + 1) });
        let span = TraceContext::new(Actor::new("gateway"), "test.big")
            .metadata(big_meta)
            .outcome(TraceOutcome::Continue)
            .finish()
            .expect("span");

        // Should NOT return an error — silently drops with counter log
        write_span_to_disk(&span, &dir)
            .await
            .expect("no error on oversize");

        let entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("readdir")
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 0, "oversized span should not be written");
    }
}
