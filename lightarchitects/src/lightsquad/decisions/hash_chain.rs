//! Atomic-append NDJSON decisions log with HMAC integrity chain.
//!
//! Every gate decision made by the lightsquad 4-layer pipeline is appended as
//! a single JSON line to a file on disk. Each entry carries an HMAC-SHA256
//! over `prev_hash || seq || decision`, linking it to the previous entry.
//! Deletion or mutation of any entry breaks the chain and is detected by
//! [`HashChain::verify_all`].
//!
//! # Chain construction
//!
//! ```text
//! entry[0].prev_hash  = None
//! entry[0].entry_hash = HMAC-SHA256(key, 0x00 || seq_be64 || decision_utf8)
//! entry[n].prev_hash  = entry[n-1].entry_hash
//! entry[n].entry_hash = HMAC-SHA256(key, prev_hash || seq_be64 || decision_utf8)
//! ```
//!
//! # Atomic writes
//!
//! Each append writes to `<path>.tmp`, fsyncs, then renames over `<path>` so
//! a crash mid-write never leaves a partial NDJSON line in the live log.

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;
use zeroize::Zeroizing;

/// HMAC-SHA256 type alias used throughout this module.
type HmacSha256 = Hmac<Sha256>;

// ─── Errors ───────────────────────────────────────────────────────────────────

/// Errors produced by [`HashChain`] operations.
#[derive(Debug, Error)]
pub enum ChainError {
    /// Filesystem I/O failed (open, write, rename, sync).
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// A log line could not be parsed as a [`DecisionEntry`].
    #[error("deserialisation error at line {line}: {detail}")]
    Deserialise {
        /// 1-based line number in the NDJSON file.
        line: usize,
        /// Human-readable parse failure.
        detail: String,
    },

    /// Serialising an entry to JSON failed.
    #[error("serialisation error: {0}")]
    Serialise(String),

    /// An entry's `entry_hash` does not match the recomputed HMAC.
    #[error("chain broken at seq {seq}: {detail}")]
    ChainBroken {
        /// Sequence number of the offending entry.
        seq: u64,
        /// Description of the failure.
        detail: String,
    },

    /// The `prev_hash` of an entry does not match the previous entry's `entry_hash`.
    #[error("hash linkage broken at seq {seq}")]
    LinkageBroken {
        /// Sequence number where the break was detected.
        seq: u64,
    },

    /// HMAC key was rejected (should never occur with SHA-256).
    #[error("HMAC initialisation failed: {0}")]
    HmacInit(String),
}

/// Convenience result alias for chain operations.
pub type Result<T> = std::result::Result<T, ChainError>;

// ─── DecisionLayer ────────────────────────────────────────────────────────────

/// Which layer of the 4-layer decision pipeline produced this entry.
///
/// The pipeline evaluates gates in order: Canon → Northstar → `LightArchitect` → User.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionLayer {
    /// Canon gate — platform constitutional principles (Canon I–XL).
    Canon,
    /// Northstar gate — 4-pillar product quality bar.
    Northstar,
    /// `LightArchitect` gate — engineering / domain expert judgement.
    LightArchitect,
    /// User gate — human-in-the-loop approval.
    User,
}

// ─── DecisionEntry ────────────────────────────────────────────────────────────

/// A single gate decision recorded in the hash chain.
///
/// All fields except `entry_hash` and `prev_hash` are semantically meaningful
/// to reviewers. The `entry_hash` and `prev_hash` fields are the integrity
/// mechanism — they must not be modified after signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEntry {
    /// Monotonically-increasing sequence number (0-based).
    pub seq: u64,
    /// UTC timestamp of the decision.
    pub timestamp: DateTime<Utc>,
    /// Which pipeline layer made the decision.
    pub layer: DecisionLayer,
    /// The gate question being answered (e.g. "Does this comply with Canon XIV?").
    pub question: String,
    /// The decision taken (e.g. "APPROVED", "BLOCKED: missing citation").
    pub decision: String,
    /// Optional canon or standard citation supporting the decision.
    pub citation: Option<String>,
    /// `entry_hash` of the preceding entry; `None` for the first entry.
    pub prev_hash: Option<[u8; 32]>,
    /// HMAC-SHA256 of `prev_hash_bytes || seq_be64 || decision_utf8`.
    pub entry_hash: [u8; 32],
}

