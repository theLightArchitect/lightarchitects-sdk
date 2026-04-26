# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
This project uses semantic versioning.

## [0.2.0] — 2026-04-25 — Stable Build: squishy-munching-tome

### Added

**lightarchitects-webshell v0.2.0 — Full Platform GUI**
- 5 main screens: Activity (live span feed + alerts), Build Queue (card/list view), Intake (build request form), Sitrep (platform situation report), Workspace (detailed build view)
- 33 composable Svelte 5 components covering UI infrastructure, 3D visualization, build management, agent interaction, and configuration
- 40+ REST + WebSocket endpoints: builds, SOUL vault search/compaction, sibling dispatch, conductor queue, arena status, browser state, agent control
- 6-step setup flow: Splash → Backend → Auth → Model → Init with auto-skip for inherited credentials
- PTY terminal support: portable-pty with WebSocket bridge, per-build sessions, SIGTERM→SIGKILL cleanup
- SSE event system: global broadcast (256-buffer), AYIN MCP client, helix filesystem watcher, strand activation events
- SOUL vault integration: SQLite backend with optional Neo4j attach, FastEmbed + ONNX search, hybrid BM25/embedding/RRF fusion, compaction with dry-run + apply
- Conductor task queue DAG visualization with real-time depth monitoring
- Arena training data factory status (discover → generate → execute → score → export)
- HMAC-based auth: constant-time Bearer token validation, OS keyring persistence, auto-generation on first run

**lightarchitects-webshell-ui v0.2.0 — Svelte 5 + Three.js**
- 3D helix visualization: Three.js + UnrealBloomPass, polytope manager (4D shapes), orb-spawn counter, strand fusion, promotion lineage edges, static :LINKS_TO edges from Neo4j
- Helix interactions: click → detail panel (entry + graph neighbors), hover → tooltip, activity pulse rotation, zoom/pan controls
- Copilot drawer: dual-mode (chat + terminal), slash commands, sibling dispatch, oscilloscope canvas, markdown rendering
- Skin system: dynamic sibling colors, glow/atmosphere/rails controls, 5 presets (Default, Midnight, Ember, Arctic, Neon), export/import .helix-skin.json
- Memory drawer: hot/cold memory display with toggle
- Command palette: Cmd+K searchable slash command launcher
- Settings persistence: debounced write to backend + localStorage fallback, cached BrowserStateSnapshot merge (preserves server-managed fields)
- Ambient particles: drifting helix-palette dots behind content
- Scrum report overlay
- Comprehensive E2E test suite: 48+ tests in headed Chrome with HAR capture

### Fixed

**Svelte 5 reactivity**
- `effect_update_depth_exceeded` — `$state` variables using `+=` inside `$effect` blocks create read+write cycles. Fixed with `untrack()` from Svelte to break the read dependency (`helixGeneration`, `pulseKey` in Helix3D.svelte)
- Settings persistence `$effect` hub node — replaced with `store.subscribe()` in `onMount` to bypass Svelte 5's reactive signal graph entirely
- CopilotDrawer `drawerHeightPx.set()` guard — only writes when value actually changes to prevent cross-component reactive cycles
- `lastAlertCount` in Activity.svelte changed from `$state(0)` to plain `let` — was a refactor trap inside `untrack()` blocks
- Tab navigation: Svelte 5 doesn't re-mount component when `$state` changes between truthy values — added `{#key ActiveScreen}` + `<svelte:component>` for forced re-mount
- Splash gate: replaced inline multi-store condition with `$derived(setupDone)` for reliable dependency tracking
- Settings initialization race: deferred `initialized = true` to `initializeStores().then()` to prevent redundant POST of just-loaded settings

**API contract**
- POST `/api/browser-state` returned 422 — frontend sent `PersistedSettings` shape but backend expects `BrowserStateSnapshot` with `viewport_width`, `viewport_height`, etc. Fixed with cached snapshot merge pattern
- Raw `fetch()` POST missing `Authorization: Bearer` header — added `...authHeaders()` to headers

