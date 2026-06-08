<script lang="ts">
  // Lightspace — platform intro + workspace shell.
  // Orchestrates the lobby → materialize → 3-panel workspace transition.
  // State lives in ls (state.svelte.ts); each panel is a focused subcomponent.

  import { ls } from '$lib/lightspace/state.svelte';
  import { startDemo } from '$lib/lightspace/demo';

  import Lobby      from './lightspace/Lobby.svelte';
  import Rail       from './lightspace/Rail.svelte';
  import Schematic  from './lightspace/Schematic.svelte';
  import Canvas     from './lightspace/Canvas.svelte';
  import StatusRail from './lightspace/StatusRail.svelte';

  // Boot the demo timeline once — replaced by SSE wiring in production.
  $effect(() => startDemo());
</script>

<!-- Root shell: grid + modifier class state machine -->
<div class={ls.rootClass}>

  <!-- Scan-line overlay (CSS ::before on .la-root provides this via global) -->

  <!-- Top bar ─────────────────────────────────────────────────────────────── -->
  <header class="la-topbar">
    <span class="la-tb-brand">Light<span class="la-acc">space</span></span>
    <span class="la-tb-sep">·</span>
    <span class="la-tb-crumb">session <b>{ls.sessionId}</b></span>
    <span class="la-tb-sep">·</span>
    <span class="la-tb-crumb">cwd <b>{ls.cwd}</b></span>

    <div class="la-tb-mid">
      <span class="la-tb-pill acc">
        <span class="dot"></span>workspace <b>{ls.wsState}</b>
      </span>
      <span class="la-tb-pill">
        <span class="dot"></span>provider <b>anthropic/claude-opus-4-7</b>
      </span>
      <span class="la-tb-pill warn">
        <span class="dot"></span>budget <b>${ls.budget.toFixed(2)} / $5.00</b>
      </span>
    </div>

    <div class="la-tb-right">
      <button class="la-tb-btn" onclick={() => ls.railCollapsed  = !ls.railCollapsed}>Rail</button>
      <button class="la-tb-btn" onclick={() => ls.schemCollapsed = !ls.schemCollapsed}>Schematic</button>
    </div>
  </header>

  <!-- Lobby overlay ────────────────────────────────────────────────────────── -->
  <Lobby />

  <!-- Materialize phase indicator ──────────────────────────────────────────── -->
  <div class="la-materialize-indicator" aria-live="polite" aria-label="Workspace materializing">
    <div class="la-mat-ring"></div>
    <div class="la-mat-phases">
      {#each (['begin','rail_collapsed','grid_revealed','drawer_revealed','cards_streaming','complete'] as const) as phase}
        <div class="la-mat-phase"
             class:is-done={ls.matPhasesSeen.has(phase) && ls.matPhase !== phase}
             class:is-active={ls.matPhase === phase}>
          <span class="la-mat-dot"></span>
          {phase.replace(/_/g, ' ')}
        </div>
      {/each}
    </div>
  </div>

  <!-- 3-panel workspace ────────────────────────────────────────────────────── -->
  <Rail />
  <Schematic />
  <Canvas />
  <StatusRail />

</div>

<style>
/* ════════════════════════════════════════════════════════════════════
 *  Google Fonts (same set as mockup)
 * ════════════════════════════════════════════════════════════════════ */
@import url('https://fonts.googleapis.com/css2?family=Azeret+Mono:ital,wght@0,300;0,400;0,500;0,600;0,700;1,400&family=Syne:wght@700;800&family=EB+Garamond:ital,wght@0,400;0,500;1,400&display=swap');

/* ════════════════════════════════════════════════════════════════════
 *  Design tokens — injected as :root so subcomponents inherit them
 * ════════════════════════════════════════════════════════════════════ */
:root {
  --la-bg-base:     #07080f;
  --la-bg-panel:    #0d0f18;
  --la-bg-card:     #111420;
  --la-bg-sunken:   #0a0c14;
  --la-bg-elev:     #181c2a;
  --la-hair-faint:  rgba(255,255,255,0.04);
  --la-hair-base:   rgba(255,255,255,0.08);
  --la-hair-strong: rgba(255,255,255,0.15);
  --la-hair-accent: rgba(77,142,255,0.28);
  --la-text-bright: rgba(255,255,255,0.95);
  --la-text-base:   rgba(255,255,255,0.80);
  --la-text-dim:    rgba(255,255,255,0.50);
  --la-text-mute:   rgba(255,255,255,0.28);
  --la-text-ghost:  rgba(255,255,255,0.14);

  --la-ok:    #39ff8a;
  --la-err:   #ff4d6a;
  --la-warn:  #ffad2e;
  --la-info:  #8aa9ff;
  --la-acc:   #4d8eff;
  --la-acc2:  #a98aff;
  --la-acc3:  #ffd166;

  --la-font-mono:    'Azeret Mono', 'JetBrains Mono', ui-monospace, monospace;
  --la-font-display: 'Syne', sans-serif;
  --la-font-serif:   'EB Garamond', Georgia, serif;

  --la-tk-loose: 0.12em;
  --la-tk-mid:   0.07em;
  --la-tk-tight: 0.04em;

  --la-fast: 0.15s ease;
  --la-mid:  0.28s cubic-bezier(0.4,0,0.2,1);
  --la-slow: 0.55s cubic-bezier(0.4,0,0.2,1);

  --la-rail-w:               320px;
  --la-rail-w-collapsed:     56px;
  --la-schematic-w:          380px;
  --la-schematic-w-collapsed: 44px;
}

/* ════════════════════════════════════════════════════════════════════
 *  Root shell — 3-row × 3-column grid
 * ════════════════════════════════════════════════════════════════════ */
:global(.la-root) {
  display: grid;
  grid-template-areas:
    "topbar  topbar    topbar"
    "rail    schematic canvas"
    "status  status    status";
  grid-template-rows: 38px 1fr 30px;
  grid-template-columns: var(--la-rail-w) var(--la-schematic-w) 1fr;
  height: 100vh;
  background: var(--la-bg-base);
  color: var(--la-text-base);
  font-family: var(--la-font-mono);
  font-size: 12px;
  letter-spacing: var(--la-tk-tight);
  overflow: hidden;
  -webkit-font-smoothing: antialiased;
  font-variant-numeric: tabular-nums;
  transition: grid-template-columns var(--la-mid);
  position: relative;
}

/* Scan-line overlay */
:global(.la-root)::before {
  content: "";
  position: fixed; inset: 0; pointer-events: none; z-index: 100;
  background:
    repeating-linear-gradient(0deg, transparent 0, transparent 2px, rgba(0,0,0,0.18) 2px, rgba(0,0,0,0.18) 3px),
    radial-gradient(ellipse at 50% -10%, rgba(77,142,255,0.04), transparent 60%);
  mix-blend-mode: overlay; opacity: 0.55;
}

/* Column collapse variants */
:global(.la-root.rail-collapsed) {
  grid-template-columns: var(--la-rail-w-collapsed) var(--la-schematic-w) 1fr;
}
:global(.la-root.schematic-collapsed) {
  grid-template-columns: var(--la-rail-w) var(--la-schematic-w-collapsed) 1fr;
}
:global(.la-root.rail-collapsed.schematic-collapsed) {
  grid-template-columns: var(--la-rail-w-collapsed) var(--la-schematic-w-collapsed) 1fr;
}

/* Lobby: hide workspace panels, reveal lobby overlay */
:global(.la-root.in-lobby) .la-topbar,
:global(.la-root.in-lobby .la-rail),
:global(.la-root.in-lobby .la-schematic),
:global(.la-root.in-lobby .la-canvas),
:global(.la-root.in-lobby .la-statusrail) { opacity: 0; pointer-events: none; }
:global(.la-root.in-lobby .la-lobby)      { opacity: 1 !important; pointer-events: auto !important; }

/* Materializing: dim canvas + schematic while spinner runs */
:global(.la-root.materializing .la-canvas),
:global(.la-root.materializing .la-schematic) { opacity: 0.18; }
:global(.la-root.materializing) .la-materialize-indicator { display: flex; }

/* ════════════════════════════════════════════════════════════════════
 *  Top bar
 * ════════════════════════════════════════════════════════════════════ */
.la-topbar {
  grid-area: topbar;
  display: flex; align-items: center; gap: 12px; padding: 0 14px;
  border-bottom: 1px solid var(--la-hair-base);
  background: linear-gradient(180deg, var(--la-bg-panel) 0%, var(--la-bg-base) 100%);
  font-size: 10px; letter-spacing: var(--la-tk-mid);
  text-transform: uppercase; color: var(--la-text-dim);
  transition: opacity var(--la-mid);
}
.la-tb-brand {
  font-family: var(--la-font-display); font-weight: 700;
  font-size: 13px; letter-spacing: var(--la-tk-loose);
  color: var(--la-text-bright); text-transform: uppercase;
}
.la-tb-brand .la-acc { color: var(--la-acc); }
.la-tb-sep { color: var(--la-text-ghost); }
.la-tb-crumb { font-size: 9px; }
.la-tb-crumb b { color: var(--la-text-base); font-weight: 500; }
.la-tb-mid { flex: 1; display: flex; gap: 16px; justify-content: center; }
.la-tb-pill {
  display: inline-flex; align-items: center; gap: 5px;
  padding: 2px 8px; border: 1px solid var(--la-hair-base);
  border-radius: 2px; font-size: 9px; background: var(--la-bg-sunken);
}
.la-tb-pill .dot {
  width: 5px; height: 5px; border-radius: 50%;
  background: var(--la-ok); box-shadow: 0 0 6px var(--la-ok);
}
.la-tb-pill.warn .dot { background: var(--la-warn); box-shadow: 0 0 6px var(--la-warn); }
.la-tb-pill.acc  .dot { background: var(--la-acc);  box-shadow: 0 0 6px var(--la-acc); }
.la-tb-right { display: flex; gap: 8px; align-items: center; }
.la-tb-btn {
  background: transparent; border: 1px solid var(--la-hair-base);
  color: var(--la-text-dim); font-family: var(--la-font-mono);
  font-size: 9px; letter-spacing: var(--la-tk-mid); text-transform: uppercase;
  padding: 3px 8px; cursor: pointer; border-radius: 2px;
  transition: all var(--la-fast);
}
.la-tb-btn:hover { color: var(--la-text-bright); border-color: var(--la-acc); }

/* ════════════════════════════════════════════════════════════════════
 *  Materialize indicator (shown during lobby→workspace transition)
 * ════════════════════════════════════════════════════════════════════ */
.la-materialize-indicator {
  position: fixed; top: 50%; left: 50%;
  transform: translate(-50%, -50%);
  display: none; flex-direction: column; align-items: center; gap: 14px;
  z-index: 80; pointer-events: none;
  font-family: var(--la-font-mono); font-size: 9px;
  letter-spacing: var(--la-tk-loose); text-transform: uppercase;
  color: var(--la-text-dim);
}
.la-mat-ring {
  width: 72px; height: 72px;
  border: 1px solid var(--la-hair-base); border-top-color: var(--la-acc);
  border-radius: 50%; animation: la-spin 1s linear infinite;
}
@keyframes la-spin { from { transform: rotate(0); } to { transform: rotate(360deg); } }

.la-mat-phases { display: flex; flex-direction: column; gap: 4px; align-items: center; }
.la-mat-phase {
  display: flex; align-items: center; gap: 7px;
  opacity: 0.3; transition: opacity var(--la-mid), color var(--la-mid);
}
.la-mat-phase.is-done   { opacity: 1; color: var(--la-ok); }
.la-mat-phase.is-active { opacity: 1; color: var(--la-text-bright); }
.la-mat-dot { width: 4px; height: 4px; border-radius: 50%; background: var(--la-hair-strong); }
.la-mat-phase.is-done   .la-mat-dot { background: var(--la-ok); box-shadow: 0 0 4px var(--la-ok); }
.la-mat-phase.is-active .la-mat-dot { background: var(--la-acc); box-shadow: 0 0 6px var(--la-acc); animation: la-pulse 1s infinite; }
@keyframes la-pulse { 50% { transform: scale(1.4); } }
</style>
