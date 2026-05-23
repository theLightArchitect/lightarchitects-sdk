<script lang="ts">
  import type { StreamRow as ESRow } from './EventStream.svelte';

  interface Props {
    rows: ESRow[];
    maxDisplay?: number;
  }

  let { rows, maxDisplay = 80 }: Props = $props();

  type CommType = 'wave' | 'gate' | 'hitl' | 'pr' | 'sys';

  interface CommRow extends ESRow {
    commType: CommType;
  }

  const TYPE_ICON: Record<CommType, string> = {
    wave: '◈',
    gate: '✓',
    hitl: '⚠',
    pr:   '⤴',
    sys:  '·',
  };

  const TYPE_COLOR: Record<CommType, string> = {
    wave: '#f5a623',
    gate: '#22c55e',
    hitl: '#f87171',
    pr:   '#38bdf8',
    sys:  '#334155',
  };

  const TYPE_LABEL: Record<CommType, string> = {
    wave: 'WAVE',
    gate: 'GATE',
    hitl: 'HITL',
    pr:   'PR',
    sys:  'SYS',
  };

  function classify(row: ESRow): CommType {
    const t = (row.text + ' ' + row.source).toLowerCase();
    if (t.includes('hitl') || t.includes('human-in-the-loop') || row.severity === 'warn') return 'hitl';
    if (t.includes('pull request') || t.includes('github_create_pr') || t.includes(' pr ') || t.includes('merge')) return 'pr';
    if (t.includes('gate') || t.includes('gate pass') || row.severity === 'ok') return 'gate';
    if (t.includes('wave') || t.includes('task') || t.includes('dispatch') || row.source === 'copilot') return 'wave';
    return 'sys';
  }

  let activeFilter = $state<CommType | 'all'>('all');

  let classified = $derived.by((): CommRow[] =>
    rows.slice(0, maxDisplay).map(r => ({ ...r, commType: classify(r) }))
  );

  let filtered = $derived.by((): CommRow[] =>
    activeFilter === 'all' ? classified : classified.filter(r => r.commType === activeFilter)
  );

  const FILTERS: Array<CommType | 'all'> = ['all', 'hitl', 'wave', 'gate', 'pr'];

  function filterCount(t: CommType | 'all'): number {
    if (t === 'all') return classified.length;
    return classified.filter(r => r.commType === t).length;
  }
</script>

