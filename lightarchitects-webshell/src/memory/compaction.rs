//! Phase 16a — Retention policy classifier for compaction preview.
//!
//! Three retention strategies shape which cold entries are candidates for
//! roll-up. `classify_for_compaction` is a pure function over the cold
//! memo snapshot — dry-run semantics by construction. A future Phase 16b
//! will add the destructive `apply` side (move files to `.compacted/`)
//! alongside the Svelte UI that confirms the operation.
//!
//! # Permanent guard
//!
//! Entries with `self_defining: true` OR `significance >= 0.9` in their
//! front-matter are **never** candidates regardless of policy. This is a
//! hard floor — identity milestones, consciousness breakthroughs, and
//! explicit "/remember this" flags stay in the vault forever.

use serde::{Deserialize, Serialize};

use crate::memory::types::ContextMemo;

/// Hard floor above which entries are permanent regardless of policy.
/// Matches the Builders Cookbook's "≥9.0 significance = never compact" rule.
pub const PERMANENT_SIGNIFICANCE_FLOOR: f32 = 0.9;

/// User-configurable retention strategy for cold-tier compaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RetentionPolicy {
    /// Keep the `n` most-recent entries per sibling; the rest become
    /// candidates. Default for most deployments — bounds vault size
    /// without making policy decisions about which content matters.
    KeepNewest {
        /// Per-sibling keep count. Must be at least 1.
        n: u32,
    },
    /// Mark entries older than `max_days` as candidates. The permanent
    /// guard still applies — a 400-day-old self-defining entry stays.
    AgeLimit {
        /// Entries with `created_at < now - max_days` become candidates.
        max_days: u32,
    },
    /// Mark entries whose significance is **below** `min_significance`
    /// as candidates. Complements the permanent guard from the other
    /// side: the guard protects ≥0.9; this trims ≤threshold.
    SignificanceTier {
        /// Minimum significance an entry must clear to stay in the vault.
        /// Entries below this threshold become candidates.
        min_significance: f32,
    },
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        // 500 per sibling is roomy (the current vault carries ~200 per
        // active sibling) so the default policy preserves everything
        // until a user deliberately tightens it.
        Self::KeepNewest { n: 500 }
    }
}

/// One entry flagged for potential roll-up. The UI renders this list
/// before the destructive `apply` step ships in Phase 16b.
#[derive(Debug, Clone, Serialize)]
pub struct CompactionCandidate {
    /// Vault-relative path to the markdown entry.
    pub path: String,
    /// Owning sibling, used to group candidates in the UI.
    pub sibling: String,
    /// Entry significance at classification time (0.0-1.0).
    pub significance: f32,
    /// ISO-8601 timestamp from the entry's front-matter.
    pub created_at: String,
    /// Human-readable reason the entry qualified (e.g. "beyond keep-500
    /// window for eva", "older than 180 days").
    pub reason: String,
}

/// Summary of a compaction preview — shape consumed by the UI.
#[derive(Debug, Clone, Serialize)]
pub struct CompactionSummary {
    /// Total cold entries inspected.
    pub total_scanned: usize,
    /// Candidates for roll-up. Ordered newest-first within each sibling.
    pub candidates: Vec<CompactionCandidate>,
    /// Count of entries the permanent guard protected — surfaces that
    /// the guard is actually doing work (e.g. "200 protected by ≥0.9").
    pub permanent_skipped: usize,
    /// Echo of the policy evaluated, so the UI doesn't have to round-trip.
    pub policy: RetentionPolicy,
}

