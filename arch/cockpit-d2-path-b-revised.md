# Path B — Revised after Path A landed

This document re-specs Path B against what Path A actually shipped, what it surfaced as harder than expected, and what's now genuinely the next-step priority.

Path A original target → **landed**:
- Contract ratified + moved to canonical location: `standards/canon/contracts/wire.http/gateway.get.v1-platform-builds-codename-progress.yaml`
- 8 open issues resolved with rationale: `arch/cockpit-d2-path-a-decisions.md`
- FleetSpan + FleetNode extended (4 fields, `AgentWaveContext`, `agent_focused_on` method): `lightarchitects/src/fleet/{span,tracker}.rs` + downstream test sites
- Gateway handler implemented with degraded-mode fallback: `lightarchitects-gateway/src/http/routes/builds.rs`
- 52 new unit tests (7 fleet + 18 builds + 27 prior cockpit JS validations)
- Quality gates pass on touched code

---

## What Path A revealed (changes the Path B plan)

### 1. Pre-existing tech debt blocks the workspace clippy gate

The gateway crate has a clippy violation in `src/core_tools/webshell_launch.rs::run` (131 lines vs 100-line `too_many_lines` cap). It's pre-existing — not from this PR — but it means `cd lightarchitects-gateway && cargo clippy --lib -- -D warnings` currently fails for the workspace.

**Implication for Path B**: any v0.2.0 work in the gateway crate will hit this same wall. The remediation isn't ours to land in this PR, but Path B should track it as `BLOCK-1` so we know to either fix `webshell_launch.rs::run` (decompose into helpers) before shipping v0.2.0 endpoints, or get an `#[allow]` exception from operator.

### 2. Manifest schema migration is more involved than the proposal anticipated

The `arch/cockpit-d2-manifest-extension.proposal.yaml` issued five M-issues. After implementing the degraded-mode fallback in `assemble_progress()`, two of them turn out to need more than a manifest schema change:

- **M-3 (commit SHA writeback)**: requires modifying `lightarchitects/src/lightsquad/wave_dispatcher.rs` to write `phases[i].waves[j].commits[]` on commit completion. The hook point exists but the writeback path doesn't.
- **M-4 (task IDs)**: requires a `/PLAN` skill finalize step that injects `<!-- id: t3.2.1 -->` HTML comments into `plan.md`. This is a skill change, not a Rust change.

Neither is a v0.2.0 endpoint blocker (degraded mode handles both), but both are required before the cockpit's full vision lights up on live builds.

### 3. Path import collision in builds.rs surfaced a workspace pattern

`axum::extract::Path` shadows `std::path::Path` in route modules. I worked around it locally by fully qualifying `std::path::Path` at helper sites. Worth promoting to a Cookbook §63 entry: "In Axum route modules, fully qualify `std::path::Path` to avoid collision with the `Path` extractor — never use `as` aliases for either, since both names show up in `cargo clippy` lints and docs."

### 4. Test infrastructure for the lightarchitects crate has feature-gating bugs

`cargo clippy -p lightarchitects --tests` fails on `OllamaCliProvider` and `conversation` items that exist in the source but aren't reachable from default-feature test compilation. Pre-existing issue (not my changes). Tracks as `BLOCK-2` — should be fixed when someone touches that test infrastructure.

---

## Revised Path B — v0.2.0 scope

### Reordered by what now unblocks what

The original Path B had 4 streams (command palette, DIFF lens, SSE streaming, symbol-aware focus). Path A's reality shifts the priority:

