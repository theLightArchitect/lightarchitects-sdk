# l-arc-ayin

Feature-gated AYIN observability wrapper for `l-arc-sdk` transports.

Wraps any `l_arc_core::transport::Transport` in an `ObservableTransport` that optionally records a `TraceSpan` for every MCP tool call — routed to the AYIN observability viewer (`localhost:3742`).

## Feature Flag

Instrumentation is **compile-time opt-in** via the `observe` Cargo feature.
Without it, `ObservableTransport<T>` is a zero-cost newtype — the only overhead is one extra function call that the compiler optimises away.

```toml
# No tracing (default — zero cost)
l-arc-ayin = { path = "../l-arc-ayin" }

# Enable AYIN span recording
l-arc-ayin = { path = "../l-arc-ayin", features = ["observe"] }
```

## Usage

```rust
use l_arc_ayin::ObservableTransport;
use l_arc_core::StdioTransport;

// Works identically with or without the `observe` feature.
// When `observe` is active, every send() writes a TraceSpan to AYIN.
// The compiler eliminates the wrapper entirely when the feature is off.
```

## Design

This crate follows the same pattern used by SOUL's optional AYIN dependency: the feature flag controls whether the implementation includes real trace I/O. The type alias `ObservableTransport<T> = T` when the feature is disabled ensures callers never see type-level differences.
