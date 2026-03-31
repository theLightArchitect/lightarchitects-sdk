# lightarchitects-sdk

Unified Rust SDK for the Light Architects sibling ecosystem — typed, ergonomic clients for SOUL, CORSO, EVA, QUANTUM, and SERAPH via stdio JSON-RPC.

> Internal use only. External API consumers go through the [IronClaw REST gateway](https://github.com/TheLightArchitects/lightarchitects).

## Workspace Crates

| Crate | Purpose |
|-------|---------|
| [`lightarchitects-crypto`](./lightarchitects-crypto/) | Crypto foundation: HKDF, HMAC, AES-256-GCM, Ed25519, SecretStore |
| [`lightarchitects-core`](./lightarchitects-core/) | Wire protocol, stdio transport, retry, error types |
| [`lightarchitects-soul`](./lightarchitects-soul/) | SOUL typed client (`soulTools`, 23 actions) |
| [`lightarchitects-corso`](./lightarchitects-corso/) | CORSO typed client (`corsoTools`, 26 actions) |
| [`lightarchitects-eva`](./lightarchitects-eva/) | EVA typed client (8 actions, dual-path adapter) |
| [`lightarchitects-quantum`](./lightarchitects-quantum/) | QUANTUM typed client (`qsTools`, 13 actions) |
| [`lightarchitects-seraph`](./lightarchitects-seraph/) | SERAPH typed client (`penTools`, 18 actions) |
| [`lightarchitects-ayin`](./lightarchitects-ayin/) | AYIN observability wrapper (feature = `"observe"`) |
| [`lightarchitects`](./lightarchitects/) | Umbrella crate — re-exports all sibling clients |
| [`lightarchitects-cli`](./lightarchitects-cli/) | CLI binary: `lightarchitects ping`, `lightarchitects health`, `lightarchitects version` |

## Usage

```toml
# Cargo.toml — all sibling clients
lightarchitects = { git = "https://github.com/TheLightArchitects/lightarchitects-sdk", features = ["full"] }

# Only what you need
lightarchitects = { git = "https://github.com/TheLightArchitects/lightarchitects-sdk", features = ["soul", "quantum"] }
```

```rust
use lightarchitects::soul::SoulClient;

#[tokio::main]
async fn main() -> Result<(), lightarchitects_core::SdkError> {
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

The transport layer (`lightarchitects-core`) handles framing (newline or `Content-Length`), retry with exponential backoff, and typed error mapping. SERAPH is the only sibling using `Content-Length` framing — configured automatically.

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
