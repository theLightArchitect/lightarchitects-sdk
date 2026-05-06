//! Background group-commit NDJSON writer.
//!
//! # Design
//!
//! One writer task per session. The [`TurnLogWriter`] handle is clone-safe;
//! callers on the hot path use [`TurnLogWriter::append`] which pushes a
//! [`ayin::TraceSpan`] into an unbounded channel and returns immediately. A
//! background tokio task drains the channel, signs each span against the HMAC
//! chain, buffers up to 100 entries or 50 ms, then `fdatasync`s in one call.
//!
//! # Durability guarantee
//!
//! On clean [`TurnLogWriter::close`]: terminal `session_ended` span written,
//! buffer flushed, `sync_data` called, file renamed from
//! `active/{id}.ndjson` to `ended/{date}/{id}.ndjson`.
//!
//! On crash: whatever was last `sync_data`'d survives. Partial un-synced
//! writes in the OS page cache may be lost. The chain makes torn tails
//! detectable — [`lightarchitects::turnlog::chain::verify_chain`] fails at the seq of the
//! first bad entry rather than silently accepting a truncated file.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::ayin::span::TraceSpan;
use crate::ayin::span::{Actor, TraceContext, TraceOutcome};
use secrecy::{SecretSlice, SecretString};
use serde::{Deserialize, Serialize};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use crate::turnlog::chain::{
    GenesisBlock, build_and_sign, derive_session_key, fresh_hkdf_salt, pepper_fingerprint,
};
use crate::turnlog::entry::TurnEntry;
use crate::turnlog::error::{Result, TurnLogError};
use crate::turnlog::store::StoreLayout;

/// Max entries buffered before an automatic flush fires.
const FLUSH_BATCH: usize = 100;

/// Max wall-clock time between first-buffered and flush.
const FLUSH_INTERVAL: Duration = Duration::from_millis(50);

// ── EndReason ───────────────────────────────────────────────────────────────────

/// Why a session ended — carried in the `session_ended` span's metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    /// User invoked `/quit` or Ctrl-C.
    UserExit,
    /// Task completed successfully.
    Complete,
    /// Unrecoverable error in the runner loop.
    Error,
    /// Token budget or cost gate forced termination.
    Timeout,
    /// Detected-on-restart by the crash-recovery path.
    ///
    /// Never written by a live writer — set on genesis discovery with no
    /// matching `session_ended` entry.
    Crashed,
}

impl EndReason {
    /// Stable lowercase string stored in span metadata.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserExit => "user_exit",
            Self::Complete => "complete",
            Self::Error => "error",
            Self::Timeout => "timeout",
            Self::Crashed => "crashed",
        }
    }
}

// ── Public handle ───────────────────────────────────────────────────────────────

/// Handle to the per-session writer task.
///
/// Cloning is cheap; all clones share the same underlying channel. Dropping
/// the last clone does NOT close the writer — call [`Self::close`] or
/// [`Self::pause`] explicitly.
#[derive(Clone)]
pub struct TurnLogWriter {
    tx: mpsc::UnboundedSender<WriterMsg>,
    next_seq: Arc<AtomicU64>,
    session_id: String,
}

impl TurnLogWriter {
    /// Open a new session writer.
    ///
    /// Creates the directory scaffold, generates the genesis block, writes the
    /// first `session_start` span, and spawns the background writer task.
    ///
    /// `pepper` is the store-level secret (typically loaded from
    /// `~/lightarchitects/lightarchitects_cli/.session-key` via
    /// [`lightarchitects::core::paths::session_key`]).
    /// An HKDF-derived per-session key is held only inside the background task.
    ///
    /// # Errors
    /// * [`TurnLogError::Io`] if directory or file creation fails.
    /// * [`TurnLogError::Crypto`] if key derivation fails.
    /// * [`TurnLogError::Serialize`] if genesis or first entry cannot be serialised.
    pub async fn open(
        layout: &StoreLayout,
        session_id: String,
        project_root: PathBuf,
        model: String,
        provider: String,
        parent_session_id: Option<String>,
        pepper: &SecretSlice<u8>,
    ) -> Result<Self> {
        layout.ensure_dirs().await?;

        let hkdf_salt = fresh_hkdf_salt();
        let session_key = derive_session_key(pepper, &hkdf_salt, &session_id)?;
        let pepper_fp = pepper_fingerprint(pepper)?;

        let mut genesis = GenesisBlock {
            session_id: session_id.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            project_hash: hash_project_root(&project_root),
            hkdf_salt,
            pepper_fingerprint: pepper_fp,
            hmac_genesis: String::new(),
        };
        genesis.sign(&session_key)?;

        let genesis_path = layout.genesis_path(&session_id);
        let genesis_json = serde_json::to_vec_pretty(&genesis)?;
        tokio::fs::write(&genesis_path, &genesis_json)
            .await
            .map_err(|e| TurnLogError::io(&genesis_path, e))?;

        let active_path = layout.active_path(&session_id);
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&active_path)
            .await
            .map_err(|e| TurnLogError::io(&active_path, e))?;

