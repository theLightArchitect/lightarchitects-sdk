<script lang="ts">
  import { quickPickOpen } from '$lib/cockpit/stores';
  import { registerHotkey } from '$lib/hotkeyRegistry';
  import { onMount } from 'svelte';

  let query = $state('');

  function close() {
    quickPickOpen.set(false);
    query = '';
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); close(); }
  }

  onMount(() => {
    const unreg = registerHotkey({
      id: 'cockpit-quick-pick',
      keys: ['⌘', 'T'],
      label: 'Open target picker',
      group: 'Cockpit',
      scope: 'global',
      matches: e => (e.metaKey || e.ctrlKey) && e.key === 't',
      handler: () => quickPickOpen.update(v => !v),
    });
    return unreg;
  });
</script>

{#if $quickPickOpen}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="qp-backdrop"
    onclick={close}
    onkeydown={handleKeydown}
  ></div>

  <div
    class="qp-panel"
    role="dialog"
    aria-label="Target picker"
    aria-modal="true"
  >
    <div class="qp-search-row">
      <span class="qp-hint" aria-hidden="true">⌘T</span>
      <input
        class="qp-input"
        type="text"
        placeholder="Pick a target — build, phase, file, branch, PR…"
        bind:value={query}
        onkeydown={handleKeydown}
        autofocus
        autocomplete="off"
        spellcheck="false"
        data-testid="quick-pick-input"
      />
      <button class="qp-esc" onclick={close} aria-label="Close picker">ESC</button>
    </div>
    <div class="qp-body">
      <p class="qp-empty">Target sources connect in Phase 2.</p>
    </div>
  </div>
{/if}

<style>
  .qp-backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    background: rgba(0, 0, 0, 0.5);
  }

  .qp-panel {
    position: fixed;
    top: 20%;
    left: 50%;
    transform: translateX(-50%);
    z-index: 91;
    width: min(560px, 90vw);
    background: var(--la-bg-panel);
    border: 1px solid var(--la-hair-strong);
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.6);
    display: flex;
    flex-direction: column;
  }

  .qp-search-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--la-hair-base);
  }

  .qp-hint {
    font-size: 9px;
    font-weight: 700;
    color: var(--la-text-mute);
    font-family: var(--la-font-mono, monospace);
    letter-spacing: 0.06em;
    flex-shrink: 0;
  }

  .qp-input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    font-size: 12px;
    color: var(--la-text-bright);
    font-family: var(--la-font-sans, sans-serif);
  }

  .qp-input::placeholder { color: var(--la-text-mute); }

  .qp-esc {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 2px 5px;
    border: 1px solid var(--la-hair-base);
    background: transparent;
    color: var(--la-text-mute);
    cursor: pointer;
    border-radius: 2px;
    font-family: var(--la-font-mono, monospace);
    flex-shrink: 0;
  }

  .qp-esc:hover { color: var(--la-text-base); }

  .qp-body {
    padding: 12px 14px;
    min-height: 80px;
    max-height: 320px;
    overflow-y: auto;
  }

  .qp-empty {
    font-size: 10px;
    color: var(--la-text-mute);
    font-style: italic;
    text-align: center;
    padding: 20px 0;
    margin: 0;
  }
</style>