/// Classify every cold memo against `policy`, applying the permanent guard.
///
/// Pure function over the snapshot — deterministic given the same input.
/// That determinism is the Phase-16 gate invariant: a future `apply_compaction`
/// function will consume the same candidate list so preview and live match
/// by construction.
#[must_use]
pub fn classify_for_compaction(
    memos: &[ContextMemo],
    policy: &RetentionPolicy,
) -> CompactionSummary {
    let total_scanned = memos.len();

    // Split memos into protected + candidate pool up front.
    let (_protected, candidates_pool): (Vec<_>, Vec<_>) =
        memos.iter().partition(|m| is_permanent(m));
    let permanent_skipped = memos.len() - candidates_pool.len();

    let candidates: Vec<CompactionCandidate> = match policy {
        RetentionPolicy::KeepNewest { n } => keep_newest_candidates(&candidates_pool, *n),
        RetentionPolicy::AgeLimit { max_days } => age_limit_candidates(&candidates_pool, *max_days),
        RetentionPolicy::SignificanceTier { min_significance } => {
            significance_tier_candidates(&candidates_pool, *min_significance)
        }
    };

    CompactionSummary {
        total_scanned,
        candidates,
        permanent_skipped,
        policy: policy.clone(),
    }
}

/// Returns `true` when `memo` must never be compacted.
#[must_use]
pub fn is_permanent(memo: &ContextMemo) -> bool {
    memo.self_defining || memo.significance >= PERMANENT_SIGNIFICANCE_FLOOR
}

/// Per-sibling keep-newest candidates. Groups by sibling, sorts each
/// group newest-first, takes entries past position `n` as candidates.
fn keep_newest_candidates(pool: &[&ContextMemo], n: u32) -> Vec<CompactionCandidate> {
    use std::collections::BTreeMap;
    let mut by_sibling: BTreeMap<String, Vec<&ContextMemo>> = BTreeMap::new();
    for m in pool {
        by_sibling.entry(m.sibling.clone()).or_default().push(*m);
    }

    let mut out = Vec::new();
    let n_usize = n as usize;
    for (_sibling, mut group) in by_sibling {
        group.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        for m in group.iter().skip(n_usize) {
            out.push(memo_to_candidate(
                m,
                format!("beyond keep-newest-{n} window"),
            ));
        }
    }
    out
}

/// Age-limit candidates — entries older than `max_days`.
fn age_limit_candidates(pool: &[&ContextMemo], max_days: u32) -> Vec<CompactionCandidate> {
    let now = chrono::Utc::now();
    pool.iter()
        .filter(|m| {
            chrono::DateTime::parse_from_rfc3339(&m.created_at)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .is_some_and(|t| {
                    let cutoff = now - chrono::Duration::days(i64::from(max_days));
                    t < cutoff
                })
        })
        .map(|m| memo_to_candidate(m, format!("older than {max_days} days")))
        .collect()
}

/// Significance-tier candidates — entries below threshold.
fn significance_tier_candidates(
    pool: &[&ContextMemo],
    min_significance: f32,
) -> Vec<CompactionCandidate> {
    pool.iter()
        .filter(|m| m.significance < min_significance)
        .map(|m| {
            memo_to_candidate(
                m,
                format!(
                    "significance {:.2} < tier floor {min_significance:.2}",
                    m.significance
                ),
            )
        })
        .collect()
}