        let (tx, rx) = mpsc::unbounded_channel::<WriterMsg>();
        let state = WriterState {
            file: BufWriter::new(file),
            session_key,
            prev_hmac: genesis.hmac_genesis.clone(),
            ended_path: layout.ended_path(
                &session_id,
                &chrono::Utc::now().format("%Y-%m-%d").to_string(),
            ),
            active_path: active_path.clone(),
        };
        tokio::spawn(writer_task(rx, state));

        let writer = Self {
            tx,
            next_seq: Arc::new(AtomicU64::new(0)),
            session_id: session_id.clone(),
        };

        // Write the first entry — session_start — via the live task so the
        // HMAC chain is uniform from entry 0 onward.
        let start_meta = serde_json::json!({
            "project_root": project_root.to_string_lossy(),
            "model": model,
            "provider": provider,
            "parent_session_id": parent_session_id,
        });
        writer.append(make_span(&session_id, "session_start", start_meta));

        Ok(writer)
    }

    /// Resume an existing abandoned or paused session.
    ///
    /// Opens the `active/{session_id}.ndjson` file in append mode, reads the
    /// tail to recover `next_seq` and the last HMAC for chain continuation,
    /// then writes a `session_resumed` marker.
    ///
    /// Use this for crash recovery (file exists in `active/` but the writer
    /// process died) and for explicit pause-then-resume workflows.
    ///
    /// # Errors
    /// * [`TurnLogError::MissingGenesis`] if the genesis block is absent.
    /// * [`TurnLogError::SessionNotFound`] if the active file does not exist.
    /// * [`TurnLogError::Io`] for other filesystem failures.
    /// * [`TurnLogError::Crypto`] if key re-derivation fails.
    pub async fn resume(
        layout: &StoreLayout,
        session_id: String,
        pepper: &SecretSlice<u8>,
    ) -> Result<Self> {
        // Load genesis block to recover the HKDF salt for key re-derivation.
        let genesis_path = layout.genesis_path(&session_id);
        if !genesis_path.is_file() {
            return Err(TurnLogError::MissingGenesis(session_id));
        }
        let genesis_bytes = tokio::fs::read(&genesis_path)
            .await
            .map_err(|e| TurnLogError::io(&genesis_path, e))?;
        let genesis: GenesisBlock = serde_json::from_slice(&genesis_bytes)?;

        let session_key = derive_session_key(pepper, &genesis.hkdf_salt, &session_id)?;

        // Active file must exist — we are resuming, not starting fresh.
        let active_path = layout.active_path(&session_id);
        if !active_path.is_file() {
            return Err(TurnLogError::SessionNotFound(session_id));
        }
        let file_bytes = tokio::fs::read(&active_path)
            .await
            .map_err(|e| TurnLogError::io(&active_path, e))?;

        // Recover `next_seq` and `prev_hmac` from the last non-empty line.
        let last_entry: Option<TurnEntry> = file_bytes
            .split(|&b| b == b'\n')
            .filter(|line| !line.is_empty())
            .next_back()
            .and_then(|line| serde_json::from_slice(line).ok());

        let (next_seq, prev_hmac) = match last_entry {
            Some(ref e) => (e.seq.saturating_add(1), e.hmac_self.clone()),
            None => (0, genesis.hmac_genesis.clone()),
        };

        let file = OpenOptions::new()
            .append(true)
            .open(&active_path)
            .await
            .map_err(|e| TurnLogError::io(&active_path, e))?;

        let (tx, rx) = mpsc::unbounded_channel::<WriterMsg>();
        let state = WriterState {
            file: BufWriter::new(file),
            session_key,
            prev_hmac,
            ended_path: layout.ended_path(
                &session_id,
                &chrono::Utc::now().format("%Y-%m-%d").to_string(),
            ),
            active_path,
        };
        tokio::spawn(writer_task(rx, state));

        let writer = Self {
            tx,
            next_seq: Arc::new(AtomicU64::new(next_seq)),
            session_id: session_id.clone(),
        };

        // Stamp the resumption point in the chain.
        let meta = serde_json::json!({ "resumed_at_seq": next_seq });
        writer.append(make_span(&session_id, "session_resumed", meta));

        Ok(writer)
    }

    /// Session UUID for this writer.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Append a span. `seq` is assigned here; `parent_seq` is `None`.
    ///
    /// Non-blocking. If the writer task has died the message is silently
    /// dropped — the caller's hot path never fails on persistence.
    pub fn append(&self, span: TraceSpan) {
        self.append_with_parent(None, span);
    }

    /// Append with an explicit `parent_seq` (e.g. spans inside a turn).
    pub fn append_with_parent(&self, parent_seq: Option<u64>, span: TraceSpan) {
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        if self
            .tx
            .send(WriterMsg::Entry(Box::new(PendingEntry {
                seq,
                parent_seq,
                span,
            })))
            .is_err()
        {
            warn!(target: "turnlog", "writer task gone; dropped entry seq={seq}");
        }
    }

    /// Force an immediate flush + fsync. Normally unnecessary — group commit
    /// fires on either [`FLUSH_BATCH`] entries or [`FLUSH_INTERVAL`].
    ///
    /// # Errors
    /// Returns [`TurnLogError::WriterGone`] if the writer task has exited.
    pub async fn flush(&self) -> Result<()> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.tx
            .send(WriterMsg::Flush(ack_tx))
            .map_err(|_| TurnLogError::WriterGone)?;
        ack_rx.await.map_err(|_| TurnLogError::WriterGone)
    }

    /// Write a `session_paused` span. File stays in `active/`;
    /// [`lightarchitects::turnlog::reader::TurnLogReader`] will surface it as a resumable session.
    ///
    /// The writer task continues running — callers must still call [`Self::close`]
    /// afterwards to fully shut down. Pause followed by close is the
    /// clean-exit-with-resumable-memo pattern.
    ///
    /// # Errors
    /// Returns [`TurnLogError::WriterGone`] if the writer task has exited.
    pub async fn pause(&self, memo_body: String, memo_weight: f64) -> Result<()> {
        let meta = serde_json::json!({
            "memo_body": memo_body,
            "memo_weight": memo_weight,
            "dimensions": [],
            "themes": [],
        });
        self.append(make_span(&self.session_id, "session_paused", meta));
        self.flush().await
    }

    /// Write `session_ended`, flush, fsync, and move the file from
    /// `active/` to `ended/{YYYY-MM-DD}/`.
    ///
    /// Consumes self — no further appends are possible.
    ///
    /// # Errors
    /// Returns [`TurnLogError::WriterGone`] if the writer task is already gone.
    /// Returns [`TurnLogError::Io`] if the rename fails.
    pub async fn close(self, reason: EndReason) -> Result<()> {
        let meta = serde_json::json!({ "reason": reason.as_str() });
        self.append(make_span(&self.session_id, "session_ended", meta));
        let (ack_tx, ack_rx) = oneshot::channel();
        self.tx
            .send(WriterMsg::Shutdown(ack_tx))
            .map_err(|_| TurnLogError::WriterGone)?;
        ack_rx.await.map_err(|_| TurnLogError::WriterGone)?
    }
}

