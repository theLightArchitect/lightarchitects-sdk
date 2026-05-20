# ADR-012: Model routing surface — Sonnet/Haiku/Ollama-Cloud tier policy

**Status**: Accepted
**Date**: 2026-05-18
**Authors**: Kevin (architect), Claude (engineer)
**Phase prerequisite**: Phase 4 (supervisor.rs + light_architects.rs routing table)
**Related**: ADR-009 (SDK), ADR-010 (worker spawn), Operators Manual §Model-Routing-Doctrine

---

## Context

lightsquad spawns workers for per-task code delivery. Workers can use different models
depending on task complexity and cost constraints. The canonical architecture spec
(`~/Downloads/ironclaw-architecture.pdf` §Model-Routing) defines a three-tier model surface.

The question: what model policy governs which tasks get which model tier?

## Decision

**Three-tier model routing policy** following the canonical PDF spec's Model Routing §
and Operators Manual §Model-Routing-Doctrine:

| Tier | Model | Task type | Trigger |
|------|-------|-----------|---------|
| T1 (Supervisor) | claude-sonnet-4-6 | Orchestration: plan parsing, gate evaluation, HITL relay, reviewer | Default; never downgraded |
| T2 (Implementation) | claude-haiku-4-5 | Code delivery: per-task implementation in worktree | Default for workers |
| T2-boost | claude-sonnet-4-6 | Complex implementation: security-sensitive, novel architecture, gate-fail recovery | Escalated by supervisor when `task.complexity >= HIGH` |
| T3 (Batch research) | ollama-cloud (qwen3-coder:480b or deepseek-v3.1:671b) | Bulk ADR generation, scaffolding, research synthesis | Explicit operator opt-in; SLA-absent |

```rust
// light_architects.rs
pub fn route_model(task: &Task, mode: ExecutionMode) -> ModelId {
    match (task.domain(), task.complexity(), mode) {
        (Domain::Supervisor | Domain::Gate | Domain::Review, _, _) => ModelId::SONNET_4_6,
        (_, Complexity::High | Complexity::Critical, _) => ModelId::SONNET_4_6,
        (_, _, ExecutionMode::Autonomous) => ModelId::HAIKU_4_5,
        (_, _, ExecutionMode::Interactive) => ModelId::HAIKU_4_5,
    }
}
```

## Consequences

- **Supervisor is always Sonnet** — gate evaluation requires highest-reliability model.
- **Workers default to Haiku** — cost-efficient for the high-frequency code-delivery path.
- **Escalation is supervisor-driven** — workers don't self-escalate; supervisor sees `task.complexity`
  from the plan and routes at dispatch time.
- **Ollama Cloud is opt-in and SLA-absent** — R4 research (2026-05-18) found no SLA guarantees
  for qwen3-coder:480b or deepseek-v3.1:671b. Feature-flagged (`ollama-cloud` feature).
  Operator must explicitly configure. Not in MVP.
- **Vertex AI deferred** — not in scope (R5 deferred per plan §2.1). Feature-flagged for future.

## Alternatives rejected

- **Single-model (Sonnet for all)**: Too expensive at scale for routine implementation tasks.
  A 72-task program at T1 pricing exceeds operator cost threshold. Rejected.
- **Worker self-selection**: Workers choosing their own model creates unpredictable cost
  profiles and audit gaps. Routing must be centralized in supervisor. Rejected.
