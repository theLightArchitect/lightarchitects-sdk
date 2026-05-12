# Design Refinements — LA Webshell
**Version**: 1.0 | **Date**: 2026-05-12 | **Against**: DESIGN-LANGUAGE.md v2.0 + WEBSHELL-UX-AUDIT.md  
**Produced by**: frontend-design review against live screenshots  
**Northstar**: elegant + simple for first-time users · maximum power for operators · multi-agent orchestration at maximum speed

> Each section = one screen. Each item = one specific, implementable change with the exact CSS property, component prop, or Svelte binding to modify. Priority: 🔴 P0 (breaks the screen) · 🟠 P1 (degrades UX significantly) · 🟡 P2 (polish/Stark fidelity).

---

## Global — Cross-Cutting Fixes (apply everywhere before per-screen work)

### G-1 🔴 OFFLINE state must visually dominate when connectivity is lost
The current "● OFFLINE reconnecting" is 11px slate text in the top bar. An operator scanning at speed will miss it entirely. When disconnected:

```css
/* Add to top nav bar when status === 'offline' */
.nav-bar[data-status="offline"] {
  border-bottom: 1px solid var(--la-semantic-warn);
  box-shadow: 0 0 24px rgba(245, 158, 11, 0.15) inset;
  animation: offline-pulse 3s ease-in-out infinite;
}
@keyframes offline-pulse {
  0%, 100% { box-shadow: 0 0 24px rgba(245, 158, 11, 0.10) inset; }
  50%       { box-shadow: 0 0 40px rgba(245, 158, 11, 0.25) inset; }
}
```

The status pill becomes:
```svelte
<!-- Replace current pill -->
<span class="status-pill" class:offline={!connected} class:live={connected}>
  <span class="dot" />
  {connected ? 'LIVE' : 'OFFLINE — reconnecting'}
  {#if !connected}<button on:click={retry}>Retry</button>{/if}
</span>
```

```css
.status-pill.offline { color: var(--la-semantic-warn); letter-spacing: 0.08em; }
.status-pill.offline .dot { background: var(--la-semantic-warn); box-shadow: var(--la-semantic-warn-glow); animation: blink 1s step-end infinite; }
```

### G-2 🟠 Background grid must have perspective convergence (p5.js)
Current grid is flat. The Stark aesthetic requires the grid to converge toward a vanishing point. Add to the p5.js layer on ALL screens:

```javascript
// p5-grid.js — runs on every screen as a fixed SVG overlay
function drawPerspectiveGrid(p) {
  const vx = p.width / 2;   // vanishing x = center
  const vy = p.height * 1.2; // vanishing y = below viewport
  const lineColor = p.color(0, 200, 255, 10); // rgba(0,200,255,0.04)
  const step = 32;

  p.stroke(lineColor);
  p.strokeWeight(0.5);

  // Horizontal lines with perspective spacing (closer = tighter)
  for (let y = 0; y < p.height; y += step) {
    const t = y / p.height; // 0 at top, 1 at bottom
    const alpha = p.map(t, 0, 1, 5, 18); // fade in toward bottom
    p.stroke(0, 200, 255, alpha);
    p.line(0, y, p.width, y);
  }

  // Vertical lines converging to vanishing point
  for (let x = 0; x <= p.width; x += step) {
    p.stroke(0, 200, 255, 8);
    p.line(x, 0, vx + (x - vx) * (vy / (vy - 0)), vy);
  }
}
```

### G-3 🟠 Scan-line shader — add to ALL panel backgrounds
```css
/* Add to .la-panel, .la-card, .la-sidebar */
.la-panel::after {
  content: '';
  position: absolute;
  inset: 0;
  background: repeating-linear-gradient(
    0deg, transparent, transparent 3px,
    rgba(0, 0, 0, 0.025) 3px, rgba(0, 0, 0, 0.025) 4px
  );
  pointer-events: none;
  z-index: 1;
}
```

### G-4 🟡 Targeting reticle — add to ALL selected/focused interactive elements
```css
/* Apply .la-selected to the focused/active card */
.la-selected {
  position: relative;
  outline: none; /* remove browser default */
}
.la-selected::before {
  content: '';
  position: absolute;
  top: -3px; left: -3px;
  width: 10px; height: 10px;
  border-top: 1px solid var(--la-struct-primary);
  border-left: 1px solid var(--la-struct-primary);
  opacity: 0.9;
  pointer-events: none;
}
.la-selected::after {
  content: '';
  position: absolute;
  bottom: -3px; right: -3px;
  width: 10px; height: 10px;
  border-bottom: 1px solid var(--la-struct-primary);
  border-right: 1px solid var(--la-struct-primary);
  opacity: 0.9;
  pointer-events: none;
}
```

### G-5 🟠 Typography: section labels must be layer-cake anchors
Every section heading (`MISSION CONTROL`, `TASK SPECIFICATION`, `BUILD PORTFOLIO`) needs to elicit the layer-cake scan pattern. Current headings are too similar in weight to body text.

```css
.la-section-label {
  font-family: 'JetBrains Mono', monospace;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.18em;
  color: var(--la-text-bright); /* #f1f5f9 — must be BRIGHT, not dim */
  text-transform: uppercase;
  /* Structural left-border accent */
  border-left: 2px solid var(--la-struct-primary);
  padding-left: 8px;
}
```

### G-6 🟡 Construct animation — wire to Svelte transitions
Replace `transition:fade` with the construct pattern on all card mounts:

```svelte
<script>
  import { draw } from 'svelte/transition';
  // Custom construct transition
  function construct(node, { duration = 300 }) {
    return {
      duration,
      css: (t, u) => `
        opacity: ${t};
        transform: scale(${0.97 + 0.03 * t});
        clip-path: inset(${u * 8}px ${u * 4}px ${u * 8}px ${u * 4}px);
      `
    };
  }
</script>

{#each builds as build (build.id)}
  <div transition:construct class="build-card">...</div>
{/each}
```

---

## S1 — OPS / Mission Control

**Eye-flow assigned**: Gutenberg diagram  
**Current verdict**: FAILS — hexagon orbital occupies fallow zone as the visual hero; Primary Optical Area (top-left) contains unreadable squad pills; Terminal Area (bottom-right) is empty

### OPS-1 🔴 Restructure visual hierarchy — sidebar IS the hero

The Mission Control sidebar must expand and move to the primary visual position. The hexagon map becomes the secondary panel (right side, or toggled):

```
BEFORE:                          AFTER:
┌──────┬────────────────┬──────┐  ┌──────────────────────┬───────────┐
│ side │   3D hex MAP   │live  │  │  MISSION CONTROL     │  3D MAP   │
│ bar  │   (60% wide)   │panel │  │  (primary hero)      │ (right,   │
│ thin │                │      │  │  squad · queue ·     │  40%)     │
│      │                │      │  │  gates · economics   │  toggle   │
└──────┴────────────────┴──────┘  └──────────────────────┴───────────┘
```

