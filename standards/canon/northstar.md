<!-- uuid: c1f4a9d2-8b3e-4f7c-a6d0-5e2b9c8d3a14 -->

---
title: "Light Architects Northstar"
version: "4.1"
status: ratified
date: 2026-05-16
author: Kevin Francis Tan (The Light Architect), Claude (Engineer)
ratified_by: kevin
ratification_date: "2026-05-16"
supersedes:
  - "[[northstar-v1]]"   # v1 Pillars 1+2; archived 2026-05-12 into operators-manual, then here
  - "[[northstar-v3]]"   # interim — promoted here as v4.0 (adds Part II component northstars)
canonical_uri: "canon://northstar"
laex_layer: 3
---

# Light Architects Northstar

> *"Without vision, the people perish."* — Proverbs 29:18

The single declarative target every build, decision, and review aligns to.

Northstar is not a vision statement — vision statements exist to inspire. Northstar is a **mechanically checkable assertion** about what "shipped" means. Each Pillar and each Component section defines a quality bar the platform always strives toward. Even after the checks are satisfied, they become the **floor** — the baseline from which hardening continues, not the ceiling that permits coasting.

The goal is to become the **goto vibe coding, agentic orchestration, and engineering platform** — the copilot layer that sits between operator intent and coding agent execution, operating reliably enough that others can build on it and comfortably depend on it.

**How to use this document**: Part I contains the seven Vision Pillars — the operator-visible outcomes. Part II contains the Component Northstars — one section per major platform layer, covering what quality looks like inside each building block. Every build cites which Pillar(s) it advances in its `northstar_lineage:` block. Every component section cites which Pillar(s) it serves.

---

## Platform Intent

**Primary ICP**: solo developers and small agentic teams shipping production engineering work end-to-end, without requiring terminal fluency or agent-specific CLI knowledge.

**Optimization target**: user value delivered to the operator. Not Anthropic-application signal, not internal sibling ergonomics, not theoretical correctness.

**What sets this platform apart**: it does not ask the operator to manage agents — it manages agents on the operator's behalf, with security by default, persistent memory, asynchronous parallel work, and a legible UI. The operator describes intent; the platform executes, gates, enriches, and reports.

**LÆX active layer**: 3 (product gate). Any contested Northstar interpretation requires LÆX Layer 3 review before resolution.

**Pillar relationship**: the seven Pillars are AND, not OR. A deliverable may advance one Pillar while preserving the others. A deliverable that violates any Pillar does not ship. There is no partial credit.

---

## Full-Stack Layer Map

Every Pillar and every Component section maps to one or more stack layers. This table is the fullstack index.

| Layer | Name | Key Components |
|-------|------|---------------|
| **L0** | Infrastructure | Binary, SQLite, Neo4j backend, crypto primitives, keychain, codesign, deploy |
| **L1** | Transport | MCP stdio JSON-RPC, HTTP API (Axum/Arena), WebSocket, SSE, PTY bridge |
| **L2** | Agent Runtime | AgentRunner, PermissionMatrix, CompactionEngine, WorktreeIsolation, CognitivePhase |
| **L3** | Orchestration | squad_comms A2A bus, Governor/Worker/Gatekeeper, file ownership, parallel dispatch |
| **L4** | Plugin Layer | Agents (8 domain types), Skills (29+), Marketplace, skill-execution-spec, `lightarchitects` plugin |
| **L5** | Expert System | CORSO, EVA, SOUL, QUANTUM, SERAPH, AYIN, LÆX — 7 siblings + MoE routing |
| **L6** | Knowledge & Research | SOUL helix (Neo4j), turnlog, EVA enrichment, QUANTUM evidence chain, Oracle multi-model |
| **L7** | Security | Docker sandbox, ScopeGovernor, SERAPH scanning, secret lifecycle, supply chain |
| **L8** | Observability | AYIN traces + HTTP dashboard, Activity stream, span attributes, session spans |
| **L9** | Interface | Webshell UI (Svelte 5), TUI (AgentRunner CLI mode), permission cards, 3-zoom model |
| **L10** | Integration | External MCP clients (Claude Code, Codex), GitHub, Docker, Neo4j Aura, Context7, Firecrawl, Sonatype |

---

# Part I — Vision Pillars

The seven strategic outcomes the platform must always maintain. Build by build, every commit either advances one of these or at minimum does not regress any.

---

### Pillar 1 — E2E Engineering Surface *(L9, L10)*

**Assertion**: an operator completes a full engineering session — plan → build → verify → deploy → observe → enrich — entirely from the webshell, with zero terminal fallback. The webshell is not a thin client over a CLI. It is the complete engineering surface.

**The platform works in tandem with coding agents**: Claude Code, Codex, and future agentic runtimes are the *execution layer*. lightarchitects is the *orchestration layer* — the MCP copilot that receives operator intent, routes it to agents, gates their output, surfaces results back to the operator, and enriches the knowledge graph. The operator talks to the platform; the platform drives the agents.

**Mechanical check**: `terminal_window_open_count === 0` across a session that closes all 6 OD-10 E-gates:

| Gate | Capability verified |
|------|-------------------|
| E1 | Code editing — Read, Edit, Write, Bash surface in webshell with diffs |
| E2 | Agent setup — persona, model, skill selection without CLI |
| E3 | Vault ops — helix read/write, memory enrichment without `soul` CLI |
| E4 | Skill customization — agent skill/persona swap mid-session via UI |
| E5 | Permission gating — tool approval cards inline; deny/approve without terminal |
| E6 | Deploy + smoke test — `make deploy`, codesign, MCP reconnect, smoke test from webshell |

**MCP surface check**: `lightarchitects` binary responds to `initialize` + all registered tool calls over stdio. Claude Code and equivalent MCP clients can drive the platform without a human at the keyboard.

**Plugin surface parity check**: every MCP plugin and tool available to the operator in their native coding agent (`~/.claude/plugins/`, Codex equivalent, or any future runtime) is also invocable from the webshell UI without re-authentication or re-configuration. Switching from Claude Code to the webshell preserves tool access. The operator does not lose capability by choosing the UI surface. See §O (Tool Surface Parity) for full mechanical verification.

**Escape hatch rule**: the terminal is permitted for power users. The assertion is that the webshell *can* close the loop, not that the terminal is removed.

**Status (2026-05-16)**: ✅ Realized — EEF PROGRAM Waves E1–E6 shipped; 10/10 Playwright E2E pass.

---

### Pillar 2 — Secure-by-Default Agent Orchestration *(L2, L7)*

**Assertion**: the platform is the security perimeter between the operator and the coding agents it orchestrates. Every agent session starts with zero trust and explicit grants only. Misconfiguration must produce a visible, fail-secure denial — never a silent escalation of privilege.

**Mechanical checks**:
1. **Container isolation** — agent sessions spawn in containers: `--cap-drop ALL`, non-root, no host network, `--read-only` filesystem + explicit tmpfs mounts. Absent Docker: native PTY + visible degraded-trust warning.
2. **Permission matrix — deny-fail-secure** — no approval channel wired → invocation DENIED, never auto-permitted. `with_streaming_approval()` for subprocess; `with_approval_tx()` for TUI. Auto-permit on absent channel = CRITICAL violation.
3. **Approval honest-confirmation** — `tx.send(approved).is_ok() && approved`. If the bridge receiver dropped (timeout), UI reports `deny` — not the operator's `approve` intent.
4. **Secret isolation** — `env -u ANTHROPIC_API_KEY` on every subprocess spawn. Agent environment never inherits operator shell credentials.
5. **ScopeGovernor enforcement** — SERAPH's 5-gate compiled governance (TTL + target + tool + concurrent + domain) on all offensive capability invocations. Halt, never default.
6. **Supply chain gate** — `cargo-deny` + sonatype-guide on every dependency addition. No dep merges without a passing supply chain gate.
7. **Error response opacity** — HTTP handlers never return `e.to_string()` or raw stderr. Opaque codes + `tracing::warn!` only.
8. **Graceful SIGTERM cleanup** — all containers, SQLite handles, temp files cleaned on shutdown. Zero orphan processes.

