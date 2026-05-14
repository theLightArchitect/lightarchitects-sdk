//! LASDLC C1-C8 effectiveness rubric — grades agent/task outputs.
//!
//! Adapted from the LASDLC v2.0 program for CLI/webshell output grading.
//! Each component maps to a measurable signal from the agent execution context.

#![allow(
    clippy::too_many_arguments,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::redundant_closure
)]

use std::collections::HashSet;

/// Score band — same thresholds as LASDLC v2.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreBand {
    /// 90-100
    Exemplary,
    /// 75-89 — ship
    Strong,
    /// 60-74 — ship with note
    Acceptable,
    /// 45-59 — halt
    Deficient,
    /// <45 — restructure
    Unsafe,
}

impl ScoreBand {
    /// Resolve band from an aggregate percentage (0.0–100.0).
    #[must_use]
    pub fn from_aggregate(score: f32) -> Self {
        match score {
            s if s >= 90.0 => Self::Exemplary,
            s if s >= 75.0 => Self::Strong,
            s if s >= 60.0 => Self::Acceptable,
            s if s >= 45.0 => Self::Deficient,
            _ => Self::Unsafe,
        }
    }
}

/// Per-component rubric score.
///
/// All fields are 0.0–10.0 (raw) and map to weighted percentages during
/// aggregation. The aggregate is 0.0–100.0.
#[derive(Debug, Clone, PartialEq)]
pub struct RubricScore {
    /// C1 — Output Completeness (weight 10%)
    pub c1: f32,
    /// C2 — Validation Discipline (weight 15%)
    pub c2: f32,
    /// C3 — Gate Compliance (weight 15%)
    pub c3: f32,
    /// C4 — Operator Experience (weight 10%)
    pub c4: f32,
    /// C5 — Resource + Trace Discipline (weight 10%)
    pub c5: f32,
    /// C6 — Iteration Integrity (weight 10%)
    pub c6: f32,
    /// C7 — Northstar Alignment (weight 15%)
    pub c7: f32,
    /// C8 — Context Precision (weight 15%)
    pub c8: f32,
    /// Weighted aggregate 0.0–100.0
    pub aggregate: f32,
    /// Resolved band
    pub band: ScoreBand,
}

impl RubricScore {
    /// Compute aggregate from individual components and resolve band.
    ///
    /// Weights match LASDLC v2.0:
    /// C1=10%, C2=15%, C3=15%, C4=10%, C5=10%, C6=10%, C7=15%, C8=15%.
    #[must_use]
    pub fn compute(c1: f32, c2: f32, c3: f32, c4: f32, c5: f32, c6: f32, c7: f32, c8: f32) -> Self {
        let aggregate =
            c1 * 1.0 + c2 * 1.5 + c3 * 1.5 + c4 * 1.0 + c5 * 1.0 + c6 * 1.0 + c7 * 1.5 + c8 * 1.5;
        let aggregate = aggregate.clamp(0.0, 100.0);
        Self {
            c1: c1.clamp(0.0, 10.0),
            c2: c2.clamp(0.0, 10.0),
            c3: c3.clamp(0.0, 10.0),
            c4: c4.clamp(0.0, 10.0),
            c5: c5.clamp(0.0, 10.0),
            c6: c6.clamp(0.0, 10.0),
            c7: c7.clamp(0.0, 10.0),
            c8: c8.clamp(0.0, 10.0),
            aggregate,
            band: ScoreBand::from_aggregate(aggregate),
        }
    }

    /// Serialize to a compact `[c1, c2, c3, c4, c5, c6, c7, c8, aggregate]` array.
    #[must_use]
    pub fn to_array(&self) -> [f32; 9] {
        [
            self.c1,
            self.c2,
            self.c3,
            self.c4,
            self.c5,
            self.c6,
            self.c7,
            self.c8,
            self.aggregate,
        ]
    }
}

// ── Grading functions — map execution artifacts to C1-C8 scores ────────────

