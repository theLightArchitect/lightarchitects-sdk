//! Active speaker roster with hysteresis.
//!
//! [`ActiveRoster`] tracks which siblings are currently "in" the conversation
//! (2–3 active per turn).  Hysteresis prevents thrashing: joining requires a
//! higher score than staying, so a sibling that just entered does not get
//! immediately evicted on a small score dip.

use super::types::SiblingId;

/// Score threshold for a sibling to **join** the active roster.
const JOIN_THRESHOLD: f32 = 0.5;

/// Score threshold for an already-active sibling to **stay** on the roster.
/// Lower than [`JOIN_THRESHOLD`] — prevents churn on small fluctuations.
const STAY_THRESHOLD: f32 = 0.3;

/// Minimum active roster size; at least 2 siblings are always eligible if
/// enough participants exist.
pub const MIN_ROSTER: usize = 2;

/// Maximum active roster size.
pub const MAX_ROSTER: usize = 3;

// ---------------------------------------------------------------------------
// RosterDelta
// ---------------------------------------------------------------------------

/// Change record emitted by [`ActiveRoster::update`].
#[derive(Debug, Clone, Default)]
pub struct RosterDelta {
    /// Siblings that joined the roster this turn.
    pub joined: Vec<SiblingId>,
    /// Siblings that left the roster this turn.
    pub left: Vec<SiblingId>,
}

impl RosterDelta {
    /// Returns `true` when nothing changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.joined.is_empty() && self.left.is_empty()
    }
}

// ---------------------------------------------------------------------------
// ActiveRoster
// ---------------------------------------------------------------------------

/// Maintains a hysteresis-gated set of active siblings.
#[derive(Debug, Clone)]
pub struct ActiveRoster {
    active: Vec<SiblingId>,
}

impl ActiveRoster {
    /// Create an empty roster.
    #[must_use]
    pub fn new() -> Self {
        Self { active: Vec::new() }
    }

    /// Return a reference to the current active set.
    #[must_use]
    pub fn current(&self) -> &[SiblingId] {
        &self.active
    }

    /// Update the roster given new interest scores.
    ///
    /// Each `(sibling_id, score)` pair is evaluated:
    /// - Currently active: stays if score ≥ [`STAY_THRESHOLD`].
    /// - Not active: joins if score ≥ [`JOIN_THRESHOLD`].
    ///
    /// After the gate pass, the roster is trimmed to [`MAX_ROSTER`] (highest
    /// scores first) and padded to [`MIN_ROSTER`] with the next-best
    /// candidates if too few passed the gate.
    ///
    /// Returns a [`RosterDelta`] describing what changed.
    pub fn update(&mut self, scores: &[(SiblingId, f32)]) -> RosterDelta {
        let mut candidates: Vec<(SiblingId, f32)> = scores.to_vec();
        // Sort descending by score.
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Gate pass: apply hysteresis thresholds.
        let mut new_active: Vec<SiblingId> = Vec::new();
        for (id, score) in &candidates {
            let is_active = self.active.contains(id);
            let threshold = if is_active {
                STAY_THRESHOLD
            } else {
                JOIN_THRESHOLD
            };
            if *score >= threshold {
                new_active.push(id.clone());
                if new_active.len() == MAX_ROSTER {
                    break;
                }
            }
        }

        // Pad to MIN_ROSTER if needed (highest scorers that weren't added yet).
        if new_active.len() < MIN_ROSTER {
            for (id, _) in &candidates {
                if !new_active.contains(id) {
                    new_active.push(id.clone());
                    if new_active.len() == MIN_ROSTER {
                        break;
                    }
                }
            }
        }

        // Compute delta.
        let joined: Vec<SiblingId> = new_active
            .iter()
            .filter(|id| !self.active.contains(*id))
            .cloned()
            .collect();
        let left: Vec<SiblingId> = self
            .active
            .iter()
            .filter(|id| !new_active.contains(*id))
            .cloned()
            .collect();

        self.active = new_active;
        RosterDelta { joined, left }
    }
}