Mission Control panel at 55% width, map at 45%, with "Show 3D View" toggle (already exists) controlling map visibility. When map is hidden, Mission Control expands full-width.

### OPS-2 🔴 Squad health pills — add health state glow + last-heartbeat

Current: 7 flat pills all showing "NEVER". Required:

```svelte
<!-- SiblingPill.svelte -->
<div class="sibling-pill" data-state={status} style:--color={siblingColor}>
  <span class="dot" />
  <span class="label">{abbreviation}</span>
  <span class="last-seen">{lastSeen}</span> <!-- "2m ago" | "NEVER" | "live" -->
</div>
```

```css
.sibling-pill { display: flex; align-items: center; gap: 4px; padding: 4px 8px; border-radius: 2px; }
.sibling-pill[data-state="online"]  { border: 1px solid var(--color); box-shadow: 0 0 8px color-mix(in srgb, var(--color) 40%, transparent); }
.sibling-pill[data-state="offline"] { border: 1px solid var(--la-semantic-offline); opacity: 0.5; }
.sibling-pill[data-state="never"]   { border: 1px solid var(--la-semantic-offline); opacity: 0.35; font-style: italic; }
.sibling-pill .dot { width: 5px; height: 5px; border-radius: 50%; background: var(--color); }
.sibling-pill[data-state="online"] .dot { animation: agent-pulse 2s ease-in-out infinite; }
@keyframes agent-pulse {
  0%, 100% { box-shadow: 0 0 3px var(--color); }
  50%       { box-shadow: 0 0 10px var(--color); }
}
```

### OPS-3 🟠 Hexagon map — add circuit trace connections + presence glow

Each hexagon needs a Three.js `PointLight` whose intensity encodes agent state. Add p5.js SVG circuit traces between hexagons that are collaborating:

```javascript
// In HelixScene or OpsMap Three.js component
function updateHexGlow(hex, siblingState) {
  const light = hex.userData.light; // PointLight attached to hex
  const target = {
    idle:      { intensity: 0.2, color: 0x475569 },
    queued:    { intensity: 0.5, color: 0x2a6496 },
    reasoning: { intensity: 1.2, color: parseInt(SIBLING_COLORS[hex.userData.sibling].slice(1), 16) },
    writing:   { intensity: 2.0, color: 0x00c8ff },
    blocked:   { intensity: 0.8, color: 0xef4444 },
  }[siblingState] ?? { intensity: 0.2, color: 0x475569 };

  // Lerp to target (smooth transition)
  light.intensity += (target.intensity - light.intensity) * 0.05;
  light.color.setHex(target.color);
}
```

For circuit traces between collaborating siblings (p5.js SVG layer):
```javascript
function drawCollaborationTrace(p, fromHex, toHex) {
  // Midpoint with 90° bend (circuit board style)
  const mid = { x: fromHex.x, y: toHex.y };
  p.stroke(0, 200, 255, 120);
  p.strokeWeight(1);
  p.line(fromHex.x, fromHex.y, mid.x, mid.y);
  p.line(mid.x, mid.y, toHex.x, toHex.y);

  // Animated pulse dot travelling the trace
  const t = (Date.now() % 2000) / 2000; // 0→1 every 2s
  const px = lerp(fromHex.x, mid.x, t * 2) + lerp(mid.x, toHex.x, Math.max(0, t * 2 - 1));
  const py = lerp(fromHex.y, mid.y, t * 2) + lerp(mid.y, toHex.y, Math.max(0, t * 2 - 1));
  p.fill(0, 200, 255, 200);
  p.noStroke();
  p.circle(px, py, 4);
}
```

### OPS-4 🟠 Executive summary card on hexagon hover

```svelte
<!-- ExecutiveSummaryCard.svelte — portal to document.body -->
{#if hoveredBuild}
  <div
    class="exec-card"
    style:left="{cardPos.x}px"
    style:top="{cardPos.y}px"
    transition:construct={{ duration: 150 }}
    use:portal
  >
    <header>
      <span class="codename">{hoveredBuild.codename}</span>
      <span class="badge" data-status={hoveredBuild.status}>{hoveredBuild.status}</span>
    </header>
    <div class="gates">
      {#each PILLARS as pillar}
        <span class="gate-dot" data-state={hoveredBuild.gates[pillar]} title={pillar} />
      {/each}
    </div>
    <div class="meta">
      <span>{hoveredBuild.activeAgents} agents active</span>
      <span>${hoveredBuild.costUsd.toFixed(2)} spent</span>
      <span>{timeAgo(hoveredBuild.lastActivity)}</span>
    </div>
    <footer>
      <button on:click={() => drillDown(hoveredBuild)}>Open Build</button>
      <button on:click={() => dispatch(hoveredBuild)}>Dispatch</button>
    </footer>
  </div>
{/if}
```

```css
.exec-card {
  position: fixed; z-index: 1000;
  background: var(--la-bg-elevated);
  border: 1px solid var(--la-struct-secondary);
  box-shadow: 0 0 32px rgba(0,200,255,0.15), 0 16px 48px rgba(0,0,0,0.6);
  padding: 12px 16px;
  min-width: 260px;
  font-family: 'JetBrains Mono', monospace;
  /* Scan-line on the card itself */
  background-image: repeating-linear-gradient(0deg, transparent, transparent 3px, rgba(0,0,0,0.025) 3px, rgba(0,0,0,0.025) 4px);
}
.exec-card .gate-dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; margin: 0 2px; }
.exec-card .gate-dot[data-state="passed"]  { background: var(--la-semantic-ok); box-shadow: var(--la-semantic-ok-glow); }
.exec-card .gate-dot[data-state="failed"]  { background: var(--la-semantic-error); box-shadow: var(--la-semantic-error-glow); }
.exec-card .gate-dot[data-state="pending"] { background: var(--la-semantic-offline); }
```

### OPS-5 🟡 Bottom sibling strip — superseded by Git Forest

**Status**: Design direction resolved — the Git Forest visualization (GIT-1 through GIT-8) replaces the need for a separate sibling activity strip. Agent activity, ownership, and worktree state are encoded directly on git tree branches. The strip below can be removed.

~~The bottom colored strips (SOUL · EVA · CORSO · QUANTUM · SERAPH · AYIN · LARC) at 8px height carry zero information. Two options:~~

~~**Option A (preferred)**: Remove — sibling health already shown in Mission Control pills.~~
~~**Option B**: Elevate to a "currently active agents" mini-bar showing what each is doing:~~

```svelte
<!-- Replace bottom strip with active-agent bar -->
<div class="agent-activity-bar">
  {#each activeSiblings as s}
    <div class="agent-activity-chip" style:--c={SIBLING_COLORS[s.id]}>
      <span class="chip-label">{s.abbreviation}</span>
      <span class="chip-action">{s.currentAction ?? '—'}</span>
    </div>
  {/each}
</div>
```

