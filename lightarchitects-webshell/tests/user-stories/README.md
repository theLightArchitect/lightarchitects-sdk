# User-stories E2E suite

Executable headed-Playwright harnesses covering all **67 user stories** from
`~/.claude/plans/option-a-and-do-replicated-goose.md`.

Unlike the sibling `tests/e2e/` directory (which uses Playwright's test-runner
for focused hot-memory and hybrid-retrieval specs), these harnesses are
standalone `.mjs` scripts — each is `node ./file.mjs` runnable and prints a
pass/fail matrix. The point is a complete regression gate for the app's full
surface, not a per-feature unit test.

**Current coverage: 67/67 (100%) across the combined suite · 0 failures.**

## Layout

| File | Covers stories | Runtime | What it does |
|------|----------------|---------|--------------|
| `all-stories-e2e.mjs` | 1–67 (primary) | ~5 min | The main harness — every story has at least one contract-level or UI-flow assertion. Baseline passes 54/67; the rest are marked as expected-skip for follow-up harnesses to upgrade. |
| `all-stories-upgrade.mjs` | [6],[32],[33],[35],[38],[40],[41] | ~90s | SSE-active-trigger harness: seeds helix files, touches `active.yaml`, drives pillars, seeds convergence fixtures, drives PTY WebSocket. Upgrades 7 primary skips to PASS. |
| `all-stories-upgrade-v2.mjs` | [28],[36],[49] | ~70s | Product-change validator: asserts `wikilinks.resolved` is surfaced, `AlertPanel` receives notify via SSE bridge, AYIN restart emits `ayin_status` events. |
| `story37-e2e.mjs` | [37] | ~30s | AYIN `strand_activations` bug-fix regression gate. Drops a synthetic span into the AYIN trace dir, verifies `strand_activation` events reach the webshell's SSE stream. |
| `stories-34-57-59.mjs` | [34],[57],[58],[59] | ~30s | Final sweep: real turnlog promotion via `/api/test/promote` + skill file structural validation for `/SQUAD`, `/BUILD`, `/ENRICH`. |
| `core-loops-e2e.mjs` | 14 core-loop assertions | ~5 min | Focused "is the tool useful?" harness: Loop 1 (build lifecycle) + Loop 2 (memory traversal). Subset of `all-stories-e2e.mjs` but with explicit narrative. |

## Prerequisites

- `lightarchitects-webshell` running on `:8733` with the Bearer token at
  `~/lightarchitects/webshell/.token` (launchd plist at
  `~/Library/LaunchAgents/io.lightarchitects.webshell.plist` handles this).
- Neo4j accessible at `bolt://localhost:7687` with `NEO4J_PASS` env set.
- AYIN running on `:3742` (launchd plist `io.lightarchitects.ayin.plist`).
- Playwright installed at `~/.npm/_npx/9833c18b2d85bc59/node_modules/playwright/`
  (the path is hard-coded in each harness — adjust if your npx cache differs).

## Run

```bash
# One at a time
NEO4J_PASS="$NEO4J_PASS" node ./all-stories-e2e.mjs
NEO4J_PASS="$NEO4J_PASS" node ./all-stories-upgrade.mjs
NEO4J_PASS="$NEO4J_PASS" node ./all-stories-upgrade-v2.mjs
NEO4J_PASS="$NEO4J_PASS" node ./story37-e2e.mjs
NEO4J_PASS="$NEO4J_PASS" node ./stories-34-57-59.mjs

# All, in order (prints composite summary)
NEO4J_PASS="$NEO4J_PASS" ./run-all.sh
```

All harnesses launch headed Chromium with `slowMo` set so the operator can
observe the browser drive through each story. To run headless, flip
`{ headless: false }` → `true` at the `chromium.launch()` call site.

## Design principles

1. **Independent try/catch per story** — one failure never aborts the suite.
   The operator gets a full pass/fail matrix from every run.
2. **Skip is first-class** — environmental preconditions that aren't met
   (e.g. Neo4j offline, no AYIN traffic) result in `⚠ SKIP` with a reason,
   not a false PASS and not a noisy FAIL.
3. **Contract-level where possible, UI-flow where it matters** — search-mode
   ordering, drawer navigation, orb spawning are tested via the DOM; everything
   else is validated against the HTTP/SSE wire shape. Mixed-mode assertions
   (click-button-then-grep-DOM) introduce races and live in fewer places.
4. **Gap tests pass today** — stories 65–67 in the plan are explicit ⚠ GAP
   stories. The harness asserts those gaps still exist. The day someone ships
   a feature that fills the gap, the assertion flips to fail and forces the
   test to be updated to match the new reality.

## Reference

- Plan file: `~/.claude/plans/option-a-and-do-replicated-goose.md`
- Sibling Playwright suite: `../e2e/` (hot_memory.spec.ts, hybrid_retrieval.spec.ts)
- Bearer auth: see `../../src/auth.rs`
- SSE event types: see `../../src/events/types.rs` (`WebEvent` enum)
- Test-only promotion endpoint: `POST /api/test/promote`
  (see `../../src/real_data.rs::test_promote`)

## Product changes this suite drove

1. `/api/soul/health.wikilinks` — added `resolved` count + `unresolved: null` + note
2. `sse.ts` — `control` events with `command: 'notify'` route to the `alerts` store
3. `io.lightarchitects.webshell.plist` — launchd-managed webshell with Neo4j env vars
4. `TraceSpanSummary.strand_activations` — top-level field + parser update (bug fix)
5. `WebshellTurnLog::pause()` + `POST /api/test/promote` — dev-only promotion trigger
6. `MemoryDrawer.svelte` — search-result buttons get `data-testid="memory-row"`
7. `lightarchitects::credentials` SDK module — canonical per-CLI credential
   detection (Claude Code / Codex / Gemini); replaces the webshell's prior
   filesystem heuristics; `/api/setup/info` now returns `login_source`.

## Canonical credential sources per backend

| Backend | macOS primary | Linux/Windows | Env override order |
|---|---|---|---|
| Claude Code | Keychain `Claude Code-credentials` (account = `$USER`) | `${CLAUDE_CONFIG_DIR ?? ~/.claude}/.credentials.json` | `ANTHROPIC_AUTH_TOKEN` → `CLAUDE_CODE_OAUTH_TOKEN` → `ANTHROPIC_API_KEY` |
| Codex | File **default**; Keyring `Codex Auth` opt-in | `${CODEX_HOME ?? ~/.codex}/auth.json` | `OPENAI_API_KEY` → `CODEX_API_KEY` |
| Gemini | — (no Keychain usage today) | `${GEMINI_HOME ?? ~/.gemini}/oauth_creds.json` | `GEMINI_API_KEY` → `GOOGLE_API_KEY` → `GOOGLE_APPLICATION_CREDENTIALS` |

Detection is done via the `lightarchitects::credentials` SDK module — a
plugin/registry with one `CliCredentialProvider` trait per CLI. Canonical
strings live only inside their provider module and are stored as byte
arrays (binary-analysis hardening, not a security boundary).

### Launchd vs terminal — env-var inheritance caveat

On macOS, processes managed by launchd (like the webshell's plist) **do
not see your shell's env vars**. OAuth credentials still flow through
(the Claude Code CLI reads `~/.claude/.credentials.json` or the Keychain
directly, and `HOME` is set in the plist), but `ANTHROPIC_API_KEY` /
`OPENAI_API_KEY` env vars only inherit if you launch the webshell from a
terminal that has them exported. To make env vars available under launchd,
add them to `EnvironmentVariables` in `io.lightarchitects.webshell.plist`.
