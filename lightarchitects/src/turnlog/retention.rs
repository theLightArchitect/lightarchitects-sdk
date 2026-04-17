//! Retention compactor.
//!
//! Policy tiers:
//! - **Hot** (<= `hot_age`): keep ended session files verbatim.
//! - **Warm** (`hot_age` < age <= `warm_age`): keep only non-turn entries plus a single
//!   `session_rollup` entry that carries the dropped turn payload.
//! - **Cold** (> `warm_age`): keep only a single `session_rollup` entry.
//!
//! The rollup output is written to `rollups/{YYYY-MM}/{session_id}.rollup.json` and is
//! itself an HMAC-verifiable NDJSON session stream. Originals are deleted only after
//! the new rollup verifies against the session genesis.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ayin::TraceSpan;
use ayin::span::{Actor, TraceOutcome};
use chrono::{NaiveDate, Utc};
use secrecy::SecretSlice;
use tracing::warn;

use crate::turnlog::chain::{GenesisBlock, derive_session_key, sign_entry, verify_chain};
use crate::turnlog::entry::{EntryKind, TurnEntry};
use crate::turnlog::error::{Result, TurnLogError};
use crate::turnlog::store::StoreLayout;

/// Configuration for the retention compactor.
#[derive(Debug, Clone)]
pub struct RetentionConfig {
    /// How long entries stay in the hot tier.
    pub hot_age: Duration,
    /// How long entries stay in the warm tier before rollup.
    pub warm_age: Duration,
    /// Max session file size before an emergency mid-session rotation.
    pub max_session_bytes: u64,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            hot_age: Duration::from_secs(2 * 24 * 3600),
            warm_age: Duration::from_secs(30 * 24 * 3600),
            max_session_bytes: 50 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tier {
    Hot,
    Warm,
    Cold,
}

#[derive(Debug, Default, Clone)]
/// Summary stats for a single retention pass.
pub struct RetentionStats {
    /// Total ended session files examined.
    pub sessions_processed: usize,
    /// Sessions compacted into warm-tier rollups.
    pub warm_compacted: usize,
    /// Sessions compacted into cold-tier rollups.
    pub cold_compacted: usize,
    /// Sessions skipped because they are still hot-tier.
    pub skipped_hot: usize,
    /// Per-session errors encountered (best-effort pass continues).
    pub errors: usize,
}

#[derive(Debug, Clone)]
/// Best-effort retention compactor over a [`StoreLayout`].
pub struct RetentionCompactor {
    config: RetentionConfig,
    layout: StoreLayout,
}

impl RetentionCompactor {
    /// Create a compactor with explicit config and layout.
    #[must_use]
    pub fn new(config: RetentionConfig, layout: StoreLayout) -> Self {
        Self { config, layout }
    }

    /// Create a compactor for the default on-disk store root.
    #[must_use]
    pub fn default_for_user() -> Option<Self> {
        StoreLayout::default_for_user().map(|layout| Self::new(RetentionConfig::default(), layout))
    }

    /// Run one compaction pass. Idempotent.
    ///
    /// This is best-effort: per-session failures are counted in stats and the
    /// pass continues.
    ///
    /// # Errors
    /// Returns [`TurnLogError::Io`] if the `ended/` directory (or a date subdirectory)
    /// cannot be enumerated.
    pub async fn run(&self, pepper: &SecretSlice<u8>) -> Result<RetentionStats> {
        let today = Utc::now().date_naive();
        let mut stats = RetentionStats::default();

        let ended_root = self.layout.root().join("ended");
        if !ended_root.is_dir() {
            return Ok(stats);
        }

        let mut date_dirs = tokio::fs::read_dir(&ended_root)
            .await
            .map_err(|e| TurnLogError::io(&ended_root, e))?;

        while let Some(date_ent) = date_dirs
            .next_entry()
            .await
            .map_err(|e| TurnLogError::io(&ended_root, e))?
        {
            let date_path = date_ent.path();
            if !date_path.is_dir() {
                continue;
            }

            let Some(date_str) = date_path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(session_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
                continue;
            };

            let month = date_str.get(0..7).unwrap_or(date_str).to_owned();
            let tier = tier_for_date(session_date, today, &self.config);

            let mut session_files = tokio::fs::read_dir(&date_path)
                .await
                .map_err(|e| TurnLogError::io(&date_path, e))?;

            while let Some(file_ent) = session_files
                .next_entry()
                .await
                .map_err(|e| TurnLogError::io(&date_path, e))?
            {
                let path = file_ent.path();
                if !path.is_file() {
                    continue;
                }
                if path.extension().and_then(|s| s.to_str()) != Some("ndjson") {
                    continue;
                }
                let Some(session_id) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };

                stats.sessions_processed = stats.sessions_processed.saturating_add(1);
                match tier {
                    Tier::Hot => {
                        stats.skipped_hot = stats.skipped_hot.saturating_add(1);
                    }
                    Tier::Warm => {
                        if let Err(e) = self
                            .compact_one(session_id, &month, &path, Tier::Warm, pepper)
                            .await
                        {
                            stats.errors = stats.errors.saturating_add(1);
                            warn!(
                                target: "turnlog",
                                session_id,
                                error = %e,
                                "retention: warm compaction failed"
                            );
                        } else {
                            stats.warm_compacted = stats.warm_compacted.saturating_add(1);
                        }
                    }
                    Tier::Cold => {
                        if let Err(e) = self
                            .compact_one(session_id, &month, &path, Tier::Cold, pepper)
                            .await
                        {
                            stats.errors = stats.errors.saturating_add(1);
                            warn!(
                                target: "turnlog",
                                session_id,
                                error = %e,
                                "retention: cold compaction failed"
                            );
                        } else {
                            stats.cold_compacted = stats.cold_compacted.saturating_add(1);
                        }
                    }
                }
            }
        }

