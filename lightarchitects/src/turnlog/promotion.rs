//! Promotion contract — Tier-1 → Tier-2 (SOUL helix) bridge.
//!
//! The turnlog crate ships a concrete [`SiblingPromoter`] that writes helix
//! entries to `~/lightarchitects/soul/helix/{sibling}/entries/`, and a
//! [`promote_session`] function that orchestrates the full promotion pipeline
//! (read entries → filter → check markers → promote → write markers).
//!
//! Each MCP server calls [`promote_session`] from a background `tokio::spawn`
//! after closing the turnlog writer.

use std::fs;
use std::io::Write as _;
use std::path::PathBuf;

use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::turnlog::entry::TurnEntry;
use crate::turnlog::reader::TurnLogReader;
use crate::turnlog::store::StoreLayout;

// ── Types ────────────────────────────────────────────────────────────────────

/// Candidate for Tier-2 promotion — passed to a [`HelixPromoter`].
#[derive(Debug, Clone)]
pub struct PromotionCandidate {
    /// The entry being considered (typically `reflection` or `session_paused`).
    pub entry: TurnEntry,
    /// Session this entry belongs to.
    pub session_id: String,
    /// Project root at the time of writing.
    pub project_root: PathBuf,
    /// Reason this candidate was surfaced.
    pub reason: PromotionReason,
    /// Optional surrounding context (prev/next entries).
    pub window: Option<Vec<TurnEntry>>,
}

/// Why a candidate was surfaced for promotion.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PromotionReason {
    /// `SessionPaused` memo — the paused-to-resume memo is intrinsically meaningful.
    PausedMemo,
    /// `Reflection` entry with weight above a threshold.
    SignificantReflection {
        /// Observed weight (helps the promoter format frontmatter).
        weight: f64,
    },
    /// Explicit user flag (e.g. `/remember this`).
    UserFlagged,
    /// Automatic detection (e.g. self-defining-moment detector).
    AutoDetected {
        /// Name of the detector that surfaced this candidate.
        detector: &'static str,
    },
}

/// Outcome reported back to the turnlog after promotion runs.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PromotionOutcome {
    /// Promoter wrote a Tier-2 entry. The marker file path goes into
    /// `{layout.promoted/}{session_id}.{seq}.ref`.
    Promoted {
        /// Path to the helix entry produced by the promoter.
        helix_path: PathBuf,
    },
    /// Promoter declined — candidate did not meet its threshold.
    Declined {
        /// Human-readable reason.
        reason: String,
    },
}

/// Error type for promotion operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PromotionError {
    /// Promoter's downstream I/O failed.
    #[error("promotion I/O: {0}")]
    Io(String),
    /// Candidate was malformed (e.g. wrong payload variant).
    #[error("invalid candidate: {0}")]
    InvalidCandidate(String),
}

/// Implemented by Tier-2 writers (e.g. [`SiblingPromoter`]).
///
/// Uses return-position `impl Future` rather than `#[async_trait]` so the
/// trait can be implemented without an external macro dependency.
pub trait HelixPromoter: Send + Sync {
    /// Consider a candidate for Tier-2 promotion.
    fn promote(
        &self,
        candidate: PromotionCandidate,
    ) -> impl std::future::Future<Output = std::result::Result<PromotionOutcome, PromotionError>> + Send;
}

// ── SiblingPromoter ──────────────────────────────────────────────────────────

/// Promotes turnlog candidates to Tier-2 SOUL helix entries for a specific sibling.
///
/// Writes to `~/lightarchitects/soul/helix/{sibling}/entries/` using helix
/// entry format v5.0.0 with atomic write protocol (flock + fdatasync + rename).
#[derive(Debug, Clone)]
pub struct SiblingPromoter {
    /// Target directory for helix entry files.
    entries_dir: PathBuf,
    /// Sibling name (e.g. "corso", "eva", "seraph").
    sibling: String,
}

impl SiblingPromoter {
    /// Construct a promoter targeting a specific entries directory.
    #[must_use]
    pub fn new(entries_dir: PathBuf, sibling: String) -> Self {
        Self {
            entries_dir,
            sibling,
        }
    }

    /// Construct a promoter targeting the canonical helix entries dir for a sibling.
    ///
    /// The target path is `~/lightarchitects/soul/helix/{sibling}/entries/`.
    /// Always returns `Some` — the fallback path is used when HOME is unavailable.
    #[must_use]
    pub fn default_for_user(sibling: &str) -> Self {
        let helix_root = crate::core::paths::helix_root_or_fallback();
        Self::new(helix_root.join(sibling).join("entries"), sibling.to_owned())
    }
}

