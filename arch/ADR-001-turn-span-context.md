# ADR-001: TurnSpanContext — parent_id propagation design

**Status**: Accepted  
**Date**: 2026-05-25  
**Build**: copilot-ayin-instrumentation

## Context

Every copilot turn emits 6–8 AYIN TraceSpans (session, turn, EVA ambient ×4, tool calls,
turn.completed). Currently all spans are emitted with `parent_id: None`, making them
appear as disconnected roots in the AYIN Lineage Circuit. This prevents the circuit from
showing real agent call graphs.

## Decision

Introduce `TurnSpanContext` — a lightweight value type carrying `session_span_id` and
`turn_span_id` — threaded through the copilot call stack for the duration of one turn.

```rust
pub struct TurnSpanContext {
    pub session_span_id: String,
    pub turn_span_id: String,
}
```

Session root span is emitted once in `call_subprocess()` when `CopilotProcess` is `None`
(first turn). Subsequent turns reuse `session_span_id` from `AppState::copilot_session`.

## Consequences

- EVA ambient spans (soul_search, git_gather, grounding_wall, prelude_bytes) become
  children of their turn span — accurate since grounding precedes the AI response.
- Tool call spans become children of the turn span.
- `call_subprocess()` accepts `Option<TurnSpanContext>` for backward compat;
  non-subprocess paths pass `None`.
- Zero new external dependencies.
- `ayin-traces-utils.ts` field rename `parent_span_id` → `parent_id` matches AYIN SSE
  payload (serde_json default serialization from Rust field `parent_id`).

## Alternatives Rejected

- **Thread span IDs via global/thread-local**: fragile under async Tokio tasks; doesn't
  survive `spawn_blocking` boundaries.
- **Emit parent_id from route handler only**: would miss tool-call spans emitted deep in
  `call_subprocess()`.