        Ok(stats)
    }

    async fn compact_one(
        &self,
        session_id: &str,
        month: &str,
        ended_path: &Path,
        tier: Tier,
        pepper: &SecretSlice<u8>,
    ) -> Result<()> {
        let genesis = load_genesis(&self.layout, session_id).await?;
        let session_key = derive_session_key(pepper, &genesis.hkdf_salt, session_id)?;

        let entries = read_entries(ended_path).await?;

        let (retained, dropped) = match tier {
            Tier::Warm => split_warm(&entries),
            Tier::Cold => (Vec::new(), entries),
            Tier::Hot => (entries, Vec::new()),
        };

        let rollup_span = make_rollup_span(session_id, tier, &dropped);
        let mut spans = Vec::with_capacity(retained.len().saturating_add(1));
        spans.extend(retained.into_iter().map(|e| e.span));
        spans.push(rollup_span);

        let mut rollup_entries = rechain(&genesis, &session_key, spans)?;
        let _ = verify_chain(&genesis, rollup_entries.clone(), &session_key)?;

        let rollup_path = self.layout.rollup_path(session_id, month);
        if let Some(parent) = rollup_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| TurnLogError::io(parent, e))?;
        }

        // Idempotency: if a rollup already exists and verifies, keep it and just delete the ended
        // file. This can happen if a previous run wrote the rollup but crashed before deletion.
        if rollup_path.is_file() {
            let existing = read_entries(&rollup_path).await?;
            let _ = verify_chain(&genesis, existing, &session_key)?;
            tokio::fs::remove_file(ended_path)
                .await
                .map_err(|e| TurnLogError::io(ended_path, e))?;
            return Ok(());
        }

        let ndjson = entries_to_ndjson(&mut rollup_entries)?;
        let tmp = tmp_path_for(&rollup_path);
        tokio::fs::write(&tmp, ndjson)
            .await
            .map_err(|e| TurnLogError::io(&tmp, e))?;
        tokio::fs::rename(&tmp, &rollup_path)
            .await
            .map_err(|e| TurnLogError::io(&rollup_path, e))?;

        tokio::fs::remove_file(ended_path)
            .await
            .map_err(|e| TurnLogError::io(ended_path, e))?;

        Ok(())
    }
}

fn tier_for_date(session_date: NaiveDate, today: NaiveDate, config: &RetentionConfig) -> Tier {
    let age_signed = (today - session_date).num_days();
    let age_days: u64 = u64::try_from(age_signed).unwrap_or_default();

    let hot_days = config.hot_age.as_secs() / 86_400;
    let warm_days = config.warm_age.as_secs() / 86_400;
    if age_days <= hot_days {
        Tier::Hot
    } else if age_days <= warm_days {
        Tier::Warm
    } else {
        Tier::Cold
    }
}

async fn load_genesis(layout: &StoreLayout, session_id: &str) -> Result<GenesisBlock> {
    let path = layout.genesis_path(session_id);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| TurnLogError::io(&path, e))?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn read_entries(path: &Path) -> Result<Vec<TurnEntry>> {
    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| TurnLogError::io(path, e))?;
    let mut out = Vec::new();
    for line in content.lines().filter(|l| !l.is_empty()) {
        match serde_json::from_str::<TurnEntry>(line) {
            Ok(e) => out.push(e),
            Err(e) => return Err(TurnLogError::Serialize(e)),
        }
    }
    Ok(out)
}

fn split_warm(entries: &[TurnEntry]) -> (Vec<TurnEntry>, Vec<TurnEntry>) {
    let mut retained = Vec::new();
    let mut dropped = Vec::new();
    for e in entries {
        match e.kind() {
            EntryKind::TurnUser | EntryKind::TurnAssistant | EntryKind::ToolResult => {
                dropped.push(e.clone());
            }
            _ => retained.push(e.clone()),
        }
    }
    (retained, dropped)
}

