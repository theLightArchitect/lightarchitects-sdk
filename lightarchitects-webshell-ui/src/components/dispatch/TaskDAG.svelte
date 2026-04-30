<script lang="ts">
  import {
    DOMAIN_AGENT_COLORS,
    DOMAIN_AGENT_LABELS,
    type DomainAgent,
    type AgentState,
    type AgentLiveState,
  } from '$lib/dispatch';

  interface Props {
    agents: DomainAgent[];
    agentStates?: Map<DomainAgent, AgentLiveState>;
  }

  let { agents, agentStates = new Map() }: Props = $props();

  function stateColor(state: AgentState | undefined): string {
    switch (state) {
      case 'running':   return '#f59e0b';
      case 'complete':  return '#10b981';
      case 'failed':    return '#ef4444';
      case 'cancelled': return '#64748b';
      default:          return '#1e293b';
    }
  }

  function stateIcon(state: AgentState | undefined): string {
    switch (state) {
      case 'running':   return '▶';
      case 'complete':  return '✓';
      case 'failed':    return '✗';
      case 'cancelled': return '⊘';
      default:          return '○';
    }
  }
</script>

{#if agents.length === 0}
  <div class="flex items-center justify-center h-12 text-[10px] text-[#475569]">
    No agents selected
  </div>
{:else}
  <div class="flex items-center gap-0 overflow-x-auto py-1" data-testid="task-dag-canvas">
    {#each agents as agent, i}
      {@const color = DOMAIN_AGENT_COLORS[agent]}
      {@const live = agentStates.get(agent)}
      {@const state = live?.state}

      {#if i > 0}
        <div class="flex-shrink-0 w-4 h-px bg-[#1e293b] mx-0.5"></div>
      {/if}

      <div
        class="flex-shrink-0 flex flex-col items-center gap-0.5 px-2 py-1 rounded border
               min-w-[52px] transition-all"
        style="border-color: {state ? stateColor(state) : color}40;
               background: {state ? stateColor(state) : color}08"
      >
        <span class="text-[10px]" style="color: {state ? stateColor(state) : color}">
          {stateIcon(state)}
        </span>
        <span class="text-[9px] text-center leading-tight" style="color: {color}">
          {DOMAIN_AGENT_LABELS[agent]}
        </span>
        {#if state}
          <span class="text-[8px] capitalize" style="color: {stateColor(state)}">
            {state}
          </span>
        {/if}
      </div>
    {/each}
  </div>
{/if}
