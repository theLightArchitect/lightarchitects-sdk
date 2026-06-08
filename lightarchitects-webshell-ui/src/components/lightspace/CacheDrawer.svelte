<!--
  @component CacheDrawer
  @description "Cached Cards" collapsible subdrawer. Auto-opens on first eviction.
    Flash animation on header when a new card is evicted. Tombstone click → hero.
  @contract none — reads canvas tombstones; no SSE events consumed directly
  @reads lightspaceCanvasStore.tombstones, lightspaceUiStore.cacheDrawerOpen, .tombFlash
  @mutates lightspaceUiStore.cacheDrawerOpen (toggle), lightspaceFilesStore.heroTombId (click)
  @mutates lightspaceCanvasStore (restore: removes tombstone, adds card back)
  @api none — tombstone data is fully local (captured at eviction time)
  @mockup-ref arch/lightspace-mockup.html → #la-cache-subdrawer, .la-cache-row, renderTombstrip()
-->
<script lang="ts">
  import { lightspaceCanvasStore, lightspaceFilesStore, lightspaceUiStore, tombstoneCount, canvasRestoreFromTomb } from '$lib/lightspace-stores';

  // Auto-open on first tombstone
  $effect(() => {
    if ($tombstoneCount > 0 && !$lightspaceUiStore.cacheDrawerOpen) {
      lightspaceUiStore.update(s => ({ ...s, cacheDrawerOpen: true }));
    }
  });
</script>

{#if $tombstoneCount > 0}
  <div class="ls-subdrawer" class:ls-subdrawer-open={$lightspaceUiStore.cacheDrawerOpen} class:ls-subdrawer-flash={$lightspaceUiStore.tombFlash}>
    <button
      class="ls-subdrawer-head"
      onclick={() => lightspaceUiStore.update(s => ({ ...s, cacheDrawerOpen: !s.cacheDrawerOpen }))}
    >
      <span class="ls-subdrawer-chev">{$lightspaceUiStore.cacheDrawerOpen ? '▾' : '▸'}</span>
      <span>Cached Cards</span>
      <span class="ls-subdrawer-count">{$tombstoneCount} cached</span>
    </button>

    {#if $lightspaceUiStore.cacheDrawerOpen}
      <div class="ls-subdrawer-body">
        {#each $lightspaceCanvasStore.tombstones as tomb (tomb.id)}
          <div class="ls-cache-row" onclick={() => lightspaceFilesStore.update(s => ({ ...s, heroTombId: tomb.id }))} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && lightspaceFilesStore.update(s => ({ ...s, heroTombId: tomb.id }))}>
            <span class="ls-cache-kind">{tomb.kind.slice(0, 5).toUpperCase()}</span>
            <div class="ls-cache-info">
              <div class="ls-cache-title">{tomb.title}</div>
            </div>
            <button
              class="ls-cache-restore"
              onclick={(e) => { e.stopPropagation(); canvasRestoreFromTomb(tomb.id); }}
              title="Restore to canvas"
            >↩</button>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
.ls-subdrawer { border-top: 1px solid var(--ls-border-base); transition: background var(--ls-mid); }
.ls-subdrawer-flash { animation: ls-tomb-flash 0.9s ease; }
@keyframes ls-tomb-flash {
  0%   { box-shadow: inset 0 4px 12px -4px rgba(169,138,255,0.5); }
  100% { box-shadow: inset 0 0 0 0 transparent; }
}
.ls-subdrawer-head {
  display: flex; align-items: center; gap: 8px; padding: 8px 12px;
  font-size: 10px; letter-spacing: var(--ls-tk-mid); text-transform: uppercase;
  color: var(--ls-text-dim); cursor: pointer; width: 100%;
  background: transparent; border: 0; text-align: left;
  font-family: var(--ls-font-code); transition: color var(--ls-fast);
}
.ls-subdrawer-head:hover { color: var(--ls-text-bright); }
.ls-subdrawer-chev { font-size: 9px; color: var(--ls-text-mute); }
.ls-subdrawer-count { margin-left: auto; color: var(--ls-text-mute); font-size: 9px; }
.ls-subdrawer-body { padding: 4px 8px 8px; }
.ls-cache-row {
  display: flex; align-items: center; gap: 8px;
  padding: 5px 7px; cursor: pointer; border-radius: 2px;
  transition: background var(--ls-fast);
}
.ls-cache-row:hover { background: rgba(255,255,255,0.04); }
.ls-cache-kind {
  font-family: var(--ls-font-display); font-weight: 700;
  font-size: 7px; letter-spacing: var(--ls-tk-loose); color: var(--ls-text-mute);
  padding: 2px 4px; border: 1px solid var(--ls-border);
}
.ls-cache-info { flex: 1; min-width: 0; }
.ls-cache-title { font-size: 10px; color: var(--ls-text-dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ls-cache-restore {
  background: transparent; border: 1px solid var(--ls-border-base);
  color: var(--ls-text-mute); font-size: 10px; cursor: pointer; padding: 2px 6px;
  transition: all var(--ls-fast);
}
.ls-cache-restore:hover { color: var(--ls-text-bright); border-color: var(--ls-acc); }
</style>
