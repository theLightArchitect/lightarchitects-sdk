//! Turn and cost budget enforcement for agentic loops.

use crate::agent::loops::error::LoopError;
use crate::agent::loops::profile::BudgetPolicy;

/// Spending limits for a single loop execution.
///
/// Both limits are enforced at each step boundary. The loop halts with
/// [`LoopError::BudgetExceeded`] when either threshold is crossed.
#[derive(Debug, Clone)]
pub struct Budget {
    /// Maximum number of strategy steps allowed.
    pub max_turns: u32,
    /// Maximum USD cost allowed (providers must report token cost per step).
    pub max_usd: f64,
    used_turns: u32,
    used_usd: f64,
}

impl Budget {
    /// Create a budget with the given turn and cost limits.
    #[must_use]
    pub fn new(max_turns: u32, max_usd: f64) -> Self {
        Self {
            max_turns,
            max_usd,
            used_turns: 0,
            used_usd: 0.0,
        }
    }

    /// Unlimited budget — useful in tests or when the caller enforces limits
    /// externally.
    #[must_use]
    pub fn unlimited() -> Self {
        Self::new(u32::MAX, f64::MAX)
    }

    /// Record one step with the given cost and check both limits.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError::BudgetExceeded`] if either the turn count or the
    /// accumulated cost would exceed the configured maximum after this step.
    pub fn record_step(&mut self, step_cost_usd: f64) -> Result<(), LoopError> {
        self.used_turns = self.used_turns.saturating_add(1);
        self.used_usd += step_cost_usd;
        if self.used_turns > self.max_turns || self.used_usd > self.max_usd {
            return Err(LoopError::BudgetExceeded {
                used_turns: self.used_turns,
                used_usd: self.used_usd,
            });
        }
        Ok(())
    }

    /// Turns consumed so far.
    #[must_use]
    pub fn used_turns(&self) -> u32 {
        self.used_turns
    }

    /// USD cost accumulated so far.
    #[must_use]
    pub fn used_usd(&self) -> f64 {
        self.used_usd
    }

    /// Construct a `Budget` from a [`BudgetPolicy`].
    ///
    /// This is the canonical conversion used by dispatch callsites that have
    /// resolved a [`crate::agent::loops::profile::LoopProfile`] for the current
    /// role and phase.
    #[must_use]
    pub fn from_policy(policy: &BudgetPolicy) -> Self {
        match policy {
            BudgetPolicy::Unlimited => Self::unlimited(),
            BudgetPolicy::StepCapped(max_steps) => Self::new(*max_steps, f64::MAX),
            BudgetPolicy::CostCapped(max_usd) => Self::new(u32::MAX, *max_usd),
            BudgetPolicy::StepOrCost {
                max_steps,
                max_cost_usd,
            } => Self::new(*max_steps, *max_cost_usd),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn turn_limit_enforced() {
        let mut b = Budget::new(2, f64::MAX);
        b.record_step(0.0).unwrap();
        b.record_step(0.0).unwrap();
        assert!(b.record_step(0.0).is_err());
    }

    #[test]
    fn cost_limit_enforced() {
        let mut b = Budget::new(u32::MAX, 1.0);
        b.record_step(0.6).unwrap();
        let err = b.record_step(0.6).unwrap_err();
        assert!(matches!(err, LoopError::BudgetExceeded { .. }));
    }

    #[test]
    fn unlimited_never_errors() {
        let mut b = Budget::unlimited();
        for _ in 0..1000 {
            b.record_step(99.9).unwrap();
        }
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn from_policy_unlimited() {
        let b = Budget::from_policy(&BudgetPolicy::Unlimited);
        assert_eq!(b.max_turns, u32::MAX);
        assert_eq!(b.max_usd, f64::MAX);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn from_policy_step_capped() {
        let b = Budget::from_policy(&BudgetPolicy::StepCapped(50));
        assert_eq!(b.max_turns, 50);
        assert_eq!(b.max_usd, f64::MAX);
    }

    #[test]
    fn from_policy_cost_capped() {
        let b = Budget::from_policy(&BudgetPolicy::CostCapped(2.50));
        assert_eq!(b.max_turns, u32::MAX);
        assert!((b.max_usd - 2.50).abs() < f64::EPSILON);
    }

    #[test]
    fn from_policy_step_or_cost() {
        let b = Budget::from_policy(&BudgetPolicy::StepOrCost {
            max_steps: 10,
            max_cost_usd: 1.00,
        });
        assert_eq!(b.max_turns, 10);
        assert!((b.max_usd - 1.00).abs() < f64::EPSILON);
    }

    #[test]
    fn from_policy_step_or_cost_halts_on_steps() {
        let mut b = Budget::from_policy(&BudgetPolicy::StepOrCost {
            max_steps: 2,
            max_cost_usd: f64::MAX,
        });
        b.record_step(0.0).unwrap();
        b.record_step(0.0).unwrap();
        assert!(b.record_step(0.0).is_err());
    }
}
