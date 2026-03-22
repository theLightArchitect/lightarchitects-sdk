# l-arc-sdk

Unified Rust SDK for the Light Architects sibling ecosystem — typed, ergonomic clients for SOUL, CORSO, EVA, QUANTUM, and SERAPH via stdio JSON-RPC.

> Internal use only. External API consumers go through the [IronClaw REST gateway](https://github.com/TheLightArchitects/l-arc).

## Workspace Crates

| Crate | Status | Purpose |
|-------|--------|---------|
| [`l-arc-crypto`](./l-arc-crypto/) | ✅ stable | Crypto foundation: HKDF, HMAC, AES-256-GCM, Ed25519, SecretStore |
| `l-arc-core` | 🔨 building | Wire protocol, stdio transport, retry, error types |
| `l-arc-soul` | 📋 planned | SOUL typed client (soulTools, 23 actions) |
| `l-arc-corso` | 📋 planned | CORSO typed client (corsoTools, 26 actions) |
| `l-arc-eva` | 📋 planned | EVA typed client (9 tools, dual-path adapter) |
| `l-arc-quantum` | 📋 planned | QUANTUM typed client (qsTools, 13 actions) |
| `l-arc-seraph` | 📋 planned | SERAPH typed client (penTools, 18 actions) |
| `l-arc-ayin` | 📋 planned | AYIN observability wrapper (feature = `"observe"`) |
| `l-arc` | 📋 planned | Umbrella crate — re-exports all sibling clients |
| `l-arc-cli` | 📋 planned | CLI binary: `larc ping`, `larc health`, `larc version` |

## Usage

```toml
# Cargo.toml
[dependencies]
l-arc = { git = "https://github.com/TheLightArchitects/l-arc-sdk" }
```

```rust
use l_arc::soul::SoulClient;

#[tokio::main]
async fn main() -> Result<(), l_arc::Error> {
    let client = SoulClient::default()?;

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
make doc       # build and open docs
```

CI enforces: macOS + Linux test matrix, MSRV 1.87, `cargo-audit`, `cargo-deny`.

## Standards

- No `.unwrap()` / `.expect()` / `panic!()` in production
- `clippy::pedantic` as errors
- All public items documented (`missing_docs = deny`)
- Cyclomatic complexity ≤ 10, functions ≤ 60 lines
- Canonical reference: `~/.soul/helix/user/standards/builders-cookbook.md`

## License

Proprietary — © The Light Architects. All rights reserved.
