<script lang="ts">
  import { a2aFeedStore } from '$lib/a2aFeed';
  import type { A2aEnvelopeTypeWire } from '$lib/types';

  interface Props {
    /** If set, only show events for this codename. */
    codename?: string | null;
  }

  let { codename = null }: Props = $props();

  // ── Envelope type helpers ─────────────────────────────────────────────────

  type EnvKind = 'task_start' | 'task_complete_ok' | 'task_complete_fail' | 'task_escalated' | 'wave_complete';

  function kindOf(et: A2aEnvelopeTypeWire): EnvKind {
    if (et === 'task_start') return 'task_start';
    if (et === 'task_escalated') return 'task_escalated';
    if (et === 'wave_complete') return 'wave_complete';
    if (typeof et === 'object' && 'task_complete' in et) {
      return et.task_complete.success ? 'task_complete_ok' : 'task_complete_fail';
    }
    return 'task_start';
  }

  const KIND_LABEL: Record<EnvKind, string> = {
    task_start:       'START',
    task_complete_ok: 'DONE',
    task_complete_fail: 'FAIL',
    task_escalated:   'HITL',
    wave_complete:    'WAVE',
  };

  const KIND_COLOR: Record<EnvKind, string> = {
    task_start:       '#38bdf8',   // sky
    task_complete_ok: '#22c55e',   // green
    task_complete_fail: '#f87171', // rose
    task_escalated:   '#f5a623',   // amber
    wave_complete:    '#2dd4bf',   // teal
  };

  const KIND_ICON: Record<EnvKind, string> = {
    task_start:       '▶',
    task_complete_ok: '✓',
    task_complete_fail: '✗',
    task_escalated:   '⚠',
    wave_complete:    '◈',
  };

  // ── Filter state ─────────────────────────────────────────────────────────

  type FilterKind = EnvKind | 'all';
  let activeFilter = $state<FilterKind>('all');

  // Per-codename filter (from prop)
  let selectedCodename = $state<string | null>(codename ?? null);

  // ── Derived feed ─────────────────────────────────────────────────────────

  let allEvents = $derived.by(() => {
    const map = $a2aFeedStore;
    if (selectedCodename) return map.get(selectedCodename) ?? [];
    // merge all buckets sorted by timestamp (string ISO — lexicographic sort is correct)
    const merged = [...map.values()].flat();
    merged.sort((a, b) => a.timestamp.localeCompare(b.timestamp));
    return merged;
  });

  let filtered = $derived.by(() =>
    activeFilter === 'all'
      ? allEvents
      : allEvents.filter(e => kindOf(e.envelope_type) === activeFilter)
  );

  let codenames = $derived.by(() => [...$a2aFeedStore.keys()].sort());

  // ── Auto-scroll ───────────────────────────────────────────────────────────

  let autoScroll = $state(true);
  let listEl: HTMLDivElement | undefined = $state();

  $effect(() => {
    if (autoScroll && listEl && filtered.length > 0) {
      listEl.lastElementChild?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }
  });

  function onScroll() {
    if (!listEl) return;
    const { scrollTop, scrollHeight, clientHeight } = listEl;
    autoScroll = scrollHeight - scrollTop - clientHeight < 40;
  }

  // ── Timestamp formatting ──────────────────────────────────────────────────

  function fmtTime(iso: string): string {
    // "2026-01-15T12:34:56Z" → "12:34:56"
    return iso.slice(11, 19);
  }
</script>