// ── Internals ───────────────────────────────────────────────────────────────────

enum WriterMsg {
    Entry(Box<PendingEntry>),
    Flush(oneshot::Sender<()>),
    Shutdown(oneshot::Sender<Result<()>>),
}

struct PendingEntry {
    seq: u64,
    parent_seq: Option<u64>,
    span: TraceSpan,
}

struct WriterState {
    file: BufWriter<File>,
    session_key: SecretString,
    prev_hmac: String,
    active_path: PathBuf,
    ended_path: PathBuf,
}

async fn writer_task(mut rx: mpsc::UnboundedReceiver<WriterMsg>, mut state: WriterState) {
    let mut buffer: Vec<TurnEntry> = Vec::with_capacity(FLUSH_BATCH);
    let mut flush_deadline: Option<tokio::time::Instant> = None;

    loop {
        let sleep_until = flush_deadline.map_or_else(
            || tokio::time::sleep(Duration::from_secs(3600)),
            tokio::time::sleep_until,
        );
        tokio::pin!(sleep_until);

        tokio::select! {
            msg = rx.recv() => match msg {
                Some(WriterMsg::Entry(boxed)) => {
                    let pending = *boxed;
                    match build_and_sign(
                        pending.seq,
                        pending.parent_seq,
                        pending.span,
                        &state.prev_hmac,
                        &state.session_key,
                    ) {
                        Ok(entry) => {
                            state.prev_hmac.clone_from(&entry.hmac_self);
                            buffer.push(entry);
                            if flush_deadline.is_none() {
                                flush_deadline =
                                    Some(tokio::time::Instant::now() + FLUSH_INTERVAL);
                            }
                            if buffer.len() >= FLUSH_BATCH {
                                flush_buffer(&mut state.file, &mut buffer, &state.active_path)
                                    .await;
                                flush_deadline = None;
                            }
                        }
                        Err(e) => {
                            warn!(target: "turnlog", "failed to sign entry: {e}");
                        }
                    }
                }
                Some(WriterMsg::Flush(ack)) => {
                    flush_buffer(&mut state.file, &mut buffer, &state.active_path).await;
                    flush_deadline = None;
                    let _ = ack.send(());
                }
                Some(WriterMsg::Shutdown(ack)) => {
                    flush_buffer(&mut state.file, &mut buffer, &state.active_path).await;
                    let result = finalise(&state.active_path, &state.ended_path).await;
                    let _ = ack.send(result);
                    break;
                }
                None => {
                    // All senders dropped — flush whatever is buffered and exit.
                    flush_buffer(&mut state.file, &mut buffer, &state.active_path).await;
                    break;
                }
            },
            () = &mut sleep_until, if flush_deadline.is_some() => {
                flush_buffer(&mut state.file, &mut buffer, &state.active_path).await;
                flush_deadline = None;
            }
        }
    }
}

