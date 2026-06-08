<!--
  @component TombHeroOverlay
  @description 620px centered modal for cached-card preview. Shows tombstone snapshot.
    Restore button returns card to canvas.
  @contract none — reads tombstones; no SSE
  @reads lightspaceFilesStore.heroTombId, lightspaceCanvasStore.tombstones
  @mutates lightspaceFilesStore.heroTombId (close), lightspaceCanvasStore (restore)
  @api none — tombstone data is fully local (captured at eviction time)
  @mockup-ref arch/lightspace-mockup.html → .la-tomb-hero, openTombHero(), closeTombHero()
-->
<script lang="ts">
  import { lightspaceFilesStore, lightspaceCanvasStore, canvasRestoreFromTomb } from '$lib/lightspace-stores';

  const heroTomb = $derived(
    $lightspaceFilesStore.heroTombId
      ? $lightspaceCanvasStore.tombstones.find(t => t.id === $lightspaceFilesStore.heroTombId)
      : null
  );

  function close() {
    lightspaceFilesStore.update(s => ({ ...s, heroTombId: null }));
  }

  function restore() {
    if ($lightspaceFilesStore.heroTombId) {
      canvasRestoreFromTomb($lightspaceFilesStore.heroTombId);
      close();
    }
  }

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
  }
</script>

<svelte:window onkeydown={handleKey} />

{#if heroTomb}
  <div class="ls-tomb-hero-backdrop" onclick={close} role="presentation"></div>
  <div class="ls-tomb-hero-frame" role="dialog" aria-modal="true" aria-label="Cached card: {heroTomb.title}">
    <div class="ls-tomb-hero-head">
      <span class="ls-tomb-hero-label">CACHED CARD</span>
      <div class="ls-tomb-hero-actions">
        <button class="ls-hero-btn ls-hero-btn-primary" onclick={restore}>↩ restore</button>
        <button class="ls-hero-btn" onclick={close}>×</button>
      </div>
    </div>
    <div class="ls-tomb-hero-body">
      <div class="ls-tomb-meta">
        <span class="ls-tomb-kind">{heroTomb.kind.toUpperCase()}</span>
        <span class="ls-tomb-title">{heroTomb.title}</span>
        <span class="ls-tomb-evicted">evicted {new Date(heroTomb.evictedAt).toLocaleTimeString()}</span>
      </div>
    </div>
  </div>
{/if}

<style>
.ls-tomb-hero-backdrop {
  position: fixed; inset: 0; z-index: 85;
  background: rgba(0,0,0,0.6); backdrop-filter: blur(2px);
}
.ls-tomb-hero-frame {
  position: fixed;
  top: 50%; left: 50%;
  transform: translate(-50%, -50%);
  z-index: 86;
  width: min(620px, 90vw);
  background: var(--ls-panel);
  border: 1px solid var(--ls-border-strong);
  display: flex; flex-direction: column;
  animation: ls-hero-in 0.2s ease both;
}
@keyframes ls-hero-in { from { opacity: 0; transform: translate(-50%, -48%); } to { opacity: 1; transform: translate(-50%, -50%); } }
.ls-tomb-hero-head {
  display: flex; align-items: center; justify-content: space-between;
  padding: 10px 14px; border-bottom: 1px solid var(--ls-border-base);
}
.ls-tomb-hero-label {
  font-family: var(--ls-font-display); font-weight: 700;
  font-size: 9px; letter-spacing: var(--ls-tk-loose); color: var(--ls-text-mute);
}
.ls-tomb-hero-actions { display: flex; gap: 8px; }
.ls-hero-btn {
  background: transparent; border: 1px solid var(--ls-border-base);
  color: var(--ls-text-dim); font-size: 9px; cursor: pointer; padding: 3px 8px;
  text-transform: uppercase; letter-spacing: var(--ls-tk-mid); font-family: var(--ls-font-code);
  transition: all var(--ls-fast);
}
.ls-hero-btn:hover { color: var(--ls-text-bright); border-color: var(--ls-acc); }
.ls-hero-btn-primary { border-color: var(--ls-acc); color: var(--ls-acc); }
.ls-tomb-hero-body { padding: 16px; }
.ls-tomb-meta { display: flex; flex-direction: column; gap: 4px; }
.ls-tomb-kind { font-size: 8px; text-transform: uppercase; letter-spacing: var(--ls-tk-loose); color: var(--ls-text-mute); }
.ls-tomb-title { font-size: 14px; color: var(--ls-text-bright); font-weight: 500; }
.ls-tomb-evicted { font-size: 9px; color: var(--ls-text-ghost); }
</style>
