<script lang="ts">
  import {
    DOMAIN_AGENT_COLORS,
    DOMAIN_AGENT_LABELS,
    type DispatchHistoryEntry,
    type DomainAgent,
  } from '$lib/dispatch';

  interface Props {
    history?: DispatchHistoryEntry[];
    onSelect?: (entry: DispatchHistoryEntry) => void;
    onClear?: () => void;
  }

  let { history = [], onSelect, onClear }: Props = $props();

  function statusColor(status: DispatchHistoryEntry['status']): string {
    switch (status) {
      case 'complete':  return '#10b981';
      case 'error':     return '#ef4444';
      case 'cancelled': return '#64748b';
      case 'running':   return '#f59e0b';
    }
  }

  function statusLabel(status: DispatchHistoryEntry['status']): string {
    switch (status) {
      case 'complete':  return '✓';
      case 'error':     return '✗';
      case 'cancelled': return '⊘';
      case 'running':   return '▶';
    }
  }

  function agentDots(agents: DomainAgent[]): { color: string; label: string }[] {
    return agents.slice(0, 5).map((a) => ({
      color: DOMAIN_AGENT_COLORS[a],
      label: DOMAIN_AGENT_LABELS[a],
    }));
  }

  function relativeTime(ts: number): string {
    const diff = Math.floor((Date.now() - ts) / 1000);
    if (diff < 60) return `${diff}s ago`;
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    return `${Math.floor(diff / 3600)}h ago`;
  }
</script>

<div class="flex flex-col gap-0">
  <div class="flex items-center justify-between px-1 pb-1 border-b border-[#0f172a]">
    <span class="text-[9px] text-[#475569] uppercase tracking-wider">History</span>
    {#if history.length > 0}
      <button
        onclick={onClear}
        class="text-[9px] text-[#475569] hover:text-[#64748b] transition-colors"
      >
        Clear
      </button>
    {/if}
  </div>

  {#if history.length === 0}
    <div class="text-[9px] text-[#334155] text-center py-4 italic">
      No past dispatches
    </div>
  {/if}

  {#each history as entry (entry.id)}
    <button
      onclick={() => onSelect?.(entry)}
      class="text-left px-1.5 py-1.5 border-b border-[#0f172a] hover:bg-[#0f172a]
             transition-colors group"
    >
      <div class="flex items-center gap-1.5">
        <span class="text-[10px] font-mono" style="color: {statusColor(entry.status)}">
          {statusLabel(entry.status)}
        </span>
        <span class="text-[9px] text-[#94a3b8] truncate flex-1 group-hover:text-[#e2e8f0]">
          {entry.task.slice(0, 50)}{entry.task.length > 50 ? '…' : ''}
        </span>
        {#if entry.dry}
          <span class="text-[8px] text-[#f59e0b] flex-shrink-0">[dry]</span>
        {/if}
      </div>

      <div class="flex items-center gap-1 mt-0.5">
        <div class="flex gap-0.5">
          {#each agentDots(entry.agents) as dot}
            <span
              class="inline-block w-1.5 h-1.5 rounded-full"
              style="background: {dot.color}"
              title={dot.label}
            ></span>
          {/each}
          {#if entry.agents.length > 5}
            <span class="text-[8px] text-[#475569]">+{entry.agents.length - 5}</span>
          {/if}
        </div>
        <span class="text-[8px] text-[#334155] ml-auto">{relativeTime(entry.startedAt)}</span>
        {#if entry.elapsed_ms !== undefined}
          <span class="text-[8px] text-[#334155]">{(entry.elapsed_ms / 1000).toFixed(1)}s</span>
        {/if}
      </div>
    </button>
  {/each}
</div>