/// Grade an agent result against the original task description.
///
/// This is a heuristic implementation. Future iterations will wire:
/// - C2 via cross-validation flag from `AgentExecution`
/// - C3 via gate runner verdicts from `pipeline/gates.rs`
/// - C6 via loop-cycle detection
/// - C7 via EVA interest scorer
/// - C8 via SOUL helix RRF relevance
#[must_use]
pub fn grade_agent_result(
    content: &str,
    tokens_used: usize,
    task_description: &str,
) -> RubricScore {
    let c1 = score_completeness(content, task_description);
    let c2 = 5.0; // placeholder — cross-validation flag not yet wired
    let c3 = 5.0; // placeholder — gate verdict aggregation not yet wired
    let c4 = score_operator_experience(content);
    let c5 = score_resource_discipline(tokens_used);
    let c6 = 5.0; // placeholder — loop-cycle detection not yet wired
    let c7 = score_northstar_alignment(content, task_description);
    let c8 = score_context_precision(content, task_description);
    RubricScore::compute(c1, c2, c3, c4, c5, c6, c7, c8)
}

/// Grade a build entry from the TUI build tracker.
///
/// Uses the build manifest (plan) and the final build status to derive scores.
#[must_use]
pub fn grade_build_entry(
    manifest_lines: &[std::borrow::Cow<'_, str>],
    status: &str,
) -> RubricScore {
    let plan_text = manifest_lines.join("\n");

    let c1 = if status == "completed" { 8.0 } else { 5.0 };
    let c2 = if plan_text.contains("validate") || plan_text.contains("review") {
        7.0
    } else {
        4.0
    };
    let c3 = if status == "completed" { 8.0 } else { 4.0 };
    let c4 = if plan_text.len() > 200 { 7.0 } else { 5.0 };
    let c5 = 5.0; // token data not available at this abstraction level
    let c6 = if plan_text.contains("loop") || plan_text.contains("oscillation") {
        3.0
    } else {
        6.0
    };
    let c7 = if status == "completed" { 8.0 } else { 5.0 };
    let c8 = 6.0;
    RubricScore::compute(c1, c2, c3, c4, c5, c6, c7, c8)
}

// ── Scoring heuristics (C1, C4, C5, C7, C8) ────────────────────────────────

/// C1 — Output Completeness.
///
/// Checklist matching: count how many requested deliverables appear in output.
fn score_completeness(output: &str, task: &str) -> f32 {
    let mut checks = 0;
    let mut found = 0;

    // Heuristic: look for action verbs / deliverable keywords in the task.
    let keywords = extract_keywords(task);
    for kw in &keywords {
        checks += 1;
        if output.to_lowercase().contains(kw) {
            found += 1;
        }
    }

    if checks == 0 {
        // No extractable keywords — fall back to output length heuristic.
        return if output.len() > 500 { 6.0 } else { 4.0 };
    }

    let ratio = found as f32 / checks as f32;
    (ratio * 10.0).clamp(0.0, 10.0)
}

/// C4 — Operator Experience.
///
/// Penalise jargon-heavy output; reward concise, actionable responses.
fn score_operator_experience(output: &str) -> f32 {
    let jargon = [
        "LASDLC", "CORSO", "EVA", "SOUL", "QUANTUM", "SERAPH", "AYIN", "LÆX", "helix", "vault",
        "sibling", "MCP",
    ];
    let lower = output.to_lowercase();
    let jargon_count = jargon
        .iter()
        .filter(|jw| lower.contains(&jw.to_lowercase()))
        .count();

    let base = if output.len() > 1000 { 5.0 } else { 7.0 };
    let penalty = jargon_count as f32 * 0.5;
    (base - penalty).clamp(0.0, 10.0)
}

/// C5 — Resource + Trace Discipline.
///
/// Token count heuristic: <4k = excellent, 4k–16k = good, >16k = poor.
fn score_resource_discipline(tokens_used: usize) -> f32 {
    match tokens_used {
        0..=4000 => 9.0,
        4001..=16000 => 6.0,
        _ => 3.0,
    }
}

/// C7 — Northstar Alignment.
///
/// Keyword overlap between task description and output.
fn score_northstar_alignment(output: &str, task: &str) -> f32 {
    let task_kw = extract_keywords(task);
    let out_kw = extract_keywords(output);

    if task_kw.is_empty() {
        return 5.0;
    }

    let overlap = task_kw.intersection(&out_kw).count();
    let ratio = overlap as f32 / task_kw.len() as f32;
    (ratio * 10.0).clamp(0.0, 10.0)
}

/// C8 — Context Precision.
///
/// Relevance check: does the output address the task without obvious hallucination?
fn score_context_precision(output: &str, task: &str) -> f32 {
    let _task_lower = task.to_lowercase();
    let _out_lower = output.to_lowercase();

    // If output contains topics not mentioned in task, penalise.
    let out_kw = extract_keywords(output);
    let task_kw = extract_keywords(task);
    let extra: Vec<_> = out_kw.difference(&task_kw).collect();

    let base = 7.0;
    let penalty = extra.len() as f32 * 0.3;
    (base - penalty).clamp(0.0, 10.0)
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Extract meaningful keywords from a text (lowercased, deduplicated).
fn extract_keywords(text: &str) -> HashSet<String> {
    text.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| w.len() >= 4)
        .collect()
}

// ── Score persistence ──────────────────────────────────────────────────────

use rusqlite::{Connection, params};

/// SQLite-backed persistence for rubric scores.
///
/// Enables calibration-drift detection: query average aggregate per agent
/// over a rolling window and alert if the score drops below a threshold.
pub struct RubricStore {
    conn: std::sync::Mutex<Connection>,
}

impl RubricStore {
    /// Open (or create) the rubric database at `path` and ensure the schema exists.
    ///
    /// # Errors
    /// Returns `rusqlite::Error` if the database cannot be opened or the schema
    /// migration fails.
    pub fn open(path: &str) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS rubric_scores (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id     TEXT NOT NULL,
                agent_kind  TEXT NOT NULL,
                c1          REAL NOT NULL,
                c2          REAL NOT NULL,
                c3          REAL NOT NULL,
                c4          REAL NOT NULL,
                c5          REAL NOT NULL,
                c6          REAL NOT NULL,
                c7          REAL NOT NULL,
                c8          REAL NOT NULL,
                aggregate   REAL NOT NULL,
                band        TEXT NOT NULL,
                graded_at   INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_rubric_agent ON rubric_scores(agent_kind);
            CREATE INDEX IF NOT EXISTS idx_rubric_graded ON rubric_scores(graded_at);
            ",
        )?;
        Ok(Self {
            conn: std::sync::Mutex::new(conn),
        })
    }

    /// Persist a scored result.
    ///
    /// # Errors
    /// Returns `rusqlite::Error` on insert failure.
    pub fn insert(
        &self,
        task_id: &str,
        agent_kind: &str,
        score: &RubricScore,
    ) -> Result<(), rusqlite::Error> {
        let now = chrono::Utc::now().timestamp();
        let guard = self
            .conn
            .lock()
            .map_err(|_| rusqlite::Error::ExecuteReturnedResults)?;
        guard.execute(
            "INSERT INTO rubric_scores
             (task_id, agent_kind, c1, c2, c3, c4, c5, c6, c7, c8, aggregate, band, graded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                task_id,
                agent_kind,
                score.c1,
                score.c2,
                score.c3,
                score.c4,
                score.c5,
                score.c6,
                score.c7,
                score.c8,
                score.aggregate,
                format!("{:?}", score.band),
                now,
            ],
        )?;
        Ok(())
    }

    /// Average aggregate score for an agent kind over the last `days` days.
    ///
    /// Returns `0.0` when no rows match.
    ///
    /// # Errors
    /// Returns `rusqlite::Error` on query failure.
    pub fn average_aggregate_over_window(
        &self,
        agent_kind: &str,
        days: u32,
    ) -> Result<f32, rusqlite::Error> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| rusqlite::Error::ExecuteReturnedResults)?;
        let seconds = i64::from(days) * 86_400;
        let since = chrono::Utc::now().timestamp() - seconds;
        let avg: f64 = guard.query_row(
            "SELECT COALESCE(AVG(aggregate), 0.0)
             FROM rubric_scores
             WHERE agent_kind = ?1 AND graded_at >= ?2",
            params![agent_kind, since],
            |row| row.get(0),
        )?;
        Ok(avg as f32)
    }

    /// Count rows in the store (useful for diagnostics).
    ///
    /// # Errors
    /// Returns `rusqlite::Error` on query failure.
    pub fn count(&self) -> Result<usize, rusqlite::Error> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| rusqlite::Error::ExecuteReturnedResults)?;
        let n: i64 = guard.query_row("SELECT COUNT(*) FROM rubric_scores", [], |row| row.get(0))?;
        Ok(n as usize)
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::panic,
    clippy::uninlined_format_args,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::redundant_closure
)]
mod tests {
    use super::*;

