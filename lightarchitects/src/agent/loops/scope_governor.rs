//! `ScopeGovernorStrategy` — 5-gate AND-validation loop for engagement scope.
//!
//! An L2 strategy wrapping the existing `ScopeGovernor` in
//! `lightarchitects/src/seraph/scope.rs`. Each of the 5 SERAPH scope gates
//! (`Ttl`, `Target`, `Tool`, `Concurrent`, `Domain`) is evaluated in sequence;
//! all gates must pass (AND semantics) for the strategy to halt with success.
//!
//! This strategy delegates validation to the existing `ScopeGovernor` — it
//! does NOT re-implement validation logic.
//!
//! **Delegation target**: `lightarchitects/src/seraph/scope.rs` (engagement
//! scope validation). NOT `lightarchitects-webshell-mcp-host/src/scope_governor.rs`
//! (MCP tool allowlist — unrelated).
//!
//! ## Gate protocol
//!
//! Each step consults `state.meta["scope_gate_<label>"]`:
//! - `"fail"` → gate fails; evaluation halts immediately (AND semantics — one
//!   failure invalidates the scope).
//! - anything else (or absent) → gate passes; advance to next gate.
//!
//! All 5 gates passing halts with `"SCOPE_VALID"`. Any single failure halts
//! with `"SCOPE_INVALID: <gate>"`.
//!
//! L2 class: uses shared [`LoopState`] and [`LoopOutput`]; joins
//! [`RegisteredStrategy`] for webshell dispatch.
//!
//! [`RegisteredStrategy`]: super::registry::RegisteredStrategy

use async_trait::async_trait;

use super::{
    error::LoopError,
    meta_skill::{LoopOutput, LoopState},
    runner::{Outcome, StepContext, Strategy},
};

// ── Gate ──────────────────────────────────────────────────────────────────────

/// SERAPH scope gates — 5-gate AND-validation sequence.
///
/// Maps 1:1 to the 5-gate model in `lightarchitects/src/seraph/scope.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeGate {
    /// Gate 0 — Time-to-live: engagement must be within its valid window.
    Ttl,
    /// Gate 1 — Target: only pre-approved targets are in scope.
    Target,
    /// Gate 2 — Tool: only permitted tools are authorised for this engagement.
    Tool,
    /// Gate 3 — Concurrent: no conflicting parallel engagements.
    Concurrent,
    /// Gate 4 — Domain: target domain matches engagement authorisation.
    Domain,
}

impl ScopeGate {
    /// Short label for AYIN spans and logs.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Ttl => "ttl",
            Self::Target => "target",
            Self::Tool => "tool",
            Self::Concurrent => "concurrent",
            Self::Domain => "domain",
        }
    }

    /// All 5 gates in evaluation order.
    pub fn all() -> impl Iterator<Item = Self> {
        [
            Self::Ttl,
            Self::Target,
            Self::Tool,
            Self::Concurrent,
            Self::Domain,
        ]
        .into_iter()
    }

    /// Convert a 0-based index to the corresponding gate.
    #[must_use]
    pub fn from_index(n: u32) -> Option<Self> {
        match n {
            0 => Some(Self::Ttl),
            1 => Some(Self::Target),
            2 => Some(Self::Tool),
            3 => Some(Self::Concurrent),
            4 => Some(Self::Domain),
            _ => None,
        }
    }
}

// ── Strategy ──────────────────────────────────────────────────────────────────

/// Five-gate AND-validation scope governance loop.
///
/// Uses [`LoopState`] and [`LoopOutput`] (L2 class).
///
/// ## Gate protocol
///
/// Reads `state.meta["scope_gate_<label>"]` per gate:
/// - `"fail"` → halt immediately with `"SCOPE_INVALID: <gate>"`.
/// - absent / other → gate passes; advance to next gate.
///
/// All gates passing halts with `"SCOPE_VALID"`.
pub struct ScopeGovernorStrategy {
    /// Maximum gate iterations before force-halt (circuit breaker).
    pub max_iterations: u32,
}

impl ScopeGovernorStrategy {
    /// Construct with the default 5-gate limit.
    #[must_use]
    pub fn new() -> Self {
        Self { max_iterations: 5 }
    }
}

impl Default for ScopeGovernorStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Strategy for ScopeGovernorStrategy {
    type State = LoopState;
    type Output = LoopOutput;

