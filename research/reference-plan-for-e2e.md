---
project: lightarchitects-sdk
codename: reference-e2e-small
status: draft
tier: SMALL
lasdlc_template_version: "2.5.1"
created: 2026-05-30
updated: 2026-05-30
---

# Reference Plan — SMALL Tier E2E Build (ironclaw-autonomous-e2e research artifact)

> **Purpose**: Reference document for the Phase 6 E2E deliverables of
> `ironclaw-autonomous-e2e`. Shows a valid SMALL-tier LASDLC plan structure
> as produced by `/PLAN`. Not intended for execution — use the parent build's
> plan at `~/.claude/plans/ironclaw-autonomous-e2e.md`.

---

## Northstar Lineage

```yaml
northstar_lineage:
  northstar_text: >
    Advances P1 (E2E Engineering from Webshell UI): the autonomous build panel
    lets an operator launch, monitor, and approve ironclaw builds end-to-end
    from the webshell without opening a terminal.
  pillar_mapping: P1
  build_to_northstar_mapping:
    - "AutonomousBuildsPanel renders at #/autonomous → operator has a UI entry point"
    - "POST /api/builds (mode=autonomous) from UI → operator initiates build without terminal"
    - "HitlModal → operator approves decisions inside the webshell"
  northstar_metric_delta_estimate:
    before: "Autonomous builds require terminal + curl invocations"
    after: "Full build lifecycle visible + actionable from webshell UI"
    measurement: "terminal_window_open_count === 0 asserted by P1 test gate"
```

---

## Tier: SMALL (4 phases)

Rationale: single-surface addition (AutonomousBuilds screen + one API endpoint integration),
no new crates, no database schema changes, no security surface changes.

---

## Phase Set

### Phase 1 — Architecture [A+C]

**Deliverables**:
- C3 component diagram: `AutonomousBuildsPanel → StartForm → WaveSlotGrid → HitlModal`
- API surface: `POST /api/builds` (mode=autonomous), SSE `/api/builds/:id/events`

**Exit criteria**:
- Component diagram authored
- API shapes documented (request/response schema)
- P1 invariant (`terminal_window_open_count === 0`) declared as acceptance criterion

**Gate 1**: `/GATE [A+C]`

---

### Phase 2 — Build [A+S+Q+C+T]

**Deliverables**:
- `AutonomousBuildsPanel.svelte` — StartForm + WaveSlotGrid + HitlModal
- `AutonomousBuilds.svelte` — thin screen wrapper
- Route wiring: `ScreenKey | 'AutonomousBuilds'`, `/^\/autonomous$/` entry
- `screenModules.AutonomousBuilds` lazy-load entry in `app.svelte`

**File-function map**:
| File | Function | Agent |
|------|----------|-------|
| `src/components/ironclaw/AutonomousBuildsPanel.svelte` | full panel | CORSO |
| `src/screens/AutonomousBuilds.svelte` | screen wrapper | CORSO |
| `src/lib/routes.ts` | ScreenKey union + route entry | CORSO |
| `src/app.svelte` | screenModules entry | CORSO |

**Exit criteria**:
- `pnpm test:run` passes
- `pnpm exec svelte-check --threshold error` clean
- No `[data-testid="terminal-panel"]` in panel markup

**Gate 2**: `/GATE [A+S+Q+C+T]`

---

### Phase 3 — E2E Verify [A+Q+C+T]

**Deliverables**:
- `lightarchitects-webshell/tests/e2e/autonomous-builds.spec.ts`
  - P1 gate: `assertNoTerminalPanel` at every step
  - Golden path: screen render → POST /api/builds → WaveSlotGrid visible
- `lightarchitects/tests/property/decision_ledger_chain.rs`
  - 6 properties: integrity, monotonicity, tamper, deletion, key-isolation, round-trip
- `lightarchitects-webshell/tests/autonomous_ollama_e2e.rs`
  - Real Ollama Cloud E2E; skip when `OLLAMA_API_KEY` absent

**Exit criteria**:
- `cargo test --test autonomous_ollama_e2e` compiles + skips cleanly without key
- `cargo test -p lightarchitects --test decision_ledger_chain` passes
- Playwright spec structured (headless: false) with CI-safe skip guards

**Gate 3**: `/GATE [A+Q+C+T]`

---

### Phase 4 — Merge + Close-out [A+S+Q+C+O+P+K+D+T+R]

**Deliverables**:
- Pre-merge `/GATE` passing all dimensions
- `feat/ironclaw-autonomous-e2e` merged to main
- `active.yaml` updated to `status: merged`
- Post-merge rename: `ironclaw_hitl_*` → `lightclaw_hitl_*` (tracked separately)

**Gate 4 (pre-merge)**: Full `/GATE [A+S+Q+C+O+P+K+D+T+R]`

---

## Pre-flight Checks (G1–G8)

| Gate | Status |
|------|--------|
| G1 `git fetch github main` | ✓ passes |
| G2 local main == github/main | ✓ clean |
| G3 `cargo check --workspace` on main | ✓ clean |
| G4 `make quality` on main | ✓ passes |
| G5 branch + worktree no collision | ✓ `feat/ironclaw-autonomous-e2e` exists |
| G6 `active.yaml` entry | ✓ `ironclaw-autonomous-e2e` present |
| G7 parent program present | n/a |
| G8 ≥10GB disk free | ✓ |

---

## Shipped-means 5 Conditions

1. `AutonomousBuilds.svelte` renders at `#/autonomous` with no terminal panel
2. `POST /api/builds` (mode=autonomous) returns a UUID from the webshell UI
3. `decision_ledger_chain` property tests pass (6 properties, ≥30 cases each)
4. `autonomous_ollama_e2e` compiles; tests skip cleanly when `OLLAMA_API_KEY` absent
5. Pre-merge `/GATE` passes all dimensions on `feat/ironclaw-autonomous-e2e`

---

## Close-out

- Archive: `.gate-evals/*.yaml` committed to branch
- Lessons: `research/reference-plan-for-e2e.md` (this file) + phase 6 findings → helix
- Git: `feat/ironclaw-autonomous-e2e` squash-merged to main
- Tracking: `active.yaml` status → `merged`, `portfolio.md` updated
