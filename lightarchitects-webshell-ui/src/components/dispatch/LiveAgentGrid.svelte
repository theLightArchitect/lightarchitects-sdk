<script lang="ts">
  import {
    DOMAIN_AGENT_COLORS,
    DOMAIN_AGENT_LABELS,
    type DomainAgent,
    type AgentLiveState,
  } from '$lib/dispatch';

  interface Props {
    agents: DomainAgent[];
    agentStates?: Map<DomainAgent, AgentLiveState>;
  }

  let { agents, agentStates = new Map() }: Props = $props();

  function stateBg(state: string | undefined): string {
    switch (state) {
      case 'running':   return 'bg-[#f59e0b]/10 border-[#f59e0b]/30';
      case 'complete':  return 'bg-[#10b981]/10 border-[#10b981]/30';
      case 'failed':    return 'bg-[#ef4444]/10 border-[#ef4444]/30';
      case 'cancelled': return 'bg-[#475569]/10 border-[#475569]/30';
      default:          return 'bg-transparent border-[#1e293b]';
    }
  }

  function stateLabel(state: string | undefined): string {
    switch (state) {
      case 'running':   return '▶ Running';
      case 'complete':  return '✓ Complete';
      case 'failed':    return '✗ Failed';
      case 'cancelled': return '⊘ Cancelled';
      default:          return '○ Pending';
    }
  }

  function stateTextColor(state: string | undefined): string {
    switch (state) {
      case 'running':   return '#f59e0b';
      case 'complete':  return '#10b981';
      case 'failed':    return '#ef4444';
      case 'cancelled': return '#64748b';
      default:          return '#475569';
    }
  }
</script>

<div data-testid="live-agent-grid">
{#if agents.length === 0}
  <div class="text-[10px] text-[#475569] text-center py-4">
    No agents in this dispatch
  </div>
{:else}
  <div class="grid grid-cols-2 gap-2">
    {#each agents as agent}
      {@const color = DOMAIN_AGENT_COLORS[agent]}
      {@const live = agentStates.get(agent)}
      {@const lastMsg = live?.messages.at(-1)}

      <div class="rounded border p-2 flex flex-col gap-1 {stateBg(live?.state)}">
        <div class="flex items-center justify-between gap-1">
          <span class="text-[10px] font-medium" style="color: {color}">
            {DOMAIN_AGENT_LABELS[agent]}
          </span>
          <span class="text-[9px]" style="color: {stateTextColor(live?.state)}">
            {stateLabel(live?.state)}
          </span>
        </div>

        {#if lastMsg}
          <p class="text-[9px] text-[#94a3b8] leading-relaxed line-clamp-2 font-mono">
            {lastMsg}
          </p>
        {:else}
          <p class="text-[9px] text-[#334155]">—</p>
        {/if}

        {#if live && live.messages.length > 1}
          <span class="text-[8px] text-[#475569]">
            {live.messages.length} messages
          </span>
        {/if}
      </div>
    {/each}
  </div>
{/if}
</div>
