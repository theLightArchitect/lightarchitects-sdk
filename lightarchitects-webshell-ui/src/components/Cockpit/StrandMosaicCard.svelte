<!--
  @component
  StrandMosaicCard — project × gatekeeper risk matrix sourced from
  `GET /api/strand-mosaic?scope=platform` (§2.52).
  Cells: ●=ok ◑=warn ○=fail ·=na. RISK column on the right.
  Polls every 60 seconds.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, type MosaicRow, type GatekeeperOwners, type MosaicCell } from '$lib/api';

  const POLL_MS = 60_000;

  let rows: MosaicRow[] = $state([]);
  let owners: GatekeeperOwners | null = $state(null);
  let loading: boolean = $state(true);
  let error: string | null = $state(null);
  let timer: ReturnType<typeof setInterval> | null = null;

  const GLYPH: Record<MosaicCell, string> = {
    ok:   '●', // ●
    warn: '◑', // ◑
    fail: '○', // ○
    na:   '·', // ·
  };

  const COLUMNS: ReadonlyArray<keyof GateCellsRow> = ['a', 's', 'q', 't', 'p', 'd', 'k'];
  type GateCellsRow = MosaicRow['cells'];

  async function refresh() {
    try {
      const res = await api.getStrandMosaic('platform');
      rows = res.rows;
      owners = res.gatekeepers;
      error = null;
    } catch (err) {
      error = err instanceof Error ? err.message : 'mosaic fetch failed';
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    refresh();
    timer = setInterval(refresh, POLL_MS);
  });

  onDestroy(() => {
    if (timer !== null) clearInterval(timer);
  });

  function ownerFor(col: keyof GateCellsRow): string {
    if (!owners) return '';
    return owners[col];
  }

  const cellCounts = $derived.by(() => {
    let total = 0;
    let warns = 0;
    let fails = 0;
    for (const row of rows) {
      for (const col of COLUMNS) {
        const v = row.cells[col];
        total += 1;
        if (v === 'warn') warns += 1;
        else if (v === 'fail') fails += 1;
      }
    }
    return { total, warns, fails };
  });
</script>