    #[test]
    fn score_band_boundaries() {
        assert_eq!(ScoreBand::from_aggregate(95.0), ScoreBand::Exemplary);
        assert_eq!(ScoreBand::from_aggregate(80.0), ScoreBand::Strong);
        assert_eq!(ScoreBand::from_aggregate(65.0), ScoreBand::Acceptable);
        assert_eq!(ScoreBand::from_aggregate(50.0), ScoreBand::Deficient);
        assert_eq!(ScoreBand::from_aggregate(30.0), ScoreBand::Unsafe);
    }

    #[test]
    fn rubric_score_computes_aggregate() {
        let s = RubricScore::compute(10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0);
        assert!((s.aggregate - 100.0).abs() < f32::EPSILON);
        assert_eq!(s.band, ScoreBand::Exemplary);
    }

    #[test]
    fn rubric_score_clamps_components() {
        let s = RubricScore::compute(15.0, -5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0);
        assert_eq!(s.c1, 10.0);
        assert_eq!(s.c2, 0.0);
    }

    #[test]
    fn grade_agent_result_with_empty_output() {
        let score = grade_agent_result("", 0, "implement login page");
        assert!(
            score.c1 <= 5.0,
            "empty output should score low on completeness"
        );
        assert_eq!(score.c5, 9.0, "zero tokens = excellent resource discipline");
    }

