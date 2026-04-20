<script lang="ts">
  import type { LogEntry, LogLevel } from '$lib/types';

  interface Props {
    entries: LogEntry[];
    maxDisplay?: number;
    autoScroll?: boolean;
  }

  let { entries, maxDisplay = 50, autoScroll = true }: Props = $props();

  let container: HTMLDivElement | undefined = $state();

  function levelColor(level: LogLevel): string {
    switch (level) {
      case 'error': return '#ef4444';
      case 'warn': return '#f59e0b';
      case 'success': return '#22c55e';
      case 'info': return '#3b82f6';
      case 'debug': return '#6b7280';
      default: return '#94a3b8';
    }
  }

  function levelBadge(level: LogLevel): string {
    return level.toUpperCase().padEnd(7);
  }

  function sourceBadge(source: string): string {
    return source.padEnd(10);
  }

  function formatTime(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }

  const displayed = $derived(entries.slice(-maxDisplay));

  // Auto-scroll to bottom when new entries arrive
  $effect(() => {
    if (autoScroll && container) {
      // Use requestAnimationFrame to ensure DOM has updated
      requestAnimationFrame(() => {
        container.scrollTop = container.scrollHeight;
      });
    }
  });
</script>

<div class="bg-[#0a0a12] border border-[#1e293b] rounded-lg overflow-hidden flex flex-col" style="min-height: 200px;">
  <!-- Header -->
  <div class="px-4 py-2 border-b border-[#1e293b] flex items-center justify-between bg-[#111827]">
    <h3 class="text-xs font-medium text-[#64748b]">LOG STREAM</h3>
    <div class="flex items-center gap-2">
      <span class="text-[10px] text-[#475569]">{entries.length} entries</span>
      <button
        class="text-[10px] px-2 py-0.5 rounded border border-[#1e293b] text-[#64748b] hover:text-white hover:border-[#334155] transition-colors"
        onclick={() => { if (container) container.scrollTop = container.scrollHeight; }}
      >
        Scroll to bottom
      </button>
    </div>
  </div>

  <!-- Log entries -->
  <div
    bind:this={container}
    class="flex-1 overflow-y-auto font-mono text-[11px] leading-relaxed"
  >
    {#if displayed.length === 0}
      <div class="px-4 py-6 text-center">
        <p class="text-[#475569]">Waiting for build output...</p>
        <p class="text-[10px] text-[#334155] mt-1">Logs will stream here when the build starts</p>
      </div>
    {:else}
      {#each displayed as entry}
        <div class="px-4 py-1 hover:bg-[#111827] flex items-start gap-2">
          <span class="text-[#334155] shrink-0">{formatTime(entry.timestamp)}</span>
          <span
            class="shrink-0 font-semibold"
            style="color: {levelColor(entry.level)}"
          >
            {levelBadge(entry.level)}
          </span>
          <span class="text-[#475569] shrink-0">[{entry.source}]</span>
          <span class="text-[#94a3b8] break-all">{entry.message}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>