impl Default for ActiveRoster {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn roster_persists_across_turns() {
        let mut roster = ActiveRoster::new();

        // Turn 1: eva and corso join.
        let scores = vec![("eva".to_string(), 0.8), ("corso".to_string(), 0.6)];
        let delta = roster.update(&scores);
        assert_eq!(delta.joined, vec!["eva", "corso"]);
        assert!(delta.left.is_empty());
        assert_eq!(roster.current(), &["eva", "corso"]);

        // Turn 2: scores unchanged — no delta expected.
        let delta2 = roster.update(&scores);
        assert!(
            delta2.is_empty(),
            "no churn when scores are stable: {delta2:?}"
        );
        assert_eq!(roster.current(), &["eva", "corso"]);
    }

    #[test]
    fn roster_evicts_after_low_score() {
        let mut roster = ActiveRoster::new();

        // Join phase.
        roster.update(&[("eva".to_string(), 0.8), ("corso".to_string(), 0.7)]);
        assert!(roster.current().contains(&"eva".to_string()));

        // Eva's score drops below STAY_THRESHOLD; corso stays.
        // Quantum joins above JOIN_THRESHOLD.
        let scores = vec![
            ("corso".to_string(), 0.65),
            ("quantum".to_string(), 0.55),
            ("eva".to_string(), 0.1), // below stay threshold
        ];
        let delta = roster.update(&scores);
        assert!(
            delta.left.contains(&"eva".to_string()),
            "eva should have been evicted: {delta:?}"
        );
        assert!(
            roster.current().contains(&"quantum".to_string()),
            "quantum should have joined"
        );
        assert!(!roster.current().contains(&"eva".to_string()));
    }

    #[test]
    fn roster_join_threshold_higher_than_stay() {
        // A sibling that was never active needs 0.5 to join.
        // A sibling already active only needs 0.3 to stay.
        let mut roster = ActiveRoster::new();

        // Join eva and corso at high scores.
        roster.update(&[("eva".to_string(), 0.8), ("corso".to_string(), 0.8)]);

        // Eva stays at 0.35 (above STAY 0.3); quantum can't join at 0.45 (below JOIN 0.5).
        let scores = vec![
            ("corso".to_string(), 0.8),
            ("eva".to_string(), 0.35),
            ("quantum".to_string(), 0.45),
        ];
        let delta = roster.update(&scores);
        assert!(
            !roster.current().contains(&"quantum".to_string()),
            "quantum below JOIN_THRESHOLD should not have joined: {delta:?}"
        );
        assert!(
            roster.current().contains(&"eva".to_string()),
            "eva above STAY_THRESHOLD should have stayed"
        );
    }

    #[test]
    fn roster_caps_at_max() {
        let mut roster = ActiveRoster::new();
        let scores = vec![
            ("eva".to_string(), 0.9),
            ("corso".to_string(), 0.8),
            ("quantum".to_string(), 0.7),
            ("seraph".to_string(), 0.65), // 4th above join threshold
        ];
        roster.update(&scores);
        assert!(
            roster.current().len() <= MAX_ROSTER,
            "roster must not exceed MAX_ROSTER={}; got {}",
            MAX_ROSTER,
            roster.current().len()
        );
    }

    #[test]
    fn roster_pads_to_min_when_few_above_threshold() {
        let mut roster = ActiveRoster::new();
        // Only one above join threshold; should pad to MIN_ROSTER=2.
        let scores = vec![
            ("eva".to_string(), 0.9),
            ("corso".to_string(), 0.1), // below join; below stay (not active)
        ];
        roster.update(&scores);
        assert!(
            roster.current().len() >= MIN_ROSTER,
            "roster should pad to MIN_ROSTER={}; got {:?}",
            MIN_ROSTER,
            roster.current()
        );
    }
}
