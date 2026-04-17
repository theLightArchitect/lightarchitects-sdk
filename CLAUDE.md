# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

---

## Project Overview

**lightarchitects-sdk** — the Light Architects unified Rust SDK. Typed, ergonomic clients for all five MCP siblings (SOUL, CORSO, EVA, QUANTUM, SERAPH) via stdio JSON-RPC, plus the gateway binary, arena training factory, and oracle. Internal use only.

**Build**: `steady-forging-lynx` (CORSO pipeline, LARGE tier)
**GitHub**: `TheLightArchitects/lightarchitects-sdk` (private)
**MSRV**: 1.87 (Rust edition 2024)

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
├── lightarchitects-auth/             # Auth — API key validation, 3-tier degradation (NoKey/GracePeriod/Valid)
├── lightarchitects-gateway/          # Gateway binary — 3 modes: MCP (stdio), Arena (HTTP), Conductor (task queue)
├── lightarchitects-soul/             # Phase 4 — SOUL typed client (soulTools, 23 actions)
├── lightarchitects-corso/            # Phase 5 — CORSO typed client (corsoTools, 26 actions)
├── lightarchitects-arena/            # Training data factory for MCP tool-use LLMs (discover→generate→score→export)
├── lightarchitects-oracle/           # Multi-model mathematical verification oracle (parallel Lean/DeepSeek/Qwen/Kimi)
├── lightarchitects-eva/              # Phase 6 — EVA typed client (9 tools via dual-path adapter)
├── lightarchitects-quantum/          # Phase 7 — QUANTUM typed client (qsTools, 13 actions)
├── lightarchitects-seraph/           # Phase 8 — SERAPH typed client (penTools, 18 actions, SSH feature)
├── lightarchitects-ayin/             # Phase 9 — AYIN observability wrapper (feature = "observe")
├── lightarchitects/                  # Phase 10 — Umbrella crate (re-exports all sibling clients, feature-gated)
# Phase 11 (lightarchitects-cli) merged into lightarchitects-gateway
└── lightarchitects-helix/            # Task #49 — Neo4j graph backend (HelixDb, 5 primitives, hybrid 4-signal RRF retrieval)
```

## Build Commands

```bash
# Quality gates (MANDATORY before commit)
make quality        # fmt --check + clippy (pedantic) + test

# Individual gates
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features

# Run a single test
cargo test -p lightarchitects-soul test_name
cargo test -p lightarchitects-core newline_frame_rejects_oversized

# Fix issues
make fix            # auto-fix fmt + clippy

# Documentation
make doc            # cargo doc --workspace --no-deps --open

# Benchmarks
cargo bench --workspace

# Feature isolation (catches cross-feature contamination in umbrella crate)
make test-features    # ~102s, tests all feature combos + reqwest security gates

# Security
cargo audit
cargo deny check