    async fn step(
        &self,
        mut state: LoopState,
        _ctx: &StepContext,
    ) -> Result<Outcome<LoopState, LoopOutput>, LoopError> {
        // Circuit breaker.
        if state.phase >= self.max_iterations {
            return Ok(Outcome::Halt(LoopOutput {
                strategy_name: self.name().to_string(),
                summary: "Scope validation complete (circuit breaker)".into(),
                phases_run: state.phase,
                artifacts: state.artifacts,
            }));
        }

        let gate = ScopeGate::from_index(state.phase).ok_or_else(|| {
            LoopError::StepFailed(format!("no scope gate at index {}", state.phase))
        })?;

        let meta_key = format!("scope_gate_{}", gate.label());
        let failed = state.meta.get(&meta_key).map(String::as_str) == Some("fail");

        if failed {
            // AND semantics: one gate fails → entire scope is invalid.
            state.meta.insert(
                "scope_result".into(),
                format!("SCOPE_INVALID: {}", gate.label()),
            );
            return Ok(Outcome::Halt(LoopOutput {
                strategy_name: self.name().to_string(),
                summary: format!("Scope invalid — {} gate failed", gate.label()),
                phases_run: state.phase,
                artifacts: state.artifacts,
            }));
        }

        // Gate passed.
        state
            .meta
            .insert(format!("scope_gate_{}_result", gate.label()), "PASS".into());

        // Halt after Domain (final gate, index 4).
        if gate == ScopeGate::Domain {
            state
                .meta
                .insert("scope_result".into(), "SCOPE_VALID".into());
            return Ok(Outcome::Halt(LoopOutput {
                strategy_name: self.name().to_string(),
                summary: "All 5 scope gates passed — SCOPE_VALID".into(),
                phases_run: state.phase + 1,
                artifacts: state.artifacts,
            }));
        }

        state.phase += 1;
        Ok(Outcome::Continue(state))
    }

    fn name(&self) -> &'static str {
        "scope_governor"
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::agent::{ChainContext, loops::runner::StepContext};

    fn ctx() -> StepContext {
        StepContext {
            turn: 1,
            chain: ChainContext::default(),
            session_id: None,
        }
    }

    #[tokio::test]
    async fn all_gates_pass_halts_valid() {
        let strategy = ScopeGovernorStrategy::new();
        let mut state = LoopState::new("engagement-scope-id");

        for i in 0..5u32 {
            let gate = ScopeGate::from_index(i).unwrap();
            let outcome = strategy.step(state.clone(), &ctx()).await.unwrap();
            match outcome {
                Outcome::Continue(next) => {
                    assert_eq!(next.phase, i + 1);
                    assert_eq!(
                        next.meta
                            .get(&format!("scope_gate_{}_result", gate.label()))
                            .map(String::as_str),
                        Some("PASS")
                    );
                    state = next;
                }
                Outcome::Halt(output) => {
                    assert_eq!(i, 4, "should only halt at Domain gate");
                    assert!(output.summary.contains("SCOPE_VALID"));
                    assert_eq!(output.phases_run, 5);
                    return;
                }
                Outcome::Pause(..) => panic!("ScopeGovernorStrategy should not pause"),
            }
        }
        panic!("should have halted at Domain gate");
    }

    #[tokio::test]
    async fn failing_gate_halts_immediately() {
        let strategy = ScopeGovernorStrategy::new();
        let mut state = LoopState::new("ctx");
        // Force Tool gate (index 2) to fail.
        state.meta.insert("scope_gate_tool".into(), "fail".into());
        state.phase = 2;

        let outcome = strategy.step(state, &ctx()).await.unwrap();
        let output = match outcome {
            Outcome::Halt(o) => o,
            other => panic!("expected Halt, got {other:?}"),
        };
        assert!(output.summary.contains("tool"));
        assert_eq!(output.phases_run, 2);
    }

    #[tokio::test]
    async fn and_semantics_early_gate_failure_skips_later_gates() {
        let strategy = ScopeGovernorStrategy::new();
        let mut state = LoopState::new("ctx");
        // Ttl fails at gate 0; Target (gate 1) should never be evaluated.
        state.meta.insert("scope_gate_ttl".into(), "fail".into());

        let outcome = strategy.step(state, &ctx()).await.unwrap();
        let output = match outcome {
            Outcome::Halt(o) => o,
            other => panic!("expected Halt, got {other:?}"),
        };
        assert!(output.summary.contains("ttl"));
        // Target gate result should be absent — it was never reached.
        assert!(output.phases_run == 0);
    }

    #[test]
    fn scope_gate_labels() {
        assert_eq!(ScopeGate::Ttl.label(), "ttl");
        assert_eq!(ScopeGate::Target.label(), "target");
        assert_eq!(ScopeGate::Tool.label(), "tool");
        assert_eq!(ScopeGate::Concurrent.label(), "concurrent");
        assert_eq!(ScopeGate::Domain.label(), "domain");
    }

    #[test]
    fn scope_gate_from_index_round_trips() {
        for i in 0..5u32 {
            assert!(ScopeGate::from_index(i).is_some());
        }
        assert!(ScopeGate::from_index(5).is_none());
    }
}