| # | Stream | Now-priority | Reason |
|---|--------|--------------|--------|
| **B1** | **AYIN fleet wave_context propagation** | **HIGHEST** | The Rust extension fields exist (Path A) but the JSONL tailer doesn't populate them yet. Without producer-side wiring, the FleetNode extensions sit unused. This is the single biggest gap between "contract exists" and "cockpit shows live agents". |
| **B2** | **GitHub PR fetch via `gh` CLI** | HIGH | `builds_progress` currently returns `pr: null`. Wiring `gh pr view --json state,url,number` adds a high-value cockpit field at low risk. The contract already specifies it. |
| **B3** | **AYIN fleet join in the gateway** | HIGH | The contract defines `agents[]` per wave. The handler currently returns `phases[]` as-is from manifest (no fleet enrichment). Wiring `reqwest::Client → GET http://127.0.0.1:3742/api/fleet` then joining by `wave_id` completes the cockpit's agent-badge story. |
| **B4** | **Conformance test fixture** | MEDIUM | Per the contract, a `test-fixture-build/` manifest is needed for runtime-verified alpha-gate evidence. Currently the alpha_gate is `declared-pass`. Build the fixture, write the integration test, upgrade verdict to `verified`. |
| **B5** | **SSE streaming variant** (`/progress/stream`) | MEDIUM | Original Path B item. Now clearer: needed once `B1+B3` ship and AYIN fleet entries change frequently enough that snapshot polling becomes wasteful. |
| **B6** | **Command palette dispatch** in cockpit HTML | MEDIUM | UI-side. The placeholders are wired (`:phase / regex |yq .gates ?file`). Implementing the actual parser + handler functions is a self-contained JS change. No backend dependency. |
| **B7** | **DIFF lens implementation** in cockpit | LOW | The lens is stubbed. Implementing real multi-file diff requires either: (a) calling `git diff` server-side and shipping hunks via a new `GET /v1/platform/builds/{codename}/diff` endpoint, or (b) embedding a diff renderer in JS. Path (a) is more useful but requires a new contract. |
| **B8** | **Symbol-aware `focus_target_fn`** | LOW | F-1 deferred to v0.2.0 explicitly. Refactor string → `{kind, name, file}`. Touches `FleetSpan`, conformance tests, contract bump to `0.2.0`. |

### Removed from original Path B (now better classified)

- ~~"Move contract to canonical location"~~ — done in Path A.
- ~~"Implement gateway handler"~~ — done in Path A.
- ~~"Add FleetNode wave context fields"~~ — done in Path A.

### Added (new in revised Path B)

- **B-X1 (BLOCK-1 resolution)**: decompose `webshell_launch.rs::run` so workspace clippy can be brought back to green. Owner: not Path A scope, but blocks Path B gateway work.
- **B-X2 (BLOCK-2 resolution)**: fix the `lightarchitects` test feature gating for `OllamaCliProvider` + `conversation` items.
- **B-X3 (Cookbook §63 amendment)**: add the `std::path::Path` vs `axum::extract::Path` qualification rule for route modules.
- **B-X4 (Manifest writeback)**: implement M-3 in `wave_dispatcher.rs::on_commit_produced` so `phases[i].waves[j].commits[]` gets written.
- **B-X5 (Plan task-ID injection)**: implement M-4 in the `/PLAN` skill's finalize step.

---

## Recommended Path B ordering

Two clean phases:

**Path B Phase 1 — Live data plumbing** (≈3 streams in parallel):
- B1 (JSONL tailer wave_context parse + agent_focused_on calls)
- B2 (`gh` CLI PR state fetch in `builds_progress`)
- B3 (AYIN fleet join in `builds_progress` — joins by `node.wave_id` from the extended FleetNode)
- B4 (test-fixture-build + integration test → alpha_gate verified)

**Path B Phase 2 — UI completion** (≈2 streams in parallel):
- B5 (SSE `/progress/stream` once B1+B3 are live)
- B6 (cockpit command palette dispatch)
- B7 (cockpit DIFF lens via new `git diff` endpoint)

Tech debt items (B-X1..B-X5) interleave as opportunistic cleanup — none of them block B1..B6 critically.

---

## What I'd ship next if asked to continue

**One PR**: B1 (JSONL wave_context propagation) + B2 (gh CLI PR fetch). Both are self-contained, both light up immediate cockpit value, neither touches the workspace clippy debt. Estimated scope: ~150-200 lines Rust, ~5-7 new tests.

That gives the next iteration a live cockpit where:
- Agent badges actually pulse for the live wave-2 TEST agent (B1)
- PR status shows on the build header (B2)

After that, B3 (AYIN fleet join) unlocks the full agent-per-task badge story — but it's the heaviest piece and benefits from B1 shipping first as a validation.