# Semver (CI runs on PRs only)
cargo semver-checks --all-features
```

## Architecture

### Transport Layer (lightarchitects-core)

```
Transport trait (async, Send + Sync + 'static)
  ├── StdioTransport           — spawns binary, handles framing, auth check before spawn
  │     McpFraming::Newline         — SOUL, CORSO, EVA, QUANTUM
  │     McpFraming::ContentLength   — SERAPH only (LSP-style)
  └── MockTransport                 — test double (in-process)
                                      cross-crate via: feature = "test-utils"

SiblingId enum — maps each sibling to binary path, subcommand, framing, orchestrator tool
  ├── default_binary_path() resolves from $HOME, overridable via LIGHTARCHITECTS_{SIBLING}_BIN
  ├── mcp_subcommand() — QUANTUM requires "mcp-server" (all others: None)
  ├── framing() — returns McpFraming
  ├── orchestrator_tool() — "soulTools", "corsoTools", "evaTools", "qsTools", "penTools"
  └── all_la() — all 5 siblings in discovery order (AYIN absent — HTTP viewer, not stdio MCP)

SdkError { Transport, Protocol, Tool, Serialization, Config, ScopeViolation, Auth }
RetryConfig — exponential backoff, 3 attempts, 0.75 jitter
  └── retries: TransportError::Timeout + TransportError::Io only
      never retries: ToolError (tool logic is not transient)
```

### Auth Subsystem (lightarchitects-auth)

3-tier degradation model wired into `StdioTransport::connect(auth: Option<&AuthChecker>)`:

| Tier | Condition | Behaviour |
|------|-----------|-----------|
| `NoKey` | No API key file | `Err(SdkError::Auth)` — blocks spawn |
| `GracePeriod` | Expired cache + endpoint down | `AuthStatus::Degraded` — spawns with warning |
| `Valid` | Fresh cache or live validation | `AuthStatus::Valid` — spawns normally |

`AuthGuard` implements `AuthProvider` (from `lightarchitects-core`) and is passed to `.auth()` on any sibling client builder. Background tasks: key refresh + revocation polling. Zero soul dependency — safe to import everywhere.

```rust
use lightarchitects::auth::AuthGuard;
use lightarchitects::soul::SoulClient;

let guard = AuthGuard::new(Default::default());
let client = SoulClient::builder().auth(guard).build().await?;
```

### Gateway (lightarchitects-gateway)

Single binary with three operating modes:

- **MCP mode** (default): stdio JSON-RPC server for Claude Code — proxies to sibling subprocesses
- **Arena mode** (`serve`): HTTP API + scheduler + autonomous heartbeat agents
- **Conductor mode** (`conductor`): autonomous task execution loop

Modules: `server`, `spawner` (sibling subprocess pool), `governance` (trust + scope), `conductor`, `arena`, `channels` (Discord/Telegram webhooks), `core_tools`.

### Arena (lightarchitects-arena)

Plug-and-play training data factory for MCP tool-use LLMs. Pipeline:
1. **Auto-discovers** tool schemas from any MCP server (stdio or HTTP)
2. **Generates** training exercises (7 types, 3 difficulty levels)
3. **Executes** exercises against real servers, recording full traces
4. **Scores** traces via 8-dimensional reward system
5. **Exports** SFT/DPO/RL-ready training data as JSONL

Also includes `ayin_exporter`: reads AYIN session traces → ChatML JSONL (for training on real sessions).

### Oracle (lightarchitects-oracle)

Multi-model mathematical verification: dispatches a claim to Leanstral (Lean 4 formal proof), DeepSeek (derivation), Qwen (numerical bounds), Kimi/Cogito (reasoning) in parallel and computes consensus.

```rust
let verdict = OracleClient::builder().build()?
    .query("Prove: |round(x) - x| <= 0.5 for all real x")
    .mode(OracleMode::Prove)
    .call().await?;
```

### Client Pattern (octocrab-style two-level builder)

```rust
// Level 1: client construction
let client = SoulClient::builder()
    .binary_path("/custom/path")
    .timeout(Duration::from_secs(30))
    .build().await?;

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

### Helix Graph Backend (lightarchitects-helix)

Neo4j graph backend for the helix knowledge graph. Provides `HelixDb` trait, `HelixNeo4j` implementation, and hybrid 4-signal RRF retrieval (graph + fulltext + semantic + structural).

**5 helix primitives**: `Helix` (container node), `Step` (atomic fact), `Strand` (domain lane), `HelixLink` (edge), `SharedExperience` (convergence node).

```rust
use lightarchitects::helix::HelixStore;

let store = HelixStore::connect("bolt://localhost:7687", "neo4j", "password").await?;
let hits = store.search("consciousness").top(10).call().await?;
```

Feature flags on lightarchitects-helix:

| Feature | Enables |
|---------|---------|
| `neo4j` | `Neo4jBackend` via neo4rs Bolt driver |
| `file` | `FileBackend` — markdown vault as graph |
| `dual` | Synchronized File + Neo4j backends (implies `neo4j` + `file`) |
| `fastembed` | In-process ONNX embedding (downloads models on first use) |
| `rerank` | Signal-diversity reranker pass after RRF fusion |

### AYIN Observability

`lightarchitects-ayin` is a thin wrapper, NOT an absorb of the ayin crate.

```rust
// Feature-gated: feature = "observe"
let transport = ObservableTransport::new(base_transport, span_factory);
// Without feature: ObservableTransport<T> = T (zero overhead, zero dep)
```

### Umbrella Crate Feature Flags (lightarchitects)

All sibling clients are opt-in. `core` module (wire protocol, errors, transport) is always available.

| Feature | Enables |
|---------|---------|
| `full` | All published sibling clients (SOUL, CORSO, EVA, QUANTUM) — seraph excluded (internal only) |
| `soul` | `soul::SoulClient` |
| `helix` | `helix::HelixStore` + 5 primitives + `HelixNeo4j` (implies `soul`) |
| `corso` | `corso::CorsoClient` |
| `eva` | `eva::EvaClient` |
| `quantum` | `quantum::QuantumClient` |
| `ayin` | `ayin::ObservableTransport` |
| `ayin-http` | `ayin::AyinClient` — HTTP client for AYIN viewer at `localhost:3742` (implies `ayin`) |
| `auth` | `auth::AuthGuard` — 3-tier key validation |
| `tracing-fmt` | `init_tracing()` helper — fmt subscriber + `RUST_LOG` env filter |

CI enforces feature isolation: `reqwest` must be absent without `ayin-http`, present with it.

## Key Design Decisions (from Phase 1 research)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| rmcp dependency | NO | mcp_pool.rs is battle-tested; avoids rmcp API churn |
| EVA transport | Dual-path | 9 tools → orchestrator adapter + typed methods |
| AYIN | Depend, not absorb | Feature-gated thin wrapper; Arena precedent |
| Error hierarchy | thiserror, no HTTP codes | Stdio transport ≠ HTTP; clean domain errors |
| Builder pattern | Two-level (octocrab) | Client construction ≠ call construction |
| Retry | TransportError only | Tool errors are not transient; don't retry logic |
| Auth | Pre-spawn check | Deny access before the trust boundary opens, not after |

## Workspace Lint Exceptions

These are already `allow`-ed at the workspace level — don't re-add them per-crate:

```toml
must_use_candidate = "allow"       # SDK builder returns are caller's choice
module_name_repetitions = "allow"  # SoulClient in lightarchitects_soul is intentional
needless_pass_by_value = "allow"   # JSON Value args are conventionally taken by value
```

`clippy::unwrap_used`, `clippy::expect_used`, and `clippy::panic` are `deny` workspace-wide. Tests override with `#[allow(clippy::unwrap_used, clippy::expect_used)]`.

## Coding Standards (Non-Negotiable)

Canonical: `~/lightarchitects/soul/helix/user/standards/builders-cookbook.md`

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
- CLI binary name: `lightarchitects` (subcommands: MCP server, arena, conductor, soul, corso, eva, quantum, seraph, status, config, builds, setup, webshell)
- Environment variables (system overrides): `LIGHTARCHITECTS_{SIBLING}_BIN` (e.g. `LIGHTARCHITECTS_SOUL_BIN`)
- Brand name in user-facing strings/docs: `Light Architects` (long form) or `LÆX` (product name)
- API types use **engineering terms only**: `tags` not `emotions`, `dimensions` not `strands`, `weight` not `significance`
- The helix data can contain those personality fields — the SDK types stay neutral

## Adding a New Crate

1. Create `lightarchitects-{name}/` with `Cargo.toml` + `src/lib.rs`
2. Add to `[workspace] members` in root `Cargo.toml` (uncomment the stub)
3. Use `dep.workspace = true` for all shared deps
4. Set `[lints] workspace = true`
5. Add `#![doc = include_str!("../README.md")]` or crate-level doc comment
6. Register in `lightarchitects` umbrella crate — add feature flag + conditional `pub mod`
7. Add the new feature to `make test-features` and CI `test-features` job

## Cross-Crate Test Utils

`MockTransport` is available in dev builds via the `test-utils` feature:

```toml
[dev-dependencies]
lightarchitects-core = { path = "../lightarchitects-core", features = ["test-utils"] }
```

## CI/CD Pipeline

Same repo, dual remotes — both CI configs coexist (GitLab ignores `.github/`, GitHub ignores `.gitlab-ci.yml`):

```bash
git push origin main    # GitHub Actions
git push gitlab main    # GitLab CI
```

### GitLab Stages (`.gitlab-ci.yml`)

| Stage | What it does |
|-------|-------------|
| quality | fmt, clippy (pedantic), banned-patterns, MSRV (1.87) |
| build | Gateway binary (`lightarchitects-gateway`) + full workspace |
| security | cargo-audit, cargo-deny, secrets scan |
| sibling-scan | CORSO guard (if available), SERAPH exposure scan |
| red-team | Fuzz, injection, secret exposure, error disclosure on gateway binary |
| test | Unit (all-features), doc tests, feature isolation, reqwest security gates, semver |
| blast-score | BCRA-lite risk: deps, unsafe, public API changes, test delta |
| smoke-test | MCP handshake (initialize + tools/list) on gateway binary |
| deploy-gate | EVA HITL — manual approval with full pipeline summary |
| artifact | Provenance (SHA256 + commit + pipeline ID) + binary bundle |

### GitHub Actions (`.github/workflows/ci.yml`)

Quality gates, tests (macOS + Linux), feature-combination tests, security audit, cargo-deny, semver (PR-only), MSRV check.

## Hook Installation

```bash
git config core.hooksPath .githooks
```