impl HelixPromoter for SiblingPromoter {
    fn promote(
        &self,
        candidate: PromotionCandidate,
    ) -> impl std::future::Future<Output = std::result::Result<PromotionOutcome, PromotionError>> + Send
    {
        let entries_dir = self.entries_dir.clone();
        let sibling = self.sibling.clone();
        async move {
            // Offload blocking I/O (flock + write + fsync + rename) to avoid
            // starving the tokio runtime.
            let result = tokio::task::spawn_blocking(move || {
                write_helix_entry_sync(&entries_dir, &sibling, &candidate)
            })
            .await
            .map_err(|e| PromotionError::Io(format!("blocking task panicked: {e}")))?;

            Ok(PromotionOutcome::Promoted {
                helix_path: result?,
            })
        }
    }
}

// ── Session promotion ─────────────────────────────────────────────────────────

/// Derive a [`PromotionReason`] from an entry's semantic kind.
fn promotion_reason_for(entry: &TurnEntry) -> PromotionReason {
    use crate::turnlog::entry::EntryKind;
    match entry.kind() {
        EntryKind::SessionPaused => PromotionReason::PausedMemo,
        EntryKind::Reflection => {
            let weight = entry
                .span
                .metadata
                .get("weight")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(7.0);
            PromotionReason::SignificantReflection { weight }
        }
        _ => PromotionReason::AutoDetected {
            detector: "turnlog_promotion",
        },
    }
}

/// Read the just-closed session file and promote eligible entries to the
/// SOUL helix via the supplied promoter.
///
/// This is the shared implementation used by all MCP servers. Each server
/// spawns a background task that calls this function after closing its
/// turnlog writer.
///
/// Errors are logged at WARN level; failures must not panic or propagate
/// (this runs inside a fire-and-forget `tokio::spawn`).
pub async fn promote_session<P: HelixPromoter>(
    layout: &StoreLayout,
    session_id: &str,
    promoter: &P,
) {
    promote_session_with_pepper(layout, session_id, promoter, None).await;
}

/// Full [`promote_session`] with optional pepper for chain verification.
///
/// When `pepper` is `None`, reads entries without HMAC verification.
/// When `Some`, verifies the chain and skips promotion on verification failure.
pub async fn promote_session_with_pepper<P: HelixPromoter>(
    layout: &StoreLayout,
    session_id: &str,
    promoter: &P,
    pepper: Option<&secrecy::SecretSlice<u8>>,
) {
    let reader = TurnLogReader::new(layout.clone());

    let entries = if let Some(pepper) = pepper {
        match reader.read_all_verified(session_id, pepper).await {
            Ok((entries, _last_seq)) => entries,
            Err(e) => {
                warn!(
                    target: "turnlog",
                    %e,
                    session_id,
                    "promote: ended session failed chain verification — skipping promotion"
                );
                return;
            }
        }
    } else {
        match reader.read_all(session_id).await {
            Ok(entries) => entries,
            Err(e) => {
                warn!(
                    target: "turnlog",
                    %e,
                    session_id,
                    "promote: failed to read ended session — skipping promotion"
                );
                return;
            }
        }
    };

    let mut promotion_count = 0u32;

    for entry in entries {
        if !entry.is_helix_promotable() {
            continue;
        }

        // Skip if a promotion marker already exists for this sequence number.
        let marker = layout.promoted_marker_path(session_id, entry.seq);
        if marker.exists() {
            continue;
        }

        let seq = entry.seq;
        let reason = promotion_reason_for(&entry);
        let candidate = PromotionCandidate {
            entry,
            session_id: session_id.to_owned(),
            project_root: PathBuf::from("."),
            reason,
            window: None,
        };

        match promoter.promote(candidate).await {
            Ok(PromotionOutcome::Promoted { helix_path }) => {
                if let Err(e) =
                    tokio::fs::write(&marker, helix_path.to_string_lossy().as_bytes()).await
                {
                    warn!(target: "turnlog", %e, session_id, seq, "promote: failed to write marker");
                } else {
                    promotion_count += 1;
                }
            }
            Ok(PromotionOutcome::Declined { reason }) => {
                tracing::debug!(target: "turnlog", session_id, seq, reason, "promote: declined");
            }
            Err(e) => {
                warn!(target: "turnlog", %e, session_id, seq, "promote: error");
            }
        }
    }

    if promotion_count > 0 {
        info!(target: "turnlog", session_id, promotion_count, "helix entries written after session close");
    }
}

// ── Atomic write ─────────────────────────────────────────────────────────────

