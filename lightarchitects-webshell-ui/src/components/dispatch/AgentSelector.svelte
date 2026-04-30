<script lang="ts">
  import {
    DOMAIN_AGENTS,
    DOMAIN_AGENT_COLORS,
    DOMAIN_AGENT_LABELS,
    type DomainAgent,
    type Classification,
  } from '$lib/dispatch';

  interface Props {
    selected?: DomainAgent[];
    classification?: Classification | null;
    disabled?: boolean;
    onchange?: (agents: DomainAgent[]) => void;
  }

  let {
    selected = $bindable([]),
    classification = null,
    disabled = false,
    onchange,
  }: Props = $props();

  function toggle(agent: DomainAgent) {
    if (disabled) return;
    const next = selected.includes(agent)
      ? selected.filter((a) => a !== agent)
      : [...selected, agent];
    selected = next;
    onchange?.(next);
  }

  function applyClassification() {
    if (!classification || disabled) return;
    selected = [...classification.agents];
    onchange?.(selected);
  }

  function selectAll() {
    if (disabled) return;
    selected = [...DOMAIN_AGENTS];
    onchange?.(selected);
  }

  function clearAll() {
    if (disabled) return;
    selected = [];
    onchange?.(selected);
  }
</script>

<div class="flex flex-col gap-2">
  <div class="flex items-center justify-between">
    <span class="text-[10px] text-[#64748b] uppercase tracking-wider">Agents</span>
    <div class="flex gap-1.5">
      {#if classification?.agents.length}
        <button
          onclick={applyClassification}
          {disabled}
          class="text-[9px] px-1.5 py-0.5 rounded border border-[#3b82f6]/40
                 text-[#3b82f6] hover:border-[#3b82f6] transition-colors
                 disabled:opacity-40 disabled:cursor-not-allowed"
        >
          Auto ({classification.agents.length})
        </button>
      {/if}
      <button
        onclick={selectAll}
        {disabled}
        class="text-[9px] px-1.5 py-0.5 rounded border border-[#1e293b]
               text-[#94a3b8] hover:border-[#334155] transition-colors
               disabled:opacity-40 disabled:cursor-not-allowed"
      >
        All
      </button>
      <button
        onclick={clearAll}
        {disabled}
        class="text-[9px] px-1.5 py-0.5 rounded border border-[#1e293b]
               text-[#94a3b8] hover:border-[#334155] transition-colors
               disabled:opacity-40 disabled:cursor-not-allowed"
      >
        Clear
      </button>
    </div>
  </div>

  <div class="grid grid-cols-3 gap-1.5">
    {#each DOMAIN_AGENTS as agent}
      {@const color = DOMAIN_AGENT_COLORS[agent]}
      {@const isSelected = selected.includes(agent)}
      {@const isSuggested = classification?.agents.includes(agent) ?? false}
      <button
        onclick={() => toggle(agent)}
        {disabled}
        class="relative px-2 py-1.5 rounded border text-[10px] text-center
               transition-all select-none
               {disabled ? 'cursor-not-allowed opacity-60' : 'cursor-pointer'}
               {isSelected
                 ? 'border-[var(--c)] bg-[var(--c)]/15'
                 : 'border-[#1e293b] bg-transparent hover:border-[#334155]'}"
        style="--c: {color}; color: {isSelected ? color : '#94a3b8'}"
      >
        {DOMAIN_AGENT_LABELS[agent]}
        {#if isSuggested && !isSelected}
          <span
            class="absolute top-0.5 right-0.5 w-1 h-1 rounded-full"
            style="background: {color}"
          ></span>
        {/if}
      </button>
    {/each}
  </div>

  {#if classification?.rationale}
    <p class="text-[9px] text-[#475569] leading-relaxed">{classification.rationale}</p>
  {/if}

  {#if selected.length === 0}
    <p class="text-[9px] text-[#ef4444]/80">Select at least one agent.</p>
  {/if}
</div>
