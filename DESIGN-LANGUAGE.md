# Design Language — Light Architects Webshell
**Version**: 2.0  
**Date**: 2026-05-12  
**Status**: Pre-implementation specification  
**Companion**: `WEBSHELL-UX-AUDIT.md` (findings that motivated these decisions)  
**Sections**: §1 Tech split · §2 Grid · §3 Eye-flow · §4 Color · §5 Motion · §6 Stark aesthetic · §7 Eye-flow impl · §8 State machines · §9 Components · §10 Accessibility · §11 Missing pieces · §12 Industry baselines · §13 Progressive sophistication · §14 Speed & efficiency · §15 Backend API contracts · §16 Gateway MCP→UI map · §17 Open decisions

This document is the implementation contract for the webshell visual language. Every engineer building UI touches this file. No aesthetic decision is made ad-hoc — it either references a rule here or requires a PR to extend this spec first.

---

## §1 — Technology Responsibility Split

Three rendering layers. They never share a compositing layer.

| Layer | Technology | Owns | Never does |
|-------|-----------|------|-----------|
| **3D space** | Three.js | Hexagon map, DNA helix, parallax depth, camera tilt | UI chrome, text, form controls |
| **2D dynamic drawing** | p5.js (SVG mode) | Circuit trace decoration, background grid perspective, construct/deconstruct particle effects | Layout, event handling |
| **UI chrome** | CSS + Svelte | All interactive components, transitions, scan-line shader, glow, targeting reticles, typography | 3D transforms, canvas |

**Frame budget**: 60fps target, 16ms frame budget per frame.  
- Three.js renders on `requestAnimationFrame`, isolated canvas element  
- p5.js runs in SVG mode as an overlay with `pointer-events: none`  
- CSS transitions use `will-change: transform, opacity` only on elements that animate  
- Never put all three layers on the same compositing layer — use `isolation: isolate` on container elements

**GPU budget rule**: if Three.js + p5.js combined exceed 8ms/frame measured on a MacBook M1 base, reduce particle count before reducing quality. Never drop below 30fps.

---

## §2 — Grid System

### Baseline grid: 8px

All spacing values are multiples of 8px. Exceptions require explicit justification in the component.

```
4px   — tight (icon padding, badge gaps)
8px   — base unit
16px  — component internal padding
24px  — section gap
32px  — panel gap
48px  — major section separation
64px  — full-bleed section break
```

### Column grid: 16 columns

16 columns at 1440px viewport with 16px gutters. 16-column grids subdivide evenly into halves, thirds (≈5.3 columns), quarters, and eighths — giving maximum layout flexibility for dashboard panels.