### OPS-6 🟡 Conductor queue — add phase/gate context to queue items

Current: "11 queued" with a progress bar. Required: show what each queued item is waiting on:

```svelte
<div class="conductor-queue">
  <div class="queue-stats">
    <span class="running">{running}</span>
    <span class="queued">{queued}</span>
  </div>
  <div class="queue-preview">
    {#each topQueue.slice(0, 3) as item}
      <div class="queue-item">
        <span class="item-codename">{item.codename}</span>
        <span class="item-gate blocked-on">{item.blockedOn ?? item.phase}</span>
      </div>
    {/each}
    {#if queued > 3}<span class="more">+{queued - 3} more</span>{/if}
  </div>
</div>
```

---

## GIT FOREST — OPS Screen: Git Tree Visualization

**Placement**: OPS center column — tab-toggles with or replaces `VoxelProjects3D`  
**Renderer**: Three.js (already loaded in `VoxelProjects3D.svelte`)  
**Backend dependency**: `gateway-worktree-coordinator` (queued, HIGH priority, MEDIUM tier)  
**Standard**: `git-orchestration-standard` v1.0 (ratified 2026-05-12, `helix/corso/builds/git-orchestration-standard/`)  
**Eye-flow**: F-pattern — grove scan left-to-right, individual tree focus on click  
**Data shape**: `GET /api/git/repos`, `GET /api/git/repos/:id/graph`, SSE `git:branch_status` / `git:file_write` / `git:merge`

### GIT-1 🔴 Git forest scene — core geometry

Three.js scene, isometric camera (same angle as `VoxelProjects3D`). One tree per tracked repository. Generated from `git log --graph --all` output seeded with real repo data.

**Rendering approach**: holographic pipeline per DESIGN-LANGUAGE.md §19 — `UnrealBloomPass` + `AdditiveBlending` + fresnel GLSL shader + `EdgesGeometry` overlay + p5.js scan line / grain atmospheric layer. All materials render as light-in-air, not solid objects. Ghost branches (merged) use `LineDashedMaterial`.

**Geometry encoding:**

| Dimension | Encodes | Primitive |
|---|---|---|
| Trunk height | Total commits on `main` — each commit = 1 unit | `CylinderGeometry` |
| Trunk base girth | Total repo file count / disk size — natural taper to tip | Radius differential top/bottom |
| Branch fork height | Commit number where `feat/<codename>` diverged | Fork point on trunk |
| Branch length | Commit count on branch — each commit = 1 unit | `TubeGeometry` (Catmull-Rom) |
| Branch girth | Unique files modified across branch — grows as work accumulates | Tube radius |
| Branch tip vs trunk tip | Above = ahead of `main`; below = behind | Relative Y position |
| Agent worktree | Sub-branch off `feat/<codename>` in agent identity color | Thin `TubeGeometry` |
| Commit node | Marker at uniform spacing along branch | `SphereGeometry` r=0.15 |
| Files per commit | Leaf particles around commit node | `InstancedMesh` `PlaneGeometry` |
| Merge point | Ring pulse at merge height on trunk; trunk girth steps up | Torus flash + radius lerp |
| Merged ghost branch | 20% opacity, no animation — persists until operator prune | Opacity + desaturate |

**Branch girth rule:** default thin (~20% trunk girth). Grows proportionally to files modified on the branch. Can exceed trunk girth only if branch introduces more net-new files than currently exist on `main` (e.g. a major feature addition). On merge, trunk girth steps up smoothly at merge height to absorb the new files.

### GIT-2 🔴 Gate status color system

Branch emissive color maps to `git-orchestration-standard` gate state. Active write (cyan pulse) overrides status color during write and returns on idle (3s no new `git:file_write` events).

| State | Color | Token | Trigger |
|---|---|---|---|
| Commit gate passed — clean | Green | `--la-semantic-ok` | fmt + clippy + unit tests pass |
| HITL checkpoint pending | Yellow | `--la-semantic-warn` | Phase-boundary gate — operator action required |
| Merge gate / PR ready | Gold | `--la-focus-ring` | All merge gate checks passed |
| Gate failure | Red | `--la-semantic-error` | Any blocking gate failed |
| Active agent write | Cyan pulse | `--la-struct-primary` | SSE `git:file_write` on worktree |
| Merged ghost | Dim | `--la-semantic-offline` | Post-merge until operator prune |

```typescript
// Branch material update on SSE event
function applyGateColor(branch: BranchMesh, state: GateState) {
  const colors: Record<GateState, number> = {
    clean:        0x22c55e,
    hitl_pending: 0xf59e0b,
    merge_ready:  0xFFD700,
    failed:       0xef4444,
    writing:      0x00c8ff,
    ghost:        0x475569,
  };
  branch.material.emissive.setHex(colors[state]);
  branch.material.emissiveIntensity = state === 'writing' ? 1.0 : 0.4;
}
```

### GIT-3 🔴 Agent worktree sub-branches

Each `feat/<codename>` branch has sub-branches for agent worktrees (`~/lightarchitects/worktrees/<codename>/`). Sub-branches use **agent domain identity color** (not gate color) — ownership is the primary encoding at sub-branch level. Sub-branches are always thinner than their parent `feat/<codename>` branch.

```typescript
const AGENT_COLORS: Record<AgentDomain, number> = {
  engineer:   0x4d8eff,
  quality:    0xa874ff,
  security:   0xff4d4d,
  ops:        0xff8e3c,
  researcher: 0x4dffe6,
  knowledge:  0xf5d440,
  testing:    0x4dff8e,
  squad:      0xff7eb6,
};
```

### GIT-4 🟠 Agent write pulse animation

On SSE `git:file_write` for a branch:
- Branch emissive: 0.3 → 1.0 → 0.3 over 1.5s (`var(--ease-project)`)
- Leaf particles at that commit node: flutter (±4px random offset, 60fps for 2s)
- Sub-branch glows in agent identity color during write
- Returns to gate-state color 3s after last write event

### GIT-5 🟠 Merged branch ghost persistence

Merged branches do not disappear. On SSE `git:merge`:
1. Leaves fall to branch surface (gravity animation, 800ms `--ease-retract`)
2. Branch desaturates to 20% opacity, emissive off
3. Merge ring: brief torus pulse outward from trunk at merge height in branch's color
4. Trunk girth lerps to new value (absorbed file count) over 600ms `--ease-project`

Ghost branches persist until operator clicks "Prune stale branches" in grove context menu. Reflects real git state: merged refs exist until explicit cleanup.

### GIT-6 🟡 Ahead-of-main upward particle trace

