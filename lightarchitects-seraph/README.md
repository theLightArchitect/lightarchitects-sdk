# lightarchitects-seraph

Typed Rust client for the [SERAPH](https://github.com/TheLightArchitects/SERAPH) MCP server.

SERAPH exposes a single MCP tool — `penTools` — with 18 actions covering a complete penetration-testing lifecycle. All responses are AI-generated offensive-security prose.

```
Wings:         capture | scan | analyze | osint | monitor | execute
Services:      detonate | orchestrate | knowledge_search | knowledge_read | knowledge_stats
Investigation: start_investigation | advance_investigation | close_investigation | report
Utilities:     vault_sync | speak | status
```

> **Authorization required.** Every call is scope-governed by SERAPH's 5-gate `ScopeGovernor` (TTL → target → tool → concurrent → domain). Ensure `~/.seraph/scope.toml` is configured with a valid engagement before use.

## Quick Start

```rust
use lightarchitects_seraph::{SeraphClient, Wing};

#[tokio::main]
async fn main() -> Result<(), lightarchitects_core::SdkError> {
    let client = SeraphClient::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .await?;

    // Recon: discover hosts on the authorised range
    let hosts = client.scan("192.168.1.0/24").await?;
    println!("{}", hosts.output);

    // OSINT
    let intel = client.osint("example.internal", None).await?;
    println!("{}", intel.output);

    Ok(())
}
```

## Transport

SERAPH uses `Content-Length` header framing (unlike other siblings which use newline-delimited JSON). `SeraphClient` configures this automatically — no additional setup needed.

## Actions

**Wings**: `capture` · `scan` · `analyze` · `osint` · `monitor` · `execute`
**Services**: `detonate` · `orchestrate` · `knowledge_search` · `knowledge_read` · `knowledge_stats`
**Investigation**: `start_investigation` · `advance_investigation` · `close_investigation` · `report`
**Utilities**: `vault_sync` · `speak` · `status`
