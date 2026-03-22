# l-arc-corso

Typed Rust client for the [CORSO](https://github.com/TheLightArchitects/CORSO) MCP server.

CORSO exposes a single MCP tool — `corsoTools` — with 26 actions covering filesystem operations, code intelligence, AI analysis (SNIFF/GUARD/FETCH/CHASE), code generation, and operational management.

## Quick Start

```rust
use l_arc_corso::CorsoClient;

#[tokio::main]
async fn main() -> Result<(), l_arc_core::SdkError> {
    let client = CorsoClient::builder().build().await?;

    // Read a source file
    let file = client.read_file("/path/to/lib.rs", None).await?;
    println!("Read {} bytes from {}", file.content.len(), file.path);

    // GUARD security audit
    let audit = client.guard("/path/to/src/").await?;
    println!("{}", audit.output);

    // Code search
    let hits = client.search_code("fn call_tool", None).await?;
    for h in hits {
        println!("{}:{} — {}", h.file, h.line, h.content);
    }
    Ok(())
}
```

## Key Types

| Type | Purpose |
|------|---------|
| `CorsoClient` | Main client — construct via `CorsoClient::builder()` |
| `FileContent` | Structured response for file read operations |
| `SearchHit` | A single code search match with file, line, and content |
| `ActionOutput` | AI-generated prose output from analysis actions |

## Actions

26 `corsoTools` actions in 5 categories:

**Filesystem**: `read_file` · `write_file` · `list_dir` · `search_code`
**Code Intelligence**: `sniff` · `code_review` · `outline`
**AI Analysis**: `guard` · `fetch` · `chase`
**Code Generation**: `hunt`
**Operations**: `scout` · `mark` · `paw` · `dig` · `play` · `watch` · `unleash` · `strike` · `chow` · `speak` · `converse` · `voice` · `helix` · `query` · `health`