Branches with tip above trunk tip (ahead of `main` HEAD) display an upward-flowing particle trace on their surface — rate of flow encodes commits-ahead count. Branches behind `main` tip carry no indicator beyond their relative height position.

### GIT-7 🟡 File leaf particles — instanced mesh (Sprint 4)

`InstancedMesh` leaf planes distributed via `fibonacci_sphere(count, radius=0.8)` around each commit node. Count = files modified in that commit. Leaf color by file type:

| File type | Color |
|---|---|
| `.rs` Rust | `#f97316` orange |
| `.ts` / `.svelte` TypeScript | `#3b82f6` blue |
| `.css` | `#00c8ff` cyan |
| `.md` Markdown | `#7a8390` dim |
| `.json` / `.yaml` | `#f5d440` yellow |
| Other | `#475569` slate |

Dense leaf clouds mark significant commits before any label is read.

### GIT-8 🟡 SOUL helix cross-reference (Sprint 4)

Helix entries tagged with a build codename appear as luminous nodes on the corresponding `feat/<codename>` branch at the commit height nearest their `created_at` timestamp. Significance ≥7.0 = full glow node (`--la-semantic-ok` green). Hover reveals the helix entry excerpt and significance score. Connects code artifacts (commits) to knowledge artifacts (helix entries) in a single view.

---

## S2 — DISPATCH / Squad Dispatch Operator Console

**Eye-flow assigned**: Z-pattern  
**Current verdict**: PARTIAL — Z start correct (task textarea, top-left), but diagonal and terminal CTA misaligned; premature error; idle right panel wastes space

### DIS-1 🔴 Remove premature validation error

```svelte
<!-- AgentGrid.svelte: change from always-shown to submit-time-only -->
<script>
  let showError = false;
  export function validateBeforeDispatch() {
    if (selectedAgents.length === 0) { showError = true; return false; }
    return true;
  }
</script>

<!-- Remove this from render: -->
<!-- <p class="error">Select at least one agent to dispatch.</p> -->

<!-- Replace with: only shown after attempted dispatch -->
{#if showError}
  <p class="error" transition:fly={{ y: -4, duration: 150 }}>
    ↑ Select at least one agent above to dispatch.
  </p>
{/if}
```

### DIS-2 🟠 Agent cards — selection state

Currently all cards look identical. Selected vs unselected must be immediately obvious:

```css
.agent-card {
  background: var(--la-bg-card);
  border: 1px solid rgba(71, 85, 105, 0.4);
  padding: 12px;
  cursor: pointer;
  transition: all 150ms var(--ease-snap);
  position: relative;
}
.agent-card:hover {
  border-color: var(--la-struct-secondary);
  background: var(--la-bg-elevated);
}
.agent-card.selected {
  border-color: var(--la-struct-primary);
  background: rgba(0, 200, 255, 0.06);
  box-shadow: 0 0 12px rgba(0, 200, 255, 0.15), inset 0 0 24px rgba(0, 200, 255, 0.04);
}
/* Targeting reticle corners on selected cards */
.agent-card.selected::before { /* top-left bracket */ }
.agent-card.selected::after  { /* bottom-right bracket */ }

/* Gate abbreviation label */
.agent-card .gate-label {
  font-size: 9px;
  letter-spacing: 0.12em;
  color: var(--la-struct-primary);
  opacity: 0.7;
}
.agent-card.selected .gate-label { opacity: 1; }
```

### DIS-3 🟠 DISPATCH button — make it the Z-pattern terminal CTA

The terminal action in a Z-pattern lives bottom-right. The Dispatch button in zone 01 should move:

```svelte
<!-- Move from below textarea to bottom-right of agent zone -->
<div class="zone zone-01">
  <textarea bind:value={task} placeholder="Describe the task for the squad…" />
  <div class="zone-01-footer">
    <label class="dry-run">
      <input type="checkbox" bind:checked={dryRun} />
      <span>Dry run <em>(no writes)</em></span>
    </label>
    <div class="context-actions">
      <button class="secondary">+ Files</button>
      <button class="secondary">+ Folder</button>
    </div>
    <span class="char-count">{task.length} / 8,192</span>
  </div>
</div>

<!-- Terminal CTA: bottom-right, glows when agents selected -->
<div class="dispatch-cta" class:ready={selectedAgents.length > 0}>
  <button class="dispatch-btn" on:click={handleDispatch} disabled={!task.trim()}>
    DISPATCH
    <span class="agent-count">{selectedAgents.length}</span>
  </button>
</div>
```

```css
.dispatch-btn {
  background: var(--la-bg-elevated);
  border: 1px solid var(--la-struct-secondary);
  color: var(--la-text-label);
  padding: 10px 24px;
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  letter-spacing: 0.16em;
  cursor: pointer;
  transition: all 200ms var(--ease-project);
}
.dispatch-cta.ready .dispatch-btn {
  border-color: var(--la-struct-primary);
  color: var(--la-text-bright);
  box-shadow: var(--la-semantic-ok-glow);
  animation: dispatch-ready-pulse 2s ease-in-out infinite;
}
@keyframes dispatch-ready-pulse {
  0%, 100% { box-shadow: 0 0 8px rgba(0, 200, 255, 0.3); }
  50%       { box-shadow: 0 0 20px rgba(0, 200, 255, 0.6); }
}
.dispatch-btn .agent-count {
  display: inline-flex; align-items: center; justify-content: center;
  width: 18px; height: 18px; border-radius: 50%;
  background: var(--la-struct-primary); color: var(--la-bg-base);
  font-size: 9px; font-weight: 700; margin-left: 8px;
  opacity: 0; transform: scale(0.5);
  transition: all 150ms var(--ease-snap);
}
.dispatch-cta.ready .dispatch-btn .agent-count { opacity: 1; transform: scale(1); }
```

### DIS-4 🟠 Right panel idle state — replace dead counter with useful content

```svelte
<!-- Zone 02 right panel -->
<div class="dispatch-status-panel">
  {#if dispatchState === 'idle'}
    <div class="idle-state">
      <p class="idle-hint">Select agents above, then dispatch.</p>
      {#if lastDispatch}
        <div class="last-dispatch">
          <span class="label">LAST</span>
          <span class="codename">{lastDispatch.codename}</span>
          <span class="time">{timeAgo(lastDispatch.timestamp)}</span>
          <button class="replay" on:click={() => replayDispatch(lastDispatch)}>↺ Replay</button>
        </div>
      {:else}
        <p class="no-history">No past dispatches</p>
      {/if}
    </div>
  {:else if dispatchState === 'running'}
    <div class="running-state">
      <div class="agents-active">
        <!-- Rolling counter using CSS @property -->
        <span class="counter" style:--num={activeAgentCount}></span>
        <span class="label">AGENTS ACTIVE</span>
      </div>
      <button class="cancel-btn" on:click={cancelDispatch}>Cancel</button>
    </div>
  {/if}
</div>
```