| Span | Use |
|------|-----|
| 2 col | Sidebar labels, icon columns |
| 3 col | Narrow stat panels |
| 4 col | Quarter-width cards |
| 6 col | Build cards (3 per row) |
| 8 col | Half-width panels |
| 12 col | Main content area with sidebar |
| 16 col | Full width (OPS bird's-eye) |

### Background grid as a visual element

The grid lines are part of the Stark aesthetic — not just layout scaffolding. Render as a p5.js SVG layer:
- Base grid: `rgba(0, 200, 255, 0.04)` lines at 32px intervals
- Perspective vanishing point: bottom-centre of viewport, lines converge toward it at 0.3° tilt
- On hover over a build card: grid lines within 200px radius pulse to `rgba(0, 200, 255, 0.12)` over 300ms
- The background grid never scrolls — it is fixed to the viewport (`position: fixed`, `z-index: 0`)

---

## §3 — Eye-flow Pattern Registry

Each screen is assigned exactly one pattern. Changing the pattern assignment requires updating this file.

### Gutenberg Diagram → OPS (Mission Control / Bird's-eye home)

```
┌─────────────────────────────────┐
│ PRIMARY OPTICAL AREA            │  ← enter here: status counters, squad health
│ (top-left)                      │
│                                 │
│         FALLOW AREA             │  ← 3D map lives here (exploration, not first-read)
│         (centre)                │
│                                 │
│                  TERMINAL AREA  │  ← exit here: CTA, drill-down affordances
│                  (bottom-right) │
└─────────────────────────────────┘
```

**Implementation rules**:
- Status counters (PROJECTS / RUNNING / QUEUED / ALERTS) anchor top-left
- The 3D map is centred and visually large but not the first read
- Primary CTAs (open build, new dispatch) anchor bottom-right
- Squad health lives in the left rail, supporting the top-left entry

### F-Pattern → BUILDS (S3), HELIX (S5)

```
━━━━━━━━━━━━━━━━━━━━━━  ← first horizontal sweep (summary bar, filter controls)
━━━━━━━━━━━━           ← second horizontal sweep (first row of cards/results)
┃                       ← vertical scan down the left rail (card names, status tags)
┃
┃
```

**Implementation rules**:
- Summary bar (status counts, filters) is the first horizontal — full width, maximum density
- First row of cards is the second horizontal — most important builds surface here
- Left edge of cards is the vertical — name, status badge, and last-activity must live on the left side of every card

### Z-Pattern → DISPATCH (S2), Build Detail (S4)

```
START ━━━━━━━━━━━━━━ TOP-RIGHT
                  ╲
                   ╲  (diagonal — user processes the agent grid)
                    ╲
BOTTOM-LEFT ━━━━━━━━ CTA (Dispatch / Confirm)
```

**Implementation rules**:
- Task Spec (zone 01) anchors top-left
- Agent count / status anchors top-right (the "how many" read)
- The agent selection grid occupies the diagonal zone (processed last-to-first visually)
- The terminal action (DISPATCH button, Confirm) anchors bottom-right — the natural Z endpoint
- Progress through the zones (01 → 02 → 03 → 04) must feel like moving *down-right* through the Z

---

## §4 — Color Token System

### Three independent signal channels

Color carries three signals simultaneously. They must not collide.

#### Channel 1: Structural (the blueprint drawing)

```css
--la-struct-primary:    #00c8ff;   /* cyan — primary schema lines, active connections */
--la-struct-secondary:  #2a6496;   /* steel-blue — secondary lines, inactive connections */
--la-struct-grid:       rgba(0, 200, 255, 0.04);  /* background grid */
--la-struct-grid-hover: rgba(0, 200, 255, 0.12);  /* grid hover pulse */
```

#### Channel 2: Semantic (state communication)

```css
/* Each semantic color has three glow levels: dim / normal / bright */
/* Glow intensity = severity / z-depth */

--la-semantic-ok:         #22c55e;   /* green — healthy, passed gate */
--la-semantic-ok-glow:    0 0 8px rgba(34, 197, 94, 0.4);
--la-semantic-ok-glow-hi: 0 0 20px rgba(34, 197, 94, 0.7);

--la-semantic-warn:         #f59e0b;  /* amber — warning, degraded */
--la-semantic-warn-glow:    0 0 8px rgba(245, 158, 11, 0.4);
--la-semantic-warn-glow-hi: 0 0 20px rgba(245, 158, 11, 0.7);

--la-semantic-error:         #ef4444; /* red — failure, blocked gate */
--la-semantic-error-glow:    0 0 8px rgba(239, 68, 68, 0.4);
--la-semantic-error-glow-hi: 0 0 20px rgba(239, 68, 68, 0.7);

--la-semantic-offline:  #475569;   /* slate — disconnected, unknown */
--la-semantic-active:   #a78bfa;   /* violet — currently processing */
```

#### Channel 3: Identity (sibling/agent ownership)

```css
--la-id-soul:    #f59e0b;   /* amber */
--la-id-eva:     #ec4899;   /* pink */
--la-id-corso:   #3b82f6;   /* blue */
--la-id-quantum: #8b5cf6;   /* purple */
--la-id-seraph:  #ef4444;   /* red — COLLISION RISK with semantic-error */
--la-id-ayin:    #f97316;   /* orange */
--la-id-laex:    #eab308;   /* yellow */
```

**SERAPH collision rule**: SERAPH identity (red) collides with semantic-error (red). When a SERAPH build card appears in an error state, add a secondary non-color indicator: a diagonal stripe pattern on the card border, plus a ⚠ icon. Never rely on red alone to communicate either meaning.

### Glow depth hierarchy (z-space)

Glow intensity communicates z-depth (how "close" or "important" an element is):

```
z-level 4 (foreground, active, selected): box-shadow with 20px spread, 70% opacity
z-level 3 (in-progress, hover state):     box-shadow with 12px spread, 50% opacity
z-level 2 (present, idle):                box-shadow with 6px spread, 30% opacity
z-level 1 (background, historical):       box-shadow with 2px spread, 15% opacity
z-level 0 (inactive, disconnected):       no glow, --la-semantic-offline color
```

### Surface colors

```css
--la-bg-base:       #0a0a0f;   /* deepest background */
--la-bg-panel:      #0f1117;   /* panel surface */
--la-bg-card:       #141820;   /* card surface */
--la-bg-elevated:   #1a2030;   /* elevated card, hover state */
--la-bg-overlay:    rgba(10, 10, 15, 0.85);  /* modal backdrop */

--la-text-bright:   #f1f5f9;   /* primary text */
--la-text-label:    #94a3b8;   /* label text */
--la-text-dim:      #475569;   /* placeholder, disabled */
--la-text-code:     #00c8ff;   /* monospace values, identifiers */
```

### Accessibility rules

- Minimum contrast: 4.5:1 for body text, 3:1 for large text (WCAG 2.1 AA)
- `--la-text-bright` on `--la-bg-base` = 14.2:1 ✓
- `--la-text-label` on `--la-bg-base` = 6.1:1 ✓
- `--la-text-code` (#00c8ff) on `--la-bg-base` = 5.8:1 ✓
- Glow effects must not be the only indicator of state — always pair with text label or icon
- Semantic colors must have a non-color secondary indicator (icon, pattern, label)

---

## §5 — Motion Grammar

### Easing vocabulary

```css
--ease-project:   cubic-bezier(0.16, 1, 0.3, 1);    /* ease-out-expo: materialising, projection */
--ease-retract:   cubic-bezier(0.7, 0, 0.84, 0);    /* ease-in-cubic: retracting, dismissing */
--ease-snap:      cubic-bezier(0.34, 1.56, 0.64, 1); /* spring: selection, confirmation */
--ease-linear:    linear;                             /* scan lines, rolling numbers */
```

### Transition semantics

Every navigation type has one canonical transition. Use no other.

| Navigation type | Enter | Exit | Duration |
|----------------|-------|------|----------|
| **Forward progress** (Z-pattern zone advance) | slide-in from right + scale 98%→100% | slide-out to left + scale 100%→98% | 250ms `--ease-project` |
| **Drill-down** (bird's-eye → detail) | zoom-in from clicked element's bounding box origin | reverse zoom to origin | 350ms `--ease-project` |
| **Back** (step backward in flow) | slide-in from left + scale 100% (no scale change — preserves spatial memory) | slide-out to right | 200ms `--ease-retract` |
| **Modal / preview** | scale 90%→100% + backdrop blur 0→8px | scale 100%→94% + backdrop blur 8px→0 | 200ms `--ease-project` |
| **Executive summary card** (hover drill-down) | scale 95%→100% from cursor origin | scale 100%→95% toward cursor origin | 150ms `--ease-snap` |
| **Panel open** (Events, Memory drawers) | slide-in from edge (right/top) | slide-out to edge | 300ms `--ease-project` |
| **Tab switch** (same-level navigation) | cross-fade, no slide | cross-fade | 150ms `--ease-linear` |

### Construct / Deconstruct animations (Stark signature)

Elements build from a source point on enter and dissolve on leave. Implemented in p5.js SVG layer.

**Construct** (element entering the DOM):
1. A point appears at the element's centre (2px dot, `--la-struct-primary`)
2. Lines extend outward along the element's bounding box edges over 180ms (`--ease-project`)
3. Fill floods inward from the edges over 120ms
4. Text/content fades in at 80% completion of step 3

**Deconstruct** (element leaving the DOM):
1. Content fades out over 80ms
2. Fill drains outward from centre over 120ms (`--ease-retract`)
3. Lines retract back to the centre point over 100ms
4. Point disappears

Total construct: ~300ms. Total deconstruct: ~300ms. Never block interaction during these animations.

### `prefers-reduced-motion`

```css
@media (prefers-reduced-motion: reduce) {
  /* All CSS transitions collapse to instant */
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    transition-duration: 0.01ms !important;
  }
}
```

In Svelte/Three.js/p5.js: check `window.matchMedia('(prefers-reduced-motion: reduce)').matches` on mount. If true:
- Three.js: stop `requestAnimationFrame` loop, render single static frame
- p5.js: call `noLoop()`, render static state
- Construct/deconstruct: skip animation, show final state immediately

### Rolling number counters

Used for: queue counts, token spend, loop iteration count, cost display.

```css
/* CSS-only implementation using @property for animatable numbers */
@property --num {
  syntax: '<integer>';
  initial-value: 0;
  inherits: false;
}
.counter {
  counter-reset: num var(--num);
  transition: --num 600ms --ease-linear;
  font-variant-numeric: tabular-nums;
}
.counter::after {
  content: counter(num);
}
```

Use `prefers-reduced-motion` check — snap to final value immediately if reduced motion is set.

---

## §6 — Stark Aesthetic Elements

### Parallax depth (Three.js)

On mouse move over any Three.js canvas: tilt the camera ±2° on both axes proportional to cursor position relative to canvas centre. This simulates the hologram being a physical object in space.

```javascript
// Camera tilt — runs in Three.js animation loop
const tiltX = (mouseY / window.innerHeight - 0.5) * 0.035; // radians
const tiltY = (mouseX / window.innerWidth  - 0.5) * 0.035;
camera.rotation.x += (tiltX - camera.rotation.x) * 0.05; // lerp
camera.rotation.y += (tiltY - camera.rotation.y) * 0.05;
```

Disable when `prefers-reduced-motion` is set.

### Circuit trace decoration (p5.js)

Dynamic circuit traces as structural ornament on panel borders and between connected elements.

Rules:
- Traces follow the 8px grid (only horizontal/vertical segments, 90° turns)
- Colour: `--la-struct-secondary` at 60% opacity for static traces
- Animated traces (active connections between agents): `--la-struct-primary` at 80% opacity, with a "pulse" dot travelling the trace at 120px/s
- Traces are drawn by p5.js on mount and redrawn on layout change — never hardcoded in CSS
- Maximum 3 animated traces visible simultaneously (performance budget)

### Targeting reticles / corner brackets (CSS only)

Applied to the currently selected/focused element:

```css
.la-selected::before,
.la-selected::after {
  content: '';
  position: absolute;
  width: 12px;
  height: 12px;
  border-color: var(--la-struct-primary);
  border-style: solid;
  opacity: 0.8;
}
.la-selected::before {
  top: -2px; left: -2px;
  border-width: 1px 0 0 1px;
}
.la-selected::after {
  bottom: -2px; right: -2px;
  border-width: 0 1px 1px 0;
}
```

Four-corner variant (all corners): add `la-selected-quad` class for high-priority selections.

### Scan-line shader (CSS)

Applied to panel backgrounds to give the "holographic glass" feel:

```css
.la-panel {
  background-image: repeating-linear-gradient(
    0deg,
    transparent,
    transparent 2px,
    rgba(0, 0, 0, 0.03) 2px,
    rgba(0, 0, 0, 0.03) 4px
  );
}
```

Subtle — the lines are nearly invisible. If they're visible from arm's length, opacity is too high. Target: only perceptible on close inspection.

### Holographic "glitch" on state rollback

When the frontend state machine receives a backend rejection and rolls back:

1. Apply `filter: hue-rotate(30deg) saturate(150%)` for 80ms
2. Translate element ±4px on X axis twice (`translate(4px)` → `translate(-4px)` → `translate(0)`) over 160ms
3. Briefly flash `--la-semantic-error-glow` on the element border for 200ms
4. Return to original state

This communicates "the hologram rejected that command" without a modal. Implemented as a CSS animation class `la-glitch` added and removed programmatically.

### Agent presence indicators (pulsing nodes)

Each agent working on a build is represented as a pulsing node in the Three.js scene:

| Agent state | Visual |
|-------------|--------|
| `idle` | Static dot, `--la-semantic-offline` colour, z-level 1 glow |
| `queued` | Static dot, `--la-struct-secondary`, z-level 2 glow |
| `reasoning` | Slow pulse (2s cycle), `--la-id-{sibling}` colour, z-level 3 glow |
| `writing` | Fast pulse (0.5s cycle), `--la-struct-primary`, z-level 4 glow |
| `blocked` | Flicker (irregular 80–200ms), `--la-semantic-error`, z-level 3 glow |

Pulse is implemented as a Three.js `PointLight` intensity animation on the agent node. Not a CSS animation — it must exist in 3D space.

---

## §7 — Eye-flow Implementation Details

### Primary Optical Area (POA) rule

Every screen must place its highest-priority actionable information in the POA defined by its pattern. The POA is not negotiable by component authors — it is reserved by layout.

**Testing the POA**: squint at the screen until it blurs. Whatever draws the eye first is the POA. If it is not the highest-priority element, the layout is wrong.

### Directional progress convention

- **Moving forward** (advancing in a flow) = **right and/or down**. Animations slide right. Progress indicators fill left-to-right.
- **Moving backward** = **left and/or up**. Animations slide left. No progress is lost; the indicator retains its fill.
- **Drill-down** = **zoom in** (scale increases, origin is the clicked element). NOT a slide.
- **Return from drill-down** = **zoom out** (scale decreases back to the list). NOT a slide.

These are absolute conventions. Inverting them (e.g., a "Next" button that slides content left) breaks spatial memory and is a regression.

### Executive summary card (hover drill-down)

On the bird's-eye OPS screen, hovering over a hexagon or project element for >400ms opens an executive summary card. It is not a tooltip — it is a mini-dashboard:

```
┌─────────────────────────────┐
│ VAULT-MIGRATION-V1    ACTIVE│  ← name + status badge
│ ─────────────────────────── │
│ Phase 3 / 7  ARCH ✓ SEC ✓  │  ← current phase + gate scores
│ 2 agents active  $0.34 used │  ← presence + economics
│ Last activity: 4 min ago    │  ← temporal data
│ ─────────────────────────── │
│ [Open Build]  [Dispatch]    │  ← primary CTAs
└─────────────────────────────┘
```

Clicking "Open Build" triggers a drill-down transition (zoom from the hexagon's position). Clicking elsewhere dismisses the card with a retract animation. The card renders as a Svelte portal component (`<ExecutiveSummaryCard>`), not inside the Three.js canvas.

---

## §8 — State Machine Architecture

### Placement decision

**Backend is authoritative. Frontend is an optimistic mirror.**

```
SOUL (helix.db, SQLite)
  └── StateMachine<Project | Program | Build>
        ├── current_state: String
        ├── event_log: append-only (HMAC-chained via turnlog)
        └── kv_snapshot: HashMap<String, Value>  ← current KV state

Frontend (Svelte $state rune)
  └── optimistic_mirror: StateMirror
        ├── committed: KvSnapshot    ← last confirmed from backend
        ├── pending: KvSnapshot      ← optimistic delta
        └── conflict: Option<KvSnapshot>  ← backend rejection, triggers glitch
```

### Event sourcing → Immutable Provenance

Every state transition emits an event to SOUL via the turnlog (HMAC-chained, Tier-1 ephemeral log, promoted to helix.db on confirmation). The event log is append-only. The current state is always derivable by replaying the event log from genesis.

This gives "Immutable Provenance" (agentic SDLC) for free: every UI action that mutates state is recorded as a signed, chained event with: `{actor, action, from_state, to_state, timestamp, kv_delta}`.

### KV → UI component mapping

State machine KV pairs bind directly to UI components. No component reads local state when a KV entry exists.

```typescript
// Example binding — Build Phase KV → Progress component
// Key: "build.phase.current" → value: "3"
// Key: "build.phase.total"   → value: "7"
// Key: "build.gates.arch"    → value: "passed"
// Key: "build.gates.sec"     → value: "pending"
// Key: "build.agent.active"  → value: "corso,quantum"
// Key: "build.cost.usd"      → value: "0.34"
// Key: "build.loops.count"   → value: "2"
```

Each KV key has a canonical owner (the Rust state machine), a canonical type (string/number/enum), and a list of UI components that read it. This mapping is maintained in `lightarchitects-webshell-ui/src/state/kv-map.ts`.

### Entity state machines

#### Project state machine

```
states: [planned → active → paused → archived]
events: [activate, pause, resume, archive]
```

#### Build Plan state machine

```
states: [
  draft → validated → queued → 
  phase_1 → phase_2 → ... → phase_N →
  gate_review → approved → deploying → complete
  | failed | blocked | cancelled
]
events: [validate, enqueue, start_phase, advance, submit_gate, approve, reject, deploy, complete, fail, block, unblock, cancel]
```

#### Agent presence state machine (per agent, per build)

```
states: [absent → assigned → idle → reasoning → writing → reviewing → blocked → done]
events: [assign, activate, start_reasoning, start_writing, start_reviewing, block, unblock, complete, unassign]
```

### Optimistic update flow

```
1. User action → UI transitions optimistically to new state
2. UI sends mutation event to backend (SOUL MCP `write_note` or dedicated state endpoint)
3. Backend validates:
   a. Valid transition → confirms → frontend commits optimistic delta → event appended to log
   b. Invalid / rejected → frontend receives rejection → la-glitch animation → rollback to committed state
4. On network timeout (>3s): show degraded indicator, keep optimistic state, retry with exponential backoff
```

---

## §9 — Component Contracts

### `<ProgressiveCommit>` (the forward-progress pattern)

The reusable component for all forward-progress flows (DISPATCH, Build phase advance, gate submissions).

```svelte
<ProgressiveCommit
  phase={currentPhase}
  totalPhases={totalPhases}
  previewContent={...}       <!-- what the next state will look like -->
  onConfirm={handleConfirm}
  onCancel={handleCancel}
  preservedProgress={[...]}  <!-- phases already completed — never cleared by cancel -->
>
  <!-- current phase content -->
</ProgressiveCommit>
```

**Contract**:
- `previewContent` renders at 90% opacity with a "PREVIEW" badge — it is not interactive
- `onCancel` returns to the previous phase with a slide-left transition; `preservedProgress` is untouched
- `onConfirm` fires the state machine event, triggers optimistic update, then slides forward on confirmation
- If the state machine rejects, the `la-glitch` animation fires on the preview content and `onCancel` is called automatically

### `<AgentPresenceNode>` (Three.js)

A Three.js mesh + light combination representing a single agent's presence on a build.

Props: `sibling`, `state` (from agent presence state machine), `position` (Vector3 in scene space).

The pulse animation is driven by the state machine `state` field — not local component state. When `state` changes, the Three.js animation parameters update via a reactive binding.

### `<ExecutiveSummaryCard>` (portal component)

Renders via Svelte portal (`target: document.body`) to escape any stacking context.

Data is sourced entirely from the state machine KV snapshot for the target entity — no prop-drilling, no local fetch. If the KV snapshot is empty (OFFLINE), the card shows a "DATA UNAVAILABLE" skeleton with `--la-semantic-offline` colour.

---

## §10 — Accessibility Requirements

### `prefers-reduced-motion` (mandatory)

```javascript
// Singleton, checked once on app mount, stored in Svelte context
const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
```

When `true`:
- Three.js: call `renderer.setAnimationLoop(null)`, render final static frame
- p5.js: call `noLoop()`, render static state
- CSS transitions: already handled by the global media query rule (§5)
- Construct/deconstruct: skip, show final state immediately
- Parallax: disable camera tilt, fix camera at default position

### Color-only communication prohibition

No UI element communicates state through color alone. Every color-coded element has a secondary indicator:

| Color signal | Required secondary |
|-------------|-------------------|
| Semantic error (red) | ✕ icon or "FAILED" text label |
| Semantic warning (amber) | ⚠ icon or "DEGRADED" text label |
| Semantic ok (green) | ✓ icon or "PASSED" text label |
| Sibling identity | Text abbreviation (SOUL, EVA, etc.) always present |
| Agent state (pulse colour) | State label in tooltip or visible text |

### Keyboard navigation

The directional flow convention maps to keyboard navigation:

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Move between interactive elements within a zone |
| `→` / `↓` | Advance to next phase (triggers ProgressiveCommit preview) |
| `←` / `↑` | Step back (preserves completed progress) |
| `Enter` | Confirm progressive commitment |
| `Escape` | Cancel / dismiss / step back |
| `O`, `D`, `B`, `H` | Jump to OPS / DISPATCH / BUILDS / HELIX |
| `E` | Toggle Events panel |
| `M` | Toggle Memory panel |
| `Cmd+K` | Command palette (future) |

All keyboard shortcuts must be visible: show them in tooltips (`title` attribute + custom tooltip component) and in a keyboard shortcut help overlay (`?` key).

### WCAG 2.1 AA checklist (per screen)

Before any screen is marked complete:
- [ ] Contrast: all text meets 4.5:1 (body) or 3:1 (large/UI)
- [ ] Focus visible: `:focus-visible` outline on all interactive elements (`outline: 2px solid var(--la-struct-primary); outline-offset: 2px`)
- [ ] No seizure risk: flicker rate < 3Hz on any element (the `la-glitch` animation is 5 flickers / 300ms = 16Hz — must be limited to < 3 per second total on screen)
- [ ] Motion: `prefers-reduced-motion` respected
- [ ] Color: non-color secondary indicator present on all semantic color uses
- [ ] Screen reader: meaningful `aria-label` on all icon-only buttons

**Note on `la-glitch`**: the glitch animation flickers at ~12Hz. Limit to one glitch event per 500ms maximum across all elements on screen simultaneously to stay below the WCAG 2.3.1 photosensitivity threshold.

---

## §11 — Missing Pieces Checklist

Items not yet fully specified — require a follow-up design decision before implementation:

- [ ] **Component library base**: headless library for accessible comboboxes, dialogs, dropdowns (Melt UI vs. Bits UI vs. custom). Decision needed before building any form controls.
- [ ] **Command palette** (`Cmd+K`): full-text search across builds, projects, helix entries, dispatch templates. Architecture TBD.
- [ ] **Temporal view**: timeline/Gantt overlay for the OPS bird's-eye. p5.js or Three.js? Horizontal axis = time, vertical = builds. Design needed.
- [ ] **Dispatch task templates**: slash-command autocomplete inside the Dispatch textarea. Schema for template storage in SOUL.
- [ ] **Spec vs. Implementation diff view**: for Build Detail, a side-by-side of the original plan vs. agent's actual output. Component design needed.
- [ ] **Token/cost API**: backend endpoint that exposes per-build token spend. Not yet implemented in SOUL/AYIN.
- [ ] **Loop iteration counter**: state machine KV key `build.loops.count` — requires the agent loop to emit this on each self-correction cycle. Agent instrumentation needed.

---

---

## §12 — Industry Baseline Reference

> Sources: Laws of UX (lawsofux.com), NNG eyetracking research, agentic-design.ai, IBM Carbon Design System, UXPin, Material Design 3, Vercel Geist, Linear. All principles below are mapped to specific LA webshell decisions.

### 12.1 Laws of UX — Applied to LA Webshell

| Law | Principle | LA Application |
|-----|-----------|---------------|
| **Aesthetic-Usability Effect** | Visually appealing design is perceived as more usable | Stark aesthetic earns trust before users interact; invest in polish |
| **Doherty Threshold** | Productivity breaks below 400ms response time | All API calls must respond or show skeleton in <400ms; SSE keeps dashboard live |
| **Fitts's Law** | Larger/closer targets reduce interaction time | Primary CTAs (Dispatch, New Build, gate approve) must be ≥44px touch target, close to cursor origin |
| **Hick's Law** | More choices = longer decision time | Agent grid in Dispatch: max 8 visible agents. Gate pillars: 7, never more |
| **Jakob's Law** | Users expect familiar patterns | Tab navigation, Cmd+K palette, Esc-to-dismiss must match Linear/VS Code conventions |
| **Miller's Law** | 7±2 items in working memory | Max 7 items per navigation level; chunk build phases into groups ≤7 |
| **Von Restorff Effect** | Differentiated items are remembered best | Use `la-glitch` animation and semantic error color only for true anomalies — not decoration |
| **Goal-Gradient Effect** | Motivation increases as goal proximity increases | Phase progress bars must show % filled, not just count — show how close the build is to done |
| **Peak-End Rule** | Experiences judged by peak moment and final moment | Design the Dispatch confirmation and Build Complete states as the emotional peaks |
| **Zeigarnik Effect** | Incomplete tasks are remembered better than completed ones | In-progress builds should visually "pulse" — they should feel actively alive, not static |
| **Law of Proximity** | Nearby elements are perceived as related | Agent cards in Dispatch must be spatially separated from Execution Stage (different zone) |
| **Law of Similarity** | Similar visual elements are perceived as a group | All 7 sibling pills use same shape/size — identity is color, not form |
| **Law of Common Region** | Elements within a boundary are perceived as a group | Each of the 4 Dispatch zones must have a clear border/background to separate them |
| **Law of Prägnanz** | Users interpret images in their simplest form | Hexagon = project. DNA strand = memory. Do not introduce new metaphors unless unavoidable |
| **Serial Position Effect** | First and last items are remembered best | In BUILDS list: most recent active build first, most recent completed build last |
| **Pareto Principle** | 80% of effects from 20% of causes | 3 entry points (Dispatch, New Build, Cmd+K) cover 80% of operator actions — optimize these first |
| **Cognitive Load** | Minimize mental demands | Never require operators to remember state between screens — surface context in every panel |
| **Progressive Disclosure** | Show critical info first, details on demand | Summary cards before detail views, always. Summary never hides critical status. |
| **Mental Model** | Design matches user expectations | "Forward = progress, back = step back" is the universal mental model — never invert it |
| **Selective Attention** | Users only process goal-relevant stimuli | When a build is in `failed` state, the failure surfaces in EVERY view the operator navigates to — not just the build detail |

### 12.2 NNG Eye-Tracking Patterns — Assignment per Screen

Four documented scanning patterns (NNG 13-year study, 500+ participants):

| Pattern | Description | Quality | Occurs when |
|---------|-------------|---------|-------------|
| **F-pattern** | Two horizontal sweeps then vertical left scan | Worst — lazy scan | No visual structure, no subheadings |
| **Spotted pattern** | Fixates on styled/bold text matching goals | Better | Links, bullets, keyword-styled text present |
| **Layer-cake pattern** | Jumps between bold headings, reads body on match | 2nd best | Strong, distinct section headings |
| **Commitment pattern** | Reads everything | Best comprehension | High motivation, clear relevance |

**Design target**: elicit **Layer-cake** on data screens (BUILDS, HELIX), **Commitment** on build detail and Dispatch. F-pattern is a failure mode.

**Implementation rule**: every section on a data-heavy screen must have a bold, high-contrast label (`font-weight: 600`, `color: var(--la-text-bright)`) that acts as a layer-cake anchor. The label must be readable in 50ms. If the section label is ambiguous or low-contrast, operators will F-scan and miss critical status.

### 12.3 Agentic AI UI Patterns — Applicable to LA Webshell

Source: agentic-design.ai. Patterns rated by applicability to LA:

| Pattern | Code | Priority | LA Implementation |
|---------|------|----------|-------------------|
| **Human-on-the-Loop (HOTL)** | HOTL | P0 | Operator supervises agent builds with real-time override. Dispatch panel = HOTL surface. Requires: exception alerts, intervention button, live status |
| **Agent Status & Activity UI** | ASP | P0 | Sibling presence nodes on OPS hexagon map. States: `idle/queued/reasoning/writing/blocked`. Live pulse animation per state |
| **Monitoring and Control** | MCP | P0 | Mission Control sidebar IS the MCP surface. Must: show exception alerts, allow intervention, display performance metrics |
| **Trust & Transparency** | TTP | P1 | Show reasoning chain in Working History. Cite sources in HELIX search results. Display confidence level on gate evaluations |
| **Progressive Disclosure UI** | PDP | P1 | Summary card → drill-down. Executive summary on hover. Never dump raw data without summary layer |
| **Confidence Visualization** | CVP | P1 | Gate scores (ARCH 0/7) need visual completion gauge. Dispatch classification confidence should show % |
| **Agent Collaboration UX** | ACX | P1 | Multi-agent dispatch: show which agents are running, handoff points, collaboration graph in Execution Stage |
| **Human-in-the-Loop** | HITL | P1 | Dry run checkbox + gate approval are HITL gates. Every destructive action requires explicit operator confirm |
| **Error Handling & Recovery** | ERP | P1 | Every error: plain-language description + specific recovery action + retry button. Never just an error code |
| **Mixed-Initiative Interface** | MIP | P2 | Agent proposes next action ("Shall I run SEC gate?"), operator approves or overrides. Conversational initiative-passing in copilot drawer |
| **Visual Reasoning Patterns** | VRP | P2 | Working History parsed into structured entries: agent + phase + tool + reasoning step. Chain-of-thought visible |
| **Adaptive Interface** | AIP | P3 | Operator-specific layout preferences stored in `/api/browser-state`. Power users get their preferred panel layout on return |

### 12.4 Operational Dashboard Principles

**From IBM Carbon Design System:**
- Prioritize data by importance → highest-priority data gets highest visual contrast AND largest area
- White space increases comprehension by 20% — use it intentionally between sections, not just to fill space
- Consistent colors per data set within a dashboard — a metric that is amber today must not be blue tomorrow
- Every color must have a reason — if you can remove a color and nothing is lost, remove it
- Exploration dashboards (BUILDS, HELIX): support search, sort, filter, drill-down. When operator manipulates one chart/list, related panels must auto-update
- Annotations highlight anomalies — add context labels to metric cards when values are outside normal range

**From UXPin / DataCamp:**
- Primary dashboards: max 5–7 key metrics (our top-bar counters: PROJECTS · RUNNING · QUEUED · ALERTS = 4 ✓)
- Empty states: contextual message + next step CTA. Never just "no data"
- Loading states: skeleton screens matching final layout — never a spinner over a blank panel
- Real-time data: "last updated" timestamp on every live metric. Pulse animation on significant change (not on every tick)
- Error states: plain language + retry + alternative path. Log details internally, show friendly message externally
- Operational dashboards (OPS): large status indicators + clear ownership + sparklines for trends

**Specific rules for LA webshell:**
1. The top-bar counter strip (27 PROJECTS · 2 RUNNING · 11 QUEUED · 0 ALERTS) is already correct density — do not add more counters here
2. Every build card must show "last activity" timestamp — "queued 5 min ago" vs "queued 8 hours ago" is critical operational information
3. The Events panel must distinguish "no events received" from "SSE disconnected" — different icons, different messages, different recovery CTAs

### 12.5 Material Design 3 Motion Reference

Material Design 3 defines a semantic motion system. Our motion grammar (§5) is aligned with but not identical to M3 — LA uses Stark aesthetic easing, not Material easing. This table cross-references both:

| LA Easing Token | CSS cubic-bezier | M3 Equivalent | Use case |
|----------------|-----------------|---------------|----------|
| `--ease-project` | `cubic-bezier(0.16, 1, 0.3, 1)` | Emphasized decelerate | Elements entering from off-screen, materialising |
| `--ease-retract` | `cubic-bezier(0.7, 0, 0.84, 0)` | Emphasized accelerate | Elements leaving, retracting to source |
| `--ease-snap` | `cubic-bezier(0.34, 1.56, 0.64, 1)` | Spring (no M3 equivalent) | Selection confirmation, toggle snap |
| `--ease-linear` | `linear` | Linear | Scan-lines, rolling counters, progress bars |

**Duration rule** (from M3): duration scales with traversal distance. A tab switch (small area) = 150ms. A full-page drill-down (large area) = 350ms. Never use the same duration for both.

**M3 principle**: "Motion should reinforce spatial relationships." Our directional convention (right = forward, left = back, zoom = drill-down) implements this. Every transition must reinforce where the user is in the information hierarchy.

### 12.6 Reference Design Systems

**Vercel Geist** — most aligned with LA aesthetic:
- Swiss design movement: precision, clarity, function over decoration
- Grid: subtle dot/line background as structural decoration (identical to our §2 background grid)
- Typography: Geist Mono for code/identifiers, Geist Sans for prose. LA uses JetBrains Mono equivalently
- Color: predominantly black/white + single accent color. LA uses `--la-struct-primary` (#00c8ff) as our accent
- Spacing: systematic (we use 8px base; Vercel uses similar multiples)
- Key insight: **the background grid IS the design** — Vercel made a grid a signature. We do the same.

**Linear** — reference for information-dense power-user UX:
- 4px base grid (we use 8px — both work, ours is more spacious for a dashboard)
- Every action accessible via Cmd+K command palette — **this is the single most important Linear pattern to adopt**
- Multiple view types (list/board/timeline/split): we have kanban/list/operator/manifest/plan — aligned ✓
- Information density: task elements dominate visually, navigation recedes. Apply to LA: build content > chrome
- Keyboard shortcuts create muscle memory. Our shortcut map (§10) implements this pattern
- "Linear design": calm, minimal chrome, high-density content — content is the UI, not the frame
- Key Linear insight: **the fastest path to any action is the command palette**. Time-to-action < 3 keystrokes for any operation.

---

## §13 — Progressive Sophistication (Novice → Power User)

> "Elegant, modern, and simple for first-time users. Even more powerful for experienced operators. Multi-agent orchestration as fast and efficient as realistically possible." — Design directive, 2026-05-12

This is the **dual-mode principle**: one interface, two interaction depths. The same screen must work for both. Neither mode is a separate "mode" — they coexist in the same layout.

### 13.1 The Dual-Mode Principle

```
NOVICE PATH                    POWER USER PATH
───────────────────────────    ──────────────────────────────
Click through tabs             Keyboard shortcuts (O, D, B, H)
Click agent cards              Cmd+K → type agent name → Enter
Read Shepherd.js tour          Skip tour immediately
Click "Dispatch" button        ⌘↵ in textarea
Hover for executive summary    Tab between cards, Enter to open
Single action at a time        Multi-select + batch dispatch
Read labels to understand UI   Muscle memory, zero reading
~15 seconds to first dispatch  ~3 seconds to first dispatch
```

Both paths achieve the same outcome. Neither degrades the other. The power user path is revealed through use, not through a "toggle to expert mode" switch.

### 13.2 First-Time User Experience

**Shepherd.js onboarding** (already implemented on DISPATCH — extend to all screens):
- OPS: 3-step tour highlighting: top-bar counters → hexagon map → Mission Control sidebar
- BUILDS: 2-step: summary bar → build card drill-down
- HELIX: 1-step: search input + what HELIX contains

**Rules**:
- Tours are dismissible at any step (Esc or "Skip")
- Tours never trigger twice — persisted in `localStorage`
- Tours use the Dispatch screen's existing Shepherd.js implementation — no new library
- Tour copy is written for a non-technical operator, not an engineer
- First dispatch: after operator clicks "Dispatch" for the first time, show a contextual tooltip: "Your task is running. Results appear in Mailbox when complete."

**Progressive context injection**: when an operator navigates to a new screen for the first time, the copilot drawer proactively offers: "This is the [screen name]. Ask me what anything does." This is EVA's role — ambient guidance without blocking the UI.

### 13.3 Power User Path — Keyboard-First Design

**Cmd+K Command Palette** — the single most important power-user feature:

```
┌─────────────────────────────────────────────────────┐
│ ⌘K  > type anything...                              │
├─────────────────────────────────────────────────────┤
│ RECENT                                              │
│   /build vault-migration-v1        (3 min ago)     │
│   /secure platform-api-v1.1        (1 hr ago)      │
├─────────────────────────────────────────────────────┤
│ BUILDS                                              │
│   vault-migration-v1               active           │
│   weaving-grafting-canon           in_progress      │
├─────────────────────────────────────────────────────┤
│ ACTIONS                                             │
│   /build <target>                                   │
│   /dispatch <task>                                  │
│   /secure <target>                                  │
│   /research <topic>                                 │
│   New Build...                     ⌘N              │
└─────────────────────────────────────────────────────┘
```

**Cmd+K requirements**:
- Opens from any screen, any focused element
- Fuzzy search across: build names, meta-skill commands, settings, helix entries
- Keyboard-only navigation (↑↓ to move, Enter to select, Esc to close)
- Recent items surfaced by default (last 5 actions)
- Action preview on selection (shows what will happen before Enter)
- Time-to-action target: **<3 keystrokes from any screen to any common action**

**Full keyboard shortcut map** (supplements §10):

| Shortcut | Action | Screen |
|----------|--------|--------|
| `⌘K` | Open command palette | All |
| `⌘↵` | Dispatch (submit task) | DISPATCH |
| `⌘N` | New Build | BUILDS |
| `⌘D` | Open/focus Dispatch | All |
| `⌘E` | Toggle Events panel | All |
| `⌘M` | Toggle Memory panel | All |
| `⌘\`` | Fork to terminal | All |
| `⌘.` | Toggle dry run | DISPATCH |
| `O` | Go to OPS | All (not in inputs) |
| `D` | Go to DISPATCH | All (not in inputs) |
| `B` | Go to BUILDS | All (not in inputs) |
| `H` | Go to HELIX | All (not in inputs) |
| `?` | Keyboard shortcut overlay | All |
| `G then B` | Go to active build | All |
| `G then D` | Go to last dispatch | All |
| `/` | Focus search (HELIX) / focus task input (DISPATCH) | HELIX, DISPATCH |
| `↑↓` | Navigate list/kanban items | BUILDS, detail |
| `⌘A` | Select all visible builds | BUILDS |
| `X` | Toggle selection on focused build | BUILDS |
| `⌘⌫` | Deselect all | BUILDS |
| `Esc` | Cancel / back / close modal | All |

### 13.4 Batch Operations (Power User Multi-Agent Orchestration)

The highest-efficiency operator workflow is **bulk dispatch**: select N builds, apply one operation to all simultaneously.

**Multi-select pattern** (Linear-inspired):
1. Hover over build card → checkbox appears (same position always, left edge)
2. Click checkbox OR press `X` with keyboard focus → adds to selection
3. Selection toolbar appears at bottom of viewport: "3 builds selected | [Dispatch] [Run Gates] [Export] [Cancel]"
4. "Dispatch" with multiple builds selected opens Dispatch with pre-filled context for all selected builds
5. `⌘A` selects all visible; `Esc` clears selection

**Batch gate evaluation**: select N builds → "Run Gates" → all 7 pillars evaluated in parallel for all selected builds → results stream back individually as each completes.

### 13.5 Dispatch Templates (Reusable Task Macros)

Power operators should not retype the same task specification repeatedly.

```
TEMPLATE: Security audit before deploy
Task: Run a full security audit of {build} before deployment.
      Check OWASP Top 10, dependency CVEs, secrets scanning.
Agents: [SEC] [QLT]
Rails: on | DAG: on
```

**Template storage**: SOUL `write_note` to `user/templates/dispatch/{name}.md`. Loaded via `GET /api/soul/search?q=dispatch+template`.

**Template invocation**: in Dispatch textarea, type `/` → autocomplete shows templates → select → template body fills textarea with `{build}` as a placeholder → cursor positions inside placeholder.

---

## §14 — Speed & Efficiency Principles

> Multi-agent orchestration should be as fast as realistically possible from the UI.

### 14.1 Time-to-Dispatch Targets

| Operation | Novice target | Power user target |
|-----------|--------------|------------------|
| First dispatch from landing | <30 seconds | <5 seconds |
| Repeat dispatch (same task) | <15 seconds | <3 keystrokes |
| New build creation | <60 seconds | <10 seconds (⌘N → name → Enter) |
| Drill into failing build | <10 seconds | <2 keystrokes (G B) |
| Run gate on active build | <15 seconds | <3 keystrokes |

### 14.2 Predictive Loading

- **Hover prefetch**: hovering a build card for >150ms triggers background fetch of `/api/builds/{id}` — so the drill-down loads instantly
- **Tab prefetch**: navigating to BUILDS prefetches the top 6 builds' full detail in background
- **Anticipated next step**: after a dispatch completes, prefetch the execution result before operator navigates to it
- Implementation: `<link rel="prefetch">` for static assets; JS `fetch()` into cache for API data

### 14.3 Optimistic UI for All Write Operations

Every user action that mutates state (dispatch, gate approve, build create) must:
1. Update UI immediately (optimistic)
2. Show a subtle "saving..." indicator (not a blocking modal)
3. Confirm silently on success
4. Rollback with `la-glitch` animation on failure

**No blocking spinners on write operations.** The operator should be able to continue working while a dispatch is in flight.

### 14.4 Streaming Results

Dispatch results stream back via SSE (`/api/dispatch/status/{id}`). The Mailbox and Execution Stage panels must render streaming output progressively:
- Each token/chunk from an agent appends to the output in real time
- Do not buffer and show all at once — streaming = trust that the system is working
- Timestamp each result chunk — operator can see the system is making progress

### 14.5 Zero-Navigation Common Paths

The most frequent operator workflows should require zero navigation (available from current screen):

| Frequent action | Available without navigation via |
|----------------|----------------------------------|
| Dispatch a task | Bottom bar chat input (any screen) |
| Check build status | OPS top-bar counters (any screen) |
| View last agent response | Copilot drawer (any screen, ⌘E) |
| Fork to terminal | Bottom bar `⌘\`` (any screen) |
| Search HELIX | Cmd+K → type query (any screen) |
| Run gate on active build | Cmd+K → "run gates" (any screen) |

---

## §15 — Backend API Contracts

> Source: Explore agent audit of `lightarchitects-webshell/src/server/mod.rs`, `dispatch/routes.rs`, `coordination/mod.rs`. All routes are Axum-based, authenticated via `Authorization: Bearer <token>` or `HttpOnly` session cookie unless noted.

### Authentication & Health

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `GET` | `/api/health` | None | Liveness probe. Returns `200 ok` |
| `GET` | `/api/auth-check` | Bearer | Validate token. Returns `200` or `401` |
| `POST` | `/api/auth/exchange` | Bearer | Swap Bearer → `HttpOnly` session cookie. Body: `{ token }` |
| `POST` | `/api/auth/nonce` | Internal | Issue one-time auth nonce (60s TTL). Returns `{ nonce: uuid }` |
| `POST` | `/api/auth/nonce-exchange` | None | Redeem nonce for session cookie. Body: `{ nonce }` |
| `GET` | `/api/auth/status` | Cookie | Validate & refresh session cookie |
| `DELETE` | `/api/auth/session` | Cookie | Logout — expire session cookie |

### Builds & Portfolio

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/builds` | List portfolio from `active.yaml`. Query: `?status=<status>` |
| `POST` | `/api/builds` | Create new build. Body: `{ cwd, metaSkill, target }` |
| `GET` | `/api/builds/{id}` | Single build detail |
| `GET` | `/api/builds/resume` | Resume persisted sessions from `SessionStore` |
| `POST` | `/api/builds/plan` | Create plan entry in `active.yaml`. Returns `{ codename, build_id }` |
| `PUT` | `/api/builds/plan/{codename}` | Update plan (phase status, gate results) |

### Build Detail & Gates

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/lasdlc` | LASDLC metadata: phases, quality gate definitions |
| `GET` | `/api/builds/{id}/findings` | Quality/security findings for a build |
| `GET` | `/api/builds/{id}/notes` | Build markdown notes. Returns `{ buildId, content, updatedAt }` |
| `PUT` | `/api/builds/{id}/notes` | Update notes. Body: `{ content: markdown }` |
| `GET` | `/api/builds/{id}/artifacts` | List artifacts (logs, reports, binaries) |
| `POST` | `/api/builds/{id}/artifacts` | Upload artifact. Multipart form-data `file` field |
| `GET` | `/api/builds/{id}/gates/{pillar}` | Gate status for pillar: `ARCH\|SEC\|QUAL\|PERF\|TEST\|DOC\|OPS` |
| `POST` | `/api/builds/{id}/pillars/{pillar}` | Trigger pillar gate evaluation |

### Dispatch & Copilot

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/builds/{id}/copilot` | Chat with copilot. Body: `{ message }` |
| `POST` | `/api/builds/{id}/dispatch` | Dispatch sibling agent. Body: `{ sibling, agent, prompt }` |
| `POST` | `/api/dispatch/classify` | Classify task → agent selections + confidence scores (no execution) |
| `POST` | `/api/dispatch/execute` | Execute classified task. Returns `{ dispatch_id }` |
| `GET` | `/api/dispatch/status/{id}` | **SSE stream** of dispatch events (real-time progress) |
| `POST` | `/api/dispatch/cancel/{id}` | Cancel in-flight dispatch |
| `POST` | `/api/dispatch/retry/{id}/{agent}` | Retry failed agent within a dispatch |

### PTY & Terminal

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/terminal/ws` | WebSocket bridge to PTY session (upgrade required) |
| `GET` | `/api/builds/{id}/terminal/ws` | Build-specific PTY WebSocket |
| `POST` | `/api/session/fork` | Fork copilot session to native terminal. Body: `{ build_id }` |

### Status & Monitoring

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/sitrep` | System health snapshot (SITREP) |
| `GET` | `/api/siblings` | Sibling health. Returns `SiblingHealth[]` (status, uptime, lastHeartbeat, capabilities) |
| `GET` | `/api/conductor/status` | Conductor queue depth + active tasks. Returns `{ nodes, edges, queue_depth }` |
| `GET` | `/api/arena/status` | Arena training status |
| `GET` | `/api/meta-skills` | Available meta-skills list |

### SOUL Vault / Knowledge Graph

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/soul/search` | Search helix. Query: `?q=&limit=&mode=bm25\|semantic\|hybrid` |
| `GET` | `/api/soul/entries/{*path}` | Read single helix entry |
| `GET` | `/api/soul/health` | Vault tier health (filesystem / sqlite / neo4j) |
| `GET` | `/api/soul/memory/hot` | Recent hot memos (active-session turnlog). Query: `?limit=` |
| `GET` | `/api/soul/memory/cold` | Recent cold memos (promoted helix). Query: `?sibling=&limit=` |
| `POST` | `/api/soul/compaction/preview` | Preview compaction (dry-run). Body: `RetentionPolicy` |
| `POST` | `/api/soul/compaction/apply` | Apply compaction (destructive). Body: `RetentionPolicy` |
| `GET` | `/api/soul/relationships/{*entry_id}` | Graph relationships (Neo4j neighbors + relation type) |
| `GET` | `/api/soul/edges` | Bulk `:LINKS_TO` edges for 3D lineage. Query: `?limit=` |
| `GET` | `/api/soul/convergences` | Cross-sibling SharedExperience convergences. Query: `?min_participants=&limit=` |
| `POST` | `/api/soul/reindex` | Trigger vault reindex |
| `GET` | `/api/debug/parity` | Parity verification (Phase 20b.3) |

### Real-Time Events (SSE)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/events` | **SSE fan-out** — authenticated. Broadcasts `WebEvent`s (AYIN spans, SOUL promotions, build state changes) |
| `GET` | `/api/builds/{id}/events` | Build-specific SSE stream |
| `GET` | `/api/builds/{id}/agent/stream` | Agent SSE protocol (hybrid SSE + WS) |
| `GET` | `/api/builds/{id}/agent/ws` | Agent WebSocket protocol |
| `POST` | `/api/builds/{id}/notify` | Gateway→webshell notification. Header: `x-la-notify-token` |

### Control & UI Manipulation

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/control` | Control command for Claude GUI. Body: `{ command, ...payload }`. Commands: `FocusPanel`, `NavigateTo`, `OpenTerminal`, `ToggleTheme` |
| `GET` | `/api/browser-state` | Read current UI state snapshot (viewport, panel sizes, zoom, active panel) |
| `POST` | `/api/browser-state` | Update browser state. Body: `BrowserStateSnapshot` |

### Squad Coordination

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/coordination/tasks` | Task queue snapshot + counts |
| `POST` | `/api/coordination/tasks/add` | Append task to queue |
| `POST` | `/api/coordination/tasks/claim/{id}` | Soft-claim a task |
| `GET` | `/api/coordination/tasks/{id}/logs` | Last 200 lines of task log |
| `GET` | `/api/coordination/chat/sessions` | List known chat sessions |
| `POST` | `/api/coordination/chat/inject` | Inject message into a session |
| `GET` | `/api/coordination/chat/stream` | **SSE** chat stream. Query: `?session_id=` |

### Workspace & Utilities

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/workspaces` | List available workspaces |
| `GET` | `/api/workspaces/{id}` | Single workspace detail |
| `GET` | `/api/polytopes` | Per-sibling 4D polytope assignments (compile-time JSON) |
| `GET` | `/api/files` | File listing for @-file autocomplete. Query: `?q=` |
| `GET` | `/api/setup/info` | Setup wizard info (backend, model, status) |
| `GET` | `/api/setup/models` | Available models. Query: `?backend=&base_url=` |
| `POST` | `/api/setup/save` | Save backend config. Body: `{ model, backend, credentials }` |
| `DELETE` | `/api/setup/reset` | Clear persisted configuration |
| `POST` | `/api/csp-report` | CSP violation receiver (SEC-3b) |

**Key source files:**
- Routes: `lightarchitects-webshell/src/server/mod.rs` (lines 358–550)
- Dispatch: `lightarchitects-webshell/src/dispatch/routes.rs`
- Coordination: `lightarchitects-webshell/src/coordination/mod.rs`
- Frontend API client: `lightarchitects-webshell-ui/src/lib/api.ts`

**Note: No OpenAPI spec exists.** API is documented via Rust doc comments inline. Generating an OpenAPI spec via `utoipa` or `aide` is in §17 open decisions.

---

## §16 — Gateway MCP → UI Component Map

> Source: Explore agent audit of `src/lib/commands.ts`, `src/lib/design-tokens.ts`, `src/lib/api.ts`, `src/screens/*.svelte`. 81 routable gateway actions across 7 agents.

### 16.1 Meta-Skill → Agent Routing (Already Wired)

| Meta-Skill | Primary Agent | UI Entry Point | File |
|-----------|--------------|---------------|------|
| `/BUILD` | CORSO | Command palette, bottom bar | `commands.ts` |
| `/RESEARCH` | QUANTUM | Command palette, bottom bar | `commands.ts` |
| `/SECURE` | SERAPH | Command palette, bottom bar | `commands.ts` |
| `/SQUAD` | SOUL | Command palette, bottom bar | `commands.ts` |
| `/PLAN` | QUANTUM | Command palette | `design-tokens.ts` |
| `/DEPLOY` | AYIN | Command palette | `design-tokens.ts` |
| `/REVIEW` | QUANTUM | Command palette | `design-tokens.ts` |
| `/OBSERVE` | AYIN | Command palette | `design-tokens.ts` |
| `/ONBOARD` | SOUL | Command palette | `design-tokens.ts` |
| `/OPTIMIZE` | CORSO | Command palette | `design-tokens.ts` |
| `/REFLECT` | EVA | Command palette | `design-tokens.ts` |
| `/ENRICH` | EVA | Command palette | `design-tokens.ts` |

### 16.2 UI Component → Gateway Action Bindings (Wired)

| Component | Gateway Actions Called | Via |
|-----------|----------------------|-----|
| `SquadDispatch.svelte` | classify → execute (all 7 agents) | `POST /api/dispatch/classify` → `POST /api/dispatch/execute` |
| `Dispatch.svelte` | Direct sibling dispatch | `POST /api/builds/{id}/dispatch` |
| `CommandPalette.svelte` | All 12 meta-skills | `POST /api/builds` with `metaSkill` |
| `QualityGateDash.svelte` | Pillar evaluation (ARCH/SEC/QUAL/PERF/TEST/DOC/OPS) | `POST /api/builds/{id}/pillars/{pillar}` |
| `CopilotDrawer.svelte` | GUI control commands | `POST /api/control` |
| `CompactionPanel.svelte` | SOUL compaction preview + apply | `POST /api/soul/compaction/preview|apply` |
| `HelixScene.svelte` | SOUL search + relationships | `GET /api/soul/search`, `GET /api/soul/relationships/{id}` |
| `EventStream.svelte` | AYIN span events via SSE | `GET /api/events` |
| `BuildDetailPanel.svelte` | CORSO/QUANTUM findings + notes | `GET /api/builds/{id}/findings|notes` |
| `PlanView.svelte` | QUANTUM/SERAPH phase enrichment | `PUT /api/builds/plan/{codename}` |

### 16.3 Gateway Actions NOT Yet Wired to UI

These 50+ actions are callable via API but have no dedicated UI surface. Priority-ranked for UI exposure:

**High Priority — build these panels next:**

| Agent | Actions | Recommended UI Surface |
|-------|---------|----------------------|
| CORSO | `code_review`, `search_code`, `find_symbol`, `get_outline`, `get_references` | Code Intelligence panel in Build Detail (like a mini IDE sidebar) |
| CORSO | `analyze_architecture` | Architecture diagram view in Build Detail (Three.js DAG) |
| CORSO | `watch` | Live file watcher status in OPS Mission Control |
| QUANTUM | `theorize`, `verify` | Hypothesis panel in HELIX (show reasoning chains) |
| SERAPH | `investigate_start`, `investigate_advance`, `investigate_close`, `investigate_report` | Security Investigation drawer (dedicated SERAPH panel) |
| AYIN | `sessions`, `spans`, `conversations` | Trace explorer panel (timeline view in Build Detail) |

**Medium Priority — wire to existing surfaces:**

| Agent | Actions | Wire to |
|-------|---------|---------|
| CORSO | `prove`, `optimize` | Build Detail > pillar actions menu |
| CORSO | `deploy`, `rollback` | Build Detail > operations tab |
| CORSO | `manage_logs` | Build Detail > terminal/logs panel |
| SERAPH | `status`, `scope_check`, `vault_sync` | OPS Mission Control sidebar |
| EVA | `ideate`, `teach`, `mindfulness` | Copilot drawer contextual actions |
| EVA | `remember`, `crystallize` | Memory panel inline actions |
| SOUL | `read_note`, `write_note`, `list_notes` | HELIX note editor |
| SOUL | `manifest`, `ingest` | HELIX admin panel |
| SOUL | `stats`, `health` | OPS Mission Control > SOUL health section |
| SOUL | `convergences`, `relate`, `links` | HELIX graph visualization (Three.js node graph) |
| SOUL | `voice`, `converse` | EVA voice panel (future) |

**Low Priority — available via Cmd+K or API only:**

| Agent | Actions |
|-------|---------|
| EVA | `bible_search`, `bible_reflect`, `celebrate`, `deploy_gate`, `pipeline_reflect` |
| SOUL | `validate`, `commit_enrichment`, `soul_search`, `query_frontmatter` |
| QUANTUM | `quick`, `list`, `discover`, `workflow` |

### 16.4 New UI Surfaces Needed (from gap analysis)

| Surface | Actions it exposes | Priority | Screen |
|---------|------------------|----------|--------|
| **Reasoning Trace panel** | QUANTUM `theorize/verify`, CORSO `analyze_architecture`, Working History structured | P1 | Build Detail |
| **AYIN Trace Explorer** | AYIN `sessions`, `spans`, `conversations` | P1 | Build Detail, OPS |
| **SERAPH Investigation** | SERAPH `investigate_*`, `scope_check` | P1 | Dedicated panel (DISPATCH or OPS) |
| **Code Intelligence sidebar** | CORSO `search_code`, `find_symbol`, `get_outline`, `get_references` | P2 | Build Detail |
| **SOUL Graph View** | SOUL `convergences`, `relate`, `links`, `soul_search` | P2 | HELIX |
| **Token/Cost Attribution** | New endpoint needed (not yet in API) | P1 | Build cards, Dispatch |
| **Loop Iteration Counter** | New KV key `build.loops.count` | P1 | Build Detail, Dispatch |
| **Dispatch Template Library** | SOUL `read_note/write_note` for templates | P2 | DISPATCH |

### 16.5 XState Machine Integration (Implementation Reference)

State machines for Project/Program/Build use XState v5 with Svelte integration:

```typescript
// Build Plan state machine — XState v5
import { setup, createActor, assign } from 'xstate';

const buildMachine = setup({
  types: {} as {
    context: { phase: number; gates: Record<string, 'pending'|'passed'|'failed'>; cost_usd: number; loops: number; };
    events:
      | { type: 'ADVANCE' }
      | { type: 'GATE_PASS'; pillar: string }
      | { type: 'GATE_FAIL'; pillar: string }
      | { type: 'SELF_CORRECT' }
      | { type: 'COMPLETE' }
      | { type: 'FAIL'; reason: string };
  },
}).createMachine({
  id: 'build',
  initial: 'queued',
  context: { phase: 0, gates: {}, cost_usd: 0, loops: 0 },
  states: {
    queued:      { on: { ADVANCE: 'phase_active' } },
    phase_active: {
      on: {
        GATE_PASS:    { actions: assign({ gates: ({ context, event }) => ({ ...context.gates, [event.pillar]: 'passed' }) }) },
        GATE_FAIL:    { actions: assign({ gates: ({ context, event }) => ({ ...context.gates, [event.pillar]: 'failed' }) }) },
        SELF_CORRECT: { actions: assign({ loops: ({ context }) => context.loops + 1 }) },
        ADVANCE:      'gate_review',
        FAIL:         'failed',
      }
    },
    gate_review: { on: { ADVANCE: 'complete', FAIL: 'failed' } },
    complete:    { type: 'final' },
    failed:      { type: 'final' },
  },
});

// Frontend: optimistic mirror via Svelte $state
let committed = $state(snapshot);
let pending = $state<Partial<typeof snapshot> | null>(null);

// On state machine event: optimistic update
function sendEvent(event: BuildEvent) {
  pending = computeOptimistic(committed, event);  // immediate
  actor.send(event);                               // async confirm
}

// On backend confirm: commit
actor.subscribe((snap) => {
  committed = snap.context;
  pending = null;
});

// On backend reject: glitch + rollback
actor.on('error', () => {
  triggerGlitch();
  pending = null;
});
```

Every state transition emits to SOUL via `POST /api/soul/compaction/preview` event log (turnlog, HMAC-chained). This is the Immutable Provenance record.

---

## §17 — Open Decisions (Updated)

Items requiring a decision before implementation can proceed:

- [ ] **Headless component library**: Melt UI vs. Bits UI vs. custom for accessible comboboxes, dialogs, tooltips
- [ ] **Command palette implementation**: build custom vs. `cmdk` port for Svelte
- [ ] **Temporal view**: timeline/Gantt in OPS — Three.js or p5.js? Canvas or SVG?
- [ ] **OpenAPI spec generation**: add `utoipa` or `aide` to Axum routes for auto-generated spec
- [ ] **Token/cost API**: new backend endpoint — which agent owns cost attribution? (AYIN spans contain timing; cost requires Anthropic API billing data)
- [ ] **Loop iteration counter instrumentation**: agents must emit `build.loops.count` KV key on each self-correction. Requires agent-side instrumentation across CORSO/QUANTUM/SERAPH
- [ ] **SERAPH investigation panel**: dedicated UI surface or extend existing DISPATCH execution zone?
- [ ] **State machine backend**: Rust enum + SOUL event log (recommended) or XState actor on server side via Node.js proxy?
- [ ] **Dispatch template storage schema**: markdown frontmatter in SOUL notes (simple) vs. structured JSON (queryable)
- [ ] **Firecrawl live web research**: wire `GET /api/soul/search` with Firecrawl fallback when SOUL returns 0 results for HELIX queries — requires gateway action routing decision

---

*This document is the implementation contract. It evolves via PR — never via ad-hoc decisions during implementation.*
