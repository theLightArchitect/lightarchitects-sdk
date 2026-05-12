# Webshell UX Audit
**Version**: 1.0  
**Date**: 2026-05-12  
**Author**: Claude (Light Architects engineer)  
**Scope**: Full screen catalogue + multi-lens evaluation of the LA Platform webshell at `http://localhost:8733`  
**Screenshots**: `~/Projects/.playwright-mcp/` + root `ws-*.png` files (captured live, 2026-05-12)

---

## Evaluation Lenses

| ID | Lens | Standard |
|----|------|----------|
| E | Engineer | Operational utility — does it reduce TTI (time-to-insight) for the operator? |
| UX | UI/UX | Visual hierarchy, ergonomics, affordances, empty states, accessibility |
| NS | Northstar | Pillar 1: operator ships E2E from webshell without terminal fallback |
| LS | LASDLC | Gate coverage, phase visibility, compliance surface |
| AS | Agentic SDLC | 4-layer visibility model: Reasoning · Execution · Verification · Economics |
| IB | Industry Baseline | Standard ops dashboard patterns (Grafana, Linear, Vercel, Temporal) |
| VC | Vibe Coding | Prompt-native UX: LLM orchestration, progressive disclosure, human-in-loop ergonomics |

---

## Screen Catalogue

### S1 — OPS / Mission Control
**URL**: `/#/ops`  
**Purpose**: High-level operational snapshot — agent health, queue, active projects  
**Key elements**: 3D hexagon project map (center), Mission Control sidebar (left), Live Events (right), Squad row (bottom), Chat/Terminal strip (bottom-most)

### S2 — DISPATCH / Squad Dispatch Operator Console
**URL**: `/#/dispatch`  
**Purpose**: Send a task to one or more domain agents in parallel  
**Key elements**: 4-zone layout (Task Spec → Agent Selection → Execution Stage → Mailbox), right-side dispatch status panel, onboarding tour (Shepherd.js), RAILS/DAG toggles

### S3 — BUILDS / Build Queue
**URL**: `/#/builds`  
**Purpose**: Portfolio view of all active/queued/completed builds  
**Key elements**: Board/List/Export/New Build controls, summary status bar, top-6 board cards with progress, portfolio grid below

### S4 — BUILDS / Detail — Kanban
**URL**: `/#/builds/:codename/kanban`  
**Purpose**: Per-build drill-down with phase columns  
**Key elements**: View mode tabs (kanban/list/operator/manifest/plan), kanban phase board, right-side Working History log

### S5 — HELIX / Knowledge Graph
**URL**: `/#/helix`  
**Purpose**: Query and browse the agent knowledge graph / memory vault  
**Key elements**: DNA helix 3D animation (left ~50%), search panel (right), "LA KNOWLEDGE GRAPH" header

### P1 — Events Panel (right drawer)
**Trigger**: Events button in top-right nav  
**Purpose**: Live event stream from connected agents  
**Key elements**: Right drawer overlay, SSE event list

### P2 — Memory Panel (modal)
**Trigger**: Memory button in top-right nav  
**Purpose**: Browse and search helix memory entries  
**Key elements**: Modal overlay, memory entry list with timestamps and content, search

### P3 — Dispatch Onboarding Tour (modal)
**Trigger**: First visit to DISPATCH  
**Purpose**: 45-second walkthrough of Dispatch controls  
**Key elements**: Shepherd.js modal with Skip/Start, covers write-path safety gate explanation

---

## Per-Screen Evaluation

### S1 — OPS / Mission Control

#### What works
- Top-bar counters (27 PROJECTS · 2 RUNNING · 11 QUEUED · 0 ALERTS) are the highest signal-density element on the page — actionable at a glance
- Tab structure (OPS / DISPATCH / BUILDS / HELIX) is logical and maps to operator mental model
- Conductor queue depth with progress bar is directionally useful
- "Show 3D View" toggle implies the map is optional — good escape hatch

#### Issues by lens