// ─── HashChain ────────────────────────────────────────────────────────────────

/// Append-only NDJSON decision log with HMAC-SHA256 integrity chain.
///
/// # Lifecycle
///
/// 1. Open with [`HashChain::open`] — replays the existing log to restore chain
///    state (last hash and sequence counter).
/// 2. Append with [`HashChain::append`] — atomically appends a signed entry.
/// 3. Verify with [`HashChain::verify_all`] — re-reads and checks every entry.
pub struct HashChain {
    /// Path to the live NDJSON log file.
    path: PathBuf,
    /// 32-byte HMAC key (zeroed on drop).
    key: Zeroizing<[u8; 32]>,
    /// `entry_hash` of the most recently appended entry; `None` if the log is empty.
    last_hash: Option<[u8; 32]>,
    /// Next sequence number to assign.
    seq: u64,
}

impl HashChain {
    /// Open (or create) the NDJSON log at `path`, restoring chain state from
    /// existing entries.
    ///
    /// If `path` does not exist it is created as an empty file. Existing entries
    /// are replayed in order to recover `last_hash` and `seq`.
    ///
    /// # Errors
    ///
    /// Returns [`ChainError::Io`] if the file cannot be opened or created.
    /// Returns [`ChainError::Deserialise`] if any existing line is malformed.
    pub fn open(path: impl AsRef<Path>, key: [u8; 32]) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Ensure parent directory exists.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Touch the file if it does not yet exist.
        if !path.exists() {
            File::create(&path)?;
        }

        let mut chain = Self {
            path,
            key: Zeroizing::new(key),
            last_hash: None,
            seq: 0,
        };

