<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props { children?: Snippet; }
  let { children }: Props = $props();

  // AD-08: CopilotDrawer is locked (always visible) in all cockpit screens.
  // Phase 5 Wave C will reparent the global CopilotDrawer into this slot
  // and wire the locked prop. For now this column reserves layout space.
</script>

<aside class="cockpit-left" aria-label="Left drawer (Copilot)">
  {#if children}
    {@render children()}
  {:else}
    <!-- Global CopilotDrawer occupies this space via app.svelte positioning.
         Phase 5: reparent CopilotDrawer here with locked={true}. -->
    <div class="left-placeholder" aria-hidden="true"></div>
  {/if}
</aside>

<style>
  .cockpit-left {
    width: var(--cockpit-left-width, 360px);
    min-width: var(--cockpit-left-width, 360px);
    height: 100%;
    border-right: 1px solid var(--scope-strip-border, rgba(255,255,255,0.06));
    overflow: hidden;
    flex-shrink: 0;
  }
  .left-placeholder { width: 100%; height: 100%; }
</style>