**[E] Engineer**
- OFFLINE / reconnecting = the dashboard's cardinal sin. Every number is stale. An operator cannot trust anything they see.
- Squad health shows `NEVER` for all 7 siblings — no time since last heartbeat, no way to distinguish "never configured" from "just crashed"
- Memory shows 0/0/0 (Steps/Strands/Helixes) — unclear if disconnected or genuinely empty
- The 3D hexagon map consumes ~60% of viewport but encodes only: color (sibling owner) and "ACTIVE" badge. Spatial position carries zero meaning.
- Codenames (SWIFT-WEAVING-08, TEMPETED-BINDING) are opaque without hover state or legend
- Bottom squad row is unreadable at normal viewport scale — text is 8–9px

**[UX] UI/UX**
- Visual hierarchy is inverted: the 3D map is the hero but has the lowest information density; the Mission Control sidebar has the highest density but is the smallest element
- Empty states are not differentiated: "0 alerts" could mean "system healthy" or "alert pipeline offline" — there's no visual distinction
- The `2 ACTIVE` badge in the top-right contradicts `OFFLINE` status — creates cognitive dissonance
- Squad pill row uses color + label but no status icon hierarchy (green/amber/red would communicate health faster than reading "NEVER")
- No loading skeleton — the transition from loading to "offline" is abrupt

**[NS] Northstar**
- Fails Pillar 1: an operator cannot initiate or monitor any production work from this screen when OFFLINE. The dashboard is decorative, not operational.
- "Show 3D View" toggle hints at a better list/table alternative that isn't built yet — the map should be the secondary view, not the default

**[LS] LASDLC**
- LASDLC gates (ARCH/SEC/QUAL/PERF/TEST/DOC/OPS) are not surfaced anywhere on this screen
- Conductor queue shows count but not which phase/gate each queued item is blocked on
- No compliance surface: no way to see if a build has passed or failed a gate from Mission Control

**[AS] Agentic SDLC** — *Four-layer visibility assessment*
- **Reasoning & Planning**: ❌ No live thought stream, no dependency graph of agent collaboration
- **Execution & Tool Telemetry**: ❌ No tool-call logs, no resource drift alerts
- **Verification & Quality Gates**: ❌ No evaluation scorecards, no gate pass/fail indicators
- **Economic & Operational Health**: ❌ No token/cost attribution, no loop iteration count

**[IB] Industry Baseline**
- Grafana/Datadog: status pages lead with RED/AMBER/GREEN aggregate health, then drill down. LA OPS leads with a 3D animation and buries health in a sidebar.
- Linear/Vercel: project cards encode status, priority, and owner at a glance. LA hexagons encode only owner.
- Temporal: workflow graphs show actual DAG topology. LA hexagons are decorative spheres with no topological meaning.

**[VC] Vibe Coding**
- The LLM-native interaction (chat strip at bottom) is present but disconnected from the operational view — there's no way to "ask about" a hexagon by clicking it and opening a chat context
- Prompt-native pattern missing: clicking a stalled build should inject context into the chat ("vault-migration-v1 has been queued for 3h — investigate?")

---

### S2 — DISPATCH / Squad Dispatch Operator Console

#### What works
- 4-zone sequential layout (01 Task Spec → 02 Dispatch → 03 Execution → 04 Mailbox) maps directly to the mental model of "compose → send → watch → receive"
- Onboarding tour (Shepherd.js) on first visit — good progressive disclosure
- Character counter (0/8,192) sets clear expectations on input size
- Dry run checkbox is a key safety gate — excellent operator ergonomic
- +Files, +Folder context injection — direct path to RAG context, well-placed
- Agent abbreviations (ENG/QLT/SEC/OPS/RES/KNW/TST/SQD) map 1:1 to LASDLC domain gates — coherent

#### Issues by lens

**[E] Engineer**
- "Select at least one agent to dispatch" error shown in red *before* any attempt — premature validation. Should only appear on attempted dispatch with no selection.
- RAILS and DAG toggles have no tooltip — an engineer first seeing this screen doesn't know what they toggle
- Right panel DISPATCH button is redundant with the main Dispatch button in zone 01 — two affordances for one action creates confusion about which is canonical
- Mailbox "OFFLINE" means the result channel is dead — dispatching produces a fire-and-forget with no feedback loop

