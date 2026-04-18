//! Read API over the turnlog: session enumeration, entry streaming, chain verification.
//!
//! Week 1 scope is minimal — `open_session`, `load_genesis`, `read_all` — enough to
//! roundtrip-test against the writer and to let Week 2's projection layer iterate.

use std::path::PathBuf;

use secrecy::SecretSlice;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::turnlog::chain::{GenesisBlock, derive_session_key, pepper_fingerprint, verify_chain};
use crate::turnlog::entry::TurnEntry;
use crate::turnlog::error::{Result, TurnLogError};
use crate::turnlog::store::StoreLayout;

/// A session candidate for resumption, enriched with Ebbinghaus-decayed weight.
///
/// Returned by [`TurnLogReader::scan_resumable`] — callers present these to the
/// user as a ranked list of prior sessions, sorted by `decayed_weight` descending.
#[derive(Debug, Clone)]
pub struct ResumableSession {
    /// Session UUID string.
    pub session_id: String,
    /// The compaction summary or user-provided memo written at pause time.
    pub memo_body: String,
    /// Raw weight recorded at pause time (typically `1.0`).
    pub raw_weight: f64,
    /// UTC timestamp of the `session_paused` span.
    pub paused_at: chrono::DateTime<chrono::Utc>,
    /// `raw_weight × 0.92^days_elapsed` — reflects memory decay since pause.
    pub decayed_weight: f64,
}

/// Summary metadata for a turnlog session, returned by [`TurnLogReader::list_sessions`].
#[derive(Debug, Clone)]
pub struct SessionSummary {
    /// Session UUID string.
    pub session_id: String,
    /// ISO 8601 timestamp from the genesis block, if available.
    pub created_at: Option<String>,
    /// Total number of entries in the session file.
    pub entry_count: usize,
    /// Action string of the last entry (e.g. `"session_ended"`, `"session_paused"`).
    pub last_action: String,
    /// `true` if the session file is in `active/`, `false` if in `ended/`.
    pub is_active: bool,
    /// Ebbinghaus-decayed weight if a `session_paused` entry was found.
    pub paused_weight: Option<f64>,
}

/// Read-side handle over a [`StoreLayout`].
#[derive(Debug, Clone)]
pub struct TurnLogReader {
    layout: StoreLayout,
}

impl TurnLogReader {
    /// Construct a reader over the given layout.
    #[must_use]
    pub fn new(layout: StoreLayout) -> Self {
        Self { layout }
    }

    /// Layout this reader operates on.
    #[must_use]
    pub fn layout(&self) -> &StoreLayout {
        &self.layout
    }

