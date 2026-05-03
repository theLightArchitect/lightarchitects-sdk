<script lang="ts">
  import { conductorTasks, siblingHealth } from '$lib/stores';
  import { SIBLINGS, SIBLING_COLORS } from '$lib/design-tokens';
  import StatusPip from '$lib/components/StatusPip.svelte';
  import type { SiblingId } from '$lib/design-tokens';

  // One card per squad sibling, showing their current conductor task (if any)
  let cards = $derived.by(() =>
    SIBLINGS.map(sib => {
      const health = $siblingHealth[sib as SiblingId];
      const running = $conductorTasks.filter(t => t.sibling === sib && t.status === 'running');
      const pending = $conductorTasks.filter(t => t.sibling === sib && t.status === 'pending');
      const current = running[0] ?? pending[0] ?? null;

      const pipState = !health || health.status === 'offline' ? 'idle' as const
                     : health.status === 'degraded'           ? 'failed' as const
                     : current?.status === 'running'          ? 'active' as const
                     : 'complete' as const;

      return {
        sib,
        color: SIBLING_COLORS[sib] ?? '#64748b',
        health,
        current,
        runningCount: running.length,
        pendingCount: pending.length,
        pipState,
      };
    })
  );
</script>

<div
  class="flex items-stretch gap-px bg-[#0f172a] border-t border-[#1e293b] shrink-0 overflow-x-auto"
  style="height: 132px;"
  data-testid="agent-task-strip"
>
  {#each cards as card (card.sib)}
    <div
      class="flex flex-col gap-1 px-2 py-1.5 min-w-[112px] border-r border-[#0f172a] bg-[#0a0a0f]"
      style="flex: 1; max-width: 160px;"
    >
      <!-- Agent header -->
      <div class="flex items-center gap-1.5">
        <StatusPip
          color={card.color}
          state={card.pipState}
          shape={card.pipState === 'failed' ? 'x' : 'filled'}
          ariaLabel="{card.sib} status"
        />
        <span class="text-[9px] font-mono font-bold uppercase" style="color: {card.color};">{card.sib}</span>
        {#if card.runningCount > 0}
          <span class="text-[7px] font-mono text-[#22c55e] ml-auto">{card.runningCount} RUN</span>
        {:else if card.pendingCount > 0}
          <span class="text-[7px] font-mono text-[#475569] ml-auto">{card.pendingCount} Q</span>
        {/if}
      </div>

      {#if card.current}
        <!-- Current task type -->
        <div class="text-[9px] font-mono text-[#94a3b8] leading-tight truncate">{card.current.taskType}</div>
        <!-- Sub-context: build association -->
        {#if card.current.buildId}
          <div class="text-[8px] font-mono text-[#475569] truncate">{card.current.buildId.slice(0, 10)}</div>
        {/if}
        <!-- Progress bar with shimmer on running -->
        <div class="mt-auto">
          <div
            class="h-0.5 rounded-full overflow-hidden"
            style="background: #1e293b;"
          >
            <div
              class="h-full rounded-full {card.current.status === 'running' ? 'shimmer-bar' : ''}"
              style="width: {card.current.status === 'running' ? '60%' : card.current.status === 'pending' ? '15%' : '100%'};{card.current.status !== 'running' ? ' background: #334155;' : ''}"
            ></div>
          </div>
        </div>
      {:else}
        <!-- Idle state -->
        <div class="text-[8px] font-mono text-[#1e293b] mt-1">
          {card.health?.status === 'offline' ? 'OFFLINE' : 'IDLE'}
        </div>
        <!-- Flat bar -->
        <div class="mt-auto h-0.5 rounded-full bg-[#0f172a]"></div>
      {/if}
    </div>
  {/each}
</div>

<style>
  @keyframes shimmer {
    0%   { background-position: -200% center; }
    100% { background-position:  200% center; }
  }

  .shimmer-bar {
    background-size: 200% auto !important;
    animation: shimmer 1.6s linear infinite;
    background-image: linear-gradient(
      90deg,
      var(--shimmer-from, #334155) 0%,
      var(--shimmer-mid, #e2e8f0) 50%,
      var(--shimmer-from, #334155) 100%
    ) !important;
  }
</style>
