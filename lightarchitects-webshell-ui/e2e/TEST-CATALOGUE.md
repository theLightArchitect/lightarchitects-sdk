# Test Suite Catalogue — lightarchitects-webshell-ui

**Standard:** §57 E2E Test Engineering Standards (Builders Cookbook Canon XXXII)
**Last updated:** 2026-05-01
**Gate:** `pnpm test:run` (426/426) + `svelte-check --threshold error` (0 errors)

---

## Summary

| Layer | Files | Tests | Gate |
|-------|-------|-------|------|
| Unit (Vitest) | 22 | 426 | Blocks merge |
| E2E integration (Playwright) | 1 | ~324 | Blocks release |
| E2E live integration | 1 | 13 | Blocks release |
| E2E visual | 1 | 5 | Manual only |
| **Total** | **25** | **~768** | |

---

## Layer 1 — Unit Tests (`src/__tests__/`)

Run: `pnpm test:run`  
Tier: **Smoke** (blocks merge)

| File | Tests | Capability | What it covers |
|------|-------|------------|----------------|
| `auth.test.ts` | 9 | Chrome / Auth | `resolveToken`, `getToken`, `authHeaders` |
| `components.test.ts` | 12 | All | Module import smoke for 12 UI components |
| `copilot.test.ts` | 33 | Copilot | `parseCommand`, `SLASH_COMMANDS`, store lifecycle, SSE-driven response, `buildBuildContext` |
| `design-tokens.test.ts` | 28 | Design system | `SIBLINGS`, colors, `QUALITY_GATES`, `DOMAIN_AGENT_COLORS`, layout/typo/motion constants |
| `featureFlags.test.ts` | 8 | Chrome | `FEATURE_FLAGS` defaults, `isEnabled()` |
| `helix-math.test.ts` | 20 | Knowledge / Helix | `getFade`, `getPrimaryFrame`, `getEntityCenter`, `seededRandom`, constants |
| `hotkeyRegistry.test.ts` | 33 | Chrome | `registerHotkey`, `dispatchHotkey`, `chordToMatches`, DiffPreview/SquadDispatch/KeymapLegend imports |
| `integration.test.ts` | 12 | Cross-capability | BuildQueue, Workspace, HierarchyNav, sibling dispatch, polytope rendering integration |
| `phase5.test.ts` | 25 | Build lifecycle | Artifacts + Findings + Notes stores, `activeBuildArtifacts`, slash commands for artifacts |
| `phase6.test.ts` | 34 | Observability | Conductor, Arena, Alert stores; `sitrepReady`, `platformHealth`, `siblingDispatchCounts`; Ops.svelte |
| `phase7.test.ts` | 18 | Build lifecycle | `intakeForm`, `META_SKILL_CARDS`, build creation flow, `PILLAR_ACTIONS` consistency |
| `phase8.test.ts` | 20 | Chrome / Polish | Code-splitting, SSE event handlers, control commands, accessibility, responsive layout |
| `polytopes4d.test.ts` | 13 | Knowledge / Helix | `getPolytope4D`, vertex normalization, edge validity, cache, all polytope types |
| `routes.test.ts` | 23 | Chrome | `matchRoute()` for all routes, BuildDetail patterns, legacy aliases, unknown fallback |
| `settings-persistence.test.ts` | 11 | Chrome | `collectSettings`, `applySettings`, `loadPersistedSettings`, debounced save |
| `setup.test.ts` | 13 | Setup | `loadSetupInfo`, `loadModels`, `saveSetup`, `resetSetup` |
| `squadComm.test.ts` | 11 | Squad dispatch | `MessageType`, `DomainAgent`, `DEFAULT_IMPORTANCE`, `importanceForFinding`, `wrapAsProgressUpdate` |
| `sse.test.ts` | 29 | SSE / Streaming | `_handleEvent` for all event types: ayin_status, build_update, pillar_update, copilot_response, etc. |
| `stores.test.ts` | 21 | State | Initial state, `buildStats`, `activeBuild`, `selectedPillar`, `spikeSibling`, wave tick |
| `types.test.ts` | 18 | Type system | `PILLARS`, `PILLAR_ACTIONS`, `META_SKILLS`, `SIBLINGS`, `SiblingWave`, type validation |
| `vocabulary.test.ts` | 8 | Vocabulary canon | `TERMS`, `NAV_LABELS`, `TOOLTIPS`, `t()`, `tip()` |
| `ws.test.ts` | 10 | Terminal | `TerminalWS` constructor, connect, message handling, `sendText`, `sendResize`, lifecycle |