    /// Load the genesis block for a session.
    ///
    /// # Errors
    /// * [`TurnLogError::MissingGenesis`] if the genesis file is absent.
    /// * [`TurnLogError::Io`] on other filesystem failures.
    /// * [`TurnLogError::Serialize`] if the genesis is malformed JSON.
    pub async fn load_genesis(&self, session_id: &str) -> Result<GenesisBlock> {
        let path = self.layout.genesis_path(session_id);
        if !path.is_file() {
            return Err(TurnLogError::MissingGenesis(session_id.to_owned()));
        }
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| TurnLogError::io(&path, e))?;
        let genesis: GenesisBlock = serde_json::from_slice(&bytes)?;
        Ok(genesis)
    }

    /// Read every entry for a session in seq order — active or ended.
    ///
    /// Does **not** verify the HMAC chain. Use [`Self::read_all_verified`]
    /// for that.
    ///
    /// # Errors
    /// * [`TurnLogError::SessionNotFound`] if neither active nor ended file exists.
    pub async fn read_all(&self, session_id: &str) -> Result<Vec<TurnEntry>> {
        let path = self.find_log_file(session_id)?;
        let file = tokio::fs::File::open(&path)
            .await
            .map_err(|e| TurnLogError::io(&path, e))?;
        let mut lines = BufReader::new(file).lines();
        let mut out = Vec::new();
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| TurnLogError::io(&path, e))?
        {
            if line.is_empty() {
                continue;
            }
            let entry: TurnEntry = serde_json::from_str(&line)?;
            out.push(entry);
        }
        Ok(out)
    }

    /// Read every entry for a session AND verify the HMAC chain against the
    /// store-level pepper.
    ///
    /// Also checks the pepper fingerprint in the genesis block matches the
    /// loaded pepper — returns [`TurnLogError::PepperMismatch`] if the store
    /// was re-keyed after this session was written.
    ///
    /// Returns the last-verified seq on success.
    ///
    /// # Errors
    /// All [`Self::read_all`] errors plus:
    /// * [`TurnLogError::PepperMismatch`] if the pepper has changed.
    /// * [`TurnLogError::ChainBroken`] at the first inconsistent entry.
    pub async fn read_all_verified(
        &self,
        session_id: &str,
        pepper: &SecretSlice<u8>,
    ) -> Result<(Vec<TurnEntry>, u64)> {
        let genesis = self.load_genesis(session_id).await?;

        // Fingerprint check — fast failure before touching the full chain.
        let loaded_fp = pepper_fingerprint(pepper)?;
        if !genesis.pepper_fingerprint.is_empty() && loaded_fp != genesis.pepper_fingerprint {
            return Err(TurnLogError::PepperMismatch {
                session_id: session_id.to_owned(),
                genesis_fp: genesis.pepper_fingerprint.clone(),
                loaded_fp,
            });
        }

        let session_key = derive_session_key(pepper, &genesis.hkdf_salt, session_id)?;
        let entries = self.read_all(session_id).await?;
        let last_seq = verify_chain(&genesis, entries.iter().cloned(), &session_key)?;
        Ok((entries, last_seq))
    }

    /// List session_ids of every file currently in `active/` — the crash-recovery
    /// candidate set. A session here is either live (writer still running) or
    /// abandoned (process exited without calling `close()`).
    ///
    /// # Errors
    /// [`TurnLogError::Io`] if the active directory cannot be read.
    pub async fn list_active(&self) -> Result<Vec<String>> {
        let dir = self.layout.active_dir();
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .map_err(|e| TurnLogError::io(&dir, e))?;
        let mut out = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| TurnLogError::io(&dir, e))?
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("ndjson") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            out.push(stem.to_owned());
        }
        Ok(out)
    }

    /// List date directories under `ended/`, sorted newest first.
    ///
    /// Each directory name is a `YYYY-MM-DD` date string. Returns an empty
    /// `Vec` if the `ended/` directory does not exist.
    ///
    /// # Errors
    /// [`TurnLogError::Io`] if the `ended/` directory exists but cannot be read.
    pub async fn list_ended_dates(&self) -> Result<Vec<String>> {
        let ended_root = self.layout.root().join("ended");
        if !ended_root.is_dir() {
            return Ok(Vec::new());
        }
        let mut entries = tokio::fs::read_dir(&ended_root)
            .await
            .map_err(|e| TurnLogError::io(&ended_root, e))?;
        let mut dates = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| TurnLogError::io(&ended_root, e))?
        {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            // Only include directories that look like YYYY-MM-DD dates.
            if name.len() == 10
                && name.chars().nth(4) == Some('-')
                && name.chars().nth(7) == Some('-')
            {
                dates.push(name.to_owned());
            }
        }
        dates.sort();
        dates.reverse();
        Ok(dates)
    }

    /// List ended session IDs within a date directory.
    ///
    /// Returns the file stems (session IDs) of all `.ndjson` files found under
    /// `ended/{date}/`. Returns an empty `Vec` if the date directory does not exist.
    ///
    /// # Errors
    /// [`TurnLogError::Io`] if the date directory exists but cannot be read.
    pub async fn list_ended_sessions(&self, date: &str) -> Result<Vec<String>> {
        let date_dir = self.layout.ended_dir(date);
        if !date_dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut entries = tokio::fs::read_dir(&date_dir)
            .await
            .map_err(|e| TurnLogError::io(&date_dir, e))?;
        let mut out = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| TurnLogError::io(&date_dir, e))?
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("ndjson") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            out.push(stem.to_owned());
        }
        Ok(out)
    }

    /// Enumerate recent sessions (active + ended) and return summary metadata.
    ///
    /// Scans `active/` first, then the last `days` days of `ended/` directories.
    /// For each session file, reads entries to extract metadata (entry count,
    /// last action, paused weight). Sessions that cannot be parsed are silently
    /// skipped — this is a best-effort enumeration.
    ///
    /// Results are sorted by `created_at` descending (most recent first).
    ///
    /// # Errors
    /// [`TurnLogError::Io`] if the root directories cannot be enumerated.
    pub async fn list_sessions(&self, days: u64) -> Result<Vec<SessionSummary>> {
        let mut summaries = Vec::new();

        // 1. Active sessions.
        let active_ids = self.list_active().await?;
        for id in &active_ids {
            if let Some(summary) = self.summarize_session(id, true).await {
                summaries.push(summary);
            }
        }

        // 2. Ended sessions from the last N days.
        let today = chrono::Utc::now();
        for day in 0..=days {
            let Some(date_dt) = today.checked_sub_signed(chrono::Duration::days(
                i64::try_from(day).unwrap_or(i64::MAX),
            )) else {
                continue;
            };
            let date = date_dt.format("%Y-%m-%d").to_string();
            let ended_ids = self.list_ended_sessions(&date).await?;
            for id in &ended_ids {
                if let Some(summary) = self.summarize_session(id, false).await {
                    summaries.push(summary);
                }
            }
        }

        // Sort by created_at descending (most recent first).
        summaries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(summaries)
    }

    /// Build a [`SessionSummary`] for a single session by reading its entries.
    ///
    /// Silently returns `None` if the session file cannot be found or parsed.
    /// Attempts to load the genesis block for `created_at`; falls back to the
    /// first entry's timestamp if genesis is unavailable.
    async fn summarize_session(&self, session_id: &str, is_active: bool) -> Option<SessionSummary> {
        const EBBINGHAUS: f64 = 0.92;

        let entries = self.read_all(session_id).await.ok()?;
        if entries.is_empty() {
            return None;
        }

        let last_entry = entries.last()?;
        let last_action = last_entry.span.action.clone();

        // Try genesis block for created_at, fall back to first entry's timestamp.
        let created_at = self
            .load_genesis(session_id)
            .await
            .ok()
            .map(|g| g.created_at);

        // Check for session_paused entry to extract decayed weight.
        let paused_weight = entries
            .iter()
            .rev()
            .find(|e| e.kind() == crate::turnlog::entry::EntryKind::SessionPaused)
            .map(|e| {
                let raw = e
                    .span
                    .metadata
                    .get("memo_weight")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(1.0);
                let paused_at = e.span.timestamp;
                let now = chrono::Utc::now();
                #[allow(clippy::cast_precision_loss)]
                let days_elapsed = (now - paused_at).num_days().max(0) as f64;
                raw * EBBINGHAUS.powf(days_elapsed)
            });

        Some(SessionSummary {
            session_id: session_id.to_owned(),
            created_at,
            entry_count: entries.len(),
            last_action,
            is_active,
            paused_weight,
        })
    }

    /// Scan `active/` for sessions that contain a `session_paused` entry.
    ///
    /// Reads at most `cap` session files. For each one that has a
    /// `session_paused` span, the `memo_body` and `memo_weight` are extracted
    /// and the weight is decayed with the Ebbinghaus factor `0.92` per elapsed
    /// calendar day since the pause timestamp.
    ///
    /// Returns sessions sorted by `decayed_weight` descending — the most
    /// contextually relevant candidate first.
    ///
    /// Sessions that cannot be parsed (truncated, permission error) are silently
    /// skipped; the caller receives a best-effort ranked list.
    ///
    /// # Errors
    /// [`TurnLogError::Io`] if the `active/` directory itself cannot be read.
    pub async fn scan_resumable(&self, cap: usize) -> Result<Vec<ResumableSession>> {
        const EBBINGHAUS: f64 = 0.92;

        let mut ids = self.list_active().await?;
        ids.truncate(cap);

        let now = chrono::Utc::now();
        let mut out = Vec::with_capacity(ids.len());

        for id in &ids {
            let Ok(entries) = self.read_all(id).await else {
                continue;
            };

            // Most recent session_paused entry wins (handles multiple compactions).
            let Some(paused) = entries
                .iter()
                .rev()
                .find(|e| e.kind() == crate::turnlog::entry::EntryKind::SessionPaused)
            else {
                continue;
            };

            let meta = &paused.span.metadata;
            let memo_body = meta
                .get("memo_body")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned();
            let raw_weight = meta
                .get("memo_weight")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(1.0);
            let paused_at = paused.span.timestamp;

            #[allow(clippy::cast_precision_loss)]
            let days_elapsed = (now - paused_at).num_days().max(0) as f64;
            let decayed_weight = raw_weight * EBBINGHAUS.powf(days_elapsed);

            out.push(ResumableSession {
                session_id: id.clone(),
                memo_body,
                raw_weight,
                paused_at,
                decayed_weight,
            });
        }

        out.sort_by(|a, b| {
            b.decayed_weight
                .partial_cmp(&a.decayed_weight)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(out)
    }

    fn find_log_file(&self, session_id: &str) -> Result<PathBuf> {
        let active = self.layout.active_path(session_id);
        if active.is_file() {
            return Ok(active);
        }
        // Search recent ended dates — today + yesterday is fine for Week 1.
        let today = chrono::Utc::now();
        for day in 0..=2 {
            let Some(date_dt) = today.checked_sub_signed(chrono::Duration::days(day)) else {
                continue;
            };
            let date = date_dt.format("%Y-%m-%d").to_string();
            let ended = self.layout.ended_path(session_id, &date);
            if ended.is_file() {
                return Ok(ended);
            }
        }
        Err(TurnLogError::SessionNotFound(session_id.to_owned()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use crate::ayin::span::{Actor, TraceContext, TraceOutcome};

    use super::*;
    use crate::turnlog::writer::{EndReason, TurnLogWriter};

    fn test_pepper() -> SecretSlice<u8> {
        SecretSlice::from(vec![0xA5_u8; 32])
    }

    fn user_span(session_id: &str, msg: &str) -> crate::ayin::span::TraceSpan {
        TraceContext::new(Actor::claude(), "turn.user")
            .session_id(session_id)
            .outcome(TraceOutcome::Continue)
            .metadata(serde_json::json!({ "content": msg }))
            .finish()
            .expect("span must build")
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn roundtrip_write_and_verify() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        let writer = TurnLogWriter::open(
            &layout,
            "r-sess-1".to_owned(),
            PathBuf::from("/p"),
            "m".to_owned(),
            "p".to_owned(),
            None,
            &test_pepper(),
        )
        .await
        .unwrap();

        for i in 0..10 {
            writer.append(user_span("r-sess-1", &format!("msg-{i}")));
        }
        writer.close(EndReason::Complete).await.unwrap();

        let reader = TurnLogReader::new(layout);
        let (entries, last_seq) = reader
            .read_all_verified("r-sess-1", &test_pepper())
            .await
            .unwrap();

        // 1 session_start + 10 turn.user + 1 session_ended = 12
        assert_eq!(entries.len(), 12);
        assert_eq!(last_seq, 11);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn pepper_mismatch_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        let pepper_a = test_pepper();
        let writer = TurnLogWriter::open(
            &layout,
            "r-sess-pm".to_owned(),
            PathBuf::from("/p"),
            "m".to_owned(),
            "p".to_owned(),
            None,
            &pepper_a,
        )
        .await
        .unwrap();
        writer.close(EndReason::Complete).await.unwrap();

        // Try to verify with a different pepper.
        let pepper_b = SecretSlice::from(vec![0xBE_u8; 32]);
        let reader = TurnLogReader::new(layout);
        let err = reader
            .read_all_verified("r-sess-pm", &pepper_b)
            .await
            .unwrap_err();
        assert!(
            matches!(err, TurnLogError::PepperMismatch { .. }),
            "expected PepperMismatch, got {err:?}"
        );
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn list_active_surfaces_unclean_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        let writer = TurnLogWriter::open(
            &layout,
            "r-sess-2".to_owned(),
            PathBuf::from("/p"),
            "m".to_owned(),
            "p".to_owned(),
            None,
            &test_pepper(),
        )
        .await
        .unwrap();
        writer.append(user_span("r-sess-2", "pending"));
        writer.flush().await.unwrap();
        drop(writer);

        let reader = TurnLogReader::new(layout);
        let active = reader.list_active().await.unwrap();
        assert!(active.contains(&"r-sess-2".to_owned()));
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn load_genesis_errors_for_missing_session() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        layout.ensure_dirs().await.unwrap();
        let reader = TurnLogReader::new(layout);
        let err = reader.load_genesis("nope").await.unwrap_err();
        assert!(matches!(err, TurnLogError::MissingGenesis(_)));
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn list_sessions_returns_active_and_ended() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        layout.ensure_dirs().await.unwrap();

        // Write an active session.
        let w = TurnLogWriter::open(
            &layout,
            "ls-active".to_owned(),
            PathBuf::from("/p"),
            "m".to_owned(),
            "p".to_owned(),
            None,
            &test_pepper(),
        )
        .await
        .unwrap();
        w.append(user_span("ls-active", "hello"));
        w.flush().await.unwrap();
        drop(w); // Leave in active/ (not closed).

        // Write an ended session for today.
        let w2 = TurnLogWriter::open(
            &layout,
            "ls-ended".to_owned(),
            PathBuf::from("/p"),
            "m".to_owned(),
            "p".to_owned(),
            None,
            &test_pepper(),
        )
        .await
        .unwrap();
        w2.append(user_span("ls-ended", "world"));
        w2.close(EndReason::Complete).await.unwrap();

        let reader = TurnLogReader::new(layout);
        let sessions = reader.list_sessions(1).await.unwrap();

        // Both sessions should appear.
        let ids: Vec<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert!(
            ids.contains(&"ls-active"),
            "active session missing: {ids:?}"
        );
        assert!(ids.contains(&"ls-ended"), "ended session missing: {ids:?}");

        // Verify metadata.
        let active = sessions
            .iter()
            .find(|s| s.session_id == "ls-active")
            .unwrap();
        assert!(active.is_active);
        assert!(active.entry_count >= 2); // session_start + turn.user
        assert_eq!(active.last_action, "turn.user");

        let ended = sessions
            .iter()
            .find(|s| s.session_id == "ls-ended")
            .unwrap();
        assert!(!ended.is_active);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn list_sessions_empty_on_no_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        // Don't create dirs — list_sessions should return empty.
        let reader = TurnLogReader::new(layout);
        let sessions = reader.list_sessions(7).await.unwrap();
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn list_ended_dates_returns_sorted_dates() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        layout.ensure_dirs().await.unwrap();

        // Create date directories manually.
        tokio::fs::create_dir_all(layout.ended_dir("2026-04-10"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(layout.ended_dir("2026-04-15"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(layout.ended_dir("2026-04-12"))
            .await
            .unwrap();

        let reader = TurnLogReader::new(layout);
        let dates = reader.list_ended_dates().await.unwrap();
        assert_eq!(dates, vec!["2026-04-15", "2026-04-12", "2026-04-10"]);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn list_ended_sessions_returns_ndjson_stems() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        layout.ensure_dirs().await.unwrap();

        let date_dir = layout.ended_dir("2026-04-15");
        tokio::fs::create_dir_all(&date_dir).await.unwrap();
        // Create a fake ended session file.
        tokio::fs::write(date_dir.join("test-sess.ndjson"), "{}\n")
            .await
            .unwrap();
        // Non-ndjson file should be ignored.
        tokio::fs::write(date_dir.join("ignore-me.txt"), "hello")
            .await
            .unwrap();

        let reader = TurnLogReader::new(layout);
        let sessions = reader.list_ended_sessions("2026-04-15").await.unwrap();
        assert_eq!(sessions, vec!["test-sess".to_owned()]);
    }
}