### DIS-5 🟡 RAILS + DAG toggles — add tooltips and visual state

```svelte
<div class="toggle-group">
  <button
    class="toggle" class:active={railsEnabled}
    on:click={() => railsEnabled = !railsEnabled}
    title="RAILS: Enforce architectural guardrails — prevents agents from modifying files outside scope"
  >
    RAILS
  </button>
  <button
    class="toggle" class:active={dagEnabled}
    on:click={() => dagEnabled = !dagEnabled}
    title="DAG: Run agents as a dependency graph — agents wait for upstream results before starting"
  >
    DAG
  </button>
</div>
```

```css
.toggle { background: none; border: 1px solid var(--la-semantic-offline); color: var(--la-text-dim); padding: 3px 8px; font-size: 9px; letter-spacing: 0.12em; cursor: pointer; transition: all 120ms; }
.toggle.active { border-color: var(--la-struct-primary); color: var(--la-struct-primary); background: rgba(0,200,255,0.06); }
```

### DIS-6 🟡 Textarea focused state — targeting reticle + cyan border

```css
.task-textarea:focus {
  outline: none;
  border-color: var(--la-struct-primary);
  box-shadow: 0 0 0 1px var(--la-struct-primary), 0 0 24px rgba(0, 200, 255, 0.08);
}
/* Corner brackets appear on focus */
.task-textarea-wrapper:focus-within::before { /* top-left */ opacity: 1; }
.task-textarea-wrapper:focus-within::after  { /* bottom-right */ opacity: 1; }
```

### DIS-7 🟡 Zone separators — circuit trace left-border decoration

```css
.zone-header {
  display: flex; align-items: center; gap: 8px;
  padding: 8px 0;
  border-top: 1px solid rgba(71, 85, 105, 0.3);
}
.zone-header::before {
  content: '';
  width: 2px; height: 20px;
  background: linear-gradient(to bottom, var(--la-struct-primary), transparent);
  flex-shrink: 0;
}
.zone-number { color: var(--la-struct-primary); font-size: 9px; opacity: 0.7; }
```

---

## S3 — BUILDS / Build Queue

**Eye-flow assigned**: F-pattern  
**Current verdict**: PARTIAL — dual-tier creates two competing F-pattern verticals; progress bars all 0% look broken; no last-activity timestamps; gate visibility limited to ARCH only

### BLD-1 🔴 Collapse dual-tier into single coherent view

The current layout shows each build TWICE (board card + portfolio card). Remove the portfolio grid section. The board cards become the single source of truth, with more information density:

```svelte
<!-- Remove this section entirely: -->
<!-- <section class="build-portfolio">...</section> -->

<!-- Expand board cards to carry full information: -->
<div class="build-card" class:active={build.status === 'in_progress'}>
  <div class="card-header">
    <span class="codename">{build.codename}</span>
    <StatusBadge status={build.status} />
    <span class="last-seen">{timeAgo(build.lastActivity)}</span>
  </div>
  <div class="card-path">{build.path}</div>
  <GateStrip gates={build.gates} />      <!-- NEW: 7-dot gate strip -->
  <ProgressBar value={build.progress} empty={!build.progress} />
  <div class="card-meta">
    <span class="agent-count">{build.activeAgents} agents</span>
    <span class="cost">${build.costUsd?.toFixed(2) ?? '—'}</span>
    <span class="loop-count" title="Self-correction cycles">{build.loops}↺</span>
  </div>
</div>
```

### BLD-2 🔴 Progress bars — distinguish "no data" from "0%"

```svelte
<!-- ProgressBar.svelte -->
<div class="progress-bar" class:no-data={!value && value !== 0}>
  {#if value !== null && value !== undefined}
    <div class="fill" style:width="{value}%" />
  {:else}
    <!-- Dashed "no data" state -->
  {/if}
</div>
```

```css
.progress-bar { height: 2px; background: rgba(71, 85, 105, 0.2); border-radius: 1px; overflow: hidden; }
.progress-bar .fill { height: 100%; background: var(--la-struct-primary); transition: width 600ms var(--ease-project); }
.progress-bar.no-data {
  background: none;
  border: 1px dashed rgba(71, 85, 105, 0.4);
  border-radius: 1px;
  height: 2px;
}
```

### BLD-3 🟠 GateStrip — 7-dot visual for all LASDLC gates

```svelte
<!-- GateStrip.svelte -->
<script>
  export let gates = {};
  const PILLARS = ['ARCH','SEC','QUAL','PERF','TEST','DOC','OPS'];
</script>
<div class="gate-strip" title="ARCH · SEC · QUAL · PERF · TEST · DOC · OPS">
  {#each PILLARS as p}
    <span
      class="gate-dot"
      data-state={gates[p] ?? 'pending'}
      title="{p}: {gates[p] ?? 'pending'}"
    />
  {/each}
</div>
```

```css
.gate-strip { display: flex; gap: 3px; align-items: center; }
.gate-dot { width: 6px; height: 6px; border-radius: 50%; }
.gate-dot[data-state="passed"]  { background: var(--la-semantic-ok);      box-shadow: var(--la-semantic-ok-glow); }
.gate-dot[data-state="failed"]  { background: var(--la-semantic-error);   box-shadow: var(--la-semantic-error-glow); }
.gate-dot[data-state="running"] { background: var(--la-semantic-active);  animation: gate-pulse 1s ease-in-out infinite; }
.gate-dot[data-state="pending"] { background: var(--la-semantic-offline); }
@keyframes gate-pulse { 0%,100%{opacity:0.5} 50%{opacity:1} }
```

### BLD-4 🟠 Status badges — fix text format + color consistency

```css
/* StatusBadge.svelte */
.badge { font-size: 9px; letter-spacing: 0.1em; padding: 2px 6px; border-radius: 2px; text-transform: uppercase; }
.badge[data-status="in_progress"] { background: rgba(245,158,11,0.12); color: var(--la-semantic-warn); border: 1px solid rgba(245,158,11,0.3); }
.badge[data-status="queued"]      { background: rgba(0,200,255,0.08);  color: var(--la-struct-primary); border: 1px solid rgba(0,200,255,0.2); }
.badge[data-status="planned"]     { background: rgba(71,85,105,0.15);  color: var(--la-text-label);     border: 1px solid rgba(71,85,105,0.3); }
.badge[data-status="complete"]    { background: rgba(34,197,94,0.08);  color: var(--la-semantic-ok);    border: 1px solid rgba(34,197,94,0.2); }
.badge[data-status="failed"]      { background: rgba(239,68,68,0.08);  color: var(--la-semantic-error); border: 1px solid rgba(239,68,68,0.2); }
```

### BLD-5 🟠 Active builds — structural cyan left-border glow

