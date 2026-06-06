<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { RouteScope } from '$lib/cockpit/stores/scope';

  interface Props {
    scope:    RouteScope | null;
    children?: Snippet;
  }
  let { scope, children }: Props = $props();

  // AD-07: bottom bar renders only at d1 (project) and d2 (build).
  // d0 (platform) and d3 (file) have no bottom bar.
  const depth = $derived(scope?.depth ?? -1);
</script>

<!-- Phase 4 Wave B: wire WaveComposer + SmartDispatch here. -->
<footer class="cockpit-bottom" aria-label="Action bar">
  {#if children}
    {@render children()}
  {:else if depth === 1}
    <div class="bottom-placeholder" data-depth="d1">
      <span>Wave Composer + Smart Dispatch — Phase 4</span>
    </div>
  {:else if depth === 2}
    <div class="bottom-placeholder" data-depth="d2">
      <span>Build Actions — Phase 4</span>
    </div>
  {/if}
</footer>

<style>
  .cockpit-bottom {
    height: var(--cockpit-bottom-height, 40px);
    display: flex;
    align-items: center;
    padding: 0 1rem;
    border-top: 1px solid var(--scope-accent, var(--scope-strip-border, rgba(255,255,255,0.06)));
    background: var(--scope-strip-bg, rgba(0,0,0,0.72));
    font-family: var(--font-mono, monospace);
    font-size: 0.72rem;
    flex-shrink: 0;
  }
  .bottom-placeholder {
    color: var(--text-muted, #555);
    opacity: 0.4;
  }
</style>