**[UX] UI/UX**
- The `#SQD-DISPATCH` identifier in the top-left is useful for operators who navigate programmatically, but looks like a debug artifact to a first-time user
- HISTORY bar ("no past dispatches") is correct empty state but positioned ambiguously — it looks like a label for a section that doesn't exist yet
- The agent grid (3×3 card layout) has no visual state for selected vs. deselected — a subtle border or fill change on hover is the only affordance
- "00 AGENTS QUEUED" in the right panel uses a large monospace counter that will always read "00" until a dispatch fires — wastes prime real estate in the idle state
- No estimated cost or agent count preview before dispatch

**[NS] Northstar**
- This is the closest screen to Northstar Pillar 1 — it directly lets an operator compose a multi-agent task
- But the missing feedback loop (Mailbox OFFLINE, no live reasoning trace during execution) means the operator dispatches and then has to navigate elsewhere to see results
- Northstar requires the operator to complete work from the webshell: Dispatch → Execution → Review → Merge. Currently Dispatch → ??? (offline)

**[LS] LASDLC**
- Agent abbreviations (ENG/QLT/SEC/OPS/RES/KNW/TST/SQD) correctly correspond to LASDLC domains — coherent mapping
- GATE labels on each card (GATE: 0, GATE: N, etc.) suggest gate sequencing but the values are all 0 or N — unclear if these are priority, order, or something else
- No indication of which LASDLC phase/tier the dispatch will target

**[AS] Agentic SDLC**
- **Reasoning & Planning**: ❌ No pre-dispatch dependency graph (which agents will collaborate, in what order)
- **Execution & Tool Telemetry**: ❌ Execution Stage (zone 03) is "AWAITING DISPATCH" — no live tool trace during execution
- **Verification & Quality Gates**: ❌ No post-dispatch scorecard or gate result visible in Mailbox
- **Economic & Operational Health**: ❌ No token estimate before dispatch, no cost attribution after
- The "Loop Iteration Count" concept from agentic SDLC research is completely absent — if an agent self-corrects 15 times, the operator has no visibility

**[IB] Industry Baseline**
- GitHub Actions / CircleCI: job composition UIs show estimated runtime and resource cost before you trigger. LA Dispatch has no pre-flight estimate.
- Temporal: workflow input editors show the input schema and validate against it before submit. LA Dispatch is a freeform textarea.

**[VC] Vibe Coding**
- The freeform textarea is the most vibe-native element on the screen — natural language task composition is the core vibe coding affordance
- Missing: task templates / slash-command completion inside the textarea (e.g., `/build platform-api` autocompletes to a structured task)
- Missing: task history as reusable templates — an operator who dispatched a successful task should be able to replay or modify it

---

### S3 — BUILDS / Build Queue

#### What works
- Summary bar (2 in progress · 24 queued · 1 completed · 0 failed) is excellent — highest-value information strip on the screen
- Board/List/Export view toggle is standard and expected
- "+ New Build" is prominently placed and clearly labeled
- Build cards show: codename, path, ARCH gate count, build count, active/planned status — reasonable density
- Codenames are meaningful and consistent with helix naming convention

#### Issues by lens

**[E] Engineer**
- Progress bars on top-6 board cards all show 0% — this is either "no data" or "genuinely 0% progress." There's no visual distinction. Empty bars look broken.
- ARCH 0/7 shown on cards but no other gate scores (SEC, QUAL, PERF, TEST, DOC, OPS) — incomplete gate visibility
- No quick-filter to see only in-progress builds — must scan 27 cards to find the 2 active ones
- "1 completed, 0 failed" in the summary bar is promising, but the Board view doesn't sort by recency — completed builds get buried in the grid

