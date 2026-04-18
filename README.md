# lightarchitects

[![Crates.io](https://img.shields.io/crates/v/lightarchitects.svg)](https://crates.io/crates/lightarchitects)
[![docs.rs](https://img.shields.io/docsrs/lightarchitects)](https://docs.rs/lightarchitects)
[![License: MPL-2.0](https://img.shields.io/crates/l/lightarchitects)](LICENSE)
[![MSRV: 1.87](https://img.shields.io/badge/rustc-1.87%2B-blue)](https://www.rust-lang.org)

Unified Rust SDK for the Light Architects platform — typed clients for all six MCP siblings,
plus observability, cryptography, auth, training data, mathematical verification, and a
Neo4j knowledge-graph backend.

---

## Quick Start

```toml
[dependencies]
lightarchitects = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

```rust
use lightarchitects::soul::SoulClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SoulClient::builder()
        .api_key("la_your_key_here")  // get yours at api.lightarchitects.ai
        .build()?;

    // Retrieve helix entries from EVA with significance ≥ 7.0
    let entries = client
        .helix()
        .sibling("eva")
        .significance_min(7.0)
        .call()
        .await?;

    for entry in &entries {
        println!("{}: {}", entry.timestamp, entry.title);
    }
    Ok(())
}
```

---

## Modules

| Module | Purpose |
|--------|---------|
| `core` | Wire protocol, stdio transport, JSON-RPC 2.0, retry, errors |
| `crypto` | HKDF, HMAC, AES-256-GCM, Ed25519, `SecretStore` |
| `auth` | API key validation — 3-tier degradation (`NoKey` / `GracePeriod` / `Valid`) |
| `soul` | SOUL MCP client — 23 actions (`soulTools`) |
| `corso` | CORSO MCP client — 26 actions (`corsoTools`) |
| `eva` | EVA MCP client — 9 tools, dual-path adapter |
| `quantum` | QUANTUM MCP client — 13 actions (`qsTools`) |
| `seraph` | SERAPH MCP client — 18 actions, `Content-Length` framing (`penTools`) |
| `ayin` | AYIN observability — `TraceSpan` types, `ObservableTransport`, HTTP viewer client |
| `arena` | Training data factory — discover → generate → execute → score → export |
| `oracle` | Multi-model mathematical verification (Lean 4 + DeepSeek + Qwen + Kimi) |
| `helix` | Neo4j graph backend — `HelixStore`, 5 primitives, 4-signal RRF retrieval |
| `turnlog` | Tier-1 ephemeral transactional log — HMAC chaining, helix promotion |

---

## Feature Flags

All features are **on by default**. Opt individual features out with `default-features = false`:

```toml
[dependencies]
lightarchitects = { version = "0.1", default-features = false, features = ["soul", "core"] }
```

| Feature | Purpose |
|---------|---------|
| `observe` | AYIN span recording in `ObservableTransport` (zero-cost newtype when off) |
| `conversations` | JSONL conversation tracing with pivot detection |
| `http-client` | `AyinClient` HTTP viewer for the `:3742` dashboard |
| `neo4j` | Neo4j backend for `HelixStore` |
| `file` | Filesystem backend for `HelixStore` |
| `dual` | Both `neo4j` + `file` backends |
| `sqlite` | SQLite backend for SOUL storage |
| `fastembed` | FastEmbed local embedding model |
| `embedding-ollama` | Ollama embedding backend |
| `embedding-fastembed` | FastEmbed embedding backend |
| `embedding-llama-cpp` | llama.cpp embedding backend |
| `ssh` | SERAPH SSH transport via `openssh` |
| `cli` | Arena CLI (`clap`-based subcommands) |
| `keychain` | macOS Keychain `SecretStore` backend |
| `text2cypher` | LLM-driven Cypher query generation |
| `test-utils` | Mock transports and test helpers |

---

## Design

- **Two-level builder pattern** — client construction (binary path, retry config) is separate from per-call parameters.
- **`StdioTransport`** — all siblings communicate via JSON-RPC 2.0 over stdio. No HTTP between SDK and siblings.
- **`ObservableTransport<T>`** — zero-cost newtype when `observe` is disabled; fire-and-forget span I/O when enabled.
- **`TransportError`-only retry** — tool errors are not transient and are never retried.
- **Pre-spawn auth check** — keys are validated before the trust boundary opens.

---

## Workspace

| Crate | Purpose |
|-------|---------|
| `lightarchitects` | This crate — unified SDK library |
| `lightarchitects-gateway` | MCP server + Arena HTTP + Conductor task queue |
| `lightarchitects-webshell` | Local web GUI (PTY + 3D helix panel) |

---

## License

Apache-2.0. See [LICENSE](LICENSE).
