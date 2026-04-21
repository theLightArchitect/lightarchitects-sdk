<script lang="ts">
  import { SIBLING_COLORS } from '$lib/design-tokens';
  import type { SiblingId } from '$lib/types';

  interface Props {
    siblings?: SiblingId[];
    selectedSibling?: SiblingId | null;
    onDispatch?: (sibling: SiblingId, prompt?: string) => void;
    compact?: boolean;
  }

  let {
    siblings = ['soul', 'eva', 'corso', 'quantum', 'seraph', 'ayin'] as SiblingId[],
    selectedSibling = null,
    onDispatch,
    compact = false,
  }: Props = $props();

  let dispatchPrompt = $state('');
  let promptTarget: SiblingId | null = $state(null);

  function handleDispatch(sib: SiblingId) {
    if (promptTarget === sib && dispatchPrompt.trim()) {
      onDispatch?.(sib, dispatchPrompt.trim());
      dispatchPrompt = '';
      promptTarget = null;
    } else {
      promptTarget = sib;
      dispatchPrompt = '';
    }
  }

  function submitPrompt() {
    if (promptTarget && dispatchPrompt.trim()) {
      onDispatch?.(promptTarget, dispatchPrompt.trim());
      dispatchPrompt = '';
      promptTarget = null;
    }
  }

  function cancelPrompt() {
    promptTarget = null;
    dispatchPrompt = '';
  }
</script>

<div class="space-y-2">
  <div class={compact ? 'grid grid-cols-3 gap-1.5' : 'grid grid-cols-3 gap-2'}>
    {#each siblings as sib}
      {@const color = SIBLING_COLORS[sib] ?? '#6b7280'}
      {@const isSelected = selectedSibling === sib}
      {@const isPrompting = promptTarget === sib}
      <button
        class="text-center rounded border transition-colors
          {compact ? 'px-1.5 py-1 text-[9px]' : 'px-2 py-1.5 text-[10px]'}
          {isSelected ? 'border-[#FFD700] bg-[#FFD700]/10' : 'border-[#1e293b] hover:border-[#334155]'}
          {isPrompting ? 'ring-1 ring-[#FFD700]' : ''}"
        style="color: {color}; {isSelected ? `border-color: ${color}80; background-color: ${color}10` : ''}"
        onclick={() => handleDispatch(sib)}
      >
        {sib.toUpperCase()}
      </button>
    {/each}
  </div>

  {#if promptTarget}
    {@const color = SIBLING_COLORS[promptTarget] ?? '#6b7280'}
    <div class="flex gap-1.5 mt-1">
      <input
        type="text"
        bind:value={dispatchPrompt}
        placeholder="Prompt for {promptTarget.toUpperCase()}…"
        class="flex-1 bg-[#111827] border border-[#1e293b] rounded px-2 py-1 text-[10px] text-[#e2e8f0] placeholder-[#475569] outline-none focus:border-[#FFD700]"
        style="border-color: {color}40"
        onkeydown={(e) => {
          if (e.key === 'Enter') submitPrompt();
          if (e.key === 'Escape') cancelPrompt();
        }}
      />
      <button
        onclick={submitPrompt}
        class="px-2 py-1 text-[10px] rounded transition-colors"
        style="background-color: {color}20; color: {color}"
      >
        Go
      </button>
      <button
        onclick={cancelPrompt}
        class="px-2 py-1 text-[10px] rounded border border-[#1e293b] text-[#64748b] hover:border-[#334155]"
      >
        X
      </button>
    </div>
  {/if}
</div>