**[UX] UI/UX**
- Two-tier layout (top-6 cards + portfolio grid below) creates spatial duplication — the same builds appear twice, once as a small board card and once as a portfolio card
- Board card progress bars are the widest element in the card but carry no information — they dominate the visual weight without earning it
- Status tags (in_progress, queued, planned, active) use inconsistent color coding — some are amber, some are teal, without a clear legend
- No "last activity" timestamp on cards — impossible to tell if "queued" means queued 5 minutes ago or 5 days ago

**[NS] Northstar**
- This is the primary navigation surface for finding and entering a build — it's the gateway to Northstar Pillar 1 work
- The dual-tier layout adds friction: an operator has to look in two places to find their build
- "Export" is available but there's no "Share" or "Link to build" — collaborative operator patterns are absent

**[LS] LASDLC**
- ARCH gate count (0/7) is the only LASDLC gate shown — the other 6 gates (SEC/QUAL/PERF/TEST/DOC/OPS) are invisible from this surface
- No indication of which LASDLC tier (SMALL/MEDIUM/LARGE) each build is running
- No compliance status rollup — a CISO-level view of "how many builds have passed SEC gate" is not possible here

**[AS] Agentic SDLC**
- **Economic & Operational Health**: ❌ No token spend or cost per build — the primary governance metric for agent loops is absent
- **Verification & Quality Gates**: ❌ Gate scores not surfaced at the portfolio level
- **Execution Telemetry**: Partial — ARCH 0/7 hints at gate progress but doesn't show real-time execution state

**[IB] Industry Baseline**
- Linear: issue list has priority sort, status filter, assignee filter accessible in one click. LA Builds has no filter affordance at all.
- Vercel: deployment list shows duration, trigger, branch, and real-time streaming logs on hover. LA builds show a static card with no temporal data.
- GitHub Actions: failed runs surface the failing step immediately in the list view. LA shows 0 failed with no drill-down affordance on the summary bar numbers.

**[VC] Vibe Coding**
- "+ New Build" could be the vibe coding entry point — clicking it should open a natural language build spec composer (like Dispatch but build-scoped)
- Currently "+ New Build" behavior is unknown (not tested) — if it opens a YAML form it's anti-vibe

---

### S4 — BUILDS / Detail — Kanban

#### What works
- 5 view modes (kanban/list/operator/manifest/plan) at URL level is architecturally sound — operators can deep-link to their preferred view
- Working History right panel preserves agent decision trail — closest thing to "reasoning trace" in the current system
- Kanban phase columns map to LASDLC phase sequence — coherent

#### Issues by lens

**[E] Engineer**
- Working History entries are raw text log lines — no structured parsing, no filtering by agent, no severity tagging
- Kanban column content is not visible from this distance — card density and label readability need testing at real viewport size
- 8 console errors on load — indicates broken API calls, likely the OFFLINE connectivity issue propagating into the detail view

**[UX] UI/UX**
- The 5-view tab strip (kanban/list/operator/manifest/plan) has no visual guidance on which to use when — operators have to discover each view through trial
- Working History and the kanban board compete for horizontal space — at 1440px, one of them will be too narrow to be useful
- No "jump to active phase" affordance — an operator has to scroll the kanban to find where the build currently is

**[NS] Northstar**
- This is where Northstar work actually happens — plan → kanban → operator console → deploy
- The operator view and manifest view are critical for Northstar Pillar 1 but not yet tested

**[LS] LASDLC**
- Kanban columns should directly correspond to LASDLC phases with explicit gate indicators at each column boundary
- If they do, this is the best LASDLC visualization surface in the app — but it needs verification

**[AS] Agentic SDLC**
- Working History is the closest to "Reasoning & Planning Trace" but lacks:
  - Chain-of-thought segmentation (which reasoning step produced which artifact)
  - Tool-call attribution (which history entry came from a file read vs. a bash command)
  - Loop iteration markers (how many self-correction cycles happened in this phase)
- **Economic & Operational Health**: No per-phase cost breakdown

**[VC] Vibe Coding**
- The operator view (untested) is the key vibe coding surface — if it exposes a live chat-with-the-build interface it would be transformative
- Working History could be made prompt-native: clicking an entry should let the operator ask "why did you make this decision?"

