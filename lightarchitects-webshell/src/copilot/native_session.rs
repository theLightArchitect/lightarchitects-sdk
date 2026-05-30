//! Persistent CLI subprocess handle for `lightarchitects_native` copilot sessions.
//!
//! Each [`CliSubprocessHandle`] owns a `lightarchitects --output-format stream-json`
//! child process that persists across HTTP turns for a given build UUID: prompts
//! are written to stdin one line at a time, and NDJSON responses are read from
//! stdout until the terminal `{"type":"result",…}` or `{"type":"strategy_halt",…}`
//! line arrives.
//!
//! ## Lifecycle
//!
//! - **Cold start** (first turn for a build): [`CliSubprocessHandle::try_spawn`] forks
//!   the `lightarchitects` binary with `--output-format stream-json --build-id <uuid>`.
//!   `kill_on_drop(true)` ensures the subprocess is cleaned up on drop.
//! - **Warm turns**: the existing handle is locked via `Arc<tokio::sync::Mutex<…>>`,
//!   a prompt line written to stdin, and NDJSON lines drained from stdout.
//! - **Interrupt / error / EOF**: the entry is removed from the pool; the next turn
//!   cold-starts a fresh subprocess.
//!
//! ## Memory model
//!
//! Session memory lives entirely inside the subprocess (via `HelixSessionMemory`).
//! The webshell does not manage conversation history — it forwards prompts and
//! translates NDJSON response events to SSE frames.

use std::sync::{Arc, atomic::AtomicBool};

use dashmap::DashMap;
use tokio::io::{AsyncBufReadExt, BufReader, BufWriter, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};
use uuid::Uuid;

/// One persistent `lightarchitects` subprocess per build.
///
/// Wraps the child process handles so the owning async task can write prompts
/// to stdin and read NDJSON response lines from stdout.  `kill_on_drop(true)`
/// on the [`Child`] ensures the subprocess is killed when this struct drops.
pub struct CliSubprocessHandle {
    /// Buffered write end of the child's stdin.  Must be flushed after each
    /// prompt line so the subprocess sees the input without buffering delay.
    pub(crate) stdin: BufWriter<ChildStdin>,
    /// Line-buffered read end of the child's stdout.  Each `next_line()` call
    /// returns one complete NDJSON object or `None` on EOF.
    pub(crate) stdout: Lines<BufReader<ChildStdout>>,
    /// Child process handle — kept alive so `kill_on_drop` fires on drop.
    _process: Child,
    /// Shared interrupt flag.  Set by the interrupt HTTP handler; polled
    /// between NDJSON lines in the turn reader loop.
    pub interrupt_flag: Arc<AtomicBool>,
}

impl CliSubprocessHandle {
    /// Spawn a new `lightarchitects` subprocess for `build_id`.
    ///
    /// Invokes `<binary> run --output-format stream-json --build-id <uuid>` in
    /// `cwd` with stdin/stdout piped and stderr discarded.  `kill_on_drop(true)`
    /// is set so the subprocess is killed when the handle is dropped.
    ///
    /// This function is **synchronous**: `tokio::process::Command::spawn()` only
    /// forks the process; no async I/O is performed here.
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if the binary cannot be found or the fork fails.
    pub fn try_spawn(
        cwd: &std::path::Path,
        build_id: Uuid,
        binary: &str,
        interrupt_flag: Arc<AtomicBool>,
    ) -> std::io::Result<Self> {
        let mut command = tokio::process::Command::new(binary);
        command
            .current_dir(cwd)
            .arg("run")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--build-id")
            .arg(build_id.to_string())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);

        let mut child = command.spawn()?;
        let stdin = BufWriter::new(child.stdin.take().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "subprocess stdin not piped")
        })?);
        let stdout = BufReader::new(child.stdout.take().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "subprocess stdout not piped",
            )
        })?)
        .lines();

        Ok(Self {
            stdin,
            stdout,
            _process: child,
            interrupt_flag,
        })
    }

    /// Provider identifier — constant for all subprocess-model sessions.
    #[must_use]
    pub const fn provider_name(&self) -> &'static str {
        "lightarchitects"
    }
}

/// Persistent subprocess pool keyed by build [`Uuid`].
///
/// Each entry is `Arc<tokio::sync::Mutex<CliSubprocessHandle>>`.  The mutex
/// serialises concurrent turn requests for the same build (one in-flight turn
/// per session) while the [`DashMap`] shards lookups across builds so unrelated
/// builds never block each other.
pub type NativeSessionPool = Arc<DashMap<Uuid, Arc<tokio::sync::Mutex<CliSubprocessHandle>>>>;

/// Construct an empty session pool.  Wired into [`crate::server::AppState`].
#[must_use]
pub fn new_pool() -> NativeSessionPool {
    Arc::new(DashMap::new())
}
