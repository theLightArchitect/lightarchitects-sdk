# l-arc-sdk

Unified Rust SDK for the Light Architects sibling ecosystem — typed, ergonomic clients for SOUL, CORSO, EVA, QUANTUM, and SERAPH via stdio JSON-RPC.

> Internal use only. External API consumers go through the [IronClaw REST gateway](https://github.com/TheLightArchitects/l-arc).

## Workspace Crates

| Crate | Purpose |
|-------|---------|
| [`l-arc-crypto`](./l-arc-crypto/) | Crypto foundation: HKDF, HMAC, AES-256-GCM, Ed25519, SecretStore |
| [`l-arc-core`](./l-arc-core/) | Wire protocol, stdio transport, retry, error types |
| [`l-arc-soul`](./l-arc-soul/) | SOUL typed client (`soulTools`, 23 actions) |
| [`l-arc-corso`](./l-arc-corso/) | CORSO typed client (`corsoTools`, 26 actions) |
| [`l-arc-eva`](./l-arc-eva/) | EVA typed client (8 actions, dual-path adapter) |
| [`l-arc-quantum`](./l-arc-quantum/) | QUANTUM typed client (`qsTools`, 13 actions) |
| [`l-arc-seraph`](./l-arc-seraph/) | SERAPH typed client (`penTools`, 18 actions) |
| [`l-arc-ayin`](./l-arc-ayin/) | AYIN observability wrapper (feature = `"observe"`) |
| [`l-arc`](./l-arc/) | Umbrella crate — re-exports all sibling clients |
| [`l-arc-cli`](./l-arc-cli/) | CLI binary: `larc ping`, `larc health`, `larc version` |

## Usage

```toml
# Cargo.toml — all sibling clients
l-arc = { git = "https://github.com/TheLightArchitects/l-arc-sdk", features = ["full"] }

# Only what you need
l-arc = { git = "https://github.com/TheLightArchitects/l-arc-sdk", features = ["soul", "quantum"] }
```

```rust
use l_arc::soul::SoulClient;

#[tokio::main]
async fn main() -> Result<(), l_arc_core::SdkError> {
    let client = SoulClient::builder().build().await?;

    let entries = client
        .helix()
        .sibling("eva")
        .significance_min(7.0)
        .call()
        .await?;

    println!("{} entries found", entries.len());
    Ok(())
}
```

## Quality Gates

```bash
make quality   # fmt --check + clippy (pedantic) + tests
make fix       # auto-fix fmt + clippy
make doc       # build and open docs (zero warnings)
```

CI enforces: macOS + Linux test matrix, MSRV 1.87, `cargo-audit`, `cargo-deny`.

## Architecture

All sibling clients share a common two-level builder pattern:

```
SoulClient::builder()         // Level 1: client construction
  .timeout(Duration::from_secs(60))
  .build().await?             // Spawns binary, handshakes MCP

client.helix()                // Level 2: per-call fluent builder
  .sibling("eva")
  .significance_min(7.0)
  .call().await?              // JSON-RPC → response
```

The transport layer (`l-arc-core`) handles framing (newline or `Content-Length`), retry with exponential backoff, and typed error mapping. SERAPH is the only sibling using `Content-Length` framing — configured automatically.

## Security

The SDK is a local-process stdio JSON-RPC client. The attack surface and mitigations are documented in [`docs/security/GUARD-REPORT-phase14.md`](./docs/security/GUARD-REPORT-phase14.md). Key points:

- Binary paths use `execve(2)` directly — shell metacharacters and NUL bytes are rejected at the OS level
- All MCP response deserialization is guarded with typed errors
- Response size is bounded to 10 MiB; header count to 32
- Cryptographic material uses `Zeroizing` and `SecretString` throughout

## Standards

- No `.unwrap()` / `.expect()` / `panic!()` in production
- `clippy::pedantic` as errors
- All public items documented (`missing_docs = deny`)
- Cyclomatic complexity ≤ 10, functions ≤ 60 lines
- Canonical reference: `~/.soul/helix/user/standards/builders-cookbook.md`

## License

Proprietary — © The Light Architects. All rights reserved.