```css
.build-card[data-status="in_progress"] {
  border-left: 2px solid var(--la-struct-primary);
  box-shadow: -4px 0 16px rgba(0, 200, 255, 0.12), 0 0 0 1px rgba(71,85,105,0.3);
}
.build-card:hover {
  background: var(--la-bg-elevated);
  box-shadow: -4px 0 20px rgba(0, 200, 255, 0.18), 0 4px 24px rgba(0,0,0,0.4);
  transform: translateY(-1px);
  transition: all 150ms var(--ease-project);
}
```

### BLD-6 🟠 Summary bar — make counts clickable filters

```svelte
<div class="summary-bar">
  {#each [
    { label: 'in progress', count: inProgress, status: 'in_progress', color: 'warn' },
    { label: 'queued',      count: queued,     status: 'queued',      color: 'struct' },
    { label: 'completed',   count: completed,  status: 'complete',    color: 'ok' },
    { label: 'failed',      count: failed,     status: 'failed',      color: 'error' },
  ] as filter}
    <button
      class="filter-chip"
      class:active={activeFilter === filter.status}
      data-color={filter.color}
      on:click={() => toggleFilter(filter.status)}
    >
      <span class="count">{filter.count}</span>
      <span class="label">{filter.label}</span>
    </button>
  {/each}
</div>
```

```css
.filter-chip { background: none; border: none; cursor: pointer; display: flex; align-items: baseline; gap: 4px; padding: 4px 8px; border-radius: 2px; transition: background 120ms; }
.filter-chip:hover { background: rgba(255,255,255,0.04); }
.filter-chip.active { background: rgba(0,200,255,0.06); }
.filter-chip[data-color="warn"]   .count { color: var(--la-semantic-warn); }
.filter-chip[data-color="struct"] .count { color: var(--la-struct-primary); }
.filter-chip[data-color="ok"]     .count { color: var(--la-semantic-ok); }
.filter-chip[data-color="error"]  .count { color: var(--la-semantic-error); }
```

### BLD-7 🟡 Build card hover — construct-origin drill-down

When clicking a build card, the drill-down transition must originate from the card's bounding box:

```svelte
<script>
  function openBuild(event, build) {
    const rect = event.currentTarget.getBoundingClientRect();
    // Store origin for zoom-from animation
    drilldownOrigin.set({ x: rect.left, y: rect.top, w: rect.width, h: rect.height });
    goto(`/builds/${build.codename}/kanban`);
  }
</script>
<div class="build-card" on:click={(e) => openBuild(e, build)}>...</div>
```

```css
/* Route transition using stored origin */
.builds-to-detail {
  animation: zoom-from-origin 350ms var(--ease-project) forwards;
}
@keyframes zoom-from-origin {
  from { transform: scale(0.95); transform-origin: var(--origin-x) var(--origin-y); opacity: 0.5; }
  to   { transform: scale(1);   transform-origin: center center;                    opacity: 1; }
}
```

---

## S4 — BUILD DETAIL / Kanban

**Eye-flow assigned**: Z-pattern  
**Current verdict**: Search bar top-left ✓; view tabs top-right ✓; Working History is the key surface but unstructured; 8 console errors on load indicate broken state

### DET-1 🔴 Fix console errors before any visual work

8 console errors on load = broken API calls. Audit `BuildDetailPanel.svelte` and `PlanView.svelte`:
- Check `/api/builds/{id}/gates/{pillar}` calls for each LASDLC pillar
- Verify `build.id` is resolved before firing `findings` + `notes` requests
- Add `try/catch` with graceful error state on each failed fetch

### DET-2 🟠 Working History — structure into attributed entries

The current unstructured text log must become a typed, attributed feed:

```svelte
<!-- WorkingHistory.svelte -->
<div class="history-feed">
  {#each entries as entry}
    <div class="history-entry" data-type={entry.type}>
      <div class="entry-meta">
        <span class="agent-tag" style:--c={SIBLING_COLORS[entry.agent]}>{entry.agent}</span>
        <span class="tool-icon">{TOOL_ICONS[entry.tool]}</span>
        <span class="timestamp">{formatTime(entry.timestamp)}</span>
      </div>
      <div class="entry-body">{entry.content}</div>
    </div>
  {/each}
</div>
```

```css
.history-entry { padding: 6px 0; border-bottom: 1px solid rgba(71,85,105,0.15); }
.history-entry[data-type="reasoning"] { border-left: 2px solid var(--la-struct-primary); padding-left: 8px; }
.history-entry[data-type="write"]     { border-left: 2px solid var(--la-semantic-ok);    padding-left: 8px; }
.history-entry[data-type="error"]     { border-left: 2px solid var(--la-semantic-error); padding-left: 8px; }
.history-entry[data-type="tool"]      { border-left: 2px solid var(--la-semantic-warn);  padding-left: 8px; }

.agent-tag {
  font-size: 8px; letter-spacing: 0.1em; font-weight: 700;
  color: var(--c); padding: 1px 4px; border: 1px solid var(--c);
  border-radius: 2px; opacity: 0.9;
}
.tool-icon { font-size: 10px; opacity: 0.6; }
.timestamp { font-size: 8px; color: var(--la-text-dim); margin-left: auto; }
```

Tool icon mapping:
```typescript
const TOOL_ICONS: Record<string, string> = {
  'file_read':  '📄',
  'file_write': '✍️',
  'bash':       '⚡',
  'mcp':        '🔌',
  'reasoning':  '💭',
  'search':     '🔍',
  'dispatch':   '📡',
};
```

### DET-3 🟠 Agent presence bar — show who's actively working

```svelte
<!-- Above Working History panel -->
<div class="agent-presence-bar">
  <span class="label">ACTIVE</span>
  {#each activeSiblings as s}
    <div class="presence-chip" style:--c={SIBLING_COLORS[s.id]} data-state={s.state}>
      <span class="chip-dot" />
      <span>{s.abbreviation}</span>
      <span class="chip-action">{s.currentAction}</span>
    </div>
  {/each}
  {#if activeSiblings.length === 0}
    <span class="no-agents">—</span>
  {/if}
</div>
```

```css
.presence-chip { display:flex; align-items:center; gap:4px; padding:2px 8px; border:1px solid var(--c); border-radius:2px; font-size:9px; }
.presence-chip[data-state="reasoning"] .chip-dot { animation:agent-pulse 1s ease-in-out infinite; }
.chip-dot { width:4px; height:4px; border-radius:50%; background:var(--c); }
```

### DET-4 🟠 Kanban — phase columns with gate indicators

Between each kanban column, add a gate separator:

