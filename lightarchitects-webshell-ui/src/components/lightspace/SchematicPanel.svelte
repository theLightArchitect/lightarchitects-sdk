<!--
  @component SchematicPanel
  @description Collapsible right panel hosting FilesDrawer, CacheDrawer, GateMatrix.
  @contract none — orchestrator panel; delegates to subdrawers
  @reads lightspaceUiStore.schematicOpen
  @mutates lightspaceUiStore.schematicOpen (collapse toggle)
  @api none
  @mockup-ref arch/lightspace-mockup.html → .la-schematic, .la-schem-head
-->
<script lang="ts">
  import { lightspaceUiStore, filesCount, tombstoneCount } from '$lib/lightspace-stores';
  import FilesDrawer from './FilesDrawer.svelte';
  import CacheDrawer from './CacheDrawer.svelte';
  import GateMatrix  from './GateMatrix.svelte';

  const open = $derived($lightspaceUiStore.schematicOpen);
</script>

<aside class="ls-schematic" class:ls-schematic-open={open} class:ls-schematic-collapsed={!open}>
  <div class="ls-schem-head">
    <span class="ls-schem-glyph">⧉</span>
    {#if open}
      <span class="ls-schem-title">Schematic</span>
    {/if}
    <button
      class="ls-schem-collapse"
      onclick={() => lightspaceUiStore.update(s => ({ ...s, schematicOpen: !s.schematicOpen }))}
      aria-label={open ? 'Collapse schematic' : 'Expand schematic'}
    >{open ? '›' : '‹'}</button>
  </div>

  {#if open}
    <div class="ls-schem-body">
      <GateMatrix />
      <FilesDrawer />
      <CacheDrawer />
    </div>
  {:else}
    <div class="ls-schem-icons">
      <div class="ls-schem-icon" title="Files">{$filesCount}</div>
      <div class="ls-schem-icon" title="Cached">{$tombstoneCount}</div>
    </div>
  {/if}
</aside>

<style>
.ls-schematic {
  display: flex;
  flex-direction: column;
  background: var(--ls-panel);
  border-left: 1px solid var(--ls-border-base);
  overflow: hidden;
  transition: width var(--ls-mid);
}
.ls-schematic-open    { width: 260px; }
.ls-schematic-collapsed { width: 44px; }

.ls-schem-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 12px;
  border-bottom: 1px solid var(--ls-border-base);
  font-family: var(--ls-font-display);
  font-weight: 700;
  font-size: 10px;
  letter-spacing: var(--ls-tk-loose);
  color: var(--ls-text-bright);
  text-transform: uppercase;
}
.ls-schem-glyph { color: var(--ls-acc-purple); }
.ls-schem-title { flex: 1; }
.ls-schem-collapse {
  background: transparent; border: 0;
  color: var(--ls-text-mute); cursor: pointer;
  font-size: 11px; padding: 0;
  transition: color var(--ls-fast);
  margin-left: auto;
}
.ls-schem-collapse:hover { color: var(--ls-text-bright); }

.ls-schem-body {
  flex: 1;
  overflow-y: auto;
  scrollbar-width: thin;
}
.ls-schem-body::-webkit-scrollbar { width: 4px; }
.ls-schem-body::-webkit-scrollbar-thumb { background: var(--ls-border-base); }

.ls-schem-icons {
  display: flex; flex-direction: column;
  gap: 8px; padding: 14px 8px; align-items: center;
}
.ls-schem-icon {
  font-size: 9px; color: var(--ls-text-dim);
  width: 28px; height: 22px;
  display: flex; align-items: center; justify-content: center;
  border: 1px solid var(--ls-border-base);
  background: var(--ls-card);
}
</style>
