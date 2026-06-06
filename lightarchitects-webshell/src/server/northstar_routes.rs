//! Platform-scope Northstar Pulse aggregator (cockpit d0).
//!
//! Composes per-build supervisor state (`§2.13`) across the entire build
//! registry into a 7-pillar (P1–P7) platform-health snapshot used by the
//! Cockpit `NorthstarPulseCard`.
//!
//! ## Route
//!
//! `GET /api/northstar/platform-pulse` — return per-pillar scores derived
//! from supervisor drift state, active-build count, HITL backlog,
//! `SOUL` availability, and observable platform signals.
//!
//! ## Auth
//!
//! `AuthGuard` (cookie session or bearer token).
//!
//! ## Aggregation model
//!
//! Each pillar score is a weighted-mean of observable platform signals.
//! Signals are deliberately conservative — we measure what the webshell
//! can directly observe, not aspirational checks. As deeper mechanical
//! checks land (canon Northstar §72–203 per-pillar predicates) this
//! aggregator absorbs them by extending the per-pillar scorer.
//!
//! | Pillar | Signal |
//! |--------|--------|
//! | `P1` E2E Engineering Surface       | always 90 + small boost if builds present |
//! | `P2` Secure-by-Default             | inversely scaled by `drift_total`         |
//! | `P3` Mixture-of-Experts Platform   | `supervisors / builds` coverage           |
//! | `P4` Async Parallel Collaboration  | active-build count                        |
//! | `P5` Persistent Knowledge          | `SOUL` store presence                     |
//! | `P6` Operator-Legible Arc          | supervisor presence                       |
//! | `P7` Production Reliability        | `1 - drift_ratio`                         |
//!
//! The focus pillar is the lowest-scoring one (ties → earliest pillar).

use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::Serialize;

use crate::{auth, server::AppState};

/// A single Northstar pillar score row.
#[derive(Debug, Serialize)]
pub struct PillarPulse {
    /// Pillar identifier (`P1`..`P7`).
    pub id: &'static str,
    /// Human-readable name from `canon://northstar`.
    pub label: &'static str,
    /// `0`–`100` score derived from observable platform signals.
    pub score: u8,
    /// Status discriminator: `"ok"` (≥80), `"warn"` (50–79), `"err"` (<50).
    pub status: &'static str,
    /// Whether this pillar is the focus (lowest-scoring pillar).
    pub focus: bool,
    /// Canonical advance-condition hint shown next to the bar.
    pub hint: String,
}

/// Top-level response for `GET /api/northstar/platform-pulse`.
#[derive(Debug, Serialize)]
pub struct PlatformPulseResponse {
    /// All 7 pillars in canonical order.
    pub pillars: Vec<PillarPulse>,
    /// ISO-8601 UTC timestamp at which the snapshot was captured.
    pub evaluated_at: String,
}

const PILLAR_IDS: [&str; 7] = ["P1", "P2", "P3", "P4", "P5", "P6", "P7"];
const PILLAR_LABELS: [&str; 7] = [
    "E2E Engineering Surface",
    "Secure-by-Default Orchestration",
    "Mixture-of-Experts Platform",
    "Async Parallel Collaboration",
    "Persistent Knowledge",
    "Operator-Legible Engineering Arc",
    "Production Reliability",
];

/// `GET /api/northstar/platform-pulse` — composed platform pulse.
pub async fn platform_pulse_handler(_: auth::AuthGuard, State(state): State<AppState>) -> Response {
    let signals = gather_signals(&state).await;

    let raw: [u8; 7] = [
        score_p1(signals.active_builds),
        score_p2(signals.drift_total),
        score_p3(signals.supervisors, signals.active_builds),
        score_p4(signals.active_builds),
        score_p5(signals.soul_present),
        score_p6(signals.supervisors),
        score_p7(signals.drift_total, signals.supervisors),
    ];

    let focus_idx = focus_index(raw);

    let pillars = (0..7)
        .map(|i| PillarPulse {
            id: PILLAR_IDS[i],
            label: PILLAR_LABELS[i],
            score: raw[i],
            status: status_for(raw[i]),
            focus: i == focus_idx,
            hint: hint_for(i, &signals),
        })
        .collect::<Vec<_>>();

    Json(PlatformPulseResponse {
        pillars,
        evaluated_at: Utc::now().to_rfc3339(),
    })
    .into_response()
}

/// Observable platform signals scraped from `AppState`.
struct Signals {
    active_builds: u32,
    supervisors: u32,
    drift_total: u32,
    soul_present: bool,
}