```svelte
<!-- KanbanColumn.svelte -->
<div class="kanban-col">
  <div class="col-header">
    <span class="phase-label">{phase.name}</span>
    <span class="phase-number">P{phase.number}</span>
  </div>
  <div class="col-cards">
    <slot />
  </div>
</div>

<!-- GateSeparator between columns -->
<div class="gate-separator" data-gate={gate.pillar} data-state={gate.status}>
  <div class="gate-line" />
  <span class="gate-label">{gate.pillar}</span>
  <span class="gate-icon">{GATE_ICONS[gate.status]}</span>
  <div class="gate-line" />
</div>
```

```css
.gate-separator { display:flex; flex-direction:column; align-items:center; padding:0 8px; gap:4px; }
.gate-line { flex:1; width:1px; background:rgba(71,85,105,0.3); }
.gate-separator[data-state="passed"] .gate-label { color:var(--la-semantic-ok); }
.gate-separator[data-state="failed"] .gate-label { color:var(--la-semantic-error); }
.gate-separator[data-state="pending"] .gate-label { color:var(--la-text-dim); }
```

### DET-5 🟡 View tab strip — add descriptions on hover

```svelte
{#each ['kanban','list','operator','manifest','plan'] as view}
  <button
    class="view-tab"
    class:active={currentView === view}
    title={VIEW_DESCRIPTIONS[view]}
    on:click={() => goto(`/builds/${codename}/${view}`)}
  >
    {VIEW_ICONS[view]}
    <span>{view}</span>
  </button>
{/each}
```

```typescript
const VIEW_DESCRIPTIONS = {
  kanban:   'Phase board — visual progress through LASDLC phases',
  list:     'Flat list of all tasks across all phases',
  operator: 'Live operator console — agent I/O and tool telemetry',
  manifest: 'Build manifest YAML — config and metadata',
  plan:     'Full LASDLC plan document',
};
```

---

## S5 — HELIX / Knowledge Graph

**Eye-flow assigned**: F-pattern  
**Current verdict**: FAILS — helix animation occupies 50%+ as visual hero; search panel tiny; 0 results (ambiguous); no graph view; no faceted filters

### HEL-1 🔴 Rebalance layout: 25% helix / 75% search

```svelte
<!-- Helix.svelte layout -->
<div class="helix-layout">
  <aside class="helix-ambient" class:collapsed={helixCollapsed}>
    <HelixAnimation />
    <button class="collapse-btn" on:click={() => helixCollapsed = !helixCollapsed}>
      {helixCollapsed ? '▶' : '◀'}
    </button>
  </aside>
  <main class="helix-search">
    <HelixSearch />
  </main>
</div>
```

```css
.helix-layout { display: grid; grid-template-columns: 25% 1fr; height: 100%; }
.helix-ambient { position: relative; overflow: hidden; border-right: 1px solid rgba(71,85,105,0.2); }
.helix-ambient.collapsed { grid-column: none; width: 40px; }

/* Helix animation bleeds into search panel as ambient background */
.helix-search {
  background: linear-gradient(
    to right,
    rgba(10, 10, 15, 0.3) 0%,
    rgba(10, 10, 15, 0.95) 30%,
    var(--la-bg-base) 60%
  );
}
```

### HEL-2 🟠 Search panel — add faceted filters + empty state suggestions

```svelte
<div class="helix-search-panel">
  <header class="search-header">
    <h2>LA KNOWLEDGE GRAPH</h2>
    <div class="search-mode">
      <button class:active={mode==='hybrid'} on:click={()=>mode='hybrid'}>Hybrid</button>
      <button class:active={mode==='semantic'} on:click={()=>mode='semantic'}>Semantic</button>
      <button class:active={mode==='bm25'} on:click={()=>mode='bm25'}>Exact</button>
    </div>
  </header>

  <div class="search-input-row">
    <input bind:value={query} placeholder="Search memory, decisions, builds…" />
    <button on:click={search}>⌘↵</button>
  </div>

  <div class="facet-row">
    <select bind:value={typeFilter}>
      <option value="">All types</option>
      <option value="memory">Memory</option>
      <option value="decision">Decision</option>
      <option value="strand">Strand</option>
      <option value="build">Build</option>
    </select>
    <select bind:value={siblingFilter}>
      <option value="">All agents</option>
      {#each SIBLINGS as s}<option value={s.id}>{s.name}</option>{/each}
    </select>
  </div>

  {#if !query && results.length === 0}
    <!-- Empty state: suggested queries -->
    <div class="suggested-queries">
      <p class="suggest-label">TRY ASKING</p>
      {#each SUGGESTED_QUERIES as q}
        <button class="suggest-chip" on:click={() => { query = q; search(); }}>{q}</button>
      {/each}
    </div>
  {/if}

  <!-- Results with construct animation -->
  {#each results as result (result.id)}
    <div class="helix-result" transition:construct>
      <div class="result-header">
        <span class="result-type" data-type={result.type}>{result.type}</span>
        <span class="result-path">{result.path}</span>
        <span class="result-score">{(result.score * 100).toFixed(0)}%</span>
      </div>
      <p class="result-excerpt">{result.excerpt}</p>
    </div>
  {/each}
</div>
```

Suggested queries:
```typescript
const SUGGESTED_QUERIES = [
  'What did we decide about auth architecture?',
  'Latest CORSO security findings',
  'vault-migration-v1 decisions',
  'API design patterns we use',
  'Platform architecture v2 decisions',
  'EVA identity strand entries',
];
```

### HEL-3 🟡 Empty vs disconnected — differentiate states

```svelte
{#if searchState === 'disconnected'}
  <div class="empty-state error">
    <span class="icon">⚠</span>
    <p>HELIX is disconnected — cannot query knowledge graph.</p>
    <button on:click={reconnect}>Reconnect</button>
  </div>
{:else if searchState === 'empty' && query}
  <div class="empty-state">
    <span class="icon">◎</span>
    <p>No results for <strong>"{query}"</strong></p>
    <p class="sub">Try: broader terms, different type filter, or check spelling.</p>
  </div>
{:else if searchState === 'no-query'}
  <!-- Suggested queries above -->
{/if}
```

### HEL-4 🟡 Graph view toggle (Three.js node graph)

Add a "Graph" view button next to "List" that renders SOUL relationships as a Three.js force-directed graph:

```svelte
<div class="view-toggle">
  <button class:active={view==='list'} on:click={()=>view='list'}>≡ List</button>
  <button class:active={view==='graph'} on:click={()=>view='graph'}>⬡ Graph</button>
</div>

{#if view === 'graph'}
  <HelixGraph
    nodes={results}
    edges={relationships}
    onNodeClick={drillToEntry}
  />
{/if}
```

The Three.js graph: nodes = helix entries (sized by significance), edges = `:LINKS_TO` relationships (from `/api/soul/edges`), colored by sibling identity. On node click: executive summary card appears.

---

## S6 — Events Panel / S7 — Memory Panel

### EVT-1 🔴 Events panel — differentiate offline vs no-events