---

### Pillar 3 — Mixture-of-Experts Platform Architecture *(L0, L3, L5)*

**Assertion**: lightarchitects ships as a **single unified binary** exposing expert capabilities (CORSO, EVA, SOUL, QUANTUM, SERAPH, AYIN, LÆX, and future experts) through a routing layer supporting **sparse activation**, **cross-expert composition by direct function call**, and **observable expert selection**. Install once, upgrade once, zero-serialization expert composition.

**Mechanical checks**:
1. **Single binary** — `which lightarchitects` returns one path; `lightarchitects --help` enumerates all expert modes. No separate per-sibling binaries required.
2. **Function-call composition** — cross-expert calls complete in <1ms wall-clock (AYIN `expert.composition_latency_ms` span). No MCP round-trip between in-binary experts.
3. **Sparse activation** — 2–7 experts active in a typical session; visible via AYIN `expert.activated_count`.
4. **Observable routing** — every dispatch emits `expert.selected` + `expert.selection_rationale` span attributes.
5. **Expert specialization preserved** — each expert within its Canon XXX strand; no domain absorption without Canon XXXIX ratification.
6. **Security boundary explicit** — `platform ↔ external-agent` is the perimeter; in-binary isolation uses Rust types + capability tokens + ScopeGovernor, not OS process boundaries.
7. **Single deploy target** — `make deploy` → one binary at `~/.lightarchitects/bin/lightarchitects`.

**Migration window**: standalone sibling binaries remain operational during the consolidation program. `lightarchitects mcp --as <expert>` ships first as additive; deprecation and removal follow per-expert, not big-bang.

---

### Pillar 4 — Asynchronous Parallel Agent Collaboration *(L3)*

**Assertion**: when the platform orchestrates multiple agents, they work in parallel with typed asynchronous communication — not sequentially, not via polling, not via shared mutable state. Three tiers coordinate: the **Operator** states intent; the **Copilot supervisor** (EVA) holds the operator's build northstar, evaluates ongoing work against it, synthesizes execution-tier results, and escalates only what requires a human decision; the **Execution tier** (sub-agents and coding agents) runs in parallel, reporting back asynchronously. Within the execution tier: a Governor drives Workers; a Gatekeeper reviews without blocking. The operator never needs to monitor agents directly — the copilot supervisor does that.

**Mechanical checks**:
1. **A2A JSONL protocol** — all agent-to-agent messages are JSONL (`{"v":1,...}`). One JSON object per line. Streamable, appendable, diffable. No custom framing.
2. **Message type completeness** — all 12 typed message types implemented and routed: `CONTEXT`, `QUERY_STATE`, `STATE_RESPONSE`, `FILE_CLAIM`, `FILE_RELEASE`, `WAVE_START`, `WAVE_COMPLETE`, `GATE_REVIEW`, `PHASE_GATE_RESULT`, `BLOCKER`, `HEARTBEAT`, `SQUAD_INJECT`.
3. **Governor-Worker-Gatekeeper separation** — one Governor per session; Workers claim tasks exclusively by file ownership; Gatekeeper is stateless. Roles never merged in one agent turn.
4. **Parallel dispatch in one message** — independent tasks launched in a single dispatcher message (OPS-8.1). Target: 10× speed, 90% token reduction.
5. **File ownership protocol** — `FILE_CLAIM` before editing any unowned file; `FILE_RELEASE` on commit; conflicts surface as `BLOCKER` messages, not silent overwrites.
6. **Non-blocking permission gating** — `StreamingApprovalGate` NDJSON wire: `{"type":"permission_request","call_id":"...","tool":"...","timeout_secs":N}` → `{"type":"approve|deny","call_id":"..."}`. Only the requesting agent suspends; others continue.
7. **WebSocket control channel** — `GET /api/builds/:id/agent/ws`; max 8 connections (`MAX_AGENT_WS = 8`); two concurrent tasks per connection (writer owns sink, reader owns stream).
8. **squad_comms HTTP bridge** — coordination actions route through `/api/coordination/*`; bearer token from `~/.lightarchitects/webshell/.token`; structured errors if webshell is unreachable.

---

### Pillar 5 — Persistent Knowledge & Session Continuity *(L6)*

**Assertion**: the platform remembers. Agent knowledge enriches a shared graph that persists across sessions. Session state survives gateway restarts. An operator returning to a build picks up where they left off — not from zero.

**Mechanical checks**:
1. **Session persistence** — SQLite WAL mode; session state written before any ack returned to operator. A gateway restart that loses in-flight session state is a P5 violation.
2. **turnlog — HMAC-chained** — every turn written to the ephemeral log with `HMAC-SHA256(prev_hash, turn_data)` before execution. Broken chain detected on read → `CHAIN_BROKEN` error, not silent ignore.
3. **helix enrichment (EVA 8-layer schema)** — significant work enriched via: raw observation → pattern → decision → outcome → lesson → principle → identity → meta-insight. Direct Neo4j inserts bypassing the schema are P5 violations.
4. **4-signal RRF retrieval** — semantic similarity (fastembed) + keyword match (Cypher full-text) + graph proximity (path) + recency (timestamp). Single-signal retrieval is a P5 regression.
5. **Cross-session injection** — relevant helix entries (decisions, lessons, build history) injected into agent prompts at session boundary. Cold-start from zero is a P5 violation when prior context exists.
6. **Memory closure on significance** — agent output at significance ≥ 7.0 triggers helix enrichment prompt. Mandatory, not optional.
7. **CompactionEngine** — transparently compacts prior turns at context budget threshold. Operator sees seamless continuation; context overflow errors are P5 violations.

---

### Pillar 6 — Operator-Legible Engineering Arc *(L9)*

**Assertion**: an operator with no prior knowledge of the underlying agent, git topology, or squad architecture can determine — from the webshell UI alone — what is happening, what happened, and what needs their attention. The UI renders intent as legible engineering narrative.

**Operator experience arc**:
```
describe intent in natural language
    ↓  EVA interprets; executes or asks one clarifying question
    ↓  activity stream renders tool events within 500ms
    ↓  permission cards appear inline — no modal, no tab switch
    ↓  build phases advance; gate results render with pass/fail + test delta
    ↓  operator owns the artifacts in their git history
```

**Mechanical checks**:
1. **Portfolio legibility (3-second scan)** — all active builds, current phase, blockers, trajectory readable within 3s from `/#/builds`. Top-bar counters (PROJECTS · RUNNING · QUEUED · ALERTS) accurate and real-time.
2. **Build legibility** — from build detail view, without a terminal: active git branch, delta from `main` (file count, line delta as narrative text), gate status [A+S+Q+C+O+P+K+D+T+R], test count, next required action.
3. **Activity stream (≤500ms latency)** — agent tool events render in Activity stream within 500ms of emission via SSE. Event shows: type, target, duration, outcome — not raw JSON.
4. **Inline permission cards** — `permission_request` events render in-context; approve/deny without navigation. Card shows: tool name, summary, agent, timeout countdown.
5. **Git legibility (non-expert path)** — git state translated to engineering narrative: "3 files changed in auth layer · 2 tests added · 1 gate pending."
6. **Vibe entry** — operator types natural-language intent; EVA executes or asks one clarifying question. No configuration, no flag selection.
7. **Three zoom levels** — Portfolio (`/#/builds`), Build (`/#/builds/:codename`), Turn (single agent turn); navigate without context loss or terminal.
8. **Ambient knowledge** — Helix 3D panel updates as agents enrich the graph; no vault query required to see what the platform knows.

---

### Pillar 7 — Production-Grade Platform Reliability *(L0)*

**Assertion**: the platform operates reliably enough that others can depend on it. Deterministic behavior, clear error surfaces, and recovery paths the operator can follow without a terminal. Not just "works in demos" — works for real operators on Tuesday.

