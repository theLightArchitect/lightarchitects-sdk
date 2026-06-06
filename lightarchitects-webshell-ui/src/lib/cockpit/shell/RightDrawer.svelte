<script lang="ts">
  import type { Snippet } from 'svelte';
  import { selection } from '$lib/cockpit/stores/selection';
  import FocusRouter from '$lib/cockpit/focus/FocusRouter.svelte';

  interface Props { children?: Snippet; }
  let { children }: Props = $props();

  const sel = $derived($selection);
  const hasSelection = $derived(sel.kind !== 'none');
</script>

<aside
  class="cockpit-right"
  class:has-selection={hasSelection}
  aria-label="Focus drawer"
  data-card-role="focus-drawer"
>
  {#if children}
    {@render children()}
  {:else}
    <FocusRouter />
  {/if}
</aside>

<style>
  .cockpit-right {
    width: var(--cockpit-right-width, 480px);
    min-width: var(--cockpit-right-width, 480px);
    height: 100%;
    border-left: 1px solid var(--scope-strip-border, rgba(255,255,255,0.06));
    overflow-y: auto;
    flex-shrink: 0;
    background: var(--bg-elevated, #111);
    transition: border-color var(--motion-scope-fade, 200ms ease-out);
  }
  .cockpit-right.has-selection {
    border-left-color: var(--scope-accent, var(--scope-d0));
  }
  .right-placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-muted, #555);
    font-family: var(--font-mono, monospace);
    font-size: 0.75rem;
  }
  .placeholder-label { opacity: 0.5; }
</style>