---

### S5 — HELIX / Knowledge Graph

#### What works
- DNA helix 3D animation is visually stunning and on-brand — the knowledge-as-DNA metaphor is strong
- Search panel concept is correct — knowledge graph needs a query interface

#### Issues by lens

**[E] Engineer**
- The DNA animation consumes ~50% of the viewport permanently — no toggle to collapse it. For an engineer querying knowledge, the animation is a distraction, not a tool.
- "0 results" empty state is ambiguous: is the knowledge graph empty, or is the search disconnected?
- No example queries, no faceted search (filter by type: memory/strand/helix), no recent queries

**[UX] UI/UX**
- 50/50 split (animation vs. search panel) is the wrong ratio for a query tool. Query interfaces are text-heavy; the animation should be ambient, not dominant (20% max, or togglable)
- The search input is small and visually disconnected from the helix — there's no visual affordance suggesting the helix *responds to* the search
- No results = no empty state illustration, just text — misses an opportunity to explain what the helix contains

**[NS] Northstar**
- An operator who wants to understand past decisions, prior art, or agent memory needs HELIX as a research tool — but 0 results + 50% animation makes it non-functional currently

**[AS] Agentic SDLC — Reflective Learning**
- Agents using HELIX for reflective learning need structured access to historical session data — the current search-only interface has no programmatic API surface visible to the operator
- No "Convergences" or "relate" visualization — the graph nature of HELIX is hidden behind a flat search

**[IB] Industry Baseline**
- Notion AI / Mem.ai: knowledge search shows results inline with context, suggested follow-ups, and source attribution. LA HELIX shows "0 results."
- Obsidian graph view: nodes are clickable, edges are labeled, the graph IS the navigation. LA's helix is decorative, not navigable.

**[VC] Vibe Coding**
- HELIX should be the operator's long-term memory — an operator asking "what did we decide about auth architecture last month?" should get an answer here
- This is the highest-potential vibe coding surface and the most underbuilt

---

### P1 — Events Panel

**[E]** Empty (`no events yet`) because OFFLINE — provides no value in current state  
**[UX]** Right drawer pattern is correct; "no events yet" empty state is clear  
**[AS]** This SHOULD be the primary "Execution & Tool Telemetry" surface — SSE event stream is architecturally correct, just disconnected

---

### P2 — Memory Panel

**[E]** Has actual data (memory entries with timestamps visible) — this is one of the few panels that shows live content  
**[UX]** Modal overlay pattern works; content is dense and text-heavy but readable  
**[AS]** Memory entries are closest to "Immutable Provenance" from the agentic SDLC model — content is there, structure could be improved

---

### P3 — Dispatch Onboarding Tour

**[UX]** Shepherd.js implementation is correct; Skip/Start/ESC dismiss options are all present  
**[VC]** 45-second walkthrough covering the write-path safety gate (Dry run) is exactly the right content for a vibe coding operator — keep this

---

## Cross-Cutting Findings

### CF-1: Connectivity is the root cause of most failures
Every screen has degraded functionality due to `OFFLINE / reconnecting`. The SSE connection to the agent backend is broken, making Events, Squad Health, Memory stats, and Mailbox all non-functional. **This is P0.** Nothing else can be properly evaluated until the connection is stable.

### CF-2: Visual weight vs. information density inversion
The three highest-visual-weight elements (3D hexagon map, DNA helix animation, progress bars) carry the lowest information density. The three highest-density elements (top-bar counters, summary status bar, Mission Control sidebar) are visually subordinate. This is a fundamental hierarchy inversion.

### CF-3: The agentic SDLC 4-layer visibility model is almost entirely absent

| Layer | Coverage | Present? |
|-------|----------|---------|
| Reasoning & Planning Trace | Live thought stream, dependency graph | ❌ |
| Execution & Tool Telemetry | Tool-call fidelity, resource drift | ❌ |
| Verification & Quality Gates | Eval scorecards, drift/comparison | Partial (ARCH 0/7 only) |
| Economic & Operational Health | Token/cost, loop iteration count | ❌ |

