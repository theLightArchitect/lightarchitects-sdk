# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
This project uses semantic versioning.

## [Unreleased]

### Changed

- **BREAKING**: `lightarchitects-cli` crate removed from workspace. All CLI subcommands (soul, corso, eva, quantum, seraph, status, config, builds, setup, webshell) merged into `lightarchitects-gateway`. The single `lightarchitects` binary now handles both MCP server mode and CLI subcommands.
- Renamed `LVL8`/`lvl8` strings to `conductor` throughout `lightarchitects-gateway`. Config file: `lvl8.toml` → `conductor.toml` (backward-compatible: `lvl8.toml` still loads with deprecation warning). PID/metrics filenames similarly renamed. Branch prefix: `lvl8/{id}` → `conductor/{id}`. CLI prompt text updated.
- Renamed `arena/conductor.rs` → `arena/curator.rs` to resolve naming collision with the top-level conductor module. The arena curator curates the bulletin board (deterministic, zero LLM); the conductor runs the task queue (spawns Claude Code subprocesses). All `ParsedRoutineKind::Conductor` → `Curator` references updated.

### Added

**Build: steady-forging-lynx (LARGE tier)**

#### Phase 2 — Workspace Scaffold & Rename
- Renamed workspace `la-sdk` → `lightarchitects-sdk`, `la-crypto` → `lightarchitects-crypto`
- Workspace-level lint configuration (`clippy::pedantic`, `missing_docs`, `unsafe_code = deny`)
- `rustfmt.toml` (edition 2024, max_width 100), `clippy.toml` (cognitive-complexity-threshold 10)
- `deny.toml` — license allowlist (MIT, Apache-2.0, BSD-2/3, ISC, Unicode, MPL-2.0, CDLA-Permissive-2.0)
- `dependabot.yml` — weekly Cargo dependency updates (RustCrypto group, secret-handling group)
- `.githooks/pre-commit` — fmt + clippy gate before every commit
- `Makefile` — standard LA targets: `quality`, `test`, `build`, `doc`, `fix`, `push`
- GitHub Actions CI (`ci.yml`): macOS + Linux test matrix, MSRV 1.87, `cargo-audit`, `cargo-deny`
- `PULL_REQUEST_TEMPLATE.md`, `CODEOWNERS`, `dependabot.yml`

#### Phase 3 — `lightarchitects-core` Foundation
- `lightarchitects-crypto` — scripture-forged cryptographic foundation
  - HKDF key derivation with 147 curated 1611 KJV Scripture verses as domain context
  - HMAC-SHA256 hashing and webhook signatures
  - AES-256-GCM authenticated encryption
  - Ed25519 digital signatures
  - `SecretStore` trait with macOS Keychain, file, and environment backends
  - `DerivedBytes(Zeroizing<[u8; 32]>)` — zeroes key material on drop
  - Property-based tests via `proptest`
- `lightarchitects-core` — MCP wire protocol foundation
  - `Transport` async trait over the stdio wire
  - `StdioTransport` — spawns sibling binaries via `tokio::process::Command::new` (`execve(2)`, no shell)
  - `McpFraming::Newline` (SOUL, CORSO, EVA, QUANTUM) and `McpFraming::ContentLength` (SERAPH)
  - `McpClient<T>` — retry-aware generic client (3 attempts, exponential backoff, 0.75 jitter)
  - `SiblingId` — per-sibling binary path, framing, and orchestrator tool name
  - `SdkError` — unified error hierarchy (`Transport`, `Protocol`, `Tool`, `Serialization`, `Config`)
  - `RetryConfig` — transient errors only; `ToolError` explicitly excluded from retry
  - `constants` — `MAX_RESPONSE_BYTES` (10 MiB), `MAX_CONTENT_LENGTH_HEADERS` (32)

#### Phase 4-8 — Sibling Clients
- `lightarchitects-soul` — SOUL typed client (`soulTools`, 23 actions, fluent `helix` and `query` builders)
- `lightarchitects-corso` — CORSO typed client (`corsoTools`, 26 actions, structured response types)
- `lightarchitects-eva` — EVA typed client (`evaTools`, 8 actions, dual-path: typed methods + generic adapter)
- `lightarchitects-quantum` — QUANTUM typed client (`qsTools`, 13 actions, `mcp-server` subcommand handled automatically)
- `lightarchitects-seraph` — SERAPH typed client (`penTools`, 18 actions, `Content-Length` framing)

