# ADR-005: AYIN as a Feature-Gated Zero-Cost Wrapper

**Status**: Accepted
**Date**: 2026-03-21
**Build**: steady-forging-lynx

## Context

The AYIN observability system (`AYIN-DEV`) records `TraceSpan` data for every MCP tool call. The question is how to integrate AYIN instrumentation into `l-arc-sdk` without making it mandatory or adding overhead for callers who don't need it.

## Decision

`l-arc-ayin` is a thin wrapper crate controlled by a `observe` Cargo feature:

- **Without `observe`**: `ObservableTransport<T> = T` — a zero-cost type alias. The compiler eliminates the wrapper entirely.
- **With `observe`**: `ObservableTransport<T>` wraps `T` and writes a `TraceSpan` via a `tokio::spawn` fire-and-forget after each `send()`.

This follows the precedent set by SOUL depending on the `ayin` crate as an optional feature-gated path dep.

## Rationale

1. **Zero overhead by default**: IronClaw and other consumers that don't need trace I/O pay nothing.
2. **Non-blocking I/O**: Span writing is fire-and-forget via `tokio::spawn`. Trace I/O never blocks the MCP call path.
3. **Compile-time, not runtime, control**: Feature flags are evaluated at compile time. No runtime `if tracing_enabled { ... }` branches.
4. **Precedent**: The pattern is established by SOUL's own `observe` and `trace` feature gates.

## Consequences

- Callers that want AYIN tracing add `features = ["observe"]` to their `l-arc-ayin` dependency.
- The `l-arc` umbrella crate exposes `features = ["ayin"]` which includes `l-arc-ayin` with no sub-features — observability is opt-in even within the umbrella.
- If a span write fails (e.g., AYIN not running), the error is silently dropped — trace I/O is best-effort.
