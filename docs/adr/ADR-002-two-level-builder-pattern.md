# ADR-002: Two-Level Builder Pattern (octocrab-style)

**Status**: Accepted
**Date**: 2026-03-20
**Build**: steady-forging-lynx

## Context

MCP sibling clients have two distinct configuration concerns:
1. **Client construction** — binary path, timeout, retry policy (set once, rarely changed)
2. **Per-call parameters** — action name, query filters, pagination (set per call)

Conflating these leads to either bloated method signatures or per-call client construction overhead.

## Decision

All sibling clients use a two-level builder pattern modelled after `octocrab`:

```rust
// Level 1: client construction
let client = SoulClient::builder()
    .binary_path("/custom/path")
    .timeout(Duration::from_secs(30))
    .build().await?;

// Level 2: per-call fluent builder
let entries = client
    .helix()
    .sibling("eva")
    .significance_min(7.0)
    .call()
    .await?;
```

The Level 2 builder is only provided for actions with multiple optional parameters (`helix`, `query`, `helix_query` on soul; `sniff`, `guard` on corpo). Simple single-parameter actions (`scan`, `sweep` on quantum; `speak` on soul) use plain async methods.

## Rationale

1. **Separation of concerns**: Client configuration is stable; call configuration is volatile. Mixing them in a single type makes both harder.
2. **Ergonomics**: Callers that share a client across multiple calls (the common case) don't re-specify binary paths and timeouts every time.
3. **Discoverability**: The Level 2 builder surfaces available parameters via IDE completion at the call site.
4. **Test isolation**: `from_transport(T)` allows injecting a mock transport at Level 1 without touching call-site code.

## Consequences

- Each sibling client has a `ClientBuilder` and, for complex actions, an action-specific `XxxBuilder`.
- Simple actions avoid builder overhead — a `client.scan("query")` call has no extra allocations.
- The `call()` method at the end of a Level 2 builder is the single consumption point, making it easy to see where async work happens.