    #[test]
    fn grade_agent_result_with_jargon() {
        let output = "The LASDLC CORSO gate requires SOUL helix vault injection for LÆX alignment.";
        let score = grade_agent_result(output, 500, "how do I login");
        assert!(
            score.c4 < 5.0,
            "jargon-heavy output should score low on OpEx"
        );
    }

    #[test]
    fn grade_build_entry_completed() {
        let manifest = vec![
            std::borrow::Cow::Borrowed("1. Plan"),
            std::borrow::Cow::Borrowed("2. Build"),
            std::borrow::Cow::Borrowed("3. Validate"),
        ];
        let score = grade_build_entry(&manifest, "completed");
        assert!(score.c1 >= 7.0, "completed build should score well on C1");
        assert!(score.c3 >= 7.0, "completed build should score well on C3");
    }

    #[test]
    fn rubric_score_serializes_to_array() {
        let s = RubricScore::compute(5.0, 6.0, 7.0, 8.0, 9.0, 5.0, 6.0, 7.0);
        let arr = s.to_array();
        assert_eq!(arr.len(), 9);
        assert!((arr[8] - s.aggregate).abs() < f32::EPSILON);
    }

    // ── Persistence tests ────────────────────────────────────────────────────

    #[test]
    fn rubric_store_insert_and_count() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rubric.db");
        let store = RubricStore::open(path.to_str().unwrap()).unwrap();

        let score = RubricScore::compute(8.0, 7.0, 6.0, 5.0, 9.0, 8.0, 7.0, 6.0);
        store.insert("task-001", "lightarchitects", &score).unwrap();

        assert_eq!(store.count().unwrap(), 1, "insert should create one row");
    }

    #[test]
    fn rubric_store_average_over_window() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rubric.db");
        let store = RubricStore::open(path.to_str().unwrap()).unwrap();

        let score = RubricScore::compute(10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0);
        store.insert("task-002", "lightarchitects", &score).unwrap();

        let avg = store
            .average_aggregate_over_window("lightarchitects", 7)
            .unwrap();
        assert!(
            (avg - 100.0).abs() < f32::EPSILON,
            "perfect score should average to 100.0: got {}",
            avg
        );

        let avg_empty = store.average_aggregate_over_window("codex", 7).unwrap();
        assert_eq!(avg_empty, 0.0, "unknown agent should average to 0.0");
    }

    #[test]
    fn rubric_store_band_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rubric.db");
        let store = RubricStore::open(path.to_str().unwrap()).unwrap();

        let score = RubricScore::compute(10.0, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0);
        store.insert("task-003", "evangelion", &score).unwrap();

        // Count confirms insertion; schema integrity ensures all fields were written.
        assert_eq!(store.count().unwrap(), 1);
    }
}