<div class="a2a-panel">
  <!-- Codename selector (only if multiple codenames) -->
  {#if codenames.length > 1 || (codenames.length === 1 && !codename)}
    <div class="codename-bar" role="toolbar" aria-label="Filter by codename">
      <button
        class="cn-btn"
        class:active={selectedCodename === null}
        onclick={() => { selectedCodename = null; }}
      >ALL</button>
      {#each codenames as cn}
        <button
          class="cn-btn"
          class:active={selectedCodename === cn}
          onclick={() => { selectedCodename = cn; }}
          title={cn}
        >{cn.slice(0, 18)}</button>
      {/each}
    </div>
  {/if}

  <!-- Envelope-type filter bar -->
  <div class="filter-bar" role="toolbar" aria-label="Filter envelope types">
    {#each (['all', 'task_start', 'task_complete_ok', 'task_complete_fail', 'task_escalated', 'wave_complete'] as const) as f}
      <button
        class="filter-btn"
        class:active={activeFilter === f}
        onclick={() => { activeFilter = f; }}
        aria-pressed={activeFilter === f}
      >
        {#if f !== 'all'}
          <span class="f-icon" style="color:{KIND_COLOR[f]}">{KIND_ICON[f]}</span>
        {/if}
        {f === 'all' ? 'ALL' : KIND_LABEL[f]}
      </button>
    {/each}
  </div>

  <!-- Event list -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="feed-list"
    role="log"
    aria-live="polite"
    aria-label="A2A envelope feed"
    bind:this={listEl}
    onscroll={onScroll}
  >
    {#if filtered.length === 0}
      <div class="feed-empty">— no envelope events yet —</div>
    {:else}
      {#each filtered as ev (ev.codename + '/' + ev.task_id + '/' + ev.timestamp)}
        {@const kind = kindOf(ev.envelope_type)}
        <div class="feed-row" class:feed-hitl={kind === 'task_escalated'}>
          <span class="fe-badge" style="color:{KIND_COLOR[kind]};border-color:{KIND_COLOR[kind]}">{KIND_LABEL[kind]}</span>
          <span class="fe-time">{fmtTime(ev.timestamp)}</span>
          <span class="fe-cn" title={ev.codename}>{ev.codename.slice(0, 16)}</span>
          <span class="fe-task" title={ev.task_id}>T:{ev.task_id.slice(0, 10)}</span>
          <span class="fe-wave">W{ev.wave}</span>
          <span class="fe-summary">{ev.payload_summary}</span>
        </div>
      {/each}
    {/if}
  </div>

  <!-- Auto-scroll toggle -->
  <div class="feed-footer">
    <button
      class="autoscroll-btn"
      class:active={autoScroll}
      onclick={() => { autoScroll = !autoScroll; }}
      title="Toggle auto-scroll"
    >{autoScroll ? '⏬ auto' : '⏸ paused'}</button>
    <span class="feed-count">{filtered.length} events</span>
  </div>
</div>

<style>
  .a2a-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    font-family: var(--la-font-chrome, 'JetBrains Mono', monospace);
    font-size: 11px;
  }

  /* ── Codename bar ── */
  .codename-bar {
    display: flex;
    gap: 2px;
    padding: 3px 6px;
    background: var(--la-bg-elev-1, #111214);
    border-bottom: 1px solid var(--la-hair-base, #1e2128);
    flex-shrink: 0;
    overflow-x: auto;
  }

  .cn-btn {
    padding: 1px 6px;
    border: 1px solid transparent;
    border-radius: 3px;
    background: transparent;
    color: var(--la-text-mute, #475569);
    cursor: pointer;
    font-size: 10px;
    font-family: inherit;
    white-space: nowrap;
    transition: color 80ms, border-color 80ms;
  }
  .cn-btn:hover { color: var(--la-text-dim, #94a3b8); border-color: var(--la-hair-base, #1e2128); }
  .cn-btn.active { color: var(--la-text-base, #e2e8f0); border-color: var(--la-hair-hi, #2e3440); }

  /* ── Filter bar ── */
  .filter-bar {
    display: flex;
    gap: 1px;
    padding: 3px 6px;
    background: var(--la-bg-elev-1, #111214);
    border-bottom: 1px solid var(--la-hair-base, #1e2128);
    flex-shrink: 0;
    flex-wrap: wrap;
  }

  .filter-btn {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 2px 6px;
    border: 1px solid transparent;
    border-radius: 3px;
    background: transparent;
    color: var(--la-text-mute, #475569);
    cursor: pointer;
    font-size: 10px;
    font-family: inherit;
    transition: color 80ms, border-color 80ms, background 80ms;
  }
  .filter-btn:hover { color: var(--la-text-dim, #94a3b8); border-color: var(--la-hair-base, #1e2128); }
  .filter-btn.active {
    color: var(--la-text-base, #e2e8f0);
    border-color: var(--la-hair-hi, #2e3440);
    background: var(--la-bg-elev-2, #161a1f);
  }
  .f-icon { font-size: 9px; }

  /* ── Feed list ── */
  .feed-list {
    flex: 1;
    overflow-y: auto;
    padding: 2px 0;
  }

  .feed-empty {
    padding: 12px;
    color: var(--la-text-mute, #475569);
    text-align: center;
    font-style: italic;
  }

  .feed-row {
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 2px 6px;
    border-bottom: 1px solid transparent;
    transition: background 80ms;
  }
  .feed-row:hover { background: var(--la-bg-elev-1, #111214); }
  .feed-row.feed-hitl { background: rgba(245, 166, 35, 0.06); }

  .fe-badge {
    flex-shrink: 0;
    padding: 0 4px;
    border: 1px solid;
    border-radius: 2px;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.04em;
  }
  .fe-time { color: var(--la-text-mute, #475569); flex-shrink: 0; }
  .fe-cn   { color: var(--la-text-dim, #94a3b8); flex-shrink: 0; max-width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fe-task { color: var(--la-text-mute, #475569); flex-shrink: 0; }
  .fe-wave { color: var(--la-text-mute, #475569); flex-shrink: 0; }
  .fe-summary { color: var(--la-text-base, #e2e8f0); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  /* ── Footer ── */
  .feed-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 8px;
    background: var(--la-bg-elev-1, #111214);
    border-top: 1px solid var(--la-hair-base, #1e2128);
    flex-shrink: 0;
  }

  .autoscroll-btn {
    padding: 1px 6px;
    border: 1px solid var(--la-hair-base, #1e2128);
    border-radius: 3px;
    background: transparent;
    color: var(--la-text-mute, #475569);
    cursor: pointer;
    font-size: 10px;
    font-family: inherit;
    transition: color 80ms, border-color 80ms;
  }
  .autoscroll-btn:hover { color: var(--la-text-dim, #94a3b8); }
  .autoscroll-btn.active { color: #2dd4bf; border-color: #2dd4bf; }

  .feed-count { color: var(--la-text-mute, #475569); margin-left: auto; }
</style>
