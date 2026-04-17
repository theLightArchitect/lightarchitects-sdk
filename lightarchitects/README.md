# lightarchitects

Unified Light Architects SDK — umbrella crate that re-exports all sibling MCP clients under a single dependency.

## Feature Flags

Individual sibling clients are feature-gated so you only compile what you use:

| Feature | Enables |
|---------|---------|
| `full` | All published sibling clients (SOUL, CORSO, EVA, QUANTUM) |
| `soul` | `soul::SoulClient` |
| `corso` | `corso::CorsoClient` |
| `eva` | `eva::EvaClient` |
| `quantum` | `quantum::QuantumClient` |
| `ayin` | `ayin::ObservableTransport` |
| `tracing-fmt` | Initialises a `tracing-subscriber` fmt subscriber (for CLI/apps) |

## Quick Start

```toml
# Cargo.toml — all sibling clients
lightarchitects = { git = "https://github.com/TheLightArchitects/lightarchitects-sdk", features = ["full"] }

# Only what you need
lightarchitects = { git = "https://github.com/TheLightArchitects/lightarchitects-sdk", features = ["soul", "quantum"] }
```

```rust
use lightarchitects::soul::SoulClient;
use lightarchitects::quantum::QuantumClient;

#[tokio::main]
async fn main() -> Result<(), lightarchitects_core::SdkError> {
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

The underlying transport layer (`lightarchitects-core`) handles framing, retry, and error propagation. All sibling clients share the same `SdkError` hierarchy.