async fn flush_buffer(
    file: &mut BufWriter<File>,
    buffer: &mut Vec<TurnEntry>,
    path: &std::path::Path,
) {
    for entry in buffer.drain(..) {
        match serde_json::to_string(&entry) {
            Ok(line) => {
                if let Err(e) = file.write_all(line.as_bytes()).await {
                    warn!(target: "turnlog", "write failed for {}: {e}", path.display());
                    continue;
                }
                if let Err(e) = file.write_all(b"\n").await {
                    warn!(target: "turnlog", "newline write failed for {}: {e}", path.display());
                }
            }
            Err(e) => {
                warn!(target: "turnlog", "serialize failed at seq {}: {e}", entry.seq);
            }
        }
    }
    if let Err(e) = file.flush().await {
        warn!(target: "turnlog", "buffer flush failed for {}: {e}", path.display());
        return;
    }
    if let Err(e) = file.get_mut().sync_data().await {
        warn!(target: "turnlog", "fdatasync failed for {}: {e}", path.display());
    }
}

async fn finalise(active: &std::path::Path, ended: &std::path::Path) -> Result<()> {
    if let Some(parent) = ended.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| TurnLogError::io(parent, e))?;
    }
    tokio::fs::rename(active, ended)
        .await
        .map_err(|e| TurnLogError::io(active, e))
}

