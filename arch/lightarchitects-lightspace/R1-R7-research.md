# Phase 2 Research — R1–R7 Findings

**Date**: 2026-06-08 | **Build**: lightarchitects-lightspace

---

## R1 — Yjs/CRDT Rejection

**Verdict**: Reject for v0.1.0.

**Why Yjs/Automerge conflict with the design:**
- Yjs `Y-Map` mutations are in-place inside `doc.transact()` — there is no `reduce(S,E)->S` path. The document IS the state store; mutations are not events over a value.
- Automerge's `change(doc, fn)` looks functional but `fn` receives a mutable proxy — the wrapper is cosmetic; the CRDT operation log grows internally.
- Both libraries assume concurrent operators. Lightspace v0.1.0 is single-operator; CRDT overhead is pure cost.
- Integrating either creates two competing state authorities alongside the pure reducer.

**v0.2.0 trigger conditions**: Multi-operator workspaces become a Northstar gate; evaluate `autosurgeon` Rust crate's `Reconcile`/`Hydrate` derive macros as the closest approximation to a pure interface.

---

## R2 — Fork/Explore/Commit (arXiv:2602.08199)

| Paper primitive | BranchLane equivalent |
|---|---|
| `fork(process)` — CoW snapshot at decision point | `lanes[]` — new lane diverges from `committed_lane_id` state at fork time |
| `run-isolated(fork)` — private address space | Lane events don't affect sibling lanes; pure reducer ensures this structurally |
| `commit-atomically(fork)` — promote fork if no conflict | Setting `committed_lane_id = lane.id` |
| Conflict detection | `E_CANVAS_BRANCH_LANE_COMMIT_RACE` — detected at event time, not merge time |

**Lightspace adopts**: fork-as-cheap-snapshot, commit-as-atomic-promotion, isolation by pure reducer.

**Deferred to v0.2.0+**: conflict resolution strategy (losers surface as race, not merged), losing-lane replay against committed state.

---

## R3 — Event-Sourced Determinism

**What we adopt from Redux/Elm/Flux**: `reduce(state, event) -> state`, pure function, no side effects.

**What Rust adds that JS cannot provide:**
1. **Ownership forbids aliased mutable state** — accidentally impure reducer doesn't compile.
2. **Enum exhaustion** — `#![deny(non_exhaustive_omitted_patterns)]` makes unhandled CanvasEvent variants a build error, not a silent fallthrough. New variant = compiler demands the handler before the binary compiles.

**Scope lock**: purpose-built for Lightspace canvas; `CanvasEvent` is a sealed enum. No plugin API for registering new variants. This is correct — the abstraction cost of a general-purpose framework is not warranted for a known-closed event set.

---

## R4 — SSE Recovery: Sequence Gaps + Snapshot Catch-Up

**Existing pattern** (feedback_broadcast_recv_lagged_security): on `RecvError::Lagged`, drop-and-warn. Do NOT replay lagged events into LLM context — indirect injection surface (OWASP LLM01).

**New per-card seq pattern** (for UI SSE consumer only):
1. Each Update event carries monotonic `seq` per `card_id` (contract: lightspace.event.update.v1 line 76)
2. Gap detection: `if event.seq != last_seq + 1 → fetch /api/lightspace/snapshot`
3. Snapshot response carries `snapshot_seq` (canvas-wide monotonic)
4. Discard buffered SSE events with `seq ≤ snapshot_seq`; resume processing `seq > snapshot_seq`

**Policy distinction** (load-bearing — two subscribers, opposite policies):

| Subscriber | Lagged policy | Reason |
|---|---|---|
| LLM watcher | drop-and-warn | OWASP LLM01 injection risk |
| Browser SSE consumer | seq-gap → snapshot | Correctness — user must see accurate canvas |

**Flat-format note**: `event.seq` is a top-level field (not `event.data.seq`) per the existing flat-format fix.

---

## R5 — Security Review (CWE-22/345/LLM01/LLM07)