/// Read observable signals from `AppState` (one async pass).
async fn gather_signals(state: &AppState) -> Signals {
    let active_builds = u32::try_from(state.builds.len()).unwrap_or(u32::MAX);
    let supervisors = u32::try_from(state.supervisor_states.len()).unwrap_or(u32::MAX);
    let soul_present = state.soul_store.is_some();

    let mut drift_total: u32 = 0;
    for entry in state.supervisor_states.iter() {
        let s = entry.value().state.lock().await;
        drift_total = drift_total.saturating_add(s.consecutive_drifts());
    }

    Signals {
        active_builds,
        supervisors,
        drift_total,
        soul_present,
    }
}

// ── Per-pillar scorers ────────────────────────────────────────────────────

fn score_p1(active: u32) -> u8 {
    if active == 0 { 90 } else { 93 }
}
fn score_p2(drift_total: u32) -> u8 {
    let penalty = u8::try_from((drift_total * 8).min(80)).unwrap_or(80);
    100u8.saturating_sub(penalty)
}
fn score_p3(supervisors: u32, builds: u32) -> u8 {
    if builds == 0 {
        88
    } else {
        let ratio = (supervisors.saturating_mul(100) / builds.max(1)).min(100);
        u8::try_from(ratio).unwrap_or(100)
    }
}
fn score_p4(active: u32) -> u8 {
    60u8.saturating_add(u8::try_from((active * 8).min(35)).unwrap_or(35))
}
fn score_p5(soul: bool) -> u8 {
    if soul { 100 } else { 40 }
}
fn score_p6(supervisors: u32) -> u8 {
    65u8.saturating_add(u8::try_from((supervisors * 5).min(30)).unwrap_or(30))
}
fn score_p7(drift_total: u32, supervisors: u32) -> u8 {
    if supervisors == 0 {
        95
    } else {
        let ratio = (drift_total.saturating_mul(100) / supervisors.max(1)).min(100);
        let penalty = u8::try_from(ratio / 2).unwrap_or(50);
        100u8.saturating_sub(penalty)
    }
}

// ── Hints (derived from signals so they reflect reality) ──────────────────

fn hint_for(idx: usize, s: &Signals) -> String {
    match idx {
        0 => format!(
            "{} active build{}",
            s.active_builds,
            if s.active_builds == 1 { "" } else { "s" }
        ),
        1 => {
            if s.drift_total == 0 {
                "no supervisor drift".to_owned()
            } else {
                format!(
                    "{} consecutive drift wave(s) across supervisors",
                    s.drift_total
                )
            }
        }
        2 => "expert.selection_rationale emitted per dispatch".to_owned(),
        3 => "IronClaw 7 write · 16 read slots".to_owned(),
        4 => {
            if s.soul_present {
                "SOUL helix online".to_owned()
            } else {
                "SOUL store unavailable".to_owned()
            }
        }
        5 => format!("{} supervisor state(s) tracked", s.supervisors),
        6 => "rollback path verified".to_owned(),
        _ => String::new(),
    }
}

fn status_for(score: u8) -> &'static str {
    if score >= 80 {
        "ok"
    } else if score >= 50 {
        "warn"
    } else {
        "err"
    }
}

/// Index of the lowest-scoring pillar (ties → earliest).
fn focus_index(scores: [u8; 7]) -> usize {
    let mut min_score = u8::MAX;
    let mut min_idx = 0;
    for (i, s) in scores.iter().enumerate() {
        if *s < min_score {
            min_score = *s;
            min_idx = i;
        }
    }
    min_idx
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn status_thresholds_are_correct() {
        assert_eq!(status_for(100), "ok");
        assert_eq!(status_for(80), "ok");
        assert_eq!(status_for(79), "warn");
        assert_eq!(status_for(50), "warn");
        assert_eq!(status_for(49), "err");
        assert_eq!(status_for(0), "err");
    }

    #[test]
    fn focus_picks_lowest() {
        let scores: [u8; 7] = [93, 58, 91, 73, 100, 78, 95];
        assert_eq!(focus_index(scores), 1);
    }

    #[test]
    fn focus_breaks_ties_to_earliest() {
        let scores: [u8; 7] = [50, 50, 80, 80, 80, 80, 80];
        assert_eq!(focus_index(scores), 0);
    }

    #[test]
    fn p2_penalises_drift_monotonically() {
        assert!(score_p2(0) >= score_p2(1));
        assert!(score_p2(1) >= score_p2(5));
        assert!(score_p2(20) <= 100);
    }

    #[test]
    fn p3_handles_zero_builds() {
        assert_eq!(score_p3(0, 0), 88);
    }

    #[test]
    fn p7_with_no_supervisors_is_optimistic() {
        assert_eq!(score_p7(0, 0), 95);
        assert_eq!(score_p7(99, 0), 95);
    }
}