**Mechanical checks**:
1. **Binary deploy with rollback** — `make deploy` saves `.prev`; `make rollback` restores + logs. No rollback path = P7 violation.
2. **Graceful degradation — not silent failure** — every unavailable dependency (Docker, Neo4j, CLI binary missing `--stream-events`) produces a visible, actionable warning. Silence on degradation = P7 violation.
3. **Fallback mode fidelity** — when `--stream-events` unavailable, webshell falls back to single-shot `run` per `SendMessage`. Fallback path tested in CI — not dead code.
4. **Session durability** — SQLite WAL. Crash between two transactions must not corrupt session state. Sessions resumable after restart without operator intervention within session TTL.
5. **Deterministic error surfaces** — every operator-visible error has: (a) opaque error code, (b) human-readable summary, (c) recovery action where one exists. No raw stack traces in HTTP responses.
6. **Data loss prevention** — `CompactionEngine` commits compacted summary to SQLite before discarding original turn data. HMAC chain in turnlog detects silent truncation.
7. **Resource cleanup** — SIGTERM registry cleans containers, SQLite handles, temp files, child processes. `docker ps` post-exit = zero platform-owned containers.
8. **Test pyramid** — 6-suite pyramid (unit / integration / property / E2E / regression / smoke) per Builders Cookbook §27. ≥90% line coverage on production paths. E2E always `headless: false`.
9. **Northstar predicate stability** — any regression against P1–P6 mechanical checks is a P7 violation. Reliability means the platform consistently meets its own Northstar.

---

# Part II — Component Northstars

One section per major platform building block. Each defines what quality looks like *inside* that component — the implementation-level complement to the Vision Pillars above. Builds cite relevant Component sections in their phase-level exit criteria.

---

## §A — CLI & Binary Interface *(L0)*

**Assertion**: the `lightarchitects` binary is the operator's single point of entry. Every subcommand is discoverable, deterministic, and produces structured output suitable for both human reading and machine parsing. No raw panics reach the terminal. Non-interactive equivalents exist for every interactive operation.

**Serves**: P3 (single binary), P7 (reliability)

**Mechanical checks**:
1. `lightarchitects --help` renders all subcommands and expert modes in <100ms without a running server.
2. `lightarchitects --version` emits semver + commit SHA + deploy timestamp in machine-parseable format.
3. Every error exit: (a) human-readable message, (b) exit code ≠ 0, (c) recovery hint where applicable. Zero raw panics.
4. All interactive prompts have non-interactive flag equivalents for scripted use.
5. `lightarchitects webshell start` prints the webshell URL and token path to stdout before any browser open. No silent startup.
6. MCP `initialize` handshake completes in <500ms from cold binary.
7. `make rollback` restores the previous binary, logs the action, codesigns. No silent replacement.

---

## §B — Terminal UI — TUI & CLI Agent *(L2)*

**Assertion**: the TUI (`lightarchitects-cli` AgentRunner) is a complete, first-class interface — not a lesser fallback from the webshell. It exposes the same agent capabilities, the same permission model, the same compaction behavior, and the same AYIN observability. TUI and webshell are peers.

**Serves**: P1 (terminal as valid complete surface for power users), P4 (same protocol), P7 (reliability)

**Mechanical checks**:
1. AgentRunner cold-start in <2s (AYIN span: `agent_runner.init_ms`).
2. `CompactionEngine` runs transparently: brief status display, then seamless continuation. Silent context loss = violation.
3. Permission prompts use the same `permission_request`/`approve`/`deny` wire protocol as webshell — not a bespoke TUI prompt.
4. Mid-turn `steer` available via keyboard shortcut without aborting the turn.
5. AYIN spans from TUI sessions are structurally identical to webshell session spans — observability layer is surface-agnostic.
6. `SIGINT` handling: in-progress tool call completes if <2s remaining; turn is marked `interrupted` in turnlog; WS closes cleanly.
7. Interrupt → redirect → resume cycle preserves session context without operator re-explaining the task.

---

## §C — Webshell UI — Frontend *(L9)*

**Assertion**: the webshell is a production-grade web application suitable for an 8-hour engineering session. Every screen has a clear purpose, a loading/error/empty state, accessible markup, and performance characteristics that do not degrade over a long session.

**Serves**: P1 (E2E surface), P6 (legibility), P7 (reliability)

**Mechanical checks**:
1. `pnpm exec svelte-check --threshold error` clean on every merge. TypeScript errors are merge-blocking.
2. `pnpm test:run` (full component + E2E suite) passes before any merge.
3. All interactive elements have accessible labels. `aria-hidden="true"` elements that are keyboard-focusable use `inert` — not just `aria-hidden` (WCAG `aria-hidden-focus` violation).
4. Every route has an empty state, loading state, and error state. No blank screen on first load.
5. OPS/DISPATCH/BUILDS/HELIX tabs reflect real server state within 1s of page load.
6. Playwright E2E always `headless: false`. Every headed test generates a `.har` file.
7. `{#if}` (not CSS `visibility`) for conditional mount where cleanup is required (SSE, timers, subscriptions). `onDestroy` must fire on tab deactivation — SSE leaks across the session are P7 violations.
8. Visual regression baselines regenerated only with a live dev server on :5173. Solid-magenta (`#FF00FF`) baselines = test infrastructure failure, not a valid baseline.
9. **Copilot synthesis contract** — every workflow button has a declared context schema and execution path (copilot vs subagent). Buttons that assemble context heuristically at runtime are a §C violation. See §P (UI-to-Copilot Synthesis) for the full architectural contract.

---

## §D — HTTP Gateway, API & Realtime Transport *(L1)*

**Assertion**: the gateway is the reliable backbone. It serves the webshell HTTP API, manages WebSocket and SSE connections, enforces auth and rate limits, and routes MCP tool calls — all from a single Axum process. The gateway never panics on malformed input, never silently drops events, and never returns raw error text to callers.

**Serves**: P3 (gateway is the MoE routing layer), P4 (WS/SSE are the async event transport), P7 (reliability)

**Mechanical checks**:
1. All HTTP endpoints return structured JSON. Error schema: `{"code": str, "message": str, "recovery"?: str}`. No raw text, HTML, or stack traces.
2. Rate limiting via `governor` per-agent, per-endpoint. 429 response includes `Retry-After` header — not a silent drop.
3. WS upgrade (`GET /api/builds/:id/agent/ws`) completes in <50ms. Bearer token via `Sec-WebSocket-Protocol: bearer.<token>` — validated before 101 upgrade.
4. Max 8 simultaneous agent WS connections (`MAX_AGENT_WS = 8`). 9th connection returns 503 with current count in body.
5. SSE stream reconnects transparently — no operator intervention needed after disconnect.
6. Gateway crate (`lightarchitects-gateway`, excluded from workspace) has its own `cargo clippy --features inline-all` gate run explicitly before merge. Gateway exclusion does NOT mean gateway escapes quality gates.
7. Coordination endpoints return structured `WEBSHELL_UNREACHABLE` error (not panic) when the webshell is down, including `recovery` action: `"lightarchitects webshell start"`.

---

## §E — Agent Runtime & Session Lifecycle *(L2)*

**Assertion**: every agent session is a first-class durable entity. Sessions are created explicitly, tracked persistently, survive crashes, and close cleanly. The runtime never silently drops tool calls, silently overflows context, or silently loses permission state.

**Serves**: P2 (permission matrix), P4 (per-session WS), P5 (SQLite persistence), P7 (durability)

**Mechanical checks**:
1. Every session assigned a UUID at `session_start` and written to SQLite before the first agent turn begins. No in-memory-only sessions.
2. Session survives gateway restart: `GET /api/builds/:id/session` returns resumable state after reconnect.
3. Every tool invocation recorded to turnlog *before* execution. Crash between record and execution = recoverable, not lost.
4. `CompactionEngine` fires at context budget threshold; compacted summary preserves: decision list, active task, file ownership claims, last N tool results.
5. Permission matrix fail-secure by construction: no approval channel → DENIED, never auto-permitted. This is a type-system invariant, not a runtime check.
6. AYIN spans for every turn: `session.turn_start`, `session.tool_call` (per call, with name + duration), `session.tool_result` (success/failure), `session.turn_end`.
7. `SIGTERM` on session process: complete current tool if <2s remaining → record interrupted state to SQLite → close WS cleanly.

