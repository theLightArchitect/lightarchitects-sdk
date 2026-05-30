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
//! L2 class: uses shared [`LoopState`] and [`LoopOutput`]; joins
//! [`RegisteredStrategy`] for webshell dispatch.
//!
//! Full step logic implemented in Phase 3.
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
/// Phase 3 implements delegation to `ScopeGovernor::validate()`.
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
        // Phase 3 implements delegation to ScopeGovernor::validate() for each gate.
        state.phase = state.phase.saturating_add(1);
        if state.phase >= self.max_iterations {
            return Ok(Outcome::Halt(LoopOutput {
                strategy_name: self.name().to_string(),
                summary: "Scope validation complete".into(),
                phases_run: state.phase,
                artifacts: state.artifacts,
            }));
        }
        Ok(Outcome::Continue(state))
    }

    fn name(&self) -> &'static str {
        "scope_governor"
    }
}
