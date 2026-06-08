<!--
  @component FileHeroOverlay
  @description Full-screen file projection — renders file content when heroFileId is set.
    ESC dismisses; ⌘O opens externally.
  @contract none — reads files store; no SSE
  @reads lightspaceFilesStore.heroFileId, .files
  @mutates lightspaceFilesStore.heroFileId (close)
  @api GET /api/builds/:id/files/:fid — for file content on open
  @mockup-ref arch/lightspace-mockup.html → .la-hero-overlay, openHero(), closeHero()
-->
<script lang="ts">
  import { lightspaceFilesStore } from '$lib/lightspace-stores';

  const heroFile = $derived(
    $lightspaceFilesStore.heroFileId
      ? $lightspaceFilesStore.files.find(f => f.id === $lightspaceFilesStore.heroFileId)
      : null
  );

  function close() {
    lightspaceFilesStore.update(s => ({ ...s, heroFileId: null }));
  }

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
  }
</script>

<svelte:window onkeydown={handleKey} />

{#if heroFile}
  <div class="ls-hero-overlay" role="dialog" aria-modal="true" aria-label="File view: {heroFile.name}">
    <div class="ls-hero-head">
      <span class="ls-hero-mime">{heroFile.mime.toUpperCase()}</span>
      <span class="ls-hero-path">{heroFile.name}</span>
      <div class="ls-hero-actions">
        <button class="ls-hero-btn" onclick={close}>× close</button>
      </div>
    </div>
    <div class="ls-hero-body">
      <div class="ls-hero-placeholder">file content loads here — connect GET /api/builds/:id/files/:fid</div>
    </div>
    <div class="ls-hero-foot">
      <span><kbd>esc</kbd> close</span>
      <span>{heroFile.prov.agent} · {heroFile.meta}</span>
    </div>
  </div>
{/if}

<style>
.ls-hero-overlay {
  position: fixed; inset: 0; z-index: 80;
  display: flex; flex-direction: column;
  background: var(--ls-bg);
  animation: ls-hero-in 0.2s ease both;
}
@keyframes ls-hero-in { from { opacity: 0; } to { opacity: 1; } }

.ls-hero-head {
  display: flex; align-items: center; gap: 10px;
  padding: 10px 16px; border-bottom: 1px solid var(--ls-border-base);
  background: var(--ls-panel);
}
.ls-hero-mime {
  font-family: var(--ls-font-display); font-weight: 700;
  font-size: 9px; letter-spacing: var(--ls-tk-loose);
  color: var(--ls-acc-purple); padding: 2px 6px;
  border: 1px solid rgba(139,92,246,0.3);
}
.ls-hero-path { flex: 1; font-size: 12px; color: var(--ls-text-bright); font-family: var(--ls-font-code); }
.ls-hero-actions { display: flex; gap: 8px; }
.ls-hero-btn {
  background: transparent; border: 1px solid var(--ls-border-base);
  color: var(--ls-text-dim); font-size: 9px; cursor: pointer; padding: 3px 8px;
  text-transform: uppercase; letter-spacing: var(--ls-tk-mid); font-family: var(--ls-font-code);
  transition: all var(--ls-fast);
}
.ls-hero-btn:hover { color: var(--ls-text-bright); border-color: var(--ls-acc); }
.ls-hero-body { flex: 1; overflow: auto; padding: 20px; }
.ls-hero-placeholder { color: var(--ls-text-mute); font-size: 11px; font-style: italic; }
.ls-hero-foot {
  display: flex; justify-content: space-between;
  padding: 6px 16px; border-top: 1px solid var(--ls-border);
  font-size: 9px; color: var(--ls-text-ghost); letter-spacing: var(--ls-tk-mid);
}
.ls-hero-foot kbd {
  background: var(--ls-sunken); border: 1px solid var(--ls-border);
  padding: 1px 4px; font-size: 8px; font-family: var(--ls-font-code);
}
</style>
