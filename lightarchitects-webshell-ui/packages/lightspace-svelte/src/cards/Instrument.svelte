<!--
  @component Instrument — gate matrix OR context burn gauge.
  @reads content discriminated by instrument_kind:
         "gate_matrix"   → 10-column gate dimension grid with status colours.
         "context_burn"  → horizontal burn bar with rate-of-change indicator.
-->
<script lang="ts">
  import type { GateMatrixContent, ContextBurnContent } from '../types';

  let { data }: { data: unknown } = $props();
  const d = $derived(data as (GateMatrixContent | ContextBurnContent) | null);
  const kind = $derived(d?.instrument_kind ?? null);

  // ── Gate matrix helpers ───────────────────────────────────────────────────
  const GATE_COLOR: Record<string, string> = {
    pass:    'var(--ls-acc-green)',
    fail:    'var(--ls-acc-red)',
    running: 'var(--ls-acc)',
    pending: 'var(--ls-sunken)',
  };
  const gm = $derived(kind === 'gate_matrix' ? (d as GateMatrixContent) : null);
  const cb = $derived(kind === 'context_burn' ? (d as ContextBurnContent) : null);

  // ── Context burn helpers ──────────────────────────────────────────────────
  const burnPct    = $derived(cb ? Math.round((cb.current_pct ?? 0) * 100) : 0);
  const burnColor  = $derived(
    cb?.level === 'l3' ? 'var(--ls-acc-red)' :
    cb?.level === 'l2' ? 'var(--ls-acc-amber)' :
    'var(--ls-acc-green)',
  );
</script>

{#if gm}
  <div class="ls-inst-gate-grid">
    {#each gm.dimensions as dim}
      {@const cell = gm.cells?.[dim] ?? { status: 'pending' }}
      <div
        class="ls-inst-gate-cell"
        title="[{dim}] {cell.status}"
        style="--cell-color: {GATE_COLOR[cell.status] ?? 'var(--ls-border)'}"
      >
        <span class="ls-inst-gate-dim">{dim}</span>
        <span class="ls-inst-gate-dot"></span>
      </div>
    {/each}
  </div>
{:else if cb}
  <div class="ls-inst-burn-wrap">
    <div class="ls-inst-burn-header">
      <span class="ls-inst-burn-label">context</span>
      <span class="ls-inst-burn-pct" style="color: {burnColor}">{burnPct}%</span>
    </div>
    <div class="ls-inst-burn-track">
      <div class="ls-inst-burn-fill" style="width: {burnPct}%; background: {burnColor}"></div>
    </div>
    {#if cb.samples.length > 1}
      <div class="ls-inst-burn-rate">
        {#each cb.samples.slice(-4) as s}
          <span class="ls-inst-burn-sample" style="height: {Math.round(s.used / s.budget * 24)}px"></span>
        {/each}
      </div>
    {/if}
  </div>
{:else}
  <div class="ls-inst-empty">awaiting…</div>
{/if}

<style>
/* gate matrix */
.ls-inst-gate-grid { display: grid; grid-template-columns: repeat(5, 1fr); gap: 3px; }
.ls-inst-gate-cell { display: flex; flex-direction: column; align-items: center; gap: 2px; padding: 3px 2px; background: var(--ls-sunken); border: 1px solid var(--ls-border); }
.ls-inst-gate-dim  { font-size: 7px; color: var(--ls-text-mute); text-transform: uppercase; }
.ls-inst-gate-dot  { width: 6px; height: 6px; border-radius: 50%; background: var(--cell-color); }
/* context burn */
.ls-inst-burn-wrap { display: flex; flex-direction: column; gap: 5px; }
.ls-inst-burn-header { display: flex; justify-content: space-between; align-items: baseline; }
.ls-inst-burn-label { font-size: 8px; text-transform: uppercase; color: var(--ls-text-mute); }
.ls-inst-burn-pct   { font-size: 13px; font-family: var(--ls-font-display); font-weight: 700; }
.ls-inst-burn-track { height: 4px; background: var(--ls-sunken); border-radius: 2px; overflow: hidden; }
.ls-inst-burn-fill  { height: 100%; border-radius: 2px; transition: width 0.6s ease; }
.ls-inst-burn-rate  { display: flex; align-items: flex-end; gap: 2px; height: 24px; padding-top: 4px; border-top: 1px solid var(--ls-border); }
.ls-inst-burn-sample { width: 6px; background: var(--ls-acc); border-radius: 1px 1px 0 0; min-height: 2px; }
.ls-inst-empty { font-size: 9px; color: var(--ls-text-ghost); }
</style>