/// Derive significance, entry type, and representative strands from the reason.
fn classify_reason(reason: &PromotionReason) -> (f64, &'static str, &'static [&'static str]) {
    match reason {
        PromotionReason::PausedMemo => (6.0, "experience", &["Methodical", "Contextual"]),
        PromotionReason::SignificantReflection { weight } => {
            (*weight, "experience", &["Analytical", "Methodical"])
        }
        PromotionReason::UserFlagged => (7.5, "milestone", &["Candid", "Collaborative"]),
        PromotionReason::AutoDetected { .. } => (7.0, "experience", &["Analytical", "Precision"]),
    }
}

/// Extract the primary text payload from the entry's span metadata.
///
/// Checks `memo_body` (session_paused) first, then `content` (reflection /
/// turn entries), and falls back to a sentinel if neither is present.
fn extract_body(candidate: &PromotionCandidate) -> String {
    let meta = &candidate.entry.span.metadata;
    meta.get("memo_body")
        .or_else(|| meta.get("content"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("(no content recorded)")
        .to_owned()
}

/// Build the YAML frontmatter block in helix entry v5.0.0 format.
fn build_frontmatter(
    id: &Uuid,
    date: &str,
    sibling: &str,
    entry_type: &str,
    significance: f64,
    strands: &[&str],
) -> String {
    let strands_yaml = strands
        .iter()
        .map(|s| format!("  - {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "---\nid: {id}\ndate: {date}\nsibling: {sibling}\ntype: {entry_type}\n\
         significance: {significance:.1}\nstrands:\n{strands_yaml}\n\
         resonance: {{}}\nthemes: []\nepoch: genesis\n---\n"
    )
}

/// Write a helix entry with flock + atomic rename.
///
/// All I/O is synchronous — call this from `spawn_blocking`.
fn write_helix_entry_sync(
    entries_dir: &PathBuf,
    sibling: &str,
    candidate: &PromotionCandidate,
) -> Result<PathBuf, PromotionError> {
    use fs2::FileExt as _;

    let (significance, entry_type, strands) = classify_reason(&candidate.reason);
    let body = extract_body(candidate);

    // Identifiers and names.
    let id = Uuid::new_v4();
    let date = Utc::now().format("%Y-%m-%d").to_string();
    // Slug: first 8 chars of session ID + "-turnlog-promotion".
    let session_prefix = candidate
        .session_id
        .get(..8)
        .unwrap_or(&candidate.session_id);
    let slug = format!("{session_prefix}-turnlog-promotion");
    let id_str = id.to_string();
    let id8 = &id_str[..8];
    let file_name = format!("{date}-{id8}-{slug}.md");

    // File content: YAML frontmatter + blank line + body.
    let frontmatter = build_frontmatter(&id, &date, sibling, entry_type, significance, strands);
    let content = format!("{frontmatter}\n{body}\n");

    // Ensure the entries directory exists.
    fs::create_dir_all(entries_dir)
        .map_err(|e| PromotionError::Io(format!("create entries dir: {e}")))?;

    let tmp_path = entries_dir.join(format!(".tmp-{id_str}.md"));
    let final_path = entries_dir.join(&file_name);

    // Open → lock → write → fdatasync → close (lock released) → rename.
    {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_path)
            .map_err(|e| PromotionError::Io(format!("open tmp file: {e}")))?;

        file.lock_exclusive()
            .map_err(|e| PromotionError::Io(format!("flock exclusive: {e}")))?;

        file.write_all(content.as_bytes())
            .map_err(|e| PromotionError::Io(format!("write content: {e}")))?;

        file.sync_data()
            .map_err(|e| PromotionError::Io(format!("fdatasync: {e}")))?;

        // `file` is dropped here — flock is released, fd is closed.
    }

    // Atomic rename on POSIX; readers either see the complete file or nothing.
    fs::rename(&tmp_path, &final_path)
        .map_err(|e| PromotionError::Io(format!("rename tmp → final: {e}")))?;

    Ok(final_path)
}

// ── Built-in policies ────────────────────────────────────────────────────────

/// Built-in policies composed on top of a [`HelixPromoter`].
pub mod policy {
    /// Auto-promote every `SessionPaused` memo.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct PromotePausedMemos;

    /// Auto-promote reflections whose weight meets or exceeds `threshold`.
    #[derive(Debug, Clone, Copy)]
    pub struct PromoteSignificantReflections {
        /// Minimum weight required (0.0–10.0 scale).
        pub threshold: f64,
    }

    impl Default for PromoteSignificantReflections {
        fn default() -> Self {
            Self { threshold: 7.0 }
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn classify_reason_paused_memo_gives_6_0() {
        let (sig, entry_type, strands) = classify_reason(&PromotionReason::PausedMemo);
        assert!((sig - 6.0).abs() < f64::EPSILON);
        assert_eq!(entry_type, "experience");
        assert!(strands.contains(&"Methodical"));
    }

    #[test]
    fn classify_reason_user_flagged_gives_7_5_milestone() {
        let (sig, entry_type, _) = classify_reason(&PromotionReason::UserFlagged);
        assert!((sig - 7.5).abs() < f64::EPSILON);
        assert_eq!(entry_type, "milestone");
    }

    #[test]
    fn sibling_promoter_default_for_user_returns_valid_path() {
        let promoter = SiblingPromoter::default_for_user("corso");
        assert!(promoter.entries_dir.to_string_lossy().contains("corso"));
        assert!(promoter.entries_dir.to_string_lossy().contains("entries"));
        assert!(promoter.sibling == "corso");
    }

    #[test]
    fn build_frontmatter_contains_sibling() {
        let id = Uuid::new_v4();
        let fm = build_frontmatter(
            &id,
            "2026-04-15",
            "seraph",
            "experience",
            7.0,
            &["Methodical"],
        );
        assert!(fm.contains("sibling: seraph"));
        assert!(fm.contains("significance: 7.0"));
        assert!(fm.contains("type: experience"));
    }

    #[tokio::test]
    async fn promote_session_writes_helix_entry_for_reflection() {
        use crate::turnlog::EndReason;
        use crate::turnlog::writer::TurnLogWriter;
        use ayin::span::{Actor, TraceContext, TraceOutcome};

        let dir = tempfile::tempdir().expect("tmpdir");
        let layout = StoreLayout::new(dir.path().to_path_buf());
        let pepper: secrecy::SecretSlice<u8> = secrecy::SecretSlice::from(vec![0x42_u8; 32]);
        let session_id = "promote-session-test-reflection";

        // Write a session with a reflection entry.
        let writer = TurnLogWriter::open(
            &layout,
            session_id.to_owned(),
            dir.path().to_path_buf(),
            "test-model".to_owned(),
            "test-provider".to_owned(),
            None,
            &pepper,
        )
        .await
        .expect("open writer");

        // Append a reflection entry.
        let reflection_span = TraceContext::new(Actor::new("test"), "reflection")
            .session_id(session_id)
            .outcome(TraceOutcome::Continue)
            .metadata(serde_json::json!({
                "content": "This was a significant reflection",
                "weight": 8.5,
            }))
            .finish()
            .expect("reflection span");
        writer.append(reflection_span);
        writer.close(EndReason::Complete).await.expect("close");

        // Promote using SiblingPromoter targeting a temp dir.
        let entries_dir = dir.path().join("helix").join("entries");
        let promoter = SiblingPromoter::new(entries_dir, "test".to_owned());
        promote_session(&layout, session_id, &promoter).await;

        // Reflection is promotable, so the promoter should have written a helix entry.
        let helix_entries: Vec<_> = std::fs::read_dir(dir.path().join("helix").join("entries"))
            .expect("read entries dir")
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
            .collect();
        assert!(
            !helix_entries.is_empty(),
            "at least one helix entry should be written"
        );

        // Check that a marker file was also written.
        let promoted_dir = dir.path().join("promoted");
        if promoted_dir.is_dir() {
            let markers: Vec<_> = std::fs::read_dir(&promoted_dir)
                .expect("read promoted dir")
                .filter_map(std::result::Result::ok)
                .collect();
            assert!(
                !markers.is_empty(),
                "at least one marker file should be written"
            );
        }
    }

    #[test]
    fn extract_body_prefers_memo_body_over_content() {
        use ayin::span::{Actor, TraceContext, TraceOutcome};

        let span = TraceContext::new(Actor::new("test"), "session_paused")
            .session_id("x")
            .outcome(TraceOutcome::Continue)
            .metadata(serde_json::json!({
                "memo_body": "primary body",
                "content":   "secondary content",
            }))
            .finish()
            .expect("span");

        let entry = TurnEntry {
            seq: 0,
            parent_seq: None,
            span,
            hmac_prev: String::new(),
            hmac_self: String::new(),
        };

        let candidate = PromotionCandidate {
            entry,
            session_id: "x".to_owned(),
            project_root: PathBuf::from("/tmp"),
            reason: PromotionReason::PausedMemo,
            window: None,
        };

        let body = extract_body(&candidate);
        assert_eq!(body, "primary body");
    }
}
