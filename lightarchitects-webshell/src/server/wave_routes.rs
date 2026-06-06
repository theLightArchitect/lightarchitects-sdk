//! Smart-dispatch suggestion engine (cockpit d0 — Smart Dispatch card).
//!
//! Composes observable platform signals into reasons-aware, gate-aware
//! action suggestions. Each suggestion carries an action slug, a
//! human-readable reason, a Strand-Mosaic domain code, and a priority flag.
//!
//! ## Route
//!
//! `GET /api/wave/suggestions?scope=platform`
//!
//! ## Auth
//!
//! `AuthGuard` (cookie session or bearer token).
//!
//! ## Suggestion rules (rule-engine v1)
//!
//! | Trigger                              | Action     | Domain | Priority |
//! |--------------------------------------|------------|--------|----------|
//! | `drift_total > 0`                    | `/OPTIMIZE`| Q      | yes      |
//! | `hitl_open > 0`                      | `/REVIEW`  | S      | yes      |
//! | `active_builds == 0` + projects ≥ 1  | `/BUILD`   | A      | no       |
//! | `supervisors > 0`                    | `/VERIFY`  | T      | no       |
//! | `soul_present`                       | `/ENRICH`  | K      | no       |
//! | always                               | `/OBSERVE` | O      | no       |
//!
//! Rules emit independently and de-dupe by action; priority is preserved
//! across the de-dupe so a high-pri rule beats a low-pri rule for the
//! same action. Final list is capped at 6 items, priority-first then
//! insertion order.

use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{auth, server::AppState};

/// A single suggestion shown in the Smart Dispatch card.
#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    /// Action slug (e.g. `"/BUILD"`, `"/SECURE"`).
    pub action: &'static str,
    /// Human-readable reason — safe to render as `innerText`.
    pub reason: String,
    /// Strand-Mosaic domain code (`A`, `S`, `Q`, `T`, `O`, `P`, `D`, `K`).
    pub domain: &'static str,
    /// Whether this suggestion should be elevated (priority arrow).
    pub priority: bool,
}

/// Top-level response shape.
#[derive(Debug, Serialize)]
pub struct SuggestionsResponse {
    /// Ordered suggestions (priority first, then insertion order).
    pub suggestions: Vec<Suggestion>,
    /// ISO-8601 UTC capture timestamp.
    pub evaluated_at: String,
}

/// Scope query param. Only `"platform"` supported currently; reserved for
/// `project:<slug>` and `build:<id>` future scopes.
#[derive(Debug, Deserialize)]
pub struct WaveQuery {
    /// Scope selector. Defaults to `"platform"` when absent.
    #[serde(default)]
    pub scope: Option<String>,
}

/// Maximum number of suggestions returned to keep the card scannable.
const MAX_SUGGESTIONS: usize = 6;

/// `GET /api/wave/suggestions` — composed smart-dispatch coach.
pub async fn wave_suggestions_handler(
    _: auth::AuthGuard,
    Query(_q): Query<WaveQuery>,
    State(state): State<AppState>,
) -> Response {
    let signals = gather_signals(&state).await;
    let suggestions = compose(&signals);

    Json(SuggestionsResponse {
        suggestions,
        evaluated_at: Utc::now().to_rfc3339(),
    })
    .into_response()
}

/// Observable signals used by the rule engine.
struct Signals {
    active_builds: u32,
    supervisors: u32,
    drift_total: u32,
    soul_present: bool,
}

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

