<script lang="ts">
  import { activeBuild, findings } from '$lib/stores';
  import { QUALITY_GATE_COLORS } from '$lib/design-tokens';
  import type { Pillar, Finding } from '$lib/types';

  const PILLAR_LABEL: Record<Pillar, string> = {
    ARCH: 'Plan', SEC: 'Research', QUAL: 'Implement',
    PERF: 'Harden', TEST: 'Verify', DOC: 'Ship', OPS: 'Learn',
  };

  type SevFilter = 'all' | Finding['severity'];
  let sevFilter = $state<SevFilter>('all');
  let sortCol = $state<'pillar' | 'severity' | 'category' | 'title'>('severity');

  const SEV_ORDER = { critical: 0, error: 1, warning: 2, info: 3 } as const;

  let build = $derived($activeBuild);

  let rows = $derived.by((): Finding[] => {
    if (!build) return [];
    const base = $findings.filter(f => f.buildId === build!.id);
    const filtered = sevFilter === 'all' ? base : base.filter(f => f.severity === sevFilter);
    return [...filtered].sort((a, b) => {
      if (sortCol === 'severity') return (SEV_ORDER[a.severity] ?? 9) - (SEV_ORDER[b.severity] ?? 9);
      if (sortCol === 'pillar') return a.pillar.localeCompare(b.pillar);
      if (sortCol === 'category') return a.category.localeCompare(b.category);
      return a.title.localeCompare(b.title);
    });
  });

  function sevColor(sev: Finding['severity']): string {
    switch (sev) {
      case 'critical': return 'var(--la-agent-security)';
      case 'error':    return '#f97316';
      case 'warning':  return 'var(--la-agent-performance)';
      default:         return 'var(--la-text-mute)';
    }
  }

  const SEV_OPTIONS: SevFilter[] = ['all', 'critical', 'error', 'warning', 'info'];
</script>

<div class="list-wrap" data-testid="list-view">
  {#if !build}
    <div class="list-empty">— no build selected —</div>
  {:else}
    <!-- Filter bar -->
    <div class="list-filter-bar">
      <span class="filter-label">SEVERITY</span>
      {#each SEV_OPTIONS as opt}
        <button
          class="filter-chip"
          class:active={sevFilter === opt}
          onclick={() => { sevFilter = opt; }}
        >{opt.toUpperCase()}</button>
      {/each}
      <span class="filter-count">{rows.length} finding{rows.length !== 1 ? 's' : ''}</span>
    </div>

    <!-- Table -->
    {#if rows.length === 0}
      <div class="list-empty">— no findings match filter —</div>
    {:else}
      <div class="list-scroll">
        <table class="findings-table">
          <thead>
            <tr>
              <th onclick={() => { sortCol = 'pillar'; }}    class:sorted={sortCol === 'pillar'}>PILLAR</th>
              <th onclick={() => { sortCol = 'severity'; }}  class:sorted={sortCol === 'severity'}>SEV</th>
              <th onclick={() => { sortCol = 'category'; }}  class:sorted={sortCol === 'category'}>CATEGORY</th>
              <th onclick={() => { sortCol = 'title'; }}     class:sorted={sortCol === 'title'}>TITLE</th>
              <th>LOCATION</th>
            </tr>
          </thead>
          <tbody>
            {#each rows as row (row.id)}
              <tr>
                <td>
                  <span class="pillar-dot"
                    style="background: {QUALITY_GATE_COLORS[row.pillar] ?? '#666'}"
                  ></span>
                  {PILLAR_LABEL[row.pillar] ?? row.pillar}
                </td>
                <td>
                  <span class="sev-badge" style="color: {sevColor(row.severity)}">
                    {row.severity.toUpperCase()}
                  </span>
                </td>
                <td class="td-cat">{row.category}</td>
                <td class="td-title">{row.title}</td>
                <td class="td-loc">
                  {#if row.file}
                    <span class="loc-file">{row.file}{row.line ? `:${row.line}` : ''}</span>
                  {:else}
                    <span class="td-none">—</span>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/if}
</div>

<style>
  .list-wrap {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .list-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--la-text-mute);
    font-size: 11px;
    letter-spacing: 0.12em;
    font-style: italic;
  }

  .list-filter-bar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--la-hair-faint);
    flex-shrink: 0;
    background: var(--la-bg-base);
  }

  .filter-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.14em;
    color: var(--la-text-mute);
    margin-right: 4px;
  }

  .filter-chip {
    background: transparent;
    border: 1px solid var(--la-hair-base);
    color: var(--la-text-mute);
    font-family: inherit;
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    padding: 2px 7px;
    cursor: pointer;
    transition: border-color 80ms, color 80ms;
  }
  .filter-chip:hover { border-color: var(--la-hair-strong); color: var(--la-text-base); }
  .filter-chip.active {
    border-color: var(--la-focus-ring);
    color: var(--la-focus-ring);
    background: color-mix(in srgb, var(--la-focus-ring) 8%, transparent);
  }

  .filter-count {
    margin-left: auto;
    font-size: 9px;
    color: var(--la-text-mute);
    font-variant-numeric: tabular-nums;
  }

  .list-scroll {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    scrollbar-width: thin;
    scrollbar-color: var(--la-hair-base) transparent;
  }

  .findings-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 11px;
  }

  thead th {
    position: sticky;
    top: 0;
    background: var(--la-bg-base);
    padding: 6px 12px;
    text-align: left;
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--la-text-mute);
    border-bottom: 1px solid var(--la-hair-base);
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
    transition: color 80ms;
  }
  thead th:hover { color: var(--la-text-base); }
  thead th.sorted { color: var(--la-focus-ring); }

  tbody tr {
    border-bottom: 1px solid var(--la-hair-faint);
    transition: background 80ms;
  }
  tbody tr:hover { background: var(--la-bg-elev-1); }

  tbody td {
    padding: 6px 12px;
    vertical-align: middle;
    color: var(--la-text-dim);
  }

  .pillar-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    margin-right: 6px;
    vertical-align: middle;
    flex-shrink: 0;
  }

  .sev-badge {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
  }

  .td-cat {
    color: var(--la-text-mute);
    font-size: 10px;
    text-transform: capitalize;
  }

  .td-title { color: var(--la-text-base); }

  .td-loc { white-space: nowrap; }

  .loc-file {
    font-size: 9px;
    color: var(--la-text-mute);
    font-variant-numeric: tabular-nums;
  }

  .td-none { color: var(--la-text-mute); opacity: 0.4; }

  @supports not (color: color-mix(in srgb, red 50%, blue)) {
    .filter-chip.active { background: rgba(240, 192, 64, 0.06); }
  }
</style>
