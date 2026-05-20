# ADR-010: Worker spawn strategy — ClaudeCliProvider (SDK) over explicit AgentRunner wrapper

**Status**: Accepted
**Date**: 2026-05-18
**Authors**: Kevin (architect), Claude (engineer)
**Phase prerequisite**: Phase 3 (wave_dispatcher + worker_spawn.rs)
**Related**: ADR-009 (lightsquad as SDK module), ADR-013 (serialized git ops)

---

## Context

lightsquad needs to spawn worker processes that execute per-task code-delivery work inside
isolated git worktrees. Two patterns were evaluated:

1. **ClaudeCliProvider (SDK, existing)** — `lightarchitects::agent::ClaudeCliProvider` spawns
   `claude --bare -p <prompt>` as a subprocess. Built-in `sanitize_params` (G1 gate) strips
   dangerous shell metacharacters before exec. Already battle-tested across all existing
   worktree-based SDK builds.

2. **Explicit AgentRunner wrapper** — wrap Claude Code's `--bg` agent dispatch via a new
   `AgentRunner` struct with custom retry/timeout logic.

## Decision

**ClaudeCliProvider selected** (Option 1). The canonical architecture spec (`~/Downloads/ironclaw-architecture.pdf`
§Worker-Spawn) explicitly calls out `claude --bare -p` as the canonical worker invocation surface.
`ClaudeCliProvider` is the SDK's existing wrapper for this exact command. No new abstraction needed.

```rust
// worker_spawn.rs
use lightarchitects::agent::ClaudeCliProvider;

pub async fn spawn_worker(task: &Task, worktree: &Path) -> Result<WorkerHandle, SpawnError> {
    let provider = ClaudeCliProvider::new()
        .worktree(worktree)
        .permission_matrix(&task.permission_matrix)  // Phase 2A PermissionMatrix
        .env_strip("ANTHROPIC_API_KEY")              // webshell_anthropic_key_strip invariant
        .build()?;

    provider.spawn(task.prompt()).await
}
```

## Consequences

- **No new crate or abstraction** — uses existing SDK surface; zero adapter layer.
- **G1 `sanitize_params` inherited** — injection protection is built-in, not bolted on.
- **`ANTHROPIC_API_KEY` stripping** is explicit (per `memory://feedback_webshell_anthropic_key_strip`).
  Must be called at spawn time, every spawn, no exceptions.
- **R3 empirical comparison** (AgentRunner vs `claude --bare -p`) was deferred; the canonical PDF
  spec's explicit `claude --bare -p` callout makes the decision canonical-first, not empirical-first.
  R3 findings (latency, permission gating fidelity, worktree-isolation correctness) remain Phase 3
  validation evidence — they do not need to reverse this decision.

## Alternatives rejected

- **Option 2 (explicit AgentRunner)**: Adds an adapter that wraps what `ClaudeCliProvider` already
  does. No observable benefit; adds a maintenance surface. Rejected per Cookbook §0 no-premature-abstraction.
