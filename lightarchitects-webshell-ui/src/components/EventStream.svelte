<script lang="ts" module>
  import type { Severity } from '$lib/atmosphere';
  import { SEVERITY_COLORS } from '$lib/atmosphere';

  /** Normalized event row — the boundary type between data sources and EventStream. */
  export interface StreamRow {
    /** Unix epoch ms — used for sort order only, not displayed directly. */
    ts: number;
    /** Human-readable time label, e.g. "08:25:50". */
    time: string;
    /** Source label (agent name, pillar, system, etc.). */
    source: string;
    /** Hex color string for the source label. */
    color: string;
    /** Main row text content. */
    text: string;
    /** Severity level — controls text color. */
    severity: Severity;
  }

  /** Convert a LogLevel string (from LogEntry) to a Severity. */
  export function logLevelToSeverity(level: string): Severity {
    if (level === 'error')   return 'err';
    if (level === 'warn')    return 'warn';
    if (level === 'success') return 'ok';
    return 'info';
  }
</script>

<script lang="ts">
  interface Props {
    rows: StreamRow[];
    /** Maximum number of rows to render (most recent kept). */
    maxDisplay?: number;
    /** Whether to render rows newest-first (default: true). */
    newestFirst?: boolean;
    /** Placeholder text when rows is empty and online. */
    emptyMessage?: string;
    /** When true, SSE transport is disconnected — shows a distinct offline state. */
    isOffline?: boolean;
  }

  let {
    rows,
    maxDisplay = 120,
    newestFirst = true,
    emptyMessage = '— no events yet —',
    isOffline = false,
  }: Props = $props();

  let scrollEl: HTMLDivElement | undefined = $state();

  let displayed = $derived.by(() => {
    const capped = rows.slice(0, maxDisplay);
    return newestFirst ? capped : [...capped].reverse();
  });

  // Auto-scroll: to top when newestFirst, to bottom when oldest-first.
  $effect(() => {
    void rows.length;
    if (!scrollEl) return;
    requestAnimationFrame(() => {
      if (!scrollEl) return;
      scrollEl.scrollTop = newestFirst ? 0 : scrollEl.scrollHeight;
    });
  });
</script>

<div
  bind:this={scrollEl}
  class="flex-1 overflow-y-auto min-h-0 font-mono"
  data-testid="event-stream"
>
  {#if displayed.length === 0}
    <div class="px-4 py-8 text-center space-y-1">
      {#if isOffline}
        <p class="text-[10px] font-bold tracking-widest uppercase" style="color: #ef444460">⬡ SSE OFFLINE</p>
        <p class="text-[9px]" style="color: #334155">gateway unreachable — reconnecting</p>
      {:else}
        <p class="text-[10px]" style="color: #334155">{emptyMessage}</p>
      {/if}
    </div>
  {:else}
    {#each displayed as row, i (`${row.ts}-${row.source}-${i}`)}
      <div
        class="px-3 py-1 border-b border-[#0f172a] hover:bg-[#0f172a]/50 flex items-start gap-2"
        style="min-height: 26px;"
      >
        <span class="text-[9px] text-[#334155] shrink-0 mt-0.5 tabular-nums">{row.time}</span>
        <span
          class="text-[9px] font-bold shrink-0 mt-0.5 uppercase"
          style="color: {row.color}; min-width: 52px;"
        >{row.source.slice(0, 8)}</span>
        <span
          class="text-[10px] break-all leading-relaxed"
          style="color: {SEVERITY_COLORS[row.severity]}"
        >{row.text}</span>
      </div>
    {/each}
  {/if}
</div>
