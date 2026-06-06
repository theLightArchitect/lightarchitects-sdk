<script lang="ts">
  import type { Snippet } from 'svelte';
  import { scope }             from '$lib/cockpit/stores/scope';
  import { clearOnScopeChange } from '$lib/cockpit/stores/selection';
  import TopStrip  from './TopStrip.svelte';
  import LeftDrawer  from './LeftDrawer.svelte';
  import RightDrawer from './RightDrawer.svelte';
  import BottomBar   from './BottomBar.svelte';
  import '$lib/cockpit/tokens.css';

  interface Props { children?: Snippet; }
  let { children }: Props = $props();

  const currentScope = $derived($scope);
  const depth        = $derived(currentScope?.depth ?? 0);

  // Clear right-drawer selection whenever scope navigates.
  $effect(() => {
    currentScope;          // reactive dependency
    clearOnScopeChange();
  });
</script>

<div
  class="cockpit-shell"
  data-scope-depth={depth}
  aria-label="Cockpit shell"
>
  <TopStrip />

  <div class="cockpit-body">
    <LeftDrawer />

    <main class="cockpit-center" role="main" aria-label="Main content">
      {#if children}
        {@render children()}
      {/if}
    </main>

    <RightDrawer />
  </div>

  <BottomBar scope={currentScope} />
</div>

<style>
  .cockpit-shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    overflow: hidden;
    background: var(--bg-base, #0a0a0a);
    /* Accent token cascades down from here via [data-scope-depth] in tokens.css */
  }

  .cockpit-body {
    display: flex;
    flex-direction: row;
    flex: 1 1 0;
    overflow: hidden;
    min-height: 0;
  }

  .cockpit-center {
    flex: 1 1 0;
    overflow-y: auto;
    min-width: 0;
    background: var(--bg-base, #0a0a0a);
  }
</style>
