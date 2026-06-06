# Architectural Decisions — scope-keyed-cockpit-routes

Date: 2026-06-05 | Status: ACTIVE (Phase 1 design inputs)

---

## AD-01: Extend existing hash router, do NOT introduce SvelteKit routing

**Decision**: Add 4 new `ScreenKey` variants and 4 regex entries to `src/lib/routes.ts`. Cockpit remains a SPA with hash-based navigation.

**Why**: The codebase has a mature hand-rolled regex router with 40+ route entries, deep-link rewrites, lazy-loading, and E2E tests against it. Introducing SvelteKit routing would require a build-system overhaul (svelte.config.js changes, file-system routing, adapter) and would break all existing E2E specs. The existing pattern handles parameterized routes already (see `/builds/:buildId/:view`). The file-path catch-all (`/cockpit/file/:codename/:path*`) needs a greedy regex but no structural change.

**Downstream**: `routes.ts` is the single extension point for Phase 3 Wave A. Existing tests continue to compile; no E2E migration required.

---

## AD-02: Polymorphic right drawer dispatched by selection kind, not fixed tabs

**Decision**: Right drawer content is determined by the `Selection` union type (`none | build | worker | escalation | span | gate | decision | pr | crate`). A `FocusRouter.svelte` component dispatches to 9 focus components. No fixed tabs.

**Why**: The operator is reacting to events in the bento, not browsing a catalogue. Selection IS the context; the drawer should reflect what was clicked. Fixed tabs create dead panels visible at all times — blank panels at d2 (no PR context) or d1 (no crate context) confuse rather than inform. Polymorphic dispatch also eliminates the existing duplication between `pr-detail-panel`, `engineer-zones`, and the right area of Cockpit.svelte.

**Downstream**: Wave B creates `FocusRouter.svelte` + 9 `*Focus.svelte` components. Wave B also deletes the now-redundant `engineer-zones` card role from the d2 bento (replaced by drawer).

---

## AD-03: FLIP scope transition via WAAPI, 400ms cubic-bezier(0.4,0,0.2,1)

**Decision**: Scope changes use First-Last-Invert-Play with Web Animations API. Shared-element origin is the card/row that triggered the navigation. Scope-accent color crossfades in parallel (CSS `color-mix` + `transition: background-color`).

**Why**: Pure CSS transitions can't handle shared-element geometry. SvelteKit's `enhance` transition system is unavailable. WAAPI is the closest native primitive. The specific easing (Material Design "standard curve") is proven for UI element motion by Google Material 3 and Linear's motion system. `prefers-reduced-motion` degrades to instant swap (no animation, no jank).

**Downstream**: `src/lib/cockpit/shell/transitions.ts` implements the core FLIP logic. CockpitShell wraps each scope mount in a `<div data-scope-mount>` for geometry capture.

---

## AD-04: Unified HITL Inbox merges three fragmented queues at d1

**Decision**: `UnifiedHitlInbox.svelte` polls/subscribes to three sources — (1) PR inbox (via `api.getOpenPRs()`), (2) Conductor HITL (`GET /api/conductor/hitl`), (3) IronClaw nonces (`GET /api/control`). Renders a single sorted list keyed by `source: 'pr'|'conductor'|'ironclaw'`.

**Why**: Per `reference_webshell_three_hitl_systems.md` (memory): three separate surfaces cause operators to miss escalations. The current HITLInbox only shows IronClaw nonces; ConductorHitlPanel is a separate card; PR inbox is on a different screen. A unified surface at d1 directly advances P2 mechanical check #3 (approval-honest-confirmation). Approve/reject actions route to the correct underlying API based on `source`.

**Downstream**: Backend changes deferred to Phase 5 (no server aggregation needed — client merges three HTTP calls). `EscalationFocus.svelte` in the right drawer exposes detail + action buttons per `source`.

---

## AD-05: Wave A is a verbatim content move — zero visible UX change

**Decision**: Wave A moves Cockpit.svelte content verbatim into CockpitPlatform.svelte. Cockpit.svelte becomes a ~10-line redirect component that navigates to `/cockpit/platform`. No data wiring changes, no component modifications.

**Why**: Smallest possible Wave A reduces regression risk. The goal is to establish the scope-routing infrastructure (routes.ts extension + CockpitShell frame + stores) before any visible UX changes. The existing 1942-LOC Cockpit.svelte is battle-tested; moving it verbatim preserves all existing behavior. Wave B introduces the scope split; Waves C/D add d1 substance.

**Downstream**: Wave A exit gate only requires that the existing E2E test `cockpit.spec.ts` still passes against the new `/cockpit/platform` URL (after redirect). Test fixtures updated to use new URL.

---

## AD-06: SSE EventSource for A2A firehose + skill pulse, not WebSocket

**Decision**: `/api/cockpit/project/:id/a2a` and `/api/cockpit/project/:id/skills` are SSE endpoints, not WebSocket upgrades.

**Why**: Webshell already uses SSE for multiple streams (build progress, fleet events, copilot tokens). SSE has a simpler per-request auth model (Bearer header via `EventSource` polyfill or `fetch`), is HTTP/1.1 compatible (CF Tunnel handles it cleanly), and maps to Axum's `Sse<>` response type which is already in use. WebSocket would require a separate upgrade handshake, a separate auth mechanism, and multiplexing logic. The A2A stream is one-directional (server → client) — WebSocket's bidirectionality is unused overhead.

**§63.P5 implication**: tokio broadcast `Receiver::recv()` returns `RecvError::Lagged` when the consumer is slow. We emit `A2AEvent::Lagged { dropped_count }` per §63.P5 tolerant pattern, then continue from the new tail. The frontend renders a "⚠ 3 events dropped — resuming" inline notice.

**Downstream**: Axum handler uses `Sse::new(stream).keep_alive(KeepAlive::default())`. Phase 5 Wave C implements the full handler.

---

## AD-07: BottomBar is scope-conditional (d0/d3 hide it)

**Decision**: `ScopeBottomBar.svelte` renders only for d1 (WaveComposer + SmartDispatch) and d2 (WaveComposer). d0 and d3 have no bottom bar.

**Why**: d0 (platform) actions are global (not project-scoped) and surface via the ambient top strip instead. d3 (file) is a read-only inspection surface with no dispatch actions. Showing an action bar at d0/d3 would present disabled or context-irrelevant controls — WCAG 1.3.6 advises against surfacing unusable controls.

**Downstream**: `CockpitShell.svelte` conditionally mounts `<ScopeBottomBar>` via `{#if scope.depth === 1 || scope.depth === 2}`.

---

## AD-08: CopilotDrawer left rail is locked (never collapses in cockpit screens)

**Decision**: In CockpitShell, the left drawer (CopilotDrawer) is always visible at 360px. `⌘\` keyboard shortcut is wired but only toggles on non-cockpit screens.

**Why**: The copilot is the operator's primary steering instrument during builds. Hiding it during cockpit navigation degrades the "no terminal fallback" Northstar goal — if the operator needs to type a command mid-triage, the copilot must be immediately available. 360px left column + 480px right drawer + 56px top + 40px bottom still leaves ~620px center bento width at 1440px screens (above the 600px bento minimum per the plan's layout spec).

**Downstream**: CockpitShell passes `locked={true}` to CopilotDrawer. The existing CopilotDrawer toggle behavior is preserved on all other screens.
