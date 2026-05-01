<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    open: boolean;
    title: string;
    subtitle?: string;
    onclose: () => void;
    width?: string;
    zIndex?: number;
    children: Snippet;
    actions?: Snippet;
    testId?: string;
    headerOnboarding?: string;
  }

  let {
    open,
    title,
    subtitle,
    onclose,
    width = '420px',
    zIndex = 40,
    children,
    actions,
    testId,
    headerOnboarding,
  }: Props = $props();

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && open) onclose();
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
  <div
    class="fixed top-0 right-0 bottom-0 bg-[var(--la-drawer-bg)] border-l border-[var(--la-drawer-border)] flex flex-col shadow-2xl"
    style="width: {width}; max-width: 90vw; z-index: {zIndex};"
    data-testid={testId}
  >
    <div
      class="flex items-center justify-between px-4 py-3 border-b border-[var(--la-drawer-border)] shrink-0"
      data-onboarding={headerOnboarding}
    >
      <div class="flex items-center gap-2">
        <span class="text-sm font-semibold text-[#e2e8f0]">{title}</span>
        {#if subtitle}
          <span class="text-[10px] text-[#64748b]">{subtitle}</span>
        {/if}
      </div>
      <div class="flex items-center gap-2">
        {#if actions}{@render actions()}{/if}
        <button
          class="text-[#64748b] hover:text-white text-lg leading-none"
          onclick={onclose}
          aria-label="Close {title}"
        >×</button>
      </div>
    </div>
    <div class="flex-1 overflow-hidden flex flex-col">
      {@render children()}
    </div>
  </div>
{/if}
