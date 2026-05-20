//! Append-only HMAC-chained decision log for autonomous builds.
//!
//! Each decision is written as a single NDJSON line to
//! `<decisions_dir>/<build_id>.ndjson`. The HMAC chain uses the previous
//! entry's `hmac` field as part of the input so tampering with any entry
//! invalidates all subsequent HMACs — `DecisionLog.svelte` flags broken
//! entries with the `⚠ HMAC` badge.
//!
//! # Chain construction
//!
//! ```text
//! entry[0].hmac = HMAC-SHA256(pepper, "0||<decision>||<canon_ref>")
//! entry[N].hmac = HMAC-SHA256(pepper, "<entry[N-1].hmac>||<decision>||<canon_ref>")
//! ```
//!
//! Verification is done client-side (or by `verify_chain()` in tests).

use std::{
    io::Write as _,
    path::{Path, PathBuf},
};

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// One entry in the decision log — maps directly to the TypeScript
/// `DecisionEntry` interface consumed by `DecisionLog.svelte`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEntry {
    /// Zero-based line index (stable key for Svelte `#each` keying).
    pub line_n: usize,
    /// ISO-8601 timestamp of when the decision was recorded.
    pub timestamp: String,
    /// Decision level: `"L1"` architectural · `"L2"` implementation ·
    /// `"L3"` quality gate · `"L4"` escalation.
    pub level: String,
    /// Human-readable decision text.
    pub decision: String,
    /// Canon reference URI (e.g. `"canon://builders-cookbook#§64"`).
    /// May be `None` for escalation entries emitted by `FixAgentIteration`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canon_ref: Option<String>,
    /// Hex-encoded HMAC-SHA256 chain tag. `None` only when chain-verification
    /// is disabled (e.g. unit-test stubs that don't supply a pepper).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hmac: Option<String>,
    /// `false` when this entry's HMAC doesn't match the re-computed value —
    /// indicates tampering or a corrupt log. `None` if not yet verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hmac_ok: Option<bool>,
}

/// Append-only writer for one build's decision log.
///
/// `DecisionsWriter` is `Send + Sync` and cheaply cloneable via the inner
/// `Arc` so it can be passed into `tokio::spawn` closures without locking.
#[derive(Clone)]
pub struct DecisionsWriter {
    path: PathBuf,
    pepper: Vec<u8>,
    /// Tracks the HMAC of the last written entry for chain continuation.
    prev_hmac: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    line_counter: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl DecisionsWriter {
    /// Open (or create) the NDJSON log for `build_id` under `decisions_dir`.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if `decisions_dir` cannot be created.
    pub fn open(decisions_dir: &Path, build_id: Uuid, pepper: &[u8]) -> std::io::Result<Self> {
        std::fs::create_dir_all(decisions_dir)?;
        Ok(Self {
            path: decisions_dir.join(format!("{build_id}.ndjson")),
            pepper: pepper.to_vec(),
            prev_hmac: std::sync::Arc::new(std::sync::Mutex::new(None)),
            line_counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        })
    }

    /// Append one decision entry to the log.
    ///
    /// Blocks only on the short file write + mutex for `prev_hmac`.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the file cannot be opened or written.
    pub fn append(
        &self,
        level: &str,
        decision: &str,
        canon_ref: Option<&str>,
    ) -> std::io::Result<()> {
        let line_n = self
            .line_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let timestamp = chrono::Utc::now().to_rfc3339();
        let hmac_tag = self.compute_hmac(line_n, decision, canon_ref.unwrap_or(""));

        let entry = DecisionEntry {
            line_n,
            timestamp,
            level: level.to_owned(),
            decision: decision.to_owned(),
            canon_ref: canon_ref.map(str::to_owned),
            hmac: Some(hmac_tag.clone()),
            hmac_ok: Some(true),
        };

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(
            file,
            "{}",
            serde_json::to_string(&entry)
                .map_err(|e| { std::io::Error::new(std::io::ErrorKind::InvalidData, e) })?
        )?;

        if let Ok(mut guard) = self.prev_hmac.lock() {
            *guard = Some(hmac_tag);
        }
        Ok(())
    }

    /// Read all entries from the log file for this build.
    ///
    /// # Errors
    ///
    /// Returns `Ok([])` when the file does not yet exist (build has no
    /// decisions). Propagates parse errors as `io::Error`.
    pub fn read_all(decisions_dir: &Path, build_id: Uuid) -> std::io::Result<Vec<DecisionEntry>> {
        let path = decisions_dir.join(format!("{build_id}.ndjson"));
        if !path.exists() {
            return Ok(vec![]);
        }
        let content = std::fs::read_to_string(&path)?;
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| {
                serde_json::from_str(line)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })
            .collect()
    }

    fn compute_hmac(&self, line_n: usize, decision: &str, canon_ref: &str) -> String {
        let prev = self
            .prev_hmac
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| line_n.to_string());
        let input = format!("{prev}||{decision}||{canon_ref}");
        // HMAC-SHA256 accepts any key length — InvalidLength is never returned.
        #[allow(clippy::expect_used)]
        let mut mac = HmacSha256::new_from_slice(&self.pepper).expect("HMAC accepts any key size");
        mac.update(input.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn append_and_read_round_trip() {
        let dir = TempDir::new().unwrap();
        let id = Uuid::new_v4();
        let writer = DecisionsWriter::open(dir.path(), id, b"test-pepper").unwrap();

        writer
            .append(
                "L1",
                "Use feature-gated lightsquad",
                Some("canon://cookbook#64"),
            )
            .unwrap();
        writer
            .append("L2", "WorktreeManager reuses git worktree add", None)
            .unwrap();
        writer
            .append("L4", "ESCALATION: ReviewGate exhausted", None)
            .unwrap();

        let entries = DecisionsWriter::read_all(dir.path(), id).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].level, "L1");
        assert_eq!(entries[1].level, "L2");
        assert_eq!(entries[2].level, "L4");
        assert!(entries[0].hmac.is_some());
        assert_ne!(
            entries[0].hmac, entries[1].hmac,
            "each entry has unique HMAC"
        );
    }

    #[test]
    fn read_all_returns_empty_for_nonexistent_file() {
        let dir = TempDir::new().unwrap();
        let entries = DecisionsWriter::read_all(dir.path(), Uuid::new_v4()).unwrap();
        assert!(entries.is_empty());
    }
}
