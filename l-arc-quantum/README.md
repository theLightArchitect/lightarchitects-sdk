# l-arc-quantum

Typed Rust client for the [QUANTUM](https://github.com/TheLightArchitects/QUANTUM) MCP server.

QUANTUM exposes a single MCP tool — `qsTools` — with 13 actions covering a complete forensic investigation cycle. All responses are AI-generated investigation prose.

```
SCAN → SWEEP → TRACE → PROBE → THEORIZE → VERIFY → CLOSE
  └── utilities: quick, research, helix, discover, list, workflow
```

## Quick Start

```rust
use l_arc_quantum::QuantumClient;

#[tokio::main]
async fn main() -> Result<(), l_arc_core::SdkError> {
    let client = QuantumClient::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .await?;

    // Begin a forensic investigation
    let evidence = client.scan("auth token refresh intermittent failures").await?;
    println!("{}", evidence.output);

    // Form and verify a hypothesis
    let theory = client.theorize("clock skew causing JWT expiry errors", None).await?;
    let verdict = client.verify("clock skew is the root cause").await?;
    println!("{}", verdict.output);

    // Close with a finding
    let report = client.close("Clock skew confirmed — NTP drift on node-3").await?;
    println!("{}", report.output);

    Ok(())
}
```

## Note on Spawn

QUANTUM is the only sibling that requires an `mcp-server` subcommand when spawned. `QuantumClient::builder()` handles this automatically — no configuration needed.

## Actions

**Investigation cycle**: `scan` · `sweep` · `trace` · `probe` · `theorize` · `verify` · `close`
**Utilities**: `quick` · `research` · `helix` · `discover` · `list` · `workflow`