**Three.js**
- `THREE.Clock` deprecated → `THREE.Timer` (both SplashStep and Helix3D)
- GPU memory leak on `helixGeneration` rebuild — added `disposeObject3D()` traversal for group, outerPolytopeGroup, activeStaticEdges, activeLineageLines in cleanup

**Security**
- `window.__e2e` hook exposed unconditionally — guarded with `import.meta.env.DEV` (tree-shaken in production)

---

## [Unreleased] — Build: option-a-and-do-replicated-goose

### Added

**lightarchitects-gateway**
- `src/core_tools/ui.rs` — 6 new meta-tool subactions (`ui_set_active_build`, `ui_focus_pillar`, `ui_flag_finding`, `ui_refresh_sitrep`, `ui_update_conductor`, `ui_notify`) dispatched via `LA_GUI_URL` + `LA_BUILD_ID` + `LA_NOTIFY_TOKEN` env vars
- SSRF guard on `LA_GUI_URL` (localhost-only); silent degradation when env vars absent returns `{"degraded":true}`
- 6 unit tests + 4 integration tests for ui module

**lightarchitects-webshell**
- Multi-build PTY session registry (`BuildSession`, `BuildRegistry`) — `DashMap<Uuid, Arc<BuildSession>>`
- `AgentSession::ClaudeCode(ClaudeBackend)` nested enum — `Anthropic` and `Ollama(OllamaConfig)`; Codex reserved for Phase 2
- Per-build routes: `POST /api/builds`, `GET /api/builds/:id`, `GET /api/builds/:id/events` (SSE), `GET /api/builds/:id/terminal/ws` (PTY), `POST /api/builds/:id/notify` (gateway push)
- Constant-time per-build notify token (`subtle::ConstantTimeEq`); global Bearer explicitly rejected on `/notify`
- `src/mcp_config.rs` — atomic `.mcp.json` writer injecting `lightarchitects-gui-bridge` MCP server entry into each build's CWD
- `src/mock_data.rs` — 15 stub routes (reads return plausible empty JSON; writes return 501) for Mockcli frontend screens
- `WebEvent::GatewayNotify { payload }` variant — serialized as `{"type":"gateway_notify","payload":{...}}`
- Per-build `build_spawn_env` injects `LA_GUI_URL`, `LA_BUILD_ID`, `LA_NOTIFY_TOKEN` + Ollama `ANTHROPIC_*` overrides
- 18 new integration tests across `phase_c_wire.rs`, `phase_d_stubs.rs`, `phase_e_multi_build.rs`, `phase_e_auth_profile.rs`

**Lightarchitectmockcli (frontend)**
- `src/lib/auth.ts` — `resolveToken()` from URL hash (strips hash after read), `authHeaders()` helper
- Bearer `Authorization` header on all API requests + WebSocket subprotocol
- `src/lib/ws.ts` rewritten — per-build URL, binary frames (arraybuffer), `sendText`/`sendResize`
- `src/lib/sse.ts` — per-build URL, auth header, `gateway_notify` → `selectedPillar` dispatch
- `src/screens/Copilot.svelte` — CHAT | TERMINAL toggle; xterm.js `$effect` with FitAddon + ResizeObserver; per-build WS + SSE on connect
- `src/components/OllamaConfigModal.svelte` — Ollama Cloud baseUrl/model/apiKey config modal
- `src/components/StatusBar.svelte` — auth profile indicator pill
- 7 auth tests + 10 WS tests via vitest

### Changed

**lightarchitects-webshell**
- Embedded frontend swapped: `web-figma/dist/` (React) → `../../Lightarchitectmockcli/dist/` (Svelte)
- `web-figma/` directory deleted (hard swap; recoverable via git)
- `scripts/figma-sync-check.sh` removed (no longer applicable)
- `Makefile`: `web-figma` target replaced by `mockcli` target; `quality` now runs `pnpm test:run` in Mockcli

---

## [0.1.0] - 2026-04-17

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
