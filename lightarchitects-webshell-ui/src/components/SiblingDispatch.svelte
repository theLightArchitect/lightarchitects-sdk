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
          {isSelected ? 'border-[var(--la-focus-ring)] bg-[var(--la-focus-ring)]/10' : 'border-[var(--la-drawer-border)] hover:border-[var(--la-hair-strong)]'}
          {isPrompting ? 'ring-1 ring-[var(--la-focus-ring)]' : ''}"
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
        class="flex-1 bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded px-2 py-1 text-[10px] text-[var(--la-text-bright)] placeholder-[var(--la-text-dim)] outline-none focus:border-[var(--la-focus-ring)]"
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
        class="px-2 py-1 text-[10px] rounded border border-[var(--la-drawer-border)] text-[var(--la-text-dim)] hover:border-[var(--la-hair-strong)]"
      >
        X
      </button>
    </div>
  {/if}
</div>