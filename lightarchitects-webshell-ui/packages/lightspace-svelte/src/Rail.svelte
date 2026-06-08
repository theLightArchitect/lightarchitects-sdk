<!--
  @component Rail — left navigation rail shell.
  @slot  default  Content to render in the collapsed/expanded body (e.g. CopilotDrawer)
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  let { children, collapsed = false }: { children?: Snippet; collapsed?: boolean } = $props();
</script>
<nav class="ls-rail" class:ls-rail-collapsed={collapsed} aria-label="Lightspace rail">
  {#if children}{@render children()}{/if}
</nav>
<style>
.ls-rail {
  width: 240px;
  min-height: 0;
  overflow-y: auto;
  border-right: 1px solid var(--ls-border);
  background: var(--ls-card);
  transition: width 0.2s ease;
  display: flex;
  flex-direction: column;
}
.ls-rail-collapsed { width: 48px; overflow: hidden; }
</style>