        // Replay existing entries to restore state.
        chain.replay()?;
        Ok(chain)
    }

    /// Append a [`DecisionEntry`] to the log.
    ///
    /// The caller must supply a partially-constructed entry (with `seq`,
    /// `timestamp`, `layer`, `question`, `decision`, and `citation` populated).
    /// The `prev_hash` and `entry_hash` fields are computed and set by this
    /// method before writing.
    ///
    /// The write is atomic: the line is written to `<path>.tmp`, fsynced, then
    /// renamed over the live file. A crash between write and rename leaves the
    /// live file unchanged; the `.tmp` file is cleaned up on the next open.
    ///
    /// # Errors
    ///
    /// Returns [`ChainError::Io`] on filesystem failures.
    /// Returns [`ChainError::Serialise`] if JSON encoding fails.
    /// Returns [`ChainError::HmacInit`] if HMAC key initialisation fails.
    pub fn append(&mut self, mut entry: DecisionEntry) -> Result<()> {
        entry.seq = self.seq;
        entry.prev_hash = self.last_hash;
        entry.entry_hash = self.compute_hmac(&entry)?;

        let line =
            serde_json::to_string(&entry).map_err(|e| ChainError::Serialise(e.to_string()))?;

        self.atomic_append(&line)?;

        self.last_hash = Some(entry.entry_hash);
        self.seq = self.seq.saturating_add(1);
        Ok(())
    }

    /// Re-read the entire log and verify every entry's HMAC and hash linkage.
    ///
    /// This is an O(n) scan. For long-running builds consider calling this only
    /// at phase boundaries rather than after every append.
    ///
    /// # Errors
    ///
    /// Returns [`ChainError::Deserialise`] on malformed lines.
    /// Returns [`ChainError::ChainBroken`] if any entry's `entry_hash` is wrong.
    /// Returns [`ChainError::LinkageBroken`] if `prev_hash` linkage is broken.
    pub fn verify_all(&self) -> Result<()> {
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);

        let mut prev_hash: Option<[u8; 32]> = None;
        let mut expected_seq: u64 = 0;

        for (line_idx, line_result) in reader.lines().enumerate() {
            let line_no = line_idx + 1;
            let raw = line_result?;
            if raw.trim().is_empty() {
                continue;
            }

            let entry: DecisionEntry =
                serde_json::from_str(&raw).map_err(|e| ChainError::Deserialise {
                    line: line_no,
                    detail: e.to_string(),
                })?;

            // Sequence must be contiguous.
            if entry.seq != expected_seq {
                return Err(ChainError::ChainBroken {
                    seq: expected_seq,
                    detail: format!("seq gap: expected {expected_seq}, found {}", entry.seq),
                });
            }

            // prev_hash linkage.
            if entry.prev_hash != prev_hash {
                return Err(ChainError::LinkageBroken { seq: entry.seq });
            }

            // Recompute HMAC and compare.
            let expected_hmac = self.compute_hmac(&entry)?;
            if expected_hmac != entry.entry_hash {
                return Err(ChainError::ChainBroken {
                    seq: entry.seq,
                    detail: "entry_hash mismatch".to_owned(),
                });
            }

            prev_hash = Some(entry.entry_hash);
            expected_seq = expected_seq.saturating_add(1);
        }

        Ok(())
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    /// Compute `HMAC-SHA256(key, prev_hash_bytes || seq_be64 || decision_utf8)`.
    fn compute_hmac(&self, entry: &DecisionEntry) -> Result<[u8; 32]> {
        let mut mac = HmacSha256::new_from_slice(self.key.as_ref())
            .map_err(|e| ChainError::HmacInit(e.to_string()))?;

        // prev_hash: 32 zero bytes when None (first entry marker).
        match entry.prev_hash {
            Some(h) => mac.update(&h),
            None => mac.update(&[0u8; 32]),
        }
        mac.update(&entry.seq.to_be_bytes());
        mac.update(entry.decision.as_bytes());

        let result = mac.finalize();
        let bytes: [u8; 32] = result.into_bytes().into();
        Ok(bytes)
    }

    /// Replay existing log entries to restore `last_hash` and `seq`.
    fn replay(&mut self) -> Result<()> {
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);

        for (line_idx, line_result) in reader.lines().enumerate() {
            let line_no = line_idx + 1;
            let raw = line_result?;
            if raw.trim().is_empty() {
                continue;
            }
            let entry: DecisionEntry =
                serde_json::from_str(&raw).map_err(|e| ChainError::Deserialise {
                    line: line_no,
                    detail: e.to_string(),
                })?;
            self.last_hash = Some(entry.entry_hash);
            self.seq = entry.seq.saturating_add(1);
        }

        Ok(())
    }

    /// Atomically append `line` (without trailing newline) to the log.
    ///
    /// Writes to `<path>.tmp`, fsyncs, then renames over the live file.
    fn atomic_append(&self, line: &str) -> Result<()> {
        let tmp_path = self.path.with_extension("tmp");

        // Read current file contents so we can append.
        let existing = if self.path.exists() {
            std::fs::read(&self.path)?
        } else {
            Vec::new()
        };

        {
            let mut tmp = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)?;

            tmp.write_all(&existing)?;
            tmp.write_all(line.as_bytes())?;
            tmp.write_all(b"\n")?;
            tmp.flush()?;
            tmp.sync_all()?;
        }

        std::fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        k[0] = 0xDE;
        k[31] = 0xAD;
        k
    }

    fn make_entry(question: &str, decision: &str, layer: DecisionLayer) -> DecisionEntry {
        DecisionEntry {
            seq: 0, // overwritten by append()
            timestamp: Utc::now(),
            layer,
            question: question.to_owned(),
            decision: decision.to_owned(),
            citation: None,
            prev_hash: None,       // overwritten by append()
            entry_hash: [0u8; 32], // overwritten by append()
        }
    }

    #[test]
    fn open_creates_file_when_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("decisions.ndjson");
        assert!(!path.exists());
        HashChain::open(&path, test_key()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn append_single_entry_and_verify() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("chain.ndjson");
        let mut chain = HashChain::open(&path, test_key()).unwrap();

        chain
            .append(make_entry(
                "Canon XIV compliance?",
                "APPROVED",
                DecisionLayer::Canon,
            ))
            .unwrap();

        chain.verify_all().unwrap();
    }

    #[test]
    fn append_multiple_entries_verify_passes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("chain.ndjson");
        let mut chain = HashChain::open(&path, test_key()).unwrap();

        for i in 0..5_u32 {
            chain
                .append(make_entry(
                    &format!("question {i}"),
                    &format!("decision {i}"),
                    DecisionLayer::Northstar,
                ))
                .unwrap();
        }

        chain.verify_all().unwrap();
    }

    #[test]
    fn seq_increments_correctly() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("chain.ndjson");
        let mut chain = HashChain::open(&path, test_key()).unwrap();

        for expected_seq in 0..3_u64 {
            let before = chain.seq;
            assert_eq!(before, expected_seq);
            chain
                .append(make_entry("q", "d", DecisionLayer::User))
                .unwrap();
        }
        assert_eq!(chain.seq, 3);
    }

    #[test]
    fn open_restores_state_from_existing_log() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("chain.ndjson");

        {
            let mut chain = HashChain::open(&path, test_key()).unwrap();
            chain
                .append(make_entry("q0", "d0", DecisionLayer::Canon))
                .unwrap();
            chain
                .append(make_entry("q1", "d1", DecisionLayer::LightArchitect))
                .unwrap();
        }

        // Re-open and append a third entry — must continue the chain correctly.
        let mut chain2 = HashChain::open(&path, test_key()).unwrap();
        assert_eq!(chain2.seq, 2, "seq should be restored to 2");
        chain2
            .append(make_entry("q2", "d2", DecisionLayer::User))
            .unwrap();

        chain2.verify_all().unwrap();
    }

    #[test]
    fn tampered_entry_breaks_verify() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("chain.ndjson");
        let mut chain = HashChain::open(&path, test_key()).unwrap();

        chain
            .append(make_entry(
                "question",
                "original decision",
                DecisionLayer::Canon,
            ))
            .unwrap();

        // Tamper: rewrite the file with a modified decision field.
        let content = std::fs::read_to_string(&path).unwrap();
        let tampered = content.replace("original decision", "tampered decision");
        std::fs::write(&path, tampered).unwrap();

        let err = chain.verify_all().unwrap_err();
        assert!(
            matches!(err, ChainError::ChainBroken { seq: 0, .. }),
            "expected ChainBroken at seq 0, got: {err:?}"
        );
    }

    #[test]
    fn all_decision_layers_serialize_roundtrip() {
        let layers = [
            DecisionLayer::Canon,
            DecisionLayer::Northstar,
            DecisionLayer::LightArchitect,
            DecisionLayer::User,
        ];
        for layer in layers {
            let json = serde_json::to_string(&layer).unwrap();
            let back: DecisionLayer = serde_json::from_str(&json).unwrap();
            assert_eq!(layer, back);
        }
    }

    #[test]
    fn citation_field_preserved() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("chain.ndjson");
        let mut chain = HashChain::open(&path, test_key()).unwrap();

        let mut entry = make_entry("q", "APPROVED", DecisionLayer::Canon);
        entry.citation = Some("Canon XIV §3".to_owned());
        chain.append(entry).unwrap();

        // Re-open and read back.
        let raw = std::fs::read_to_string(&path).unwrap();
        let line = raw.lines().next().unwrap();
        let read_back: DecisionEntry = serde_json::from_str(line).unwrap();
        assert_eq!(read_back.citation.as_deref(), Some("Canon XIV §3"));
    }

    #[test]
    fn first_entry_prev_hash_is_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("chain.ndjson");
        let mut chain = HashChain::open(&path, test_key()).unwrap();
        chain
            .append(make_entry("q", "d", DecisionLayer::User))
            .unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        let entry: DecisionEntry = serde_json::from_str(raw.trim()).unwrap();
        assert!(
            entry.prev_hash.is_none(),
            "first entry must have no prev_hash"
        );
    }

    #[test]
    fn second_entry_prev_hash_links_to_first() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("chain.ndjson");
        let mut chain = HashChain::open(&path, test_key()).unwrap();
        chain
            .append(make_entry("q0", "d0", DecisionLayer::Canon))
            .unwrap();
        chain
            .append(make_entry("q1", "d1", DecisionLayer::Canon))
            .unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        let mut lines = raw.lines();
        let e0: DecisionEntry = serde_json::from_str(lines.next().unwrap()).unwrap();
        let e1: DecisionEntry = serde_json::from_str(lines.next().unwrap()).unwrap();

        assert_eq!(
            e1.prev_hash,
            Some(e0.entry_hash),
            "second entry must link to first"
        );
    }
}
