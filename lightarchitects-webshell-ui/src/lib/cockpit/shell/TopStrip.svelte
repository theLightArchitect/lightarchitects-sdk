<script lang="ts">
  import { ambient } from '$lib/cockpit/stores/ambient';
  import { scope } from '$lib/cockpit/stores/scope';

  const amb   = $derived($ambient);
  const scp   = $derived($scope);
  const depth = $derived(scp?.depth ?? 0);

  const DEPTH_LABEL: Record<number, string> = {
    0: 'PLATFORM', 1: 'PROJECT', 2: 'BUILD', 3: 'FILE',
  };

  function slotBar(used: number, cap: number): string {
    const pct = cap > 0 ? Math.min(1, used / cap) : 0;
    if (pct >= 0.9) return 'gauge-critical';
    if (pct >= 0.7) return 'gauge-warn';
    return 'gauge-ok';
  }

  function availDot(state: string): string {
    switch (state) {
      case 'online':    return 'dot-online';
      case 'saturated': return 'dot-saturated';
      case 'offline':   return 'dot-offline';
      default:          return 'dot-down';
    }
  }

  const SIBLINGS = ['CORSO', 'EVA', 'SOUL', 'QUANTUM', 'SERAPH', 'AYIN', 'LAEX'] as const;

  const costLabel = $derived(
    amb.cost_per_hour_usd !== null
      ? `$${amb.cost_per_hour_usd.toFixed(2)}/h`
      : '—',
  );
</script>

<header class="top-strip" role="banner" aria-label="Cockpit ambient strip">
  <!-- Scope breadcrumb -->
  <div class="strip-segment strip-scope">
    <span class="scope-depth-badge">{DEPTH_LABEL[depth]}</span>
    {#if scp && scp.depth >= 1 && 'project_id' in scp}
      <span class="scope-crumb">{scp.project_id}</span>
    {/if}
    {#if scp && scp.depth >= 2 && 'codename' in scp}
      <span class="scope-sep">›</span>
      <span class="scope-crumb">{scp.codename}</span>
    {/if}
    {#if scp && scp.depth === 3 && 'file_path' in scp}
      <span class="scope-sep">›</span>
      <span class="scope-crumb scope-crumb-file">{scp.file_path.split('/').at(-1)}</span>
    {/if}
  </div>

  <!-- Slot economy gauge -->
  <div class="strip-segment strip-slots" title="Agent slot economy">
    <span class="gauge-label">W</span>
    <span class="gauge-val {slotBar(amb.slot_economy.write_used, amb.slot_economy.write_cap)}">
      {amb.slot_economy.write_used}/{amb.slot_economy.write_cap}
    </span>
    <span class="gauge-sep">·</span>
    <span class="gauge-label">R</span>
    <span class="gauge-val {slotBar(amb.slot_economy.read_used, amb.slot_economy.read_cap)}">
      {amb.slot_economy.read_used}/{amb.slot_economy.read_cap}
    </span>
    {#if amb.slot_economy.queue_depth > 0}
      <span class="gauge-queue">+{amb.slot_economy.queue_depth}q</span>
    {/if}
  </div>

  <!-- Sibling availability dots -->
  <div class="strip-segment strip-siblings" aria-label="Sibling availability">
    {#each SIBLINGS as sid}
      <span
        class="sib-dot {availDot(amb.sibling_availability[sid])}"
        title="{sid}: {amb.sibling_availability[sid]}"
        aria-label="{sid} {amb.sibling_availability[sid]}"
      ></span>
    {/each}
  </div>

  <!-- Cost ticker -->
  <div class="strip-segment strip-cost" title="Estimated LLM cost per hour">
    <span class="cost-label">{costLabel}</span>
  </div>

  <!-- Northstar mini — Phase 6 will replace with animated health bars -->
  <div class="strip-segment strip-northstar" aria-label="Northstar pulse">
    {#each (['P1','P2','P3'] as const) as pillar}
      <div class="ns-mini" title="Northstar {pillar}">
        <div
          class="ns-bar"
          style:height="{Math.round(amb.northstar_pulse[pillar] * 100)}%"
        ></div>
      </div>
    {/each}
  </div>

  <!-- Alert badge -->
  {#if amb.unread_alerts > 0}
    <div class="strip-segment strip-alerts">
      <span class="alert-badge">{amb.unread_alerts}</span>
    </div>
  {/if}
</header>

<style>
  .top-strip {
    display: flex;
    align-items: center;
    gap: 1rem;
    height: var(--cockpit-top-height, 56px);
    padding: 0 1rem;
    background: var(--scope-strip-bg, rgba(0,0,0,0.72));
    border-bottom: 1px solid var(--scope-strip-border, rgba(255,255,255,0.06));
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    font-size: 0.72rem;
    color: var(--text-muted, #777);
    user-select: none;
    overflow: hidden;
  }

  .strip-segment { display: flex; align-items: center; gap: 0.3rem; white-space: nowrap; }

  /* Scope breadcrumb */
  .scope-depth-badge {
    font-size: 0.65rem;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--scope-accent, var(--scope-d0));
    padding: 1px 5px;
    border: 1px solid var(--scope-accent, var(--scope-d0));
    border-radius: 2px;
  }
  .scope-crumb { color: var(--text-secondary, #aaa); }
  .scope-crumb-file { color: var(--scope-accent, var(--scope-d3)); }
  .scope-sep { opacity: 0.4; }

  /* Slot gauges */
  .gauge-label  { font-size: 0.6rem; opacity: 0.5; }
  .gauge-ok       { color: #7ed321; }
  .gauge-warn     { color: #f5a623; }
  .gauge-critical { color: #e040fb; animation: pulse-critical 1s ease-in-out infinite alternate; }
  .gauge-queue    { color: #f5a623; font-size: 0.65rem; }
  .gauge-sep      { opacity: 0.3; }

  /* Sibling dots */
  .sib-dot {
    width: 6px; height: 6px;
    border-radius: 50%;
    transition: background var(--motion-scope-fade, 200ms ease-out);
  }
  .dot-online    { background: #7ed321; }
  .dot-saturated { background: #f5a623; }
  .dot-offline   { background: rgba(255,255,255,0.15); }
  .dot-down      { background: #cf6679; }

  /* Cost ticker */
  .cost-label { color: var(--text-secondary, #aaa); }

  /* Northstar mini bars */
  .ns-mini {
    width: 4px; height: 14px;
    background: rgba(255,255,255,0.08);
    border-radius: 2px;
    overflow: hidden;
    display: flex;
    align-items: flex-end;
  }
  .ns-bar {
    width: 100%;
    min-height: 2px;
    background: var(--scope-accent, var(--scope-d0));
    border-radius: 2px;
    transition: height var(--motion-scope, 400ms cubic-bezier(0.4,0,0.2,1));
  }

  /* Alert badge */
  .alert-badge {
    background: #cf6679;
    color: #fff;
    font-size: 0.6rem;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 8px;
    min-width: 16px;
    text-align: center;
  }

  @keyframes pulse-critical {
    from { opacity: 1; }
    to   { opacity: 0.4; }
  }
</style>