fn make_rollup_span(session_id: &str, tier: Tier, dropped: &[TurnEntry]) -> TraceSpan {
    let mut dropped_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut dropped_rows = Vec::with_capacity(dropped.len());
    for e in dropped {
        *dropped_counts
            .entry(e.kind().as_action().to_owned())
            .or_insert(0) += 1;
        dropped_rows.push(serde_json::json!({
            "action": e.span.action,
            "timestamp": e.span.timestamp,
            "metadata": e.span.metadata,
        }));
    }

    let tier_str = match tier {
        Tier::Hot => "hot",
        Tier::Warm => "warm",
        Tier::Cold => "cold",
    };

    TraceSpan {
        id: uuid::Uuid::new_v4(),
        parent_id: None,
        session_id: Some(session_id.to_owned()),
        actor: Actor::claude(),
        action: "session_rollup".to_owned(),
        timestamp: Utc::now(),
        duration_ms: 0,
        decision_points: Vec::new(),
        strand_activations: Vec::new(),
        outcome: TraceOutcome::Continue,
        metadata: serde_json::json!({
            "tier": tier_str,
            "dropped_counts": dropped_counts,
            "dropped_entries": dropped_rows,
        }),
    }
}

fn rechain(
    genesis: &GenesisBlock,
    session_key: &secrecy::SecretString,
    spans: Vec<TraceSpan>,
) -> Result<Vec<TurnEntry>> {
    let mut out = Vec::with_capacity(spans.len());
    let mut prev = genesis.hmac_genesis.clone();
    let mut seq: u64 = 0;

    for span in spans {
        let mut entry = TurnEntry {
            seq,
            parent_seq: None,
            span,
            hmac_prev: prev.clone(),
            hmac_self: String::new(),
        };
        sign_entry(&mut entry, session_key)?;
        prev.clone_from(&entry.hmac_self);
        seq = seq.saturating_add(1);
        out.push(entry);
    }

    Ok(out)
}

fn entries_to_ndjson(entries: &mut [TurnEntry]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for (idx, e) in entries.iter().enumerate() {
        let mut line = serde_json::to_vec(e)?;
        if idx + 1 < entries.len() {
            line.push(b'\n');
        }
        out.extend_from_slice(&line);
    }
    Ok(out)
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("rollup");
    parent.join(format!(".tmp-{file}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::turnlog::writer::TurnLogWriter;

    fn test_pepper() -> SecretSlice<u8> {
        SecretSlice::from(vec![0x11_u8; 32])
    }

    fn span(session_id: &str, action: &str, meta: serde_json::Value) -> TraceSpan {
        TraceSpan {
            id: uuid::Uuid::new_v4(),
            parent_id: None,
            session_id: Some(session_id.to_owned()),
            actor: Actor::claude(),
            action: action.to_owned(),
            timestamp: Utc::now(),
            duration_ms: 1,
            decision_points: Vec::new(),
            strand_activations: Vec::new(),
            outcome: TraceOutcome::Continue,
            metadata: meta,
        }
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn warm_compaction_writes_rollup_and_deletes_original() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().join("turnlog"));
        let pepper = test_pepper();

        let session_id = "sess-retention-1".to_owned();
        let writer = TurnLogWriter::open(
            &layout,
            session_id.clone(),
            tmp.path().to_path_buf(),
            "m".to_owned(),
            "p".to_owned(),
            None,
            &pepper,
        )
        .await
        .unwrap();

        writer.append(span(
            &session_id,
            "turn.user",
            serde_json::json!({ "content": "hi" }),
        ));
        writer.append(span(
            &session_id,
            "turn.assistant",
            serde_json::json!({ "content": "hello" }),
        ));
        writer.append(span(
            &session_id,
            "reflection",
            serde_json::json!({ "memo": "keep me" }),
        ));

        writer
            .close(crate::turnlog::writer::EndReason::Complete)
            .await
            .unwrap();

        // Move ended file into an older date directory so tier calculation hits WARM.
        let original_date = Utc::now().format("%Y-%m-%d").to_string();
        let ended_today = layout.ended_path(&session_id, &original_date);
        assert!(ended_today.is_file());

        let warm_date = (Utc::now().date_naive() - chrono::Duration::days(10))
            .format("%Y-%m-%d")
            .to_string();
        let warm_dir = layout.root().join("ended").join(&warm_date);
        tokio::fs::create_dir_all(&warm_dir).await.unwrap();
        let ended_warm = warm_dir.join(format!("{session_id}.ndjson"));
        tokio::fs::rename(&ended_today, &ended_warm).await.unwrap();

        let compactor = RetentionCompactor::new(RetentionConfig::default(), layout.clone());
        let stats = compactor.run(&pepper).await.unwrap();
        assert_eq!(stats.errors, 0);
        assert_eq!(stats.warm_compacted, 1);
        assert!(!ended_warm.is_file(), "original ended file must be deleted");

        let month = warm_date.get(0..7).unwrap();
        let rollup_path = layout.rollup_path(&session_id, month);
        assert!(rollup_path.is_file(), "rollup file must exist");

        // Verify chain for rollup file.
        let genesis = load_genesis(&layout, &session_id).await.unwrap();
        let session_key = derive_session_key(&pepper, &genesis.hkdf_salt, &session_id).unwrap();
        let rollup_entries = read_entries(&rollup_path).await.unwrap();
        let _ = verify_chain(&genesis, rollup_entries, &session_key).unwrap();
    }
}
