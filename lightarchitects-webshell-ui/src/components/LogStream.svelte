<script lang="ts">
  import type { LogEntry } from '$lib/types';
  import EventStream, { type StreamRow, logLevelToSeverity } from './EventStream.svelte';
  import { sourceColor } from '$lib/atmosphere';

  interface Props {
    entries: LogEntry[];
    maxDisplay?: number;
  }

  let { entries, maxDisplay = 50 }: Props = $props();

  function formatTime(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }

  // Reverse so input is newest-first — EventStream expects newest-first input
  // regardless of newestFirst prop, which only controls display direction.
  let rows = $derived(entries.slice(-maxDisplay).reverse().map((e): StreamRow => ({
    ts:       new Date(e.timestamp).getTime(),
    time:     formatTime(e.timestamp),
    source:   e.source,
    color:    sourceColor(e.source),
    text:     e.message,
    severity: logLevelToSeverity(e.level),
  })));
</script>

<div class="bg-[var(--la-bg-frame)] border border-[var(--la-drawer-border)] rounded-lg overflow-hidden flex flex-col" style="min-height: 200px;">
  <!-- Header -->
  <div class="px-4 py-2 border-b border-[var(--la-drawer-border)] flex items-center justify-between bg-[var(--la-bg-elev-1)] shrink-0">
    <h3 class="text-xs font-medium text-[var(--la-text-label)]">LOG STREAM</h3>
    <span class="text-[10px] text-[var(--la-text-dim)]">{entries.length} entries</span>
  </div>

  <!-- Delegate row rendering to EventStream -->
  <EventStream
    {rows}
    newestFirst={false}
    maxDisplay={maxDisplay}
    emptyMessage="Waiting for build output..."
  />
</div>
