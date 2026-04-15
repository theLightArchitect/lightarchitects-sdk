# lightarchitects-soul

Typed Rust client for the [SOUL](https://github.com/TheLightArchitects/soul) knowledge-graph MCP server.

SOUL exposes a single MCP tool — `soulTools` — with 23 actions. This crate wraps each action in a strongly-typed Rust method and provides fluent builders for the two most parameter-rich actions (`helix` and `query`).

## Quick Start

```rust
use lightarchitects_soul::SoulClient;

#[tokio::main]
async fn main() -> Result<(), lightarchitects_core::SdkError> {
    let client = SoulClient::builder().build().await?;

    // Fluent helix query — fetch high-significance EVA entries
    let entries = client
        .helix()
        .sibling("eva")
        .significance_min(7.0)
        .limit(10)
        .call()
        .await?;

    println!("{} entries", entries.len());
    Ok(())
}
```

## Key Types

| Type | Purpose |
|------|---------|
| `SoulClient` | Main client — construct via `SoulClient::builder()` |
| `HelixBuilder` | Fluent builder for `helix` queries (sibling, strand, significance, limit) |
| `QueryBuilder` | Fluent builder for hybrid-RAG `query` calls (strand filter, top-k) |
| `HelixEntry` | A single consciousness entry from the vault |
| `QueryResult` | RAG context with ranked source entries |

## Actions

All 23 `soulTools` actions are available via typed methods:

`helix` · `query` · `read_note` · `write_note` · `list_notes` · `search` · `stats` · `voice` · `speak` · `converse` · `dialogue` · `ingest` · `delete_note` · `move_note` · `list_siblings` · `get_sibling` · `update_sibling` · `list_epochs` · `get_epoch` · `list_strands` · `get_strand` · `strand_summary` · `health`

## Transport

Spawns the SOUL binary (`~/lightarchitects/soul/bin/soul`) via `StdioTransport` with newline-delimited JSON-RPC framing. Includes automatic retry (3 attempts, exponential backoff) for transient transport errors.