<div class="sm-card">
  {#if loading && rows.length === 0}
    <div class="sm-empty">loading mosaic…</div>
  {:else if error && rows.length === 0}
    <div class="sm-error">mosaic unavailable — {error}</div>
  {:else if rows.length === 0}
    <div class="sm-empty">no projects registered</div>
  {:else}
    <div class="sm-grid" role="table" aria-label="Strand mosaic — projects by gatekeeper">
      <div class="sm-head" role="row">
        <div class="sm-h-label" role="columnheader">PROJECT</div>
        {#each COLUMNS as col (col)}
          <div class="sm-h" role="columnheader" title={ownerFor(col)}>{col.toUpperCase()}</div>
        {/each}
        <div class="sm-h-risk" role="columnheader">RISK</div>
      </div>

      {#each rows as row (row.id)}
        <div class="sm-row" role="row" data-id={row.id}>
          <div class="sm-label" role="cell">
            <span class="sm-label-name">{row.label}</span>
            <span class="sm-label-meta">{row.meta}</span>
          </div>
          {#each COLUMNS as col (col)}
            <div class="sm-cell sm-cell-{row.cells[col]}" role="cell">
              {GLYPH[row.cells[col]]}
            </div>
          {/each}
          <div class="sm-risk sm-risk-{row.risk.toLowerCase()}" role="cell">{row.risk}</div>
        </div>
      {/each}
    </div>

    <div class="sm-foot">
      <div class="sm-legend">
        <span><span class="sm-lg-ok">●</span> pass</span>
        <span><span class="sm-lg-warn">◑</span> warn</span>
        <span><span class="sm-lg-fail">○</span> fail</span>
        <span><span class="sm-lg-na">·</span> n/a</span>
      </div>
      <div class="sm-stat">
        <span>cells <strong>{cellCounts.total}</strong></span>
        <span>warns <strong>{cellCounts.warns}</strong></span>
        <span>fails <strong>{cellCounts.fails}</strong></span>
      </div>
    </div>
  {/if}
</div>

<style>
  .sm-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-family: var(--la-font-mono, monospace);
    min-height: 0;
  }

  .sm-empty, .sm-error {
    font-size: 9px;
    color: var(--la-text-mute);
    font-style: italic;
    padding: 14px 0;
    text-align: center;
  }
  .sm-error { color: var(--la-err, #ff4d6a); }

  .sm-grid {
    display: grid;
    grid-template-columns: minmax(120px, 1.6fr) repeat(7, 1fr) 50px;
    font-variant-numeric: tabular-nums;
  }

  .sm-head { display: contents; }
  .sm-h, .sm-h-label, .sm-h-risk {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-mute, rgba(255,255,255,0.28));
    padding: 4px 0 8px;
    border-bottom: 1px solid var(--la-hair-base, rgba(255,255,255,0.08));
    text-align: center;
    text-transform: uppercase;
  }
  .sm-h-label { text-align: left; padding-left: 4px; }
  .sm-h-risk  { text-align: right; padding-right: 4px; }
  .sm-h       { cursor: help; }

  .sm-row { display: contents; }
  .sm-row > * {
    border-bottom: 1px solid var(--la-hair-faint, rgba(255,255,255,0.04));
    height: 26px;
    display: flex;
    align-items: center;
  }
  .sm-row:hover > * { background: var(--la-hair-faint, rgba(255,255,255,0.04)); }

  .sm-label {
    padding-left: 4px;
    font-size: 10px;
    font-weight: 500;
    color: var(--la-text-base, rgba(255,255,255,0.8));
    gap: 6px;
  }
  .sm-label-name {
    color: var(--la-text-bright, rgba(255,255,255,0.95));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sm-label-meta {
    font-size: 8px;
    color: var(--la-text-mute, rgba(255,255,255,0.28));
    font-style: italic;
  }

  .sm-cell {
    justify-content: center;
    font-size: 14px;
    color: var(--la-text-mute, rgba(255,255,255,0.28));
  }
  .sm-cell-ok   { color: var(--la-ok, #39ff8a); }
  .sm-cell-warn { color: var(--la-warn, #ffad2e); }
  .sm-cell-fail { color: var(--la-err, #ff4d6a); }
  .sm-cell-na   { opacity: 0.32; }

  .sm-risk {
    justify-content: flex-end;
    padding-right: 4px;
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
  }
  .sm-risk-ok   { color: var(--la-text-dim, rgba(255,255,255,0.5)); }
  .sm-risk-low  { color: var(--la-text-dim, rgba(255,255,255,0.5)); }
  .sm-risk-med  { color: var(--la-warn, #ffad2e); }
  .sm-risk-high { color: var(--la-err, #ff4d6a); }
  .sm-risk-crit { color: var(--la-err, #ff4d6a); font-weight: 800; }

  .sm-foot {
    margin-top: 4px;
    padding-top: 8px;
    border-top: 1px solid var(--la-hair-faint, rgba(255,255,255,0.04));
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 9px;
    color: var(--la-text-mute, rgba(255,255,255,0.28));
  }
  .sm-legend { display: flex; gap: 12px; align-items: center; }
  .sm-legend span { display: flex; gap: 4px; align-items: center; }
  .sm-lg-ok   { color: var(--la-ok, #39ff8a); font-size: 11px; }
  .sm-lg-warn { color: var(--la-warn, #ffad2e); font-size: 11px; }
  .sm-lg-fail { color: var(--la-err, #ff4d6a); font-size: 11px; }
  .sm-lg-na   { opacity: 0.5; font-size: 11px; }
  .sm-stat { display: flex; gap: 12px; color: var(--la-text-dim, rgba(255,255,255,0.5)); }
  .sm-stat strong { color: var(--la-text-bright, rgba(255,255,255,0.95)); font-weight: 700; }
</style>