#### Phase 9-11 — Umbrella, Observability, and CLI
- `lightarchitects-ayin` — feature-gated AYIN observability wrapper (`observe` feature = compile-time opt-in)
  - Zero-cost newtype when feature is disabled; fire-and-forget span I/O when enabled
- `lightarchitects` — umbrella crate with feature-gated sibling re-exports (`soul`, `corso`, `eva`, `quantum`, `seraph`, `ayin`, `full`)
  - `tracing-fmt` feature for CLI/app tracing initialisation
- `lightarchitects-cli` — `lightarchitects` binary with `ping`, `health`, `version` subcommands; rich table output via `lightarchitects-arena`

#### Phase 12 — Integration Tests
- `lightarchitects-core/tests/transport_integration.rs` — end-to-end JSON-RPC round-trip tests (7 scenarios)
- `lightarchitects-core/tests/retry_integration.rs` — transient error retry verification
- `lightarchitects-core/tests/sibling_ids_integration.rs` — binary path and framing validation
- All tests run without spawning real sibling binaries (mock transport)

#### Phase 13 — Observability Integration
- `lightarchitects-ayin` wired into `lightarchitects-core` transport via optional `observe` feature
- `tracing` spans emitted on every `Transport::send` call when feature is active
- AYIN span schema: actor, method, duration, outcome (success/error)

#### Phase 14 — Security Audit (GUARD)
- STRIDE threat model documented in `docs/security/GUARD-REPORT-phase14.md`
- `cargo audit` clean — one accepted WARNING (RUSTSEC-2025-0119: `number_prefix` unmaintained, lightarchitects-arena only)
- `cargo deny check` passes — licenses, bans, advisories, sources all clear
- `MAX_CONTENT_LENGTH_HEADERS = 32` enforced to bound header-parsing loop (D3 fix)
- All deserialization boundaries guarded with typed errors

#### Phase 15 — Red Team
- `read_newline_frame` and `read_content_length_frame` made generic over `AsyncBufRead + Unpin`
  - Enables adversarial unit testing with `BufReader<&[u8]>` without spawning real processes
- `lightarchitects-core/src/transport.rs` — 12 adversarial unit tests covering all framing failure modes:
  - Oversized response (> 10 MiB), malformed UTF-8, missing `Content-Length` header
  - Zero-length body, non-numeric CL value, negative CL value
  - Too-many-headers DoS (> 32), invalid UTF-8 body, happy paths
- `lightarchitects-core/tests/adversarial.rs` — integration-level adversarial tests:
  - Shell metacharacter injection safety (`execve` vs. shell — verified ENOENT, no execution)
  - NUL-byte path safety (OS rejects NUL-terminated path)
  - `MAX_RESPONSE_BYTES` and `MAX_CONTENT_LENGTH_HEADERS` constant verification
  - Deserialization boundary audit (5 call sites documented, fuzzing priority ranked)

#### Phase 16 — Documentation
- All per-crate READMEs written (`lightarchitects-soul`, `lightarchitects-corso`, `lightarchitects-eva`, `lightarchitects-quantum`, `lightarchitects-seraph`, `lightarchitects-ayin`, `lightarchitects`)
- `cargo doc --workspace --no-deps` produces zero warnings
- `cargo test --doc` passes (all doc examples verified)
- Resolved intra-doc link issues across `lightarchitects-ayin`, `lightarchitects-seraph`, `lightarchitects-quantum`, `lightarchitects-eva`, `lightarchitects-crypto`, `lightarchitects-arena`, `lightarchitects-auth`
- Binary target `doc = false` on `lightarchitects-cli` to prevent filename collision with umbrella crate

### Changed

- `lightarchitects-arena/src/discovery.rs` — resolved `ToolRegistry` intra-doc link
- `lightarchitects-cli/Cargo.toml` — `doc = false` on binary target to prevent doc filename collision

### Fixed

- `lightarchitects-core` content-length framing: header count now bounded to 32 (prevents unbounded memory allocation from malicious sibling responses)