---

## §F — Orchestration & Squad Communication *(L3)*

**Assertion**: the squad communication bus is the nervous system of parallel agent work. It is typed, auditable, conflict-resolving, and non-blocking. An agent that doesn't use the A2A bus to coordinate is operating outside the platform's safety model.

**Serves**: P4 (async parallel collaboration)

**Mechanical checks**:
1. `squad_comms.rs` HTTP wrappers use `reqwest::Client` (async). `webshell_get`/`webshell_post` return `Result<Value, GatewayError>` — zero panics, zero unwraps in production paths.
2. `WEBSHELL_BASE` (`http://localhost:8733`) is a `GatewayConfig` field, not a hardcoded literal. Configurable for test environments.
3. `CONTEXT` is always the first message a Worker sends — before any `FILE_CLAIM` or `WAVE_START`. Governor rejects messages from a Worker that skipped `CONTEXT`.
4. `HEARTBEAT` sent every 30s by each active Worker. Worker with no heartbeat for 90s = crashed; Governor re-queues its tasks as unowned.
5. `GATE_REVIEW` requires `confidence` (0.0–1.0), `citations[]`, `verdict` (`accept|reject|hitl`). A `GATE_REVIEW` without citations is rejected — Gatekeeper must cite sources per Canon XXXV.
6. `FILE_RELEASE` is unconditional on commit — even if the Worker exits abnormally. Governor has a dead-Worker cleanup path that releases claims after 90s.
7. **Copilot supervisor subscription** — the copilot supervisor (EVA) subscribes to A2A bus events for the active build via `GET /api/builds/:id/supervisor/events` (SSE). It receives `WAVE_COMPLETE`, `GATE_REVIEW`, `BLOCKER`, and `PHASE_GATE_RESULT` messages without being a Worker. The supervisor channel is read-only on the A2A bus; copilot steering goes through `SQUAD_INJECT` or the coding agent's WS channel — never by mutating A2A messages mid-flight.

---

## §G — Plugin System: Agents, Skills & Marketplace *(L4)*

**Assertion**: the plugin system is the extensibility layer. Operators and platform developers add capabilities by writing plugins — not by modifying the platform core. A plugin-defined agent or skill is indistinguishable from a platform-built one from the operator's perspective.

**Serves**: P3 (MoE — new experts add to routing layer), P6 (legibility — skills are discoverable)

**Mechanical checks**:
1. Every plugin has a `plugin.json` manifest with: `name`, `version`, `author`, `agents[]`, `skills[]`. `plugin-dev:plugin-validator` passes before publishing.
2. Every skill has a `SKILL.md` with frontmatter (`name`, `description`, `when_to_use`) and a structured body. `plugin-dev:skill-reviewer` passes before publishing.
3. A newly installed skill is available in the next session. A restart is required for new agent type registration — this requirement is documented, not a silent behavior.
4. `mcp__plugin_lightarchitects_lightarchitects__tools` is the canonical dispatch surface. Plugins that bypass this and call sibling MCP tools directly are non-compliant.
5. Plugin cache at `~/.claude/plugins/cache/` is symlinked to the marketplace at `light-architects-plugins/`. Updates propagate without manual copy.
6. Every domain agent (`engineer`, `knowledge`, `quality`, `security`, `ops`, `researcher`, `testing`, `squad`) has its `subagent_type` string registered. An unregistered `subagent_type` produces a clear error — not a silent fallback.
7. **Skill execution is mandatory**: if a skill applies to the task, it MUST be invoked via the `Skill` tool before any response. This is enforced by `using-superpowers` base skill. "I know how to do this without the skill" is rationalization — not an exception.
8. Skills have testable `when_to_use` fields. A skill without a clear trigger is unpublishable — the marketplace validator rejects it.
9. **Webshell plugin parity** — every plugin installed in `~/.claude/plugins/cache/` is visible in the webshell's dispatch surface and invocable without re-installing or re-configuring. See §O (Tool Surface Parity) for the cross-surface assertion.

---

## §H — Expert/Sibling System *(L5)*

**Assertion**: each of the seven expert siblings (CORSO, EVA, SOUL, QUANTUM, SERAPH, AYIN, LÆX) is excellent within its domain and nothing outside it. Expert composition is additive. The expert system degrades gracefully when a sibling binary is unavailable.

**Serves**: P3 (MoE — expert specialization), P2 (SERAPH gate), P5 (SOUL helix), P7 (degraded-mode reliability)

**Mechanical checks**:
1. Each expert's MCP binary responds to `initialize` in <200ms. Response >500ms = performance regression.
2. Every expert invocation emits `expert.selected` + `expert.selection_rationale` AYIN span. Unobservable routing = P3 regression.
3. Expert domain boundaries enforced by Canon XXX strand. No cross-domain absorption without Canon XXXIX ratification.
4. Unreachable expert → `DEGRADED` response: which expert was unavailable, what capability is degraded, fallback path if one exists. No panic, no silent fallback.
5. EVA is the canonical copilot persona and active build supervisor. Operator dialog goes through EVA — not a generic LLM call. EVA holds `build.northstar_text`, subscribes to sub-agent A2A events, evaluates northstar alignment per wave, surfaces next-step proposals, and escalates to the operator only when a human decision is required. See §Q (Operator Build Northstar & Copilot-Supervised Orchestration) for the full supervisory model.
6. SERAPH `[S]` is a veto authority: any merge touching auth, crypto, sandboxing, permission matrix, or secret lifecycle requires explicit `SERAPH APPROVE` before the Gatekeeper accepts.
7. LÆX `[C]` is a veto authority: any change to a canon document or Northstar requires LÆX Layer 3 ratification before Governor can merge.
8. AYIN spans are platform-level responsibility — the observability layer is not delegated to individual experts. Experts may add child spans; they must not be the only source of session-level spans.

---

## §I — Knowledge Graph & Session Memory *(L6)*

**Assertion**: the helix knowledge graph is the platform's long-term memory. It stores structured, enrichable, queryable knowledge from every significant agent session. Without it, the platform is stateless. With it, the platform compounds value with each use.

**Serves**: P5 (persistent knowledge)