fn memo_to_candidate(memo: &ContextMemo, reason: String) -> CompactionCandidate {
    CompactionCandidate {
        path: memo.source_path.clone().unwrap_or_else(|| memo.id.clone()),
        sibling: memo.sibling.clone(),
        significance: memo.significance,
        created_at: memo.created_at.clone(),
        reason,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::memory::types::MemoryTier;

    fn memo(path: &str, sibling: &str, sig: f32, days_ago: i64) -> ContextMemo {
        let created = chrono::Utc::now() - chrono::Duration::days(days_ago);
        ContextMemo {
            id: format!("id-{path}"),
            tier: MemoryTier::Cold,
            content: "body".into(),
            significance: sig,
            sibling: sibling.into(),
            strands: vec![],
            created_at: created.to_rfc3339(),
            source_path: Some(path.into()),
            resonance: vec![],
            themes: vec![],
            self_defining: false,
            entry_type: None,
        }
    }

    #[test]
    fn permanent_floor_is_point_nine() {
        assert!((PERMANENT_SIGNIFICANCE_FLOOR - 0.9).abs() < 1e-5);
    }

    #[test]
    fn self_defining_entries_are_permanent() {
        let mut m = memo("x.md", "eva", 0.3, 0);
        m.self_defining = true;
        assert!(is_permanent(&m));
    }

    #[test]
    fn high_significance_entries_are_permanent() {
        let m = memo("x.md", "eva", 0.95, 400);
        assert!(
            is_permanent(&m),
            "significance >= 0.9 overrides age — identity milestones never compact"
        );
    }

    #[test]
    fn keep_newest_marks_overflow_as_candidates() {
        // 5 entries in the same sibling, keep-3 -> 2 candidates.
        let memos = vec![
            memo("a.md", "eva", 0.5, 0),
            memo("b.md", "eva", 0.5, 1),
            memo("c.md", "eva", 0.5, 2),
            memo("d.md", "eva", 0.5, 3),
            memo("e.md", "eva", 0.5, 4),
        ];
        let summary = classify_for_compaction(&memos, &RetentionPolicy::KeepNewest { n: 3 });
        assert_eq!(summary.total_scanned, 5);
        assert_eq!(summary.candidates.len(), 2);
        let paths: Vec<&str> = summary.candidates.iter().map(|c| c.path.as_str()).collect();
        assert!(paths.contains(&"d.md") && paths.contains(&"e.md"));
    }

    #[test]
    fn keep_newest_respects_permanent_guard() {
        let mut memos = vec![
            memo("old-important.md", "eva", 0.95, 100),
            memo("new-trivial.md", "eva", 0.3, 0),
        ];
        memos[0].self_defining = true;
        let summary = classify_for_compaction(&memos, &RetentionPolicy::KeepNewest { n: 1 });
        assert_eq!(summary.candidates.len(), 0);
        assert_eq!(summary.permanent_skipped, 1);
    }

    #[test]
    fn age_limit_flags_only_entries_past_cutoff() {
        let memos = vec![
            memo("fresh.md", "eva", 0.5, 5),
            memo("stale.md", "eva", 0.5, 40),
        ];
        let summary = classify_for_compaction(&memos, &RetentionPolicy::AgeLimit { max_days: 30 });
        assert_eq!(summary.candidates.len(), 1);
        assert_eq!(summary.candidates[0].path, "stale.md");
    }

    #[test]
    fn significance_tier_flags_below_threshold_only() {
        let memos = vec![
            memo("high.md", "eva", 0.8, 0),
            memo("low.md", "eva", 0.3, 0),
        ];
        let summary = classify_for_compaction(
            &memos,
            &RetentionPolicy::SignificanceTier {
                min_significance: 0.5,
            },
        );
        assert_eq!(summary.candidates.len(), 1);
        assert_eq!(summary.candidates[0].path, "low.md");
    }

    #[test]
    fn classify_is_deterministic() {
        // Phase 16 gate invariant — classify must return identical result
        // for identical input so preview and live-apply agree.
        let memos = vec![
            memo("a.md", "eva", 0.5, 10),
            memo("b.md", "corso", 0.4, 5),
            memo("c.md", "eva", 0.6, 20),
        ];
        let policy = RetentionPolicy::AgeLimit { max_days: 7 };
        let first = classify_for_compaction(&memos, &policy);
        let second = classify_for_compaction(&memos, &policy);
        let a: std::collections::BTreeSet<_> =
            first.candidates.iter().map(|c| c.path.clone()).collect();
        let b: std::collections::BTreeSet<_> =
            second.candidates.iter().map(|c| c.path.clone()).collect();
        assert_eq!(a, b, "dry-run determinism holds — preview == apply");
    }

    #[test]
    fn default_policy_is_keep_500() {
        match RetentionPolicy::default() {
            RetentionPolicy::KeepNewest { n } => assert_eq!(n, 500),
            other => panic!("expected KeepNewest, got {other:?}"),
        }
    }
}