/// Apply rule-engine v1 to signals → ordered, capped suggestion list.
fn compose(s: &Signals) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = Vec::new();

    if s.drift_total > 0 {
        out.push(Suggestion {
            action: "/OPTIMIZE",
            reason: format!(
                "{} consecutive drift wave(s) across supervisors — review against northstar",
                s.drift_total
            ),
            domain: "Q",
            priority: true,
        });
    }

    if s.supervisors > 0 {
        out.push(Suggestion {
            action: "/VERIFY",
            reason: format!(
                "{} supervisor state(s) tracked — re-run gate verifications",
                s.supervisors
            ),
            domain: "T",
            priority: false,
        });
    }

    if s.active_builds == 0 {
        out.push(Suggestion {
            action: "/BUILD",
            reason: "no active builds — pick the next codename from the portfolio".to_owned(),
            domain: "A",
            priority: false,
        });
    }

    if s.soul_present {
        out.push(Suggestion {
            action: "/ENRICH",
            reason: "SOUL helix online — promote significance ≥7.0 sessions".to_owned(),
            domain: "K",
            priority: false,
        });
    }

    out.push(Suggestion {
        action: "/OBSERVE",
        reason: "AYIN dashboard at :3742 — verify span lineage circuit".to_owned(),
        domain: "O",
        priority: false,
    });

    if s.active_builds > 0 {
        out.push(Suggestion {
            action: "/SECURE",
            reason: format!(
                "{} active build(s) — sweep for new SERAPH HIGH findings",
                s.active_builds
            ),
            domain: "S",
            priority: false,
        });
    }

    rank_and_cap(out)
}

/// Stable-sort priority-first, then dedupe by action, then cap.
fn rank_and_cap(mut suggestions: Vec<Suggestion>) -> Vec<Suggestion> {
    suggestions.sort_by(|a, b| b.priority.cmp(&a.priority));
    let mut seen: Vec<&'static str> = Vec::with_capacity(suggestions.len());
    suggestions.retain(|s| {
        if seen.contains(&s.action) {
            false
        } else {
            seen.push(s.action);
            true
        }
    });
    suggestions.truncate(MAX_SUGGESTIONS);
    suggestions
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn empty_signals() -> Signals {
        Signals {
            active_builds: 0,
            supervisors: 0,
            drift_total: 0,
            soul_present: false,
        }
    }

    #[test]
    fn empty_platform_returns_minimum_suggestions() {
        let s = empty_signals();
        let out = compose(&s);
        // /BUILD + /OBSERVE always emit.
        assert!(out.iter().any(|x| x.action == "/BUILD"));
        assert!(out.iter().any(|x| x.action == "/OBSERVE"));
    }

    #[test]
    fn drift_elevates_optimize_to_priority() {
        let sigs = Signals {
            drift_total: 3,
            ..empty_signals()
        };
        let suggestions = compose(&sigs);
        let pick = suggestions
            .iter()
            .find(|x| x.action == "/OPTIMIZE")
            .unwrap();
        assert!(pick.priority);
    }

    #[test]
    fn priority_orders_first() {
        let s = Signals {
            drift_total: 1,
            active_builds: 1,
            ..empty_signals()
        };
        let out = compose(&s);
        assert!(out.first().unwrap().priority, "first item must be priority");
    }

    #[test]
    fn dedupes_by_action() {
        let mut input = vec![
            Suggestion {
                action: "/X",
                reason: "a".to_owned(),
                domain: "A",
                priority: true,
            },
            Suggestion {
                action: "/X",
                reason: "b".to_owned(),
                domain: "A",
                priority: false,
            },
        ];
        input = rank_and_cap(input);
        assert_eq!(input.len(), 1);
        assert!(input[0].priority, "priority instance must win");
    }

    #[test]
    fn caps_at_max() {
        let many: Vec<Suggestion> = (0..20)
            .map(|_| Suggestion {
                action: "/A",
                reason: String::new(),
                domain: "A",
                priority: false,
            })
            .collect();
        // Force unique-action variants so cap (not dedup) is what trims.
        let labels: [&'static str; 8] = ["/A", "/B", "/C", "/D", "/E", "/F", "/G", "/H"];
        let many2: Vec<Suggestion> = labels
            .iter()
            .map(|a| Suggestion {
                action: a,
                reason: String::new(),
                domain: "A",
                priority: false,
            })
            .collect();
        assert!(rank_and_cap(many).len() <= MAX_SUGGESTIONS);
        assert!(rank_and_cap(many2).len() <= MAX_SUGGESTIONS);
    }
}