| Surface | Threat | Mitigation | Verification |
|---|---|---|---|
| session_id path | CWE-22 traversal (`../../etc`) | `safe_lightspace_path()` ancestor-walk; assert canonical path is strict prefix of lightspace root | `cargo test -- safe_lightspace_path` (table of traversal patterns) |
| content_uri | CWE-22 + LLM07 exfil (`file:///etc`, `http://attacker`) | Scheme allowlist: `file://~/.lightarchitects/lightspace/`, `helix://`, project-rooted only | `cargo test -- content_uri_scheme` (each scheme variant) |
| events.jsonl | CWE-345 HMAC tampering | HMAC chain over NDJSON; break → `409 E_REPLAY_INTEGRITY`; key in `SecretStore` | `cargo test -- replay_integrity` (byte-flip + truncation scenarios) |
| Card content | OWASP LLM01 indirect injection | `IndirectInjectionShield` at SSE producer; detected injection → sanitized tombstone + AYIN span flag | `cargo test -- injection_shield_card_content` (_audit pending_ verify callsite) |

---

## R6 — Mermaid Streaming + Ratatui Async

**Mermaid v11.15.0** (installed): No streaming/incremental API exists. `mermaid.render(id, definition)` requires a complete syntactically valid diagram string.

**Fallback strategy** (confirmed required):
1. Buffer SSE delta events in card-local state until definition is complete (balanced delimiters or explicit `diagram_complete` SSE event)
2. Call `mermaid.render()` once on completion; show skeleton during accumulation
3. On subsequent updates: diff full definition, re-render, swap SVG
→ Discrete batch-renders, not continuous streaming. Acceptable for card format.

**Ratatui + pure reducer**: fully compatible via `tokio::mpsc` bridge:
```
SSE events (tokio task) ──mpsc::Sender──► main task
main task: state = reduce(state, event)  ← synchronous, cheap
           terminal.draw(|f| view(&state, f))  ← owns Terminal
```
`terminal.draw()` stays on the task that initialized Terminal. Reducer is pure sync; no blocking. Keyboard + SSE events both route through the mpsc channel via `tokio::select!`.

---

## R7 — Cockpit Depth Shift Validation

**Current routes**: `/cockpit/platform` (d0), `/cockpit/project/:id` (d1), `/cockpit/build/:codename` (d2), `/cockpit/file/:codename/*` (d3).

**URL paths do NOT change** — only the `depth` integer in the `RouteScope` TypeScript union changes.

**Impact** (medium risk):
- `RouteScope` union: 4 variants need `depth` literal incremented (0→1, 1→2, 2→3, 3→4)
- Add new `{ depth: 0; kind: 'lightspace' }` variant for the Lightspace screen
- `scopeFromParams()`, `toScopeUrl()`, `CockpitShell.svelte`, `BottomBar.svelte`, `TopStrip.svelte`, 2 test files all branch on `depth`
- Tests in `cockpit-selection-store.test.ts`, `gitforest-p7-units.test.ts` may assert `depth === 0` — require manual update

**Recommended approach**: Single atomic commit — add `lightspace` variant at depth 0, increment existing four variants, update all consumers. TypeScript exhaustiveness at compile time catches any missed cases.

---

## Research gate [R] summary

| | Verdict | Key decision |
|---|---|---|
| R1 | Reject Yjs/Automerge for v0.1.0 | Revisit at v0.2.0 multi-operator milestone |
| R2 | Adopt fork/commit primitives | BranchLane = arXiv:2602.08199; race detection only in v0.1.0 |
| R3 | Adopt Redux/Elm pure-function model | Rust ownership + enum exhaustion are the implementation moat |
| R4 | Per-card seq + snapshot catch-up | UI consumer: seq-gap → snapshot; LLM watcher: drop-and-warn (distinct policies) |
| R5 | 4 attack surfaces documented | All mitigations planned; _audit pending_ on injection_shield callsite |
| R6 | Mermaid batch-render only | Fallback: buffer until complete, re-render on update |
| R7 | Depth shift: medium risk, purely TypeScript | URL paths unchanged; atomic commit + TypeScript exhaustion catches all consumers |