The Working History panel in S4 (Build Detail) is the only existing surface that approximates reasoning trace — but it's unstructured text.

### CF-4: LASDLC gate surface is fragmented
Gates are referenced (ARCH 0/7 in Builds, GATE labels in Dispatch) but not coherently exposed. An operator cannot answer "which builds have passed SEC gate?" from any screen in the current UI.

### CF-5: Navigation model is inconsistent
- Home (`/`) defaults to BUILDS — but the tab highlighted is BUILDS
- OPS feels like the intended home page (it's called "Mission Control")
- Breadcrumb shows "LIGHT ARCHITECTS / BUILDS / PIPELINE / LIVE" on some screens but the sub-nav shows "OPS / DISPATCH / BUILDS / HELIX"
- Two competing navigation signals create orientation confusion

### CF-6: Empty states do not differentiate "disconnected" from "empty"
`no events yet`, `0 results`, `0/0/0`, `NEVER` — none of these distinguish "the system is healthy and there are no items" from "the system cannot reach its data source." Operators need to know which failure mode they're in.

### CF-7: Agent self-regulation surface is missing
Per the agentic SDLC research, agents should be able to query dashboard telemetry to perform self-governance (self-correction loop, reflective learning, governance enforcement, immutable provenance). There is no API surface or structured telemetry format exposed to agents in the current UI. AYIN is deployed and connected (SSE at :3742) but its data is not reaching the UI.

---

## Priority Matrix

### P0 — Blockers (breaks the primary value proposition)

| ID | Finding | Lens | Screen |
|----|---------|------|--------|
| P0-1 | SSE connection OFFLINE — all live data broken | E, NS | All |
| P0-2 | Squad health all "NEVER" — agent status unusable | E | S1 |
| P0-3 | Mailbox OFFLINE — Dispatch result channel broken | E, NS | S2 |

### P1 — High (degrades operator effectiveness significantly)

| ID | Finding | Lens | Screen |
|----|---------|------|--------|
| P1-1 | 3D hexagon map spatial position encodes no meaning | E, UX, IB | S1 |
| P1-2 | No reasoning trace / chain-of-thought surface anywhere | AS, VC | All |
| P1-3 | No token/cost attribution per build or dispatch | AS, LS | S2, S3 |
| P1-4 | Loop iteration count absent — blind to runaway agent loops | AS, E | All |
| P1-5 | Visual hierarchy inverted — low-density elements dominate | UX, IB | S1, S5 |
| P1-6 | No LASDLC gate scorecard rollup | LS | S1, S3 |
| P1-7 | HELIX search returns 0 results — knowledge graph unreachable | E, NS | S5 |
| P1-8 | Progress bars all 0% — broken vs. empty undistinguishable | UX, E | S3 |
| P1-9 | Navigation home (`/`) and conceptual home (OPS) are different tabs | UX | All |

### P2 — Medium (operator friction, polish)

| ID | Finding | Lens | Screen |
|----|---------|------|--------|
| P2-1 | Premature error state on Dispatch agent grid | UX | S2 |
| P2-2 | RAILS/DAG toggles have no tooltip | UX, E | S2 |
| P2-3 | DNA helix animation: no collapse/minimize toggle | UX | S5 |
| P2-4 | Build cards: no "last activity" timestamp | E, IB | S3 |
| P2-5 | Dual-tier Build layout duplicates the same builds twice | UX | S3 |
| P2-6 | Bottom squad row unreadable at normal viewport | UX | S1 |
| P2-7 | Working History is unstructured text — no agent attribution | AS | S4 |
| P2-8 | `#SQD-DISPATCH` identifier looks like a debug artifact | UX | S2 |
| P2-9 | Status tags (in_progress/queued/planned) inconsistent color | UX | S3 |
| P2-10 | Events/Memory panels: "disconnected" and "empty" look identical | UX | P1, P2 |

---

## Recommendations

### R1: Fix the connection first (P0-1 through P0-3)
No UI improvement matters until the SSE pipeline is stable. Instrument a `connection-health` indicator that distinguishes: `connecting` / `connected` / `degraded` / `offline` — with an explicit retry button. Surface this state prominently in the top bar (currently it's a small amber dot + "reconnecting…" that blends into the nav).

### R2: Invert the visual hierarchy on OPS (P1-1, P1-5)
Make the Mission Control sidebar the visual hero (move it center, expand it). Demote the 3D hexagon map to a secondary panel or toggle (like "Show 3D View" already hints at). The map can remain for visual identity — it should not dominate the default view.

### R3: Add the Agentic SDLC 4-layer telemetry surface (P1-2, P1-3, P1-4)
This is the single highest-leverage investment. Specifically:
- **Reasoning trace**: In Build Detail, parse Working History into structured entries tagged by agent, phase, and tool. Add a "Thought Stream" panel that shows the current agent's reasoning step in real time.
- **Tool telemetry**: Surface tool-call fidelity (success/fail, latency) per phase as a mini-timeline in Build Detail.
- **Economics**: Add token spend + cost per build to Build cards. Add estimated cost before Dispatch fires.
- **Loop count**: Add a "self-correction cycles" counter to in-progress builds and dispatches. Alert when > threshold.

### R4: Build a proper LASDLC gate dashboard (P1-6)
On the OPS screen, replace or augment the hexagon map with a gate matrix: builds × gates (ARCH/SEC/QUAL/PERF/TEST/DOC/OPS), color-coded pass/fail/pending. This gives an operator a compliance snapshot in seconds.

### R5: Fix HELIX to be a functional knowledge tool (P1-7)
- Connect the search to the SOUL backend (likely an API routing issue)
- Reduce animation to 20% of viewport or make it a toggleable background
- Add faceted filters (type: memory/strand/decision/build), recent queries, and example searches
- Make the helix graph *navigable* — clicking a node should show its connections

### R6: Vibe coding affordances (VC across all screens)
- Clicking a hexagon/build card should inject context into the chat strip: "You're looking at vault-migration-v1 (queued 3h). Ask me anything."
- Dispatch textarea should support slash-command autocomplete (`/build`, `/secure`, `/research`)
- Past dispatches should be replayable as templates
- "+ New Build" should open a natural language spec composer, not a YAML form

### R7: Navigation coherence (P1-9, CF-5)
- Set OPS as the default route (`/` → `/#/ops`)
- Reconcile the breadcrumb (BUILDS/PIPELINE) with the sub-nav (OPS/DISPATCH/BUILDS/HELIX)
- Add keyboard shortcut labels to each tab (e.g., `O`, `D`, `B`, `H`) — the bottom bar hints at keyboard shortcuts but the main nav doesn't

---

## Appendix: Agentic SDLC Research Alignment

The following table maps the agentic SDLC research framework [sources 1–19, provided 2026-05-12] to current webshell coverage:

| Research Concept | LA Webshell Coverage | Gap |
|-----------------|----------------------|-----|
| Live Thought Stream | ❌ None | Add structured Working History + Thought Stream panel in S4 |
| Dependency Graph | ❌ None | Add agent collaboration DAG to Dispatch (zone 03) |
| Tool-Call Fidelity logs | ❌ None (AYIN connected but data not surfaced) | Pipe AYIN spans to Build Detail timeline |
| Resource Drift Detection | ❌ None | Add drift alerts to OPS sidebar |
| Evaluation Scorecards | Partial (ARCH 0/7 only) | Expose all 7 LASDLC gate scores per build |
| Spec vs. Implementation Diff | ❌ None | Add manifest/plan diff view to Build Detail |
| Token & Cost Attribution | ❌ None | Add economics row to Build cards + Dispatch |
| Loop Iteration Count | ❌ None | Add self-correction cycle counter to in-flight builds |
| Self-Correction Loop (agent-readable) | ❌ No API surface visible | Expose telemetry API endpoint for agent consumption |
| Reflective Learning (session history) | Partial (Memory panel has entries) | Structure memory entries for programmatic agent access |
| Governance Enforcement (critic agents) | ❌ Not visible | Surface critic/supervisor agent results in Dispatch mailbox |
| Immutable Provenance | Partial (Working History exists) | Structure Working History as signed, append-only audit log |
| Self-Healing (Observability Agent → Fixer Agent) | ❌ Not present | Long-term: wire AYIN anomaly detection to auto-commission fix dispatches |

The most urgent gap: **agents currently cannot query webshell telemetry to govern themselves**. AYIN is deployed at :3742 and connected, but its spans and sessions are not exposed as a structured API surface that agents can consume. This is the foundational infrastructure for the self-correction loop, reflective learning, and governance enforcement patterns.

---

*Next step: review this audit, prioritize a P0+P1 sprint, and create a LASDLC build plan before writing any code.*

---

## Design System Principles

> Full implementation spec: `DESIGN-LANGUAGE.md` (root of this repo). This section maps principles to audit findings.

### DS-1: Eye-flow is assigned per screen type, not globally

Three patterns are in use — each screen is assigned exactly one:

| Pattern | Screens | Rationale |
|---------|---------|-----------|
| **Gutenberg diagram** (primary optical area top-left → terminal bottom-right, fallow centre) | OPS (S1) | Bird's-eye exploration, not task execution |
| **F-pattern** (two horizontal sweeps → vertical left rail) | BUILDS (S3), HELIX (S5) | Data-heavy scanning |
| **Z-pattern** (top-left → top-right → diagonal → bottom CTA) | DISPATCH (S2), Build Detail (S4) | Sequential task execution with a terminal action |

Violating this assignment on any screen is a UX regression. See `DESIGN-LANGUAGE.md §3`.

### DS-2: Forward feels like progress; back preserves it

All forward navigation uses the **Progressive Commitment** pattern (`<ProgressiveCommit>` component):
- Show next state as a preview before committing
- Back returns to prior state — completed progress is unchanged
- Every flow is a phased journey with an explicit "where you are" indicator

This is the UX contract that makes the "visual journey" feel coherent. Directional motion (slide-right on advance, slide-left on retreat) reinforces spatial memory. See `DESIGN-LANGUAGE.md §5`.

### DS-3: State machines are the source of truth, not the UI

Every Project, Program, and Build Plan has a corresponding state machine:
- **Authoritative**: Rust side, persisted in SOUL `helix.db` as an append-only event log
- **Frontend**: optimistic mirror — transitions immediately on user action, rolls back with a visible "glitch" animation on backend rejection
- **Agent-readable**: state machine events are the telemetry surface for agent self-governance (CF-7)

KV pairs from state machine transitions map 1:1 to UI component state. No UI component reads from ad-hoc local state when a state machine entry exists. See `DESIGN-LANGUAGE.md §8`.

### DS-4: The Stark aesthetic is a visual language, not decoration

The "digital blueprint / Tony Stark holographic schematic" aesthetic has a formal vocabulary defined in `DESIGN-LANGUAGE.md §6`. Key rules:
- Three.js owns 3D space; p5.js owns 2D dynamic drawing; CSS owns UI chrome
- Glow intensity = z-depth = priority (brighter = closer = more important)
- Elements construct from a source point on enter, deconstruct/dissolve on leave
- `prefers-reduced-motion` is mandatory — all Three.js and p5.js animations pause

### DS-5: Color carries three independent signals

Color is never used for only one purpose:

| Channel | Colors | Meaning |
|---------|--------|---------|
| Structural | Cyan, steel-blue | The blueprint drawing — schema, connections, boundaries |
| Semantic | Red, amber, green (with glow) | State: failure, warning, healthy |
| Identity | Per-sibling palette (existing) | Agent ownership |

Sibling identity colors must not collide with semantic colors. SERAPH (red identity) builds require a secondary non-color indicator to distinguish from error state. Full token table: `DESIGN-LANGUAGE.md §4`.
