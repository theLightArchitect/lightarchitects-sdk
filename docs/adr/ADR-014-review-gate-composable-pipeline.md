# ADR-014: ReviewGate as composable GatePipeline — fail-closed, approval split from enforcement

**Status**: Accepted
**Date**: 2026-05-18
**Authors**: Kevin (architect), Claude (engineer)
**Phase prerequisite**: Phase 5 (review_gate.rs)
**Related**: ADR-009 (SDK), nearai/ironclaw `gate/mod.rs` design invariants (2026-05-18)
**Prior art (VALIDATED)**: nearai/ironclaw `crates/ironclaw_engine/src/gate/mod.rs:1-15` (Apache-2.0, accessed 2026-05-18)

---

## Context

lightsquad needs a ReviewGate that evaluates every wave's output before merging to
`feat/ironclaw-spine`. The gate must enforce canonical standards (canon check), Northstar
pillar advancement (northstar check), and security constraints (security check).

Two structural designs were evaluated:

1. **Monolithic function** — single `fn review_wave(wave: &Wave) -> GateDecision` that runs
   all checks inline. Simple, but not composable.

2. **Composable GatePipeline** — gate checks are `ExecutionGate` trait implementations
   evaluated in sequence through a `GatePipeline`. New checks added without modifying existing ones.

Additionally: nearai/ironclaw's `ironclaw_approvals` crate (accessed 2026-05-18) makes a
key architectural distinction: approval issuance (minting scoped authorization leases) is
**separate** from approval enforcement (the gate runtime that checks those leases). Current
draft of `review_gate.rs` conflates both.

## Decision

**Option 2: Composable GatePipeline with approval split.**

Three invariants from nearai/ironclaw `gate/mod.rs:1-15` are adopted verbatim:
1. `GateDecision` has **no `None` variant** — fail-closed by construction.
2. All pause paths (escalation, HITL, FixAgent re-entry) flow through the **same persistence + SSE pipeline**.
3. Approval issuance (lease service) is **separated** from approval enforcement (gate runtime).

### FixAgent first-error-step targeting (Agent-R MCTS pattern)

When a wave fails the gate, lightsquad spawns a `FixAgent` targeting the **first error step**
rather than the entire diff. This is the architectural claim of Agent-R (arXiv 2501.11425):
fix agents that target the first-error-step achieve better repair rates than agents targeting
the full diff.

> **Validation status**: Core architectural claim VALIDATED (multi-source: arXiv 2501.11425 abstract
> + HuggingFace paper metadata confirmed ID + CAID git-worktree cross-reference). Specific +5.59%
> metric is UNVALIDATED until paper-text quote obtained. Design adopts the structural pattern
> (first-error-step targeting) without claiming the specific metric.

```rust
// review_gate.rs

pub trait ExecutionGate: Send + Sync {
    fn evaluate(&self, wave: &CompletedWave) -> GateDecision;
    fn name(&self) -> &str;
}

// GateDecision: NO None variant — fail-closed by construction
pub enum GateDecision {
    Pass,
    Escalate { reason: String, hitl_required: bool },
    FixAgent { first_error_step: usize, context: FixContext },
    Reject { blocking_findings: Vec<Finding> },
    // (never: None, Unknown, Skip)
}

pub struct GatePipeline {
    gates: Vec<Box<dyn ExecutionGate>>,
    pause_channel: Arc<PauseChannel>,  // same channel for all pause paths (SSE)
}

impl GatePipeline {
    pub fn evaluate(&self, wave: &CompletedWave) -> GateDecision {
        for gate in &self.gates {
            match gate.evaluate(wave) {
                GateDecision::Pass => continue,
                decision => {
                    // All non-pass decisions routed through same pause_channel
                    self.pause_channel.record(gate.name(), &decision);
                    return decision;
                }
            }
        }
        GateDecision::Pass
    }
}

// Three composable checks (Phase 5 deliverables):
pub struct CanonCheck { /* SHA256 of canon files verified */ }
pub struct NorthstarCheck { /* Pillar advancement verified in diff */ }
pub struct SecurityCheck { /* SERAPH-sourced invariants */ }
```

### Approval split

```rust
// lease.rs (separate module — approval issuance)
pub struct LeaseService {
    active_leases: HashMap<TaskId, AuthLease>,
}
impl LeaseService {
    pub fn issue(&mut self, scope: AuthScope) -> AuthLease { ... }
}

// review_gate.rs (approval enforcement — checks lease validity, does NOT issue)
impl ExecutionGate for AuthorizationCheck {
    fn evaluate(&self, wave: &CompletedWave) -> GateDecision {
        if !self.lease_service.is_valid(&wave.task_id) {
            GateDecision::Reject { blocking_findings: vec![Finding::unauthorized()] }
        } else {
            GateDecision::Pass
        }
    }
}
```

## Consequences

- **No `None` variant** — every gate evaluation produces an explicit decision. `rustc`
  enforces exhaustiveness; no implicit pass-through.
- **Single SSE pipeline for all pauses** — HITL, escalation, and FixAgent re-entry all emit
  through `pause_channel`; operator sees a unified event stream.
- **FixAgent targets `first_error_step`** — index into the wave's commit sequence where the
  first test/lint failure occurred. FixAgent receives a narrowed diff + context rather than
  the full wave output.
- **`MAX_GATE_ITERATIONS = 3`** (Cookbook §64.3) — FixAgent loops are capped; infinite retry
  prevention. After 3 attempts the gate emits `GateDecision::Reject`.
- **Approval split** prevents `review_gate.rs` from becoming an authorization authority.
  Credentials come from `lease.rs`; `review_gate.rs` only checks them.

## Alternatives rejected

- **Monolithic function**: Non-composable. Adding a new gate check requires editing the
  function body; risk of breaking existing checks. Rejected.
- **Enum-of-gates** (gate type dispatch inside a single enum): Not a pipeline; order-of-evaluation
  is implicit and cannot be introspected. Rejected in favour of explicit Vec<Box<dyn ExecutionGate>>.