/// Construct a lifecycle or writer-internal span for the given session.
///
/// The span's actor is always `claude` (the turnlog writer is Claude's
/// persistence layer). Outcome is `Continue` for all lifecycle entries.
fn make_span(session_id: &str, action: &str, metadata: serde_json::Value) -> TraceSpan {
    // SAFETY: outcome is always set here — TraceContext::finish() only fails
    // when outcome is None, which cannot happen with our explicit `.outcome()` call.
    // If construction somehow fails, we log and produce a minimal fallback span
    // rather than panicking on the hot path.
    TraceContext::new(Actor::claude(), action)
        .session_id(session_id)
        .outcome(TraceOutcome::Continue)
        .metadata(metadata)
        .finish()
        .unwrap_or_else(|e| {
            warn!(target: "turnlog", "span construction failed for {action}: {e}; using fallback");
            // Construct a minimal span directly when the builder fails (should never happen).
            TraceSpan {
                id: uuid::Uuid::new_v4(),
                parent_id: None,
                session_id: Some(session_id.to_owned()),
                actor: Actor::claude(),
                action: action.to_owned(),
                timestamp: chrono::Utc::now(),
                duration_ms: 0,
                decision_points: Vec::new(),
                strand_activations: Vec::new(),
                outcome: TraceOutcome::Continue,
                metadata: serde_json::Value::Null,
            }
        })
}

fn hash_project_root(p: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    // 6-char shortened SHA-256 — sha2 is already in the transitive tree via
    // lightarchitects-crypto, so no extra dependency is added.
    let mut h = Sha256::new();
    h.update(p.to_string_lossy().as_bytes());
    let out = h.finalize();
    let mut hex = String::with_capacity(12);
    for b in &out[..6] {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

// ── Tests ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn test_pepper() -> SecretSlice<u8> {
        SecretSlice::from(vec![0xA5_u8; 32])
    }

    fn user_span(session_id: &str, content: &str) -> TraceSpan {
        make_span(
            session_id,
            "turn.user",
            serde_json::json!({ "content": content }),
        )
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn open_creates_genesis_and_writes_session_start() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        let writer = TurnLogWriter::open(
            &layout,
            "sess-1".to_owned(),
            PathBuf::from("/p"),
            "model".to_owned(),
            "prov".to_owned(),
            None,
            &test_pepper(),
        )
        .await
        .unwrap();

        writer.close(EndReason::Complete).await.unwrap();

        assert!(layout.genesis_path("sess-1").is_file());
        // Closed file must be under ended/{date}/, not active/.
        let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        assert!(layout.ended_path("sess-1", &date).is_file());
        assert!(!layout.active_path("sess-1").is_file());
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn genesis_carries_pepper_fingerprint() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        let pepper = test_pepper();
        let writer = TurnLogWriter::open(
            &layout,
            "sess-fp".to_owned(),
            PathBuf::from("/p"),
            "m".to_owned(),
            "p".to_owned(),
            None,
            &pepper,
        )
        .await
        .unwrap();
        writer.close(EndReason::Complete).await.unwrap();

        let genesis_bytes = tokio::fs::read(layout.genesis_path("sess-fp"))
            .await
            .unwrap();
        let genesis: GenesisBlock = serde_json::from_slice(&genesis_bytes).unwrap();
        assert!(!genesis.pepper_fingerprint.is_empty());
        assert_eq!(genesis.pepper_fingerprint.len(), 16);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn append_and_close_writes_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        let writer = TurnLogWriter::open(
            &layout,
            "sess-2".to_owned(),
            PathBuf::from("/p"),
            "m".to_owned(),
            "p".to_owned(),
            None,
            &test_pepper(),
        )
        .await
        .unwrap();

        for i in 0..5 {
            writer.append(user_span("sess-2", &format!("msg-{i}")));
        }
        writer.close(EndReason::UserExit).await.unwrap();

        let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let content = tokio::fs::read_to_string(layout.ended_path("sess-2", &date))
            .await
            .unwrap();
        let lines: Vec<_> = content.lines().collect();
        // 1 session_start + 5 turn.user + 1 session_ended
        assert_eq!(lines.len(), 7);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn pause_leaves_file_in_active() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        let writer = TurnLogWriter::open(
            &layout,
            "sess-3".to_owned(),
            PathBuf::from("/p"),
            "m".to_owned(),
            "p".to_owned(),
            None,
            &test_pepper(),
        )
        .await
        .unwrap();

        writer.append(user_span("sess-3", "hello"));
        writer.pause("working on X".to_owned(), 5.0).await.unwrap();

        assert!(layout.active_path("sess-3").is_file());

        // Still need to close for clean shutdown.
        writer.close(EndReason::UserExit).await.unwrap();
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn end_reason_serialises_as_snake_case() {
        let r = EndReason::UserExit;
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, r#""user_exit""#);
    }
}