---

## Layer 2 — E2E Integration (`e2e/webshell.spec.ts`)

Run: `pnpm test:e2e -- webshell`  
Mode: serial, shared browser context  
Mocks: setup flow + browser-state only; SOUL/siblings/AYIN hit real backend  
Tier: **Capability** (blocks release)

### Boot & Chrome

| Section | Line | Stability tier | Capability | Key assertions |
|---------|------|----------------|------------|----------------|
| 1. Boot sequence | 128 | **Smoke** | Chrome | Page loads, token accepted, hash routing, nav renders |
| 2. Navigation | 161 | **Smoke** | Chrome | All 4 nav tabs navigate, back to Builds |
| 12. Status bar | 604 | Capability | Chrome | Status bar visible, agent indicator present |
| 14. Console health | 654 | Capability | Chrome | Zero JS errors at load |
| 26. Status bar detailed | 1342 | Capability | Chrome | Platform health, uptime, ayin status |
| 27. Keyboard shortcuts | 1372 | Capability | Chrome | Cmd+K opens dispatch, Escape closes |
| 33. AuthBanner (#13) | 3490 | Capability | Chrome / Auth | 401/403 surfaced in UI |
| 34. Tooltip primitive (#26) | 3521 | Capability | Chrome | Tooltip renders on hover |
| 35. DiffPreview modal (#47) | 3535 | Capability | Chrome | Diff modal opens/closes |
| 36. Auth token lifecycle | 2203 | Capability | Chrome / Auth | Token persists across navigation, expired token rejected |
| 37. API contract validation | 2232 | Capability | Chrome | API endpoints respond with correct shape |
| 38. Empty-state hero (#10/#48) | 3610 | Capability | Chrome | Empty state affordance visible when no builds |
| 39 / 70. Console health (final) | 3759 / 4247 | Capability | Chrome | Zero errors after full session |
| 40. Security headers & XSS | 2359 | Capability | Chrome / Security | CSP, XSS inputs rejected |
| 41. Graceful degradation | 2400 | Capability | Chrome | Missing backend, offline mode |
| 42. Accessibility | 2442 | Capability | Chrome | Axe audit on key screens |
| 40 (§57). Accessibility WCAG 2.1 AA | 3632 | Capability | Chrome | WCAG checks across all 4 routes |
| 45. Responsive viewport | 2545 | Capability | Chrome | 375px / 768px / 1440px layout |
| 48. Memory bounds | 2692 | Capability | Chrome | Memory leak — 6 route cycles |
| 66. KeymapLegend (#4) | 3876 | Capability | Chrome | Cmd+/ opens legend, all hotkeys listed |
| 67. StatusBar auth chip (#13) | 3948 | Capability | Chrome | Auth chip reflects login state |
| 68. Header band 56px (#38) | 4000 | Capability | Chrome | OPS + Dispatch headers exactly 56px |
| 69. Tutorial T1 — Shepherd.js (#27) | 4048 | Capability | Chrome | First-build tour steps render |
| 80. Vocabulary canon | 4760 | Capability | Chrome | No "siblings" in OPS panel; "agents online" present |

### OPS / Observability

| Section | Line | Stability tier | Capability | Key assertions |
|---------|------|----------------|------------|----------------|
| 3. OPS screen | 219 | **Smoke** | Observability | SQUAD HEALTH tab, 7 agent names, LIVE TRACE tab switch, platform health indicator |
| 18. OPS screen deep (real data) | 908 | Capability | Observability | Real sibling health from backend, status indicators |
| 19. Compaction panel | 956 | Capability | Observability | Compaction panel renders, SOUL stats present |
| 28. AYIN connectivity (real) | 1421 | Capability | Observability | AYIN status indicator not "offline" |
| 72. OPS staleness + chevron (#61) | 4186 | Capability | Observability | Heartbeat staleness badges, expand/collapse agent card |

### Build Lifecycle

| Section | Line | Stability tier | Capability | Key assertions |
|---------|------|----------------|------------|----------------|
| 4. Queue screen | 262 | **Smoke** | Build lifecycle | Build queue renders, empty state, active builds chip |
| 15. BuildDetail (Workspace) | 690 | Capability | Build lifecycle | Build detail panel, pillar rail, view mode switcher |
| 29. Plan Builder | 1439 | Capability | Build lifecycle | Plan step creation, phase management |
| 30. LASDLC Framework | 1556 | Capability | Build lifecycle | LASDLC phases, gate markers |
| 31. Plan Creation Journey | 1662 | Capability | Build lifecycle | Full plan creation end-to-end |
| 32. Project drill-down | 1939 | Capability | Build lifecycle | Project → build drill-down |
| 34. Kanban Board View | 2004 | Capability | Build lifecycle | Kanban columns render, card drag |
| 44. Build notes | 2518 | Capability | Build lifecycle | Notes editor opens, saves, renders markdown |
| 46. Roadmap export | 2655 | Capability | Build lifecycle | Export button, file output |
| 47. Plan lifecycle | 2672 | Capability | Build lifecycle | Plan phase transitions |
| 52. Build session creation | 3083 | Integration | Build lifecycle | POST `/api/builds`, ID returned, build in registry |
| 55. Quality gate execution | 3219 | Integration | Build lifecycle | Pillar trigger, gate status response |
| 58. Notes & artifacts | 3311 | Integration | Build lifecycle | Artifact list, note CRUD |

### Intake

| Section | Line | Stability tier | Capability | Key assertions |
|---------|------|----------------|------------|----------------|
| 5. Intake screen | 303 | **Smoke** | Build lifecycle | Form renders, fields present |
| 17. Intake screen deep | 868 | Capability | Build lifecycle | All meta-skill cards, field validation, submit |
| 37. BuildQueue header dedupe (#35) | 3585 | Capability | Build lifecycle | Header count doesn't duplicate |
| 73. Intake field validation (#60) | 4125 | Capability | Build lifecycle | Required fields, duplicate name guard |

### Squad Dispatch

| Section | Line | Stability tier | Capability | Key assertions |
|---------|------|----------------|------------|----------------|
| 6. Dispatch screen | 322 | **Smoke** | Squad dispatch | Input panel, agent chips, submit button, SQUAD DISPATCH header |
| 43. Dispatch & sibling interaction | 2493 | Capability | Squad dispatch | Agent selected, dispatch submitted |
| 59. Sibling dispatch | 3349 | Capability | Squad dispatch | Sibling list, dispatch request |
| 60. Squad Dispatch screen | 3396 | Capability | Squad dispatch | Full screen render, agent grid |
| 78. AgentDetail panel | 4663 | Capability | Squad dispatch | Rail click → detail opens, Escape closes, phase strip |
| 79. HistoryRail geometry | 4730 | Capability | Squad dispatch | `.history-strip` computed height = 36px |

### Helix / Knowledge

| Section | Line | Stability tier | Capability | Key assertions |
|---------|------|----------------|------------|----------------|
| 7. Helix panel | 351 | Capability | Knowledge / Helix | 3D canvas renders (WebGL), no crash |
| 21. SOUL vault integration (real) | 1077 | Integration | Knowledge / Helix | Real SOUL search results, health check |
| 22. Sibling wiring (real) | 1140 | Integration | Knowledge / Helix | Sibling health endpoints respond |
| 24. Helix detail & tooltip | 1275 | Capability | Knowledge / Helix | Detail panel opens on node click, tooltip renders |
| 25. Canvas/WebGL components | 1308 | Capability | Knowledge / Helix | ParticleCanvas, PolytopeDecor render without error |
| 71. HelixLegend — ? button (#39) | 3809 | Capability | Knowledge / Helix | Legend opens, entity/pillar color map present |
| 41. Visual regression baselines | 3710 | **Visual** | Knowledge / Helix | Screenshot diff against committed baselines |

### Copilot

| Section | Line | Stability tier | Capability | Key assertions |
|---------|------|----------------|------------|----------------|
| 10. Copilot drawer | 505 | Capability | Copilot | Drawer opens/closes, input renders |
| 16. Copilot drawer deep | 817 | Capability | Copilot | Message send, loading state, response renders |
| 38. Copilot chat flow | 2291 | Capability | Copilot | Full send→response cycle (mocked) |
| 39. SSE resilience | 2334 | Capability | Copilot | SSE disconnect, reconnect, fallback |
| 51. Copilot comprehensive | 2851 | Capability | Copilot | Slash commands, context injection, sibling routing |
| 53. Copilot — Anthropic | 3119 | Integration | Copilot | Real Haiku/Sonnet response via `/api/builds/:id/copilot` |
| 54. Copilot — Ollama | 3166 | Integration | Copilot | Backend switch → Ollama → restore Anthropic |
| 56. Slash commands | 3268 | Capability | Copilot | All slash command names parsed + dispatched |
| 57. Provider switching | 3291 | Integration | Copilot | Switch backend, test, restore |
| 74. Copilot history + search (#57) | 4298 | Capability | Copilot | History persists across clear, search filters |
| 75. @-file autocomplete (#55) | 4430 | Capability | Copilot | `@` triggers file picker, file attached to message |
| 76. Copy-code-block (#55) | 4522 | Capability | Copilot | Code block copy button, clipboard content |
| 77. Drag-drop file (#55) | 4581 | Capability | Copilot | File drop zone, file appears in message |

### Memory / SOUL

| Section | Line | Stability tier | Capability | Key assertions |
|---------|------|----------------|------------|----------------|
| 9. Memory drawer | 471 | Capability | Knowledge / SOUL | Drawer opens, hot/cold memory lists |
| 20. Memory drawer deep (real data) | 997 | Integration | Knowledge / SOUL | Real SOUL entries load, search works |

### Setup / Config

| Section | Line | Stability tier | Capability | Key assertions |
|---------|------|----------------|------------|----------------|
| 8. Skin editor | 415 | Capability | Setup | Skin editor opens, theme tokens update |
| 13. Settings overlay | 617 | Capability | Setup | Settings panel, token display |
| 35. API error handling | 2135 | Capability | Setup | 4xx/5xx surfaced correctly, no crash |
| 50. Provider & model switching | 2739 | Integration | Setup | Switch backend + model, verify config |
| 23. ScrumReport overlay | 1217 | Capability | Setup | Scrum report renders, findings list |
| 11. Command palette | 546 | Capability | Chrome | Cmd+K opens, fuzzy search, action executes |

---

## Layer 3 — E2E Live Integration (`e2e/claude-code-oauth.spec.ts`)

Run: `pnpm test:e2e -- claude-code-oauth`  
Mode: serial, headed Chrome  
Mocks: **none** — hits real setup endpoints, real Claude Code auth  
Tier: **Integration** (blocks release)  
Prerequisite: `claude --print "hi"` works without `ANTHROPIC_API_KEY`

| # | Test | Capability | What it covers |
|---|------|------------|----------------|
| 1 | Setup wizard appears after reset | Setup | `DELETE /api/setup/reset` forces wizard |
| 2 | Wizard shows Choose Backend step | Setup | Wizard entry point renders |
| 3 | Claude Code card visible and selectable | Setup | `id: 'anthropic'` backend card |
| 4 | Continue → advances to Authentication | Setup | BackendStep → AuthStep transition |
| 5 | Use existing auth radio selected by default | Setup | `authMode === 'existing'` default |
| 6 | Continue → enabled without API key | Setup | `canProceed = true` when `authMode === 'existing'` |
| 7 | Continue → advances to Model step | Setup | AuthStep → ModelStep transition |
| 8 | Model cards render after loading | Setup | `GET /api/setup/models` response renders |
| 9 | Haiku model card visible | Setup | `claude-haiku-4-5-20251001` in card list |
| 10 | Clicking Haiku card selects it | Setup | `selectedModel` store set |
| 11 | Launch → saves config and transitions to app | Setup | `POST /api/setup/save`, wizard → app shell |
| 12 | setup/info reports Claude Code + Haiku | Setup | `backend: 'anthropic'`, `model: 'claude-haiku-4-5-20251001'` |
| 13 | Create a build for copilot test | Build lifecycle | `POST /api/builds` returns `build_id` |
| 14 | Haiku responds to arithmetic (real AI) | Copilot | Real Claude response contains "4" (60s timeout) |
| 15 | Haiku response is coherent | Copilot | Non-empty string from second real query |

---

## Layer 4 — E2E Visual (`e2e/screenshot-tour.spec.ts`)

Run: `pnpm test:e2e -- screenshot-tour`  
Mode: serial, headed Chrome  
Tier: **Visual** — manual only, never blocks CI  
Output: `screenshots/*.png` + `test-results/screenshot-tour.har`

| # | Route | Name | Settle | Capability |
|---|-------|------|--------|------------|
| 1 | `/` | 01-Builds | 2500ms | Build lifecycle — default landing |
| 2 | `/ops` | 02-Ops | 2000ms | Observability — squad health grid |
| 3 | `/intake` | 03-Intake | 1500ms | Build lifecycle — intake form |
| 4 | `/dispatch` | 04-Dispatch | 2000ms | Squad dispatch — agent selector |
| 5 | `/helix` | 05-Helix | 3000ms | Knowledge — 3D helix + vault entries |

---

## Capability → Test Coverage Map

| Capability | Unit | E2E (webshell) | E2E (live) | Visual |
|------------|------|----------------|------------|--------|
| **Setup / Config** | `setup.test.ts` (13) | §8, §13, §23, §35, §50, §57 | §1–§12 | — |
| **Build lifecycle** | `phase5.ts` (25), `phase7.ts` (18) | §4, §5, §15, §17, §29–§32, §34, §44, §46–§47, §52, §55, §58, §73 | §13 | 01-Builds |
| **Squad dispatch** | `squadComm.ts` (11) | §6, §43, §59, §60, §78, §79 | — | 04-Dispatch |
| **Copilot** | `copilot.ts` (33), `sse.ts` (29) | §10, §16, §38, §39, §51, §53–§54, §56, §74–§77 | §14–§15 | — |
| **Observability** | `phase6.ts` (34) | §3, §18–§19, §28, §72 | — | 02-Ops |
| **Knowledge / Helix** | `helix-math.ts` (20), `polytopes4d.ts` (13) | §7, §9, §20–§22, §24–§25, §71 | — | 05-Helix |
| **Chrome / Auth** | `auth.ts` (9), `routes.ts` (23), `hotkeyRegistry.ts` (33), `settings.ts` (11), `featureFlags.ts` (8), `phase8.ts` (20) | §1–§2, §12, §14, §26–§27, §33–§41, §42, §45, §48, §66–§70, §80 | — | — |
| **Design system** | `design-tokens.ts` (28), `vocabulary.ts` (8), `types.ts` (18) | §80 | — | — |
| **SSE / Streaming** | `sse.ts` (29), `ws.ts` (10) | §39, §51 | — | — |

---

## Known Accepted Deviations

| File | Line | Code | Reason | Disposition |
|------|------|------|--------|-------------|
| `src/lib/components/AgentRail.svelte` | 53 | `a11y_no_noninteractive_tabindex` | `div` conditionally acts as `button` (role computed at runtime); suppressed via `svelte-ignore`. Static analysis cannot resolve conditional role. | Accepted — `svelte-ignore` in place |

---

## Running the Suite

### Default — one persistent headed session

```bash
# Unit gate (blocks merge) — 426 tests, ~8s
pnpm test:run

# svelte-check gate (blocks merge)
pnpm exec svelte-check --threshold error

# E2E — opens ONE headed Chrome, runs all 324 tests serially,
# collects snapshot baselines, HAR, traces, and evidence bundles.
# Requires webshell backend running at :8733
pnpm test:e2e

# First run (no baselines yet) — creates committed PNG baselines
pnpm test:e2e --update-snapshots
```

### Opt-in specs (not in default run)

```bash
# OAuth wizard + real Haiku copilot (resets setup, restores after)
pnpm test:e2e -- claude-code-oauth

# Standalone visual capture tour (separate browser, no assertions)
pnpm test:e2e -- screenshot-tour
```

### Snapshot baseline workflow

```
First run:   pnpm test:e2e --update-snapshots   → creates e2e/snapshots/*.png
Review:      open playwright-report/index.html   → inspect all 7 baselines
Commit:      git add e2e/snapshots/              → baselines are the contract
Subsequent:  pnpm test:e2e                       → diffs against committed baselines
Update:      pnpm test:e2e --update-snapshots    → intentional design change
```

**Baselines collected in the default session** (section 41, within the same browser context as all other tests):

| File | Screen |
|------|--------|
| `queue-screen.png` | `/` — build queue |
| `ops-screen.png` | `/ops` — squad health grid |
| `dispatch-screen.png` | `/dispatch` — agent selector + input |
| `intake-screen.png` | `/intake` — intake form |
| `helix-screen.png` | `/helix` — 3D canvas (canvas masked) |
| `builds-screen.png` | `/builds` — build list |
| `squad-dispatch-idle.png` | Legacy name, now captured via `/dispatch` |

## Adding New Tests

1. Identify the capability (column in the capability map above)
2. Check which spec file owns that capability (§57.3a)
3. Add `data-testid` to any new component elements (§57.5b)
4. Use `ROUTES` from `e2e/lib/routes.ts` — never hardcode route strings (§57.4a)
5. Wire `EvidenceCollector` in `beforeEach`/`afterEach` (§57.2)
6. Update this catalogue
