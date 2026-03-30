# CLAUDE.md

This file provides guidance to Claude Code when working with code in this repository.

---

## Project Overview

**lightarchitects-sdk** — the Light Architects unified Rust SDK. Typed, ergonomic clients for all five MCP siblings (SOUL, CORSO, EVA, QUANTUM, SERAPH) via stdio JSON-RPC. Internal use only; external consumers go through the Arena REST gateway (`lightarchitects/sdk/`).

**Build**: `steady-forging-lynx` (CORSO pipeline, LARGE tier)
**GitHub**: `TheLightArchitects/lightarchitects-sdk` (private)

## Workspace Structure

```
lightarchitects-sdk/
├── Cargo.toml              # Workspace root (centralized deps, workspace lints)
├── deny.toml               # cargo-deny: license + security policy
├── rustfmt.toml            # fmt: edition 2024, max_width 100
├── clippy.toml             # clippy: cognitive-complexity-threshold 10
├── Makefile                # Standard LA targets: quality / test / build / doc / fix / push
├── .github/
│   ├── workflows/ci.yml    # Quality gates + tests (macOS + Linux) + MSRV + audit + deny
│   ├── dependabot.yml      # Weekly Cargo dep updates (RustCrypto group, secret-handling group)
│   ├── CODEOWNERS          # KFT reviews all; lightarchitects-crypto requires stricter review
│   └── PULL_REQUEST_TEMPLATE.md
├── .githooks/pre-commit    # fmt + clippy gate (install: git config core.hooksPath .githooks)
│
├── lightarchitects-crypto/           # Phase 3 — Crypto foundation (HKDF, HMAC, AES-256-GCM, Ed25519, SecretStore)
├── lightarchitects-core/             # Phase 3 — Wire protocol, transport, error types, retry (stdio JSON-RPC)
├── lightarchitects-soul/             # Phase 4 — SOUL typed client (soulTools, 23 actions)
├── lightarchitects-corso/            # Phase 5 — CORSO typed client (corsoTools, 26 actions)
├── lightarchitects-eva/              # Phase 6 — EVA typed client (9 tools via dual-path adapter)
├── lightarchitects-quantum/          # Phase 7 — QUANTUM typed client (qsTools, 13 actions)
├── lightarchitects-seraph/           # Phase 8 — SERAPH typed client (penTools, 18 actions, SSH feature)
├── lightarchitects-ayin/             # Phase 9 — AYIN observability wrapper (feature = "observe")
├── lightarchitects/                  # Phase 10 — Umbrella crate (re-exports all sibling clients)
└── lightarchitects-cli/              # Phase 11 — CLI binary (sibling ping, health, version)
```

## Build Commands

```bash
# Quality gates (MANDATORY before commit)
make quality        # fmt --check + clippy (pedantic) + test

# Individual gates
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features

# Fix issues
make fix            # auto-fix fmt + clippy

# Documentation
make doc            # cargo doc --workspace --no-deps --open

# Benchmarks
cargo bench --workspace

# Security
cargo audit
cargo deny check
```

## Architecture

### Transport Layer (lightarchitects-core)

```
McpTransport trait
  ├── StdioTransport<T: SiblingId>  — spawns binary, handles framing
  │     McpFraming::Newline         — SOUL, CORSO, EVA, QUANTUM
  │     McpFraming::ContentLength   — SERAPH only
  └── MockTransport                 — test double (in-process)

SiblingId trait — per-sibling binary path, subcommand, framing
  ├── QUANTUM requires mcp-server subcommand (all others: None)
  └── default_binary_path() resolves from $HOME

SdkError { Transport, Protocol, Tool, Serialization, Config }
RetryConfig — exponential backoff, 3 attempts, 0.75 jitter
  └── retries: TransportError::Timeout + TransportError::Io only
      never retries: ToolError (tool logic is not transient)
```

### Client Pattern (octocrab-style two-level builder)

```rust
// Level 1: client construction
let client = SoulClient::builder()
    .binary_path("/custom/path")
    .timeout(Duration::from_secs(30))
    .build()?;

// Level 2: per-call fluent builder
let result = client
    .helix()
    .sibling("eva")
    .significance_min(7.0)
    .call()
    .await?;
```

### EVA Dual-Path Design

EVA exposes 9 individual tools (not a single orchestrator). lightarchitects-eva provides:
- **Orchestrator adapter**: `client.action("speak", params)` — routes to any of the 9 tools
- **Individual tool methods**: `client.speak()`, `client.visualize()`, etc. — typed, ergonomic

Both paths are available. Orchestrator path enables uniform treatment; individual methods enable type safety.

### AYIN Observability

`lightarchitects-ayin` is a thin wrapper, NOT an absorb of the ayin crate.

```rust
// Feature-gated: feature = "observe"
let transport = ObservableTransport::new(base_transport, span_factory);
// Without feature: ObservableTransport<T> = T (zero overhead, zero dep)
```

Pattern from Arena: SOUL depends on AYIN with `optional = true`.

## Key Design Decisions (from Phase 1 research)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| rmcp dependency | NO | mcp_pool.rs is battle-tested; avoids rmcp API churn |
| EVA transport | Dual-path | 9 tools → orchestrator adapter + typed methods |
| AYIN | Depend, not absorb | Feature-gated thin wrapper; Arena precedent |
| Error hierarchy | thiserror, no HTTP codes | Stdio transport ≠ HTTP; clean domain errors |
| Builder pattern | Two-level (octocrab) | Client construction ≠ call construction |
| Retry | TransportError only | Tool errors are not transient; don't retry logic |

## Coding Standards (Non-Negotiable)

Canonical: `~/.soul/helix/user/standards/builders-cookbook.md`

- NO `.unwrap()` / `.expect()` — use `?` or `match`
- NO `panic!()` — use `Result<T, E>`
- `unsafe` requires `// SAFETY:` comment
- `clippy::pedantic` as errors
- Cyclomatic complexity <= 10, functions <= 60 lines
- All public items must have doc comments (`missing_docs = deny`)
- Checked arithmetic (`checked_add`, `saturating_sub`)

## SDK Naming Rules

- Crate names: `lightarchitects-{name}` (hyphen, no underscore prefix)
- Module paths in Rust: `lightarchitects_{name}` (Rust converts hyphens automatically)
- CLI binary name: `lightarchitects` (subcommands: `ping`, `health`, `version`)
- Environment variables (system overrides): `LIGHTARCHITECTS_{SIBLING}_BIN` (e.g. `LIGHTARCHITECTS_SOUL_BIN`)
- Brand name in user-facing strings/docs: `Light Architects` (long form) or `LÆX` (when referring to the L-ARC product name)
- API types use **engineering terms only**: `tags` not `emotions`, `dimensions` not `strands`, `weight` not `significance`
- The helix data can contain those personality fields — the SDK types stay neutral

## Adding a New Crate

1. Create `lightarchitects-{name}/` with `Cargo.toml` + `src/lib.rs`
2. Add to `[workspace] members` in root `Cargo.toml` (uncomment the stub)
3. Use `dep.workspace = true` for all shared deps
4. Set `[lints] workspace = true`
5. Add `#![doc = include_str!("../README.md")]` or crate-level doc comment
6. Register in `lightarchitects` umbrella crate (`Phase 10`)

## Hook Installation

```bash
git config core.hooksPath .githooks
```