<div class="comms-panel">
  <!-- Filter tabs -->
  <div class="filter-bar" role="toolbar" aria-label="Filter agent comms">
    {#each FILTERS as f}
      {@const count = filterCount(f)}
      <button
        class="filter-btn"
        class:active={activeFilter === f}
        class:hitl-filter={f === 'hitl' && count > 0}
        onclick={() => { activeFilter = f; }}
        aria-pressed={activeFilter === f}
        title={f === 'all' ? 'All events' : TYPE_LABEL[f as CommType]}
      >
        {#if f !== 'all'}
          <span class="filter-icon" style="color:{TYPE_COLOR[f as CommType]}">{TYPE_ICON[f as CommType]}</span>
        {/if}
        {f === 'all' ? 'ALL' : TYPE_LABEL[f as CommType]}
        {#if count > 0}
          <span class="filter-count" class:hitl-count={f === 'hitl'}>{count}</span>
        {/if}
      </button>
    {/each}
  </div>

  <!-- Event list -->
  <div class="comms-list">
    {#if filtered.length === 0}
      <div class="comms-empty">— no events yet —</div>
    {:else}
      {#each filtered as row (row.ts + row.text)}
        <div
          class="comm-row"
          class:comm-hitl={row.commType === 'hitl'}
          class:comm-gate={row.commType === 'gate'}
          class:comm-pr={row.commType === 'pr'}
        >
          <span
            class="comm-icon"
            style="color:{TYPE_COLOR[row.commType]}"
            title={TYPE_LABEL[row.commType]}
          >{TYPE_ICON[row.commType]}</span>
          <span class="comm-time">{row.time.slice(0, 5)}</span>
          <span class="comm-source" style="color:{row.color}">{row.source.slice(0, 10)}</span>
          <span class="comm-text" class:comm-text-warn={row.severity === 'warn'} class:comm-text-ok={row.severity === 'ok'}>{row.text}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .comms-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    font-family: var(--la-font-chrome, 'JetBrains Mono', monospace);
  }

  /* ── Filter bar ── */
  .filter-bar {
    display: flex;
    gap: 1px;
    padding: 3px 6px;
    background: var(--la-bg-elev-1, #111214);
    border-bottom: 1px solid var(--la-hair-base);
    flex-shrink: 0;
    flex-wrap: wrap;
  }

  .filter-btn {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 7px;
    font-weight: 700;
    font-family: var(--la-font-chrome, 'JetBrains Mono', monospace);
    letter-spacing: 0.08em;
    color: var(--la-text-mute);
    background: none;
    border: 1px solid transparent;
    border-radius: 3px;
    padding: 2px 5px;
    cursor: pointer;
    transition: color 80ms, border-color 80ms, background 80ms;
    white-space: nowrap;
  }
  .filter-btn:hover { color: var(--la-text-dim); border-color: var(--la-hair-base); }
  .filter-btn.active {
    color: var(--la-text-base);
    background: var(--la-bg-elev-2, #0a1520);
    border-color: var(--la-hair-strong);
  }
  .filter-btn.hitl-filter {
    border-color: rgba(248, 113, 113, 0.3);
    animation: hitl-filter-pulse 2s infinite;
  }
  @keyframes hitl-filter-pulse {
    0%, 100% { border-color: rgba(248,113,113,0.3); }
    50%       { border-color: rgba(248,113,113,0.7); }
  }

  .filter-icon { font-size: 8px; }

  .filter-count {
    font-size: 6px;
    background: var(--la-bg-elev-2, #0a1520);
    border: 1px solid var(--la-hair-base);
    border-radius: 2px;
    padding: 0 3px;
    font-variant-numeric: tabular-nums;
  }
  .hitl-count {
    color: #f87171;
    border-color: rgba(248, 113, 113, 0.4);
    background: rgba(248, 113, 113, 0.08);
  }

  /* ── Event list ── */
  .comms-list {
    flex: 1;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .comms-list::-webkit-scrollbar { display: none; }

  .comms-empty {
    padding: 16px 10px;
    font-size: 8px;
    color: var(--la-text-mute);
    text-align: center;
    letter-spacing: 0.04em;
  }

  .comm-row {
    display: grid;
    grid-template-columns: 12px 38px 52px 1fr;
    gap: 4px;
    align-items: baseline;
    padding: 3px 8px;
    border-left: 2px solid transparent;
    transition: background 60ms;
  }
  .comm-row:hover { background: rgba(255,255,255,0.025); }

  .comm-hitl {
    border-left-color: rgba(248, 113, 113, 0.6);
    background: rgba(248, 113, 113, 0.04);
    animation: hitl-row-pulse 3s infinite;
  }
  @keyframes hitl-row-pulse {
    0%, 100% { background: rgba(248,113,113,0.04); }
    50%       { background: rgba(248,113,113,0.08); }
  }

  .comm-gate { border-left-color: rgba(34, 197, 94, 0.4); }
  .comm-pr   { border-left-color: rgba(56, 189, 248, 0.4); }

  .comm-icon {
    font-size: 9px;
    text-align: center;
    font-weight: 700;
    flex-shrink: 0;
  }

  .comm-time {
    font-size: 7px;
    color: var(--la-text-mute);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .comm-source {
    font-size: 7px;
    font-weight: 600;
    letter-spacing: 0.04em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .comm-text {
    font-size: 7px;
    color: var(--la-text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    line-height: 1.4;
  }
  .comm-text-warn { color: #f59e0b; }
  .comm-text-ok   { color: #22c55e; }
</style>