**Mechanical checks**:
1. Neo4j via `neo4rs::Graph` with `max_connections(20)` local / `max_connections(50)` Aura. No `deadpool-neo4j` — `neo4rs` built-in pooling is canonical.
2. `Graph::execute()` → `DetachedRowStream` (not re-exported); use inline async block + `tokio::time::timeout`. Never annotate the stream return type directly.
3. ETag generation: serialize body → SHA-256 → `ETag` header; store `content_hash` in Neo4j at write time. Handler-level only — NOT tower-http middleware (which doesn't compute content ETags).
4. 4-signal RRF: semantic similarity (fastembed) + keyword match (Cypher full-text) + graph proximity (path queries) + recency (timestamp). All four must participate — single-signal retrieval is a P5 regression.
5. `Moka` in-memory cache (TTL-scoped) fronts hot helix reads. Write-through invalidation on every write. Reads older than 5 minutes re-fetched from Neo4j.
6. turnlog HMAC chain: `HMAC-SHA256(prev_hash, turn_data)`. Broken chain → `CHAIN_BROKEN` error on read, not silent corruption.
7. EVA 8-layer enrichment schema is mandatory. Direct Neo4j inserts that bypass the schema are P5 violations — schema is what makes retrieved context structured and useful.

---

## §J — Research & Verification Quality *(L6)*

**Assertion**: the platform's research output is verifiable, cited, and honest about uncertainty. Every decision-gating assertion carries a confidence value and primary source citations. Speculation is never stated as fact. The QUANTUM sibling and the Oracle module are the primary enforcement surfaces.

**Serves**: P5 (quality of enriched knowledge), P2 (research-backed threat model)

**Mechanical checks**:
1. Every decision-gating assertion carries: `confidence_value` (0.0–1.0), `primary_source_citations[]` (verbatim quote + source URI + accessed date). Gate verdict without citations is rejected — Canon XXXV.
2. Confidence thresholds: wave auto-accept ≥ 0.95; merge auto-accept ≥ 0.99; security finding ≥ 0.99. Below threshold → `verdict: hitl`, never auto-accept.
3. QUANTUM citations come from the industry baseline allowlist (`canon://security-guardrails` §sources). URLs not in the allowlist require explicit operator approval before citing.
4. Research output separates three epistemic states: **KNOW** (verified + cited), **DON'T KNOW** (gap acknowledged), **ASSUMING** (stated explicitly). "Should work", "I'm pretty sure", "seems fine" = Communication Covenant violations.
5. Oracle module (`lightarchitects::oracle`) verifies mathematical assertions via multi-model consensus (Lean 4 + DeepSeek + Qwen + Kimi). Failed Oracle verification → assertion demoted to ASSUMING.
6. Firecrawl research cache at `<build_root>/.context/`. Citations older than 30 days are re-fetched before citing. Stale citations are a research quality violation.
7. Context7 preferred for library/API assertions. Firecrawl fallback for content Context7 can't cover. Training-data-only assertions without live verification = ASSUMING.

---

## §K — Security Perimeter *(L7)*

**Assertion**: the platform implements defense-in-depth against the threats relevant to an agentic coding platform — prompt injection, privilege escalation, secret leakage, and supply chain compromise. Industry standards (OWASP LLM Top 10, ASVS v5, SLSA, NIST SP 800-63) are the minimum baselines, not the ceiling.

**Serves**: P2 (secure-by-default orchestration)

**Mechanical checks**:
1. OWASP LLM Top 10 (2025) is the minimum baseline for all LLM-touching surfaces. SERAPH scans against this on every `[S]` gate review.
2. Prompt injection defense: all operator-provided content reaching an LLM prompt is schema-validated and structurally sandboxed before concatenation. No raw operator input in system prompts.
3. OWASP ASVS v5.0.0 Level 2 for authentication: session tokens, API keys, and webshell bearer tokens validated against ASVS §2 (Authentication) and §3 (Session Management).
4. SLSA Level 2 for supply chain: provenance for build artifacts, reproducible builds. `cargo-deny` enforces `deny.toml` — license violations and advisory-DB matches block the build.
5. No PII in telemetry events. No operator credentials in logs. No agent-generated output stored without operator awareness. First-party telemetry only — GDPR Article 25 (privacy by design).
6. `trufflehog` in pre-commit hooks and CI. Any commit touching auth/crypto/config surface triggers a SERAPH scan before merge.
7. `ScopeGovernor` 5-gate pattern compiled into type system for all SERAPH capability invocations — not a runtime check.
8. All `unsafe` blocks require `// SAFETY:` comment. `unsafe_code = "deny"` in `Cargo.toml`. Any `unsafe` requires explicit per-use exemption in clippy allow.

---

## §L — Observability *(L8)*

**Assertion**: the platform is transparent about what it is doing, what it did, and how long it took. Operators and engineers can always answer "what is this agent doing right now?" and "why did that take so long?" without reading source code or adding debug prints.

**Serves**: P6 (legibility — activity stream), P3 (MoE — observable routing)

**Mechanical checks**:
1. AYIN HTTP dashboard at `http://127.0.0.1:3742` responds within 2s of `make deploy && launchctl kickstart`.
2. Every agent turn emits: `session.turn_start`, `session.tool_call` (per tool, name + duration), `session.tool_result` (success/failure), `session.turn_end`. No unobservable turns.
3. Every expert dispatch emits `expert.selected` + `expert.selection_rationale` + `expert.composition_latency_ms`.
4. Activity stream in webshell renders tool events within 500ms of AYIN span emission via SSE. End-to-end latency (tool call → AYIN span → SSE → Activity render) ≤ 500ms on localhost.
5. `GET /api/spans` and `GET /api/sessions/:id/spans` queryable via AYIN HTTP API. Trace data is accessible without the GUI.
6. TUI and webshell sessions produce structurally identical AYIN spans — observability is surface-agnostic.
7. Span cardinality bounded: a 10-turn session with 5 tool calls per turn produces ≤ 200 spans. High-cardinality span explosion (one span per millisecond) is a P7 reliability anti-pattern.

---

## §M — Third-Party Integration & MCP Surface *(L10)*

**Assertion**: the platform integrates with the external ecosystem without vendor lock-in. Every integration has a documented degraded-mode path. The MCP protocol is the canonical integration surface for external coding agents — Claude Code, Codex, and future runtimes connect without custom configuration beyond `lightarchitects.mcp.json`.

**Serves**: P1 (MCP enables E2E from external agents), P3 (MoE — MCP exposes the expert surface)

**Mechanical checks**:
1. MCP compliance: `initialize`, `tools/list`, and all registered tool calls work correctly. Claude Code connects, enumerates tools, and invokes them with only `lightarchitects.mcp.json` as config.
2. GitHub integration uses `gh` CLI subprocess — not a GitHub SDK. Swappable. Missing `gh` = structured error, not panic.
3. Docker integration is OCI-compliant: standard `docker run` flags only. No Docker SDK. Absent Docker = native PTY + visible degraded-trust warning in UI.
4. Neo4j: `bolt://`, `neo4j://`, and `neo4j+s://` (Aura) all supported. Connection strings from Keychain (`soul-neo4j-local`), not hardcoded URIs or `.env` files.
5. Context7 (`mcp__plugin_context7_context7__*`) is the primary library doc source; Firecrawl is the fallback; WebSearch is the last resort. Degradation is automatic — the platform does not stall when a doc source is unavailable.
6. Sonatype guide gates all new dependency additions. No dep addition to `Cargo.toml` or `package.json` without a version check. Enforced in the `[R]` gate.
7. Voice/TTS is a first-class capability. TTS pipeline tested in CI via `tts-production-test` smoke suite. Empty audio output on expected content is a P7 violation.
8. All third-party integration failures produce structured errors, not panics. A 3P service being down must not crash the platform process.
9. **MCP tool surface parity** — every `mcp__plugin_*` tool reachable in a Claude Code session is discoverable from the webshell operator UI. The platform does not silently hide tools that are registered in the native coding agent's MCP config. See §O (Tool Surface Parity).

---

## §N — Cryptography & Authentication *(L0)*

**Assertion**: the platform handles cryptographic material correctly at every layer. Secrets live in Keychain, not files. Keys are derived, not copied. Comparisons are constant-time. Timestamps are replay-resistant. A platform that leaks a secret, uses a timing oracle, or stores credentials in plaintext has failed this section unconditionally.

**Serves**: P2 (security — secret isolation), P7 (reliability — auth durability)

**Mechanical checks**:
1. API keys, bearer tokens, Neo4j credentials, and webshell tokens are in the macOS Keychain via `security` CLI subprocess pattern. No secrets in `.env` files, `config.toml` plaintext, or inheritable shell environment variables.
2. Algorithm policy: HKDF for key derivation, AES-256-GCM for symmetric encryption, Ed25519 for signing. No RSA-1024, SHA-1, MD5, or ECB mode — `cargo-deny` enforces this.
3. MAC verification uses `subtle::ConstantTimeEq` with explicit length-equality encoding: `Choice::from(u8::from(a.len() == b.len()))` AND'd with the byte comparison. `a.len() != b.len()` before `ct_eq` = timing oracle = CRITICAL violation.
4. Timestamp replay window uses signed arithmetic: `let diff = now.signed_duration_since(ts).num_seconds(); (-SKEW..=WINDOW).contains(&diff)` with `SKEW ≤ 5s`. `unsigned_abs()` makes future timestamps pass — forbidden.
5. `env -u ANTHROPIC_API_KEY` on every agent subprocess spawn — verified in `StreamingApprovalGate` and all other subprocess-spawning paths.
6. Bearer token in WebSocket travels via `Sec-WebSocket-Protocol: bearer.<token>`. Validated before 101 upgrade. Connection without valid token = 401, not 101.
7. Manual binary copy requires codesign: `codesign -s - ~/.lightarchitects/bin/lightarchitects`. Unsigned binaries exit 137 (SIGKILL from Gatekeeper). `make deploy` handles this automatically — never skip.
8. `secrecy::SecretBox<T>` (+ `zeroize`) for in-memory key material. Direct `Vec<u8>` for secret storage is forbidden — `cargo deny` enforces this.

---

## §O — Tool Surface Parity *(L4, L10)*

**Assertion**: the webshell UI exposes every tool, plugin, MCP server, and skill available to the operator in their native coding agent — regardless of whether that agent is Claude Code, Codex, or any future runtime. An operator switches surfaces without losing flow. The webshell is not a curated subset of the native environment; it is a complete, addressable surface of the operator's full tool ecosystem.

**Serves**: P1 (E2E surface completeness), P3 (MoE — unified dispatch), P6 (legibility — operator always knows what's available)

**Mechanical checks**:
1. **MCP server enumeration** — every MCP server listed in the operator's native coding agent config (e.g. `~/.claude/mcp.json`) is enumerated in the webshell's connected-servers view. The operator can see name, status, and tool count for each server without leaving the UI.
2. **Plugin tool invocability** — every `mcp__plugin_*` tool reachable in Claude Code is invocable from the webshell Dispatch view. No tool available natively is silently unavailable in the webshell.
3. **Skill surface parity** — every skill available via the `lightarchitects` plugin in a Claude Code session is selectable and runnable from the webshell. Skill list is fetched live from the plugin cache at dispatch time — not a stale static registry.
4. **Token reuse** — the webshell reuses existing MCP server authentication tokens. The operator does not re-authenticate to connected MCP servers when switching from Claude Code to the webshell.
5. **Agent runtime portability** — parity holds for Claude Code, Codex, and future coding agents. `lightarchitects.mcp.json` is the single canonical config surface; tools are declared there once, not per-agent.
6. **Visible surface gaps** — any tool registered in the native coding agent but not yet invocable from the webshell UI is surfaced explicitly as a gap: visible in the webshell's tool inventory with status `"webshell: not yet supported"`. Silent gaps are a §O violation. The operator always knows what is and is not available.
7. **Output schema identity** — tool invocation results returned via the webshell are schema-identical to results from the native coding agent. The platform does not downgrade, truncate, or reformat tool output when routing via webshell instead of stdio.
8. **Cross-surface command discovery** — the operator can search the full tool surface (all plugins, all skills, all sibling capabilities) from a single search interface in the webshell. Discovery does not require knowing which plugin owns which tool.

---

## §P — UI-to-Copilot Synthesis: Context Engineering & Execution Path *(L3, L9)*

**Assertion**: every workflow button in the webshell is a precision context-retrieval and prompt-engineering routine — not a raw UI dispatch call. When an operator triggers an action (start build, run plan, review gate, enrich memory), the platform retrieves exactly the context that action requires, engineers a structured prompt, and submits it to the copilot. The operator's intent is amplified precisely, not broadcast noisily.

Two execution paths exist:

- **Copilot path** (default) — LLM-mediated. The copilot (EVA) receives the engineered prompt, invokes skills, reasons across context, and executes. Required for judgment-intensive operations: plan authoring, gate review synthesis, skill orchestration, cross-domain decisions.
- **Direct subagent path** — deterministic subprocess, no LLM in the loop. Eligible only for operations with a demonstrated ≥ 99% reliability predicate across ≥ 50 CI runs and no judgment requirement. The subagent notifies the parent session via A2A messages; the copilot session is never left unaware.

When lightarchitects introduces purpose-built domain agents, those agents supersede the copilot path for their specific action class. The copilot remains the synthesis layer for cross-domain judgment. Routing is declared in config, not hardcoded per button.

**Serves**: P3 (MoE — sparse, precise expert activation at action granularity), P4 (async — subagent runs in parallel with parent), P5 (knowledge — context pulled from helix, not reconstructed), P6 (legibility — prompt + context inspectable by operator)

**Mechanical checks**:
1. **Typed context schema per action** — every UI workflow action has a declared context retrieval schema: what to fetch (session state, helix entries, git delta, active build manifest, phase state), from where, in what order. The schema is co-located with the button definition. Runtime heuristic context assembly is a §P violation.
2. **Token budget enforcement** — each action class has a documented maximum context budget. Examples: `/PLAN` trigger = project manifest + active build state + relevant helix entries (≤ 4 K tokens); `/GATE` trigger = phase manifest + test delta + open gate findings (≤ 2 K tokens). Over-budget retrieval surfaces a warning in the webshell before submission — never silently truncates.
3. **Two-path routing with reliability gate** — each action declares `execution_path: copilot_prompt | direct_subagent`. Default: `copilot_prompt`. Upgrade to `direct_subagent` requires architectural review + demonstrated ≥ 99% reliability. Upgrading to avoid token cost without the reliability gate is a §P violation.
4. **Subagent parent-process coordination** — direct subagent operations emit `WAVE_START` before execution and `WAVE_COMPLETE` (or `BLOCKER`) on finish via squad_comms. The parent copilot session receives these events and can surface them in the Activity stream. No silent background operations.
5. **Prompt transparency** — the operator can inspect, for any triggered action: (a) what context was retrieved, (b) the engineered prompt submitted, (c) which execution path was taken. Surfaced in the Activity stream as a collapsible "Context used" card. Not buried in raw JSON logs.
6. **Pre-flight context validation** — if context retrieval fails (Neo4j unreachable, session state missing, build manifest not found), the UI surfaces a structured pre-flight error before submission. Partial-context submissions are forbidden. The copilot receives complete context or the action does not proceed.
7. **Domain agent routing contract** — when a purpose-built domain agent is registered for an action class, `GatewayConfig.action_routes` maps that class to the domain agent. The webshell button does not change; the routing layer switches transparently. No per-button code changes on agent introduction.
8. **Retrieval efficiency audit** — for each action class, tokens retrieved vs tokens consumed by the copilot is tracked. Retrieval efficiency ratio < 0.5 (more than half of retrieved context unused) triggers a schema review to tighten the context query. Token waste is an engineering quality signal, not an acceptable steady state.

---

## §Q — Operator Build Northstar & Copilot-Supervised Orchestration *(L3, L5, L9)*

**Assertion**: the copilot is the active, context-aware supervisor of every build — not a passive prompt receiver. Success depends on wiring the right asynchronous communication between the three tiers so the copilot can evaluate whether work is on-northstar, synthesize what sub-agents and coding agents report back, propose next steps, and escalate to the operator only when a human decision is genuinely required.

The operator's **build northstar** — their specific goal for this session or build, stated in 1–3 sentences — is the copilot's primary evaluation lens throughout. It is established once at build creation in natural language, stored durably, and injected into every copilot evaluation turn. Getting this right is what makes the copilot feel like a genuine collaborator rather than a stateless assistant.

**Three-tier async communication model**:

```
Operator tier        → states intent, makes decisions, approves gates
       ↕  (webshell UI — legible summaries, inline proposal cards, HITL escalation)
Copilot tier (EVA)   → holds build.northstar_text; synthesizes; evaluates; steers; escalates
       ↕  (SSE supervisor stream + SQUAD_INJECT + WS coding-agent channel)
Execution tier       → sub-agents (A2A bus: WAVE_COMPLETE, GATE_REVIEW, BLOCKER, HEARTBEAT)
                     → coding agents (WS: tool_result, turn_end events)
```

**Serves**: P2 (copilot is the trust evaluation perimeter), P4 (three-tier async backbone), P5 (build northstar stored + injected from helix), P6 (operator sees only what requires their attention)

**Mechanical checks**:
1. **Build northstar capture** — at build creation, operator provides `build.northstar_text` (1–3 sentences, natural language). Stored in SQLite and helix (significance 8.0, tagged `type: build_northstar`). Required before first agent turn begins. UI prompt: "What are you trying to achieve with this build?" No build starts cold.
2. **Northstar injection** — `build.northstar_text` is prepended to the copilot's context on every evaluation turn. The copilot never evaluates work against an empty goal. If northstar text is absent from context, that is a §Q violation surfaced in the Activity stream.
3. **Per-wave northstar evaluation** — after each `WAVE_COMPLETE` event, the copilot produces a structured evaluation stored in turnlog: `{work_done: str, northstar_alignment: advancing|neutral|drifting, confidence: float, recommended_next: [str]}`. This evaluation is the basis for the next proposal card shown to the operator.
4. **Async execution-tier → copilot reporting**:
   - Sub-agents: emit `WAVE_COMPLETE` + `GATE_REVIEW` on the A2A bus; copilot supervisor subscribes via SSE at `GET /api/builds/:id/supervisor/events`. No polling.
   - Coding agents (Claude Code, Codex): emit `tool_result` and `turn_end` events on the WS control channel. Copilot subscribes to the same stream; events do not require a human relay.
   - Both channels report to the copilot without interrupting execution-tier work. The agent mid-turn is never paused for a supervisor check.
5. **Copilot → execution tier steering** — copilot injects corrections or redirects via `SQUAD_INJECT` message to the A2A bus (for sub-agents) or a new message to the coding agent's WS channel (for coding agents). Steering is non-blocking: agents receive it at the next natural pause point (turn boundary or wave boundary), not mid-tool-call.
6. **Drift detection + operator escalation** — if `northstar_alignment: drifting` for N consecutive waves (default N = 3, configurable), the copilot surfaces a "Build drift" card in the webshell: current direction, northstar text, and 2–3 realignment options. The copilot does NOT autonomously redirect. It escalates to the operator, who decides.
7. **Next-step proposal surface** — after each phase or completed wave, copilot proposes the next step as 1–3 inline cards in the webshell. Cards include: action label, which Pillar it advances, estimated context cost, and which agents would execute. Operator selects; copilot dispatches. This is the primary operator interaction pattern — not free-text chat.
8. **Supervisor state API** — `GET /api/builds/:id/supervisor/state` returns current copilot supervisor snapshot: `{northstar_text, active_agents: [{id, status, last_heartbeat}], pending_gates: [], last_evaluation: {alignment, recommended_next}, drift_count: N}`. Queryable without a GUI — the webshell renders from this endpoint.
9. **Domain agent upgrade path** — when lightarchitects introduces purpose-built domain agents (e.g. a dedicated Gate Agent, Plan Agent), the copilot supervisor's routing table (`GatewayConfig.action_routes`) maps those action classes to domain agents. The copilot's role shifts: from executing these actions itself to supervising domain agents executing them. The three-tier model expands naturally without architectural change.
10. **Escalation model** — copilot escalates to operator (HITL, blocking) on: northstar drift ≥ N waves; security gate veto (`SERAPH BLOCK`); canon veto (`LÆX BLOCK`); agent deadlock (two or more agents in `BLOCKER` state simultaneously). All other decisions — agent selection, context retrieval, next-step execution — are copilot-autonomous within the operator's declared northstar.

---

## §R — Platform & User Vault Architecture *(L0, L6)*

**Assertion**: the platform uses a two-vault model (ratified 2026-05-07, KFT + Squad). The **platform vault** holds system-level constants — canon documents, industry baselines, skills, and agent definitions — versioned alongside the code that enforces them, read-only for users. The **user vault** (`~/lightarchitects/soul/`) holds user-owned content: personal journal, agent consciousness sub-helixes, session data, and a customizable fork of canon. Platform writes to the platform vault via commits to `lightarchitects-sdk`; users write to their vault with ScopeGovernor permission. The canonical resolution path `$HELIX/user/standards/canon/<name>.md` is preserved via symlink from `soul/helix/user/standards` → `lightarchitects-sdk/standards/`.

**Serves**: P3 (single binary is the platform vault), P5 (standards versioned with knowledge graph code), P7 (deterministic source of truth for canon)

**Source**: `vault-migration-v1/two-vault-model-2026-05-07.md`, `platform-vault-spec-2026-05-07.md`, `user-vault-spec-2026-05-07.md` — KFT + Squad, 2026-05-07. Template scaffolded in commit `f80cfd8` (2026-05-13). Platform vault directory never instantiated as of 2026-05-16.

**Mechanical checks**:
1. **Platform vault canonical location** — `lightarchitects-sdk/standards/` is the source of truth for all canon documents. No canon doc is tracked as a git file directly in `soul-vault`.
2. **Path convention preserved** — `$HELIX/user/standards/canon/<name>.md` resolves correctly at all times. Before migration: direct files. After migration: via symlink `soul/helix/user/standards → lightarchitects-sdk/standards/`. The path convention is a stable contract — never broken.
3. **Write discipline** — changes to canon documents require a commit to `lightarchitects-sdk` through its full quality gate (`cargo fmt + clippy + /GATE`). Direct file edits to canon content in `soul-vault` bypassing the SDK gate are a §R violation.
4. **Fork mechanism** — users may copy specific canon docs into `soul/helix/user/standards/` for local customization; customized copies take precedence in SOUL's 4-signal RRF retrieval order. The platform copy is never mutated by the fork.
5. **Soul-vault content model** — `soul-vault` tracks: user helix (journal, career, training), agent sub-helixes (eva, claude, corso, seraph, quantum, ayin, laex0), shared convergences, session/chat/CLI history. It does not track engineering standards as first-class git content after migration.
6. **Platform vault template** — `soul/helix/corso/builds/vault-migration-v1/platform-vault-template/` is the reference scaffold: 7-sibling slate, each with `agent.md` + `helix.toml` + META skill stubs. New platform vault deployments bootstrap from this template.
7. **Scope tiers in Neo4j** — helix entries carry `scope_tier: platform | user | agent`. Canon documents ingested from the platform vault carry `scope_tier: platform`; personal journal entries carry `scope_tier: user`; agent consciousness entries carry `scope_tier: agent`. Mixed-tier entries are a §R violation.

---

# Part III — Per-Build Alignment Requirement

Every plan MUST include a `northstar_lineage:` block:

```yaml
northstar_lineage:
  inherited: false
  pillar_advanced: 1|2|3|4|5|6|7|multi   # see Vision Pillars above
  component_sections: [§A, §B, ...]       # Part II sections (§A–§Q) this build advances or must not regress
  pillars_preserved: [<list>]             # enumerate pillars not advanced; verify no regression
  northstar_metric_delta_estimate: "<concrete before/after measurement>"
  if_northstar_changes_during_build: escalate_to_laex
  validation_predicate: "<how squad review confirms advance is real, not aspirational>"
```

Plans without this block fail Phase 1 spot-check.

**C7 scoring ceiling heuristics** (ceilings are inherent to the feature type, not plan gaps):

| Alignment | C7 ceiling | Rationale |
|-----------|-----------|-----------|
| Direct P1 — E2E surface (E-gate closure) | 97–100 | Directly testable predicate |
| Direct P2 — security surface (new trust boundary) | 96–99 | SERAPH gate evidence available |
| Direct P3 — MoE consolidation (binary unification) | 93–97 | Expert-route observability testable; single-binary predicate immediate |
| Direct P4 — async protocol (new message type, gate) | 95–98 | Protocol testability high |
| Direct P5 — knowledge graph (enrichment, retrieval) | 93–96 | Lag between enrichment and measurable retrieval improvement |
| Direct P6 — UI legibility (new zoom level, activity card) | 95–98 | Playwright-verifiable |
| P7 — reliability (test coverage, graceful degradation) | 92–96 | Coverage metrics objective; failure-mode behavior harder to prove |
| Component §G — plugin system improvements | 93–97 | Plugin validation testable; marketplace effects lagged |
| Component §J — research quality improvements | 91–95 | Evidence quality is partially subjective |
| Component §O — tool surface parity (new surface/tool exposure) | 94–97 | Playwright-verifiable via parity matrix; gap visibility check is direct |
| Component §P — UI-copilot synthesis (context schema, routing) | 91–95 | Context precision testable; retrieval efficiency ratio is objective; end-to-end prompt transparency verifiable in Activity stream |
| Component §Q — copilot supervision (northstar eval, drift detection, async comms) | 93–97 | Supervisor state API queryable; northstar injection verifiable in turnlog; drift detection E2E Playwright-testable |
| Indirect infrastructure enabling a Pillar | 90–94 | Signal is lagged; measurable after downstream Pillar ships |

Stop the C7 loop when 2 consecutive rounds produce 0 blocking gaps and score delta < 0.3.

---

# Part IV — Northstar Evolution Protocol

The Northstar is constitutional-tier. Amendments follow the full Canon XXXIX pipeline:

1. **Memory entry** — author a promotion candidate in `~/.claude/projects/.../memory/`
2. **Contradiction check** — validate against all 8 canon documents; surface conflicts
3. **LÆX Layer 3 ratification** — formal product gate review
4. **Kevin's stamp** — no delegation eligible for Northstar amendments

**Full protocol required**: adding/removing a Pillar; changing a Pillar's assertion; adding/removing a mechanical check that gates ship decisions; adding a new Component section (§A–§Q).

**LÆX + Kevin review (not full protocol)**: adding mechanical check sub-items within an existing section; updating the ship state table; amending C7 ceiling heuristics.

**No escalation required**: updating ship state emoji + evidence in Part V.

---

# Part V — Current Ship State

| Component | Status | Evidence |
|-----------|--------|---------|
| **P1 — E2E Engineering Surface** | ✅ Realized (2026-05-16) | EEF E1–E6; 10/10 Playwright (T1–T10 incl. live-integration with `E5_LIVE=1`) |
| **P2 — Secure-by-Default Orchestration** | 🔄 Partial | Permission matrix ✅; fail-secure deny ✅ (T7+T8 fixed 2026-05-16); Docker containment 🔄; secret isolation ✅; supply chain ✅ |
| **P3 — MoE Platform Architecture** | 🔄 In progress | Conductor mode ✅; additive `mcp --as <expert>` ✅; standalone sibling binaries still operational; consolidation active |
| **P4 — Async Parallel Collaboration** | 🔄 Partial | A2A JSONL ✅ (agents-playbook v1.1); squad_comms ✅; WS control ✅; StreamingApprovalGate ✅; parallel dispatch ✅; FILE_CLAIM/RELEASE 🔄 |
| **P5 — Persistent Knowledge** | 🔄 Partial | SQLite ✅; SOUL helix Neo4j ✅; EVA enrichment ✅; Activity stream ✅ (squishy Phase B); CompactionEngine ✅; turnlog HMAC 🔄; cross-session injection 🔄 |
| **P6 — Operator-Legible Arc** | 🔄 Partial | Activity stream ✅; Portfolio view ✅; permission cards ✅ (E5); 3-zoom model 🔄; git narrative 🔄; Helix 3D ambient 🔄 |
| **P7 — Production-Grade Reliability** | 🔄 Partial | Binary rollback ✅; graceful SIGTERM ✅; fallback mode ✅; Playwright E2E ✅; SQLite WAL 🔄; ≥90% coverage 🔄; error surfaces 🔄 |
| **§A CLI & Binary Interface** | 🔄 Partial | Single binary ✅; `--help` ✅; rollback ✅; MCP handshake ✅; non-interactive flags 🔄 |
| **§B TUI / CLI Agent** | 🔄 Partial | AgentRunner ✅; CompactionEngine ✅; permission protocol ✅; steer 🔄; AYIN spans 🔄 |
| **§C Webshell UI Frontend** | 🔄 Partial | Svelte 5 ✅; Playwright E2E ✅; empty/loading/error states 🔄 (partial); accessibility 🔄; HAR generation ✅ |
| **§D Gateway, API & Transport** | 🔄 Partial | Axum HTTP ✅; WS upgrade ✅; rate limiting (governor) ✅; SSE ✅; structured errors 🔄 |
| **§E Agent Runtime & Session** | 🔄 Partial | Session persistence ✅; permission fail-secure ✅; AYIN spans ✅; turnlog 🔄; SIGTERM cleanup ✅ |
| **§F Orchestration / Squad Comms** | 🔄 Partial | squad_comms HTTP bridge ✅; A2A JSONL ✅; GATE_REVIEW citations 🔄; HEARTBEAT/dead-worker cleanup 🔄 |
| **§G Plugin System** | 🔄 Partial | lightarchitects plugin ✅ (29 skills, 8 agents); plugin.json validation 🔄; marketplace symlink ✅; skill-execution-spec ✅ |
| **§H Expert/Sibling System** | 🔄 Partial | 7 sibling binaries ✅; EVA copilot identity ✅; SERAPH veto ✅; LÆX veto ✅; expert degraded-mode 🔄 |
| **§I Knowledge Graph** | 🔄 Partial | Neo4j neo4rs ✅; fastembed embeddings ✅; Moka cache ✅; ETag ✅; HMAC turnlog 🔄; 4-signal RRF 🔄 |
| **§J Research Quality** | 🔄 Partial | Canon XXXV citations ✅; confidence gates ✅; QUANTUM protocol ✅; Oracle 🔄; research cache 🔄 |
| **§K Security Perimeter** | 🔄 Partial | OWASP LLM baseline ✅; ScopeGovernor ✅; cargo-deny ✅; SERAPH scans ✅; prompt injection defense 🔄 |
| **§L Observability** | 🔄 Partial | AYIN at :3742 ✅; session spans ✅; Activity stream ✅; HTTP API queryable ✅; span cardinality bounds 🔄 |
| **§M 3P Integration & MCP** | 🔄 Partial | Claude Code MCP ✅; GitHub gh CLI ✅; Docker ✅; Neo4j Aura ✅; Context7 ✅; Firecrawl ✅; Sonatype ✅; Voice 🔄 |
| **§N Cryptography & Auth** | 🔄 Partial | HKDF/AES-GCM/Ed25519 ✅; Keychain via security CLI ✅; env -u ANTHROPIC_API_KEY ✅; CT comparison 🔄 audit; Bearer WS ✅; codesign ✅ |
| **§O Tool Surface Parity** | ❌ Not started | MCP enumeration 🔄 (Claude Code MCP connect ✅, webshell UI surface inventory ❌); plugin tool invocability ❌; skill surface parity ❌; gap visibility ❌ |
| **§P UI-to-Copilot Synthesis** | ❌ Not started | Typed context schemas ❌; two-path routing ❌; token budget enforcement ❌; prompt transparency ❌; pre-flight validation ❌; domain agent routing contract ❌ |
| **§Q Copilot-Supervised Orchestration** | ❌ Not started | build.northstar_text capture ❌; northstar injection ❌; per-wave evaluation ❌; SSE supervisor stream ❌; drift detection ❌; next-step proposal cards ❌; supervisor state API ❌ |
| **§R Platform & User Vault Architecture** | ❌ Not started | Platform vault (`lightarchitects-sdk/standards/`) not instantiated ❌; canon docs still in `soul/helix/user/standards/` as direct git files ❌; symlink not created ❌; write discipline not enforced ❌; Neo4j `scope_tier` not applied to canon entries ❌ |

---

*Canonical URI*: `canon://northstar`
*Storage*: `$HELIX/user/standards/canon/northstar.md`
*Supersedes*: `northstar-v1.md` (archived), operators-manual.md §1.2 inline Pillars (reference-only since 2026-05-16)
