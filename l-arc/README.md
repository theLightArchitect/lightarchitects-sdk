# l-arc

Unified Light Architects SDK — umbrella crate that re-exports all sibling MCP clients under a single dependency.

## Feature Flags

Individual sibling clients are feature-gated so you only compile what you use:

| Feature | Enables |
|---------|---------|
| `full` | All sibling clients (SOUL, CORSO, EVA, QUANTUM, SERAPH) |
| `soul` | `soul::SoulClient` |
| `corso` | `corso::CorsoClient` |
| `eva` | `eva::EvaClient` |
| `quantum` | `quantum::QuantumClient` |
| `seraph` | `seraph::SeraphClient` |
| `ayin` | `ayin::ObservableTransport` |
| `tracing-fmt` | Initialises a `tracing-subscriber` fmt subscriber (for CLI/apps) |

## Quick Start

```toml
# Cargo.toml — all sibling clients
l-arc = { git = "https://github.com/TheLightArchitects/l-arc-sdk", features = ["full"] }

# Only what you need
l-arc = { git = "https://github.com/TheLightArchitects/l-arc-sdk", features = ["soul", "quantum"] }
```

```rust
use l_arc::soul::SoulClient;
use l_arc::quantum::QuantumClient;

#[tokio::main]
async fn main() -> Result<(), l_arc_core::SdkError> {
    let soul = SoulClient::builder().build().await?;
    let quantum = QuantumClient::builder().build().await?;

    let entries = soul.helix().sibling("eva").limit(5).call().await?;
    let report = quantum.scan("why does the auth middleware fail on cold start?").await?;

    println!("{} entries · {}", entries.len(), report.output);
    Ok(())
}
```

## Architecture

All sibling clients follow the same two-level builder pattern:

```
SoulClient::builder()         // Level 1: client construction (binary path, timeout, retry)
  .build().await?             // Spawns the sibling binary, handshakes MCP initialize

client.helix()                // Level 2: per-call fluent builder
  .sibling("eva")
  .significance_min(7.0)
  .call().await?              // Serialises to JSON-RPC, sends, deserialises response
```

The underlying transport layer (`l-arc-core`) handles framing, retry, and error propagation. All sibling clients share the same `SdkError` hierarchy.
