# ADR-002 — CategoricalExclusion: Pre-Layer-1 Gate in DecisionPipeline

**Status**: Accepted  
**Date**: 2026-05-25  
**Authors**: SERAPH (security lens, SCRUM iter-3), Claude

## Context

The 4-layer DecisionPipeline (Canon → Northstar → LightArchitect → User) is designed to minimize Layer 4 (User) escalations to 2–5 per multi-build program. However, certain decision categories must **always** escalate to the operator regardless of what Layers 1–3 say, because:

1. Canon may explicitly permit something that is nevertheless destructive in context.
2. Northstar may align with an action that has irreversible production consequences.
3. A LightArchitect specialist may be too optimistic about an action's safety.

SERAPH identified during SCRUM iter-3 that without a pre-screen, an LLM worker could craft a task prompt that tricks the DecisionPipeline into auto-approving:
- `rm -rf` on a directory
- Secret file reads (`.env`, `~/.ssh/`)
- Dependency additions (supply-chain risk — sonatype audit bypassed)
- `unsafe` blocks / FFI calls / network egress outside declared scope

## Decision

Introduce `CategoricalExclusion` as a **Layer 0 pre-screen** that fires BEFORE the 4-layer pipeline. Any decision matching a categorical exclusion unconditionally routes to `Layer 4: User Escalation` — no canon check, no Northstar check, no LightArchitect consultation.

```rust
pub enum CategoricalExclusion {
    /// Destructive filesystem operation (rm -rf, truncate, overwrite outside file_ownership)
    DestructiveOp { description: String },
    /// File touching secrets: .env, .ssh/, *.pem, *.key, secrets.*, ANTHROPIC_API_KEY patterns
    SecretTouching { path: String },
    /// Cargo.toml [dependencies] or [patch] modification (supply chain)
    DepAddition { dep_name: String },
    /// unsafe block introduced outside existing unsafe context
    UnsafeBlock { location: String },
    /// FFI extern "C" call
    FfiCall { symbol: String },
    /// Network egress to non-declared host (outside SSRF allowlist)
    NetworkEgress { host: String },
    /// Irreversible migration (DROP TABLE, DELETE without WHERE, schema migration)
    IrreversibleMigration { operation: String },
}

impl CategoricalExclusion {
    /// Returns Some(exclusion) if the decision_context matches a categorical exclusion.
    /// Returns None if the decision can proceed through the 4-layer pipeline.
    pub fn screen(decision_context: &DecisionContext) -> Option<Self> { ... }
}
```

## Consequences

**Positive**:
- Hard upper bound on autonomous agency: the platform literally cannot auto-approve destructive ops.
- OWASP LLM01 (prompt injection) mitigation: even if a worker crafts a prompt that tricks layers 1-3, `CategoricalExclusion` catches it at layer 0.
- Clear operator expectation: the list of categorical exclusions is documented and auditable.

**Negative**:
- Slightly increases Layer 4 escalation rate for legitimate tasks that happen to touch dep files.
- Mitigation: scope `DepAddition` to _net-new_ dependencies only (updating a pinned version in Cargo.lock is not a categorical exclusion).

## Relationship to Security Guardrails

This pattern is a direct extension of Security Guardrails §6.1 (IndirectInjectionShield) applied at the orchestration decision layer rather than the content/transport layer. The IndirectInjectionShield guards _content re-entering LLM context_; CategoricalExclusion guards _actions decided by LLM reasoning_.