```svelte
<div class="events-panel">
  <header>
    <span class="panel-title">LIVE EVENTS</span>
    <span class="connection-badge" data-status={sseStatus}>
      {#if sseStatus === 'connected'}● LIVE{:else}● OFFLINE{/if}
    </span>
  </header>
  {#if sseStatus === 'disconnected'}
    <div class="events-empty error">
      <p>SSE stream disconnected.</p>
      <button on:click={reconnectSSE}>Reconnect</button>
    </div>
  {:else if events.length === 0}
    <div class="events-empty">
      <p>Listening for events…</p>
      <span class="listening-indicator" />
    </div>
  {:else}
    {#each events as event}
      <EventRow {event} />
    {/each}
  {/if}
</div>
```

```css
.listening-indicator {
  width: 8px; height: 8px; border-radius: 50%;
  background: var(--la-semantic-ok);
  display: inline-block; margin-left: 8px;
  animation: agent-pulse 2s ease-in-out infinite;
}
```

### MEM-1 🟠 Memory panel — type-coded entries + significance badges

```svelte
<div class="memory-entry" data-type={entry.type}>
  <div class="entry-header">
    <span class="type-badge" data-type={entry.type}>{entry.type}</span>
    {#if entry.significance >= 7.0}
      <span class="sig-badge">{entry.significance.toFixed(1)} ★</span>
    {/if}
    <span class="entry-date">{formatDate(entry.date)}</span>
  </div>
  <p class="entry-body">{entry.body}</p>
</div>
```

```css
.type-badge[data-type="user"]      { color: var(--la-struct-primary);  border-color: var(--la-struct-primary); }
.type-badge[data-type="feedback"]  { color: var(--la-semantic-warn);   border-color: var(--la-semantic-warn); }
.type-badge[data-type="project"]   { color: var(--la-semantic-ok);     border-color: var(--la-semantic-ok); }
.type-badge[data-type="reference"] { color: var(--la-text-label);      border-color: var(--la-text-label); }
.sig-badge { color: var(--la-semantic-warn); font-size: 9px; }
```

---

## Priority Implementation Order

### Sprint 1 — Structural (P0 blockers)
1. G-1: OFFLINE dominant state
2. BLD-1: Collapse dual-tier
3. BLD-2: Progress bar no-data state
4. DIS-1: Remove premature validation error
5. DET-1: Fix 8 console errors in Build Detail
6. HEL-1: Rebalance HELIX 25/75 split

### Sprint 2 — Utility elevation (P1 high-value)
7. OPS-1: Sidebar as hero, map as secondary
8. OPS-2: Squad pills with health state glow
9. **GIT-1: Git forest scene — core geometry** (Three.js, static git data)
10. **GIT-2: Gate status color system** (green/yellow/gold/red mapping)
11. **GIT-3: Agent worktree sub-branches** (identity color encoding)
12. DIS-2: Agent card selection states
13. DIS-3: DISPATCH button as Z-pattern terminal CTA
14. BLD-3: GateStrip (7 dots) on all cards
15. BLD-5: Active build left-border glow
16. BLD-6: Summary bar as clickable filters
17. DET-2: Working History structured entries
18. DET-3: Agent presence bar
19. HEL-2: Faceted search + suggested queries
20. EVT-1: Events panel offline vs no-events

### Sprint 3 — Stark aesthetic depth + live git (P2 polish)
21. G-2: Perspective grid (p5.js)
22. G-3: Scan-line shader on all panels
23. G-4: Targeting reticles on selection
24. G-5: Section label typography
25. G-6: Construct transition on card mount
26. **GIT-4: Agent write pulse animation** (SSE `git:file_write` → branch flash)
27. **GIT-5: Merged branch ghost persistence** (leaves fall, ghost until prune)
28. **GIT-6: Ahead-of-main upward particle trace**
29. OPS-3: Hexagon circuit traces + presence glow (Three.js) — or retired if git forest fully replaces
30. OPS-4: Executive summary card on hover
31. DIS-5: RAILS + DAG tooltips
32. DIS-6: Textarea focus state
33. DIS-7: Zone separator circuit traces
34. BLD-7: Drill-down zoom from card origin
35. DET-4: Kanban gate separators
36. DET-5: View tab descriptions
37. HEL-3: Graph view (Three.js force-directed)
38. MEM-1: Memory entry type coding

### Sprint 4 — Deep features (P2 depth)
39. **GIT-7: File leaf particles** — instanced mesh, file-type color coding
40. **GIT-8: SOUL helix cross-reference** — commits linked to helix entries, significance glow nodes

---

## Quick CSS Token Additions

Add these to the global CSS token file (`:root`):

```css
:root {
  /* Motion */
  --ease-project: cubic-bezier(0.16, 1, 0.3, 1);
  --ease-retract: cubic-bezier(0.7, 0, 0.84, 0);
  --ease-snap:    cubic-bezier(0.34, 1.56, 0.64, 1);

  /* Sibling identity */
  --la-id-soul:    #f59e0b;
  --la-id-eva:     #ec4899;
  --la-id-corso:   #3b82f6;
  --la-id-quantum: #8b5cf6;
  --la-id-seraph:  #ef4444;
  --la-id-ayin:    #f97316;
  --la-id-laex:    #eab308;

  /* Structural */
  --la-struct-primary:   #00c8ff;
  --la-struct-secondary: #2a6496;

  /* Semantic with glow */
  --la-semantic-ok:         #22c55e;
  --la-semantic-ok-glow:    0 0 8px rgba(34, 197, 94, 0.4);
  --la-semantic-ok-glow-hi: 0 0 20px rgba(34, 197, 94, 0.7);
  --la-semantic-warn:         #f59e0b;
  --la-semantic-warn-glow:    0 0 8px rgba(245, 158, 11, 0.4);
  --la-semantic-warn-glow-hi: 0 0 20px rgba(245, 158, 11, 0.7);
  --la-semantic-error:         #ef4444;
  --la-semantic-error-glow:    0 0 8px rgba(239, 68, 68, 0.4);
  --la-semantic-error-glow-hi: 0 0 20px rgba(239, 68, 68, 0.7);
  --la-semantic-active:  #a78bfa;
  --la-semantic-offline: #475569;

  /* Surfaces */
  --la-bg-base:     #0a0a0f;
  --la-bg-panel:    #0f1117;
  --la-bg-card:     #141820;
  --la-bg-elevated: #1a2030;

  /* Text */
  --la-text-bright: #f1f5f9;
  --la-text-label:  #94a3b8;
  --la-text-dim:    #475569;
  --la-text-code:   #00c8ff;
}
```

---

*Implementation note: Sprint 1 items can be completed without touching Three.js or p5.js — pure Svelte + CSS. Sprint 3 requires the p5.js grid layer and Three.js scene modifications. Tackle Sprint 1 + 2 first; the Stark aesthetic depth of Sprint 3 will land on a structurally sound foundation.*
