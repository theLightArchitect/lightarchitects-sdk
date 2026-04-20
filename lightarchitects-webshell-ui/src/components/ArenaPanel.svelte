<script lang="ts">
  import { arenaStatus, arenaStats } from '$lib/stores';
  import { SIBLING_COLORS } from '$lib/design-tokens';
  import type { ArenaAgent } from '$lib/types';

  interface Props {
    onAgentClick?: (agent: ArenaAgent) => void;
  }

  let { onAgentClick }: Props = $props();

  function agentStatusColor(status: ArenaAgent['status']): string {
    switch (status) {
      case 'active': return '#22c55e';
      case 'idle': return '#6b7280';
      case 'error': return '#ef4444';
    }
  }

  function formatHeartbeat(iso: string): string {
    const d = new Date(iso);
    const now = Date.now();
    const diff = now - d.getTime();
    if (diff < 10000) return 'just now';
    if (diff < 60000) return `${Math.floor(diff / 1000)}s`;
    return `${Math.floor(diff / 60000)}m`;
  }

  // Sort agents: active first, then idle, then error
  let sortedAgents = $derived(
    [...$arenaStatus.agents].sort((a, b) => {
      const order: Record<string, number> = { active: 0, idle: 1, error: 2 };
      return order[a.status] - order[b.status];
    })
  );
</script>

<div class="bg-[#111827] border border-[#1e293b] rounded-lg overflow-hidden">
  <!-- Header -->
  <div class="px-4 py-2 border-b border-[#1e293b] flex items-center justify-between">
    <h3 class="text-xs font-medium text-[#64748b]">ARENA STATUS</h3>
    <div class="flex items-center gap-3 text-[10px]">
      <span class="text-[#22c55e]">{$arenaStats.activeAgents} active</span>
      <span class="text-[#6b7280]">{$arenaStats.idleAgents} idle</span>
    </div>
  </div>

  <!-- Routine counts -->
  <div class="px-4 py-2 bg-[#0d1117] border-b border-[#1e293b] flex items-center gap-4">
    <div class="flex items-center gap-2">
      <span class="text-[10px] text-[#64748b]">Active Routines:</span>
      <span class="text-[12px] font-semibold text-[#22c55e]">{$arenaStatus.activeRoutines}</span>
    </div>
    <div class="flex items-center gap-2">
      <span class="text-[10px] text-[#64748b]">Queued:</span>
      <span class="text-[12px] font-semibold text-[#f59e0b]">{$arenaStatus.queuedRoutines}</span>
    </div>
  </div>

  <!-- Agent list -->
  <div class="divide-y divide-[#1e293b]">
    {#each sortedAgents as agent (agent.id)}
      {@const sibColor = SIBLING_COLORS[agent.sibling] ?? '#6b7280'}
      {@const stColor = agentStatusColor(agent.status)}

      <button
        class="w-full text-left px-4 py-2 flex items-center gap-3 hover:bg-[#0d1117] transition-colors"
        onclick={() => onAgentClick?.(agent)}
      >
        <!-- Status pulse -->
        <div class="relative flex-shrink-0">
          <div
            class="w-2 h-2 rounded-full"
            style="background-color: {stColor}; {agent.status === 'active' ? `box-shadow: 0 0 6px ${stColor}` : ''}"
          ></div>
          {#if agent.status === 'active'}
            <div
              class="absolute inset-0 w-2 h-2 rounded-full animate-ping"
              style="background-color: {stColor}; opacity: 0.5"
            ></div>
          {/if}
        </div>

        <!-- Sibling badge -->
        <div
          class="flex-shrink-0 w-6 h-6 rounded flex items-center justify-center text-[8px] font-bold"
          style="background-color: {sibColor}20; color: {sibColor}"
        >
          {agent.sibling.slice(0, 2).toUpperCase()}
        </div>

        <!-- Agent info -->
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2">
            <span class="text-[11px] text-[#e2e8f0]">{agent.id}</span>
            <span
              class="text-[9px] px-1.5 py-0.5 rounded"
              style="background-color: {stColor}20; color: {stColor}"
            >
              {agent.status}
            </span>
          </div>
          <div class="flex items-center gap-2 text-[9px] text-[#475569]">
            <span>heartbeat: {formatHeartbeat(agent.lastHeartbeat)}</span>
            {#if agent.currentBuildId}
              <span>&middot;</span>
              <span class="text-[#7C3AED]">{agent.currentBuildId.slice(-8)}</span>
            {/if}
          </div>
        </div>

        <!-- Routine count -->
        <div class="text-[10px] text-[#94a3b8]">
          {agent.routineCount} routines
        </div>
      </button>
    {/each}
  </div>
</div>