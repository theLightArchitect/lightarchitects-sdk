<script lang="ts">
  /**
   * KeymapLegend — modal listing every registered keybinding (#4 / #102).
   *
   * Groups are derived live from the central hotkeyRegistry store.
   * Users can rebind any shortcut by clicking its row and pressing a new chord.
   * Overrides persist in localStorage via hotkeyRegistry.setUserOverride().
   */
  import {
    hotkeyRegistry,
    hotkeyOverrides,
    groupedEntries,
    setUserOverride,
    clearUserOverride,
    eventToChord,
  } from '$lib/hotkeyRegistry';

  let open = $state(false);

  // Reactive: re-groups whenever any component registers/deregisters.
  const groups = $derived(groupedEntries($hotkeyRegistry));

  // Rebind state — id of the entry currently waiting for a new key press.
  let capturingId = $state<string | null>(null);

  $effect(() => {
    function show() { open = true; }
    function hide() { open = false; }
    function toggle() { open = !open; }
    window.addEventListener('la:open-keymap-legend', show);
    window.addEventListener('la:close-keymap-legend', hide);
    window.addEventListener('la:toggle-keymap-legend', toggle);
    return () => {
      window.removeEventListener('la:open-keymap-legend', show);
      window.removeEventListener('la:close-keymap-legend', hide);
      window.removeEventListener('la:toggle-keymap-legend', toggle);
    };
  });

  function onKeydown(e: KeyboardEvent) {
    if (!open) return;

    if (capturingId !== null) {
      // Esc cancels capture without changing anything.
      if (e.key === 'Escape') {
        e.preventDefault();
        capturingId = null;
        return;
      }
      // Ignore bare modifier keys — wait for a complete chord.
      if (['Meta', 'Control', 'Alt', 'Shift'].includes(e.key)) return;
      e.preventDefault();
      setUserOverride(capturingId, eventToChord(e));
      capturingId = null;
      return;
    }

    if (e.key === 'Escape') {
      e.preventDefault();
      open = false;
    }
  }

  function startCapture(id: string) {
    capturingId = id;
  }

  function revertOverride(id: string) {
    clearUserOverride(id);
  }

  function hasOverride(id: string): boolean {
    return $hotkeyOverrides.has(id);
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="legend-scrim"
    role="dialog"
    aria-modal="true"
    aria-labelledby="legend-title"
    data-testid="keymap-legend"
    tabindex={-1}
    onclick={() => { if (capturingId) { capturingId = null; } else { open = false; } }}
  >
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="legend-modal" onclick={(e) => e.stopPropagation()} role="presentation">
      <header class="legend-header">
        <h2 id="legend-title">Keyboard shortcuts</h2>
        <span class="legend-hint">Click any row to rebind</span>
        <button class="legend-close" aria-label="Close" onclick={() => { open = false; }}>×</button>
      </header>

      <div class="legend-body">
        {#each groups as group (group.title)}
          <section class="legend-group">
            <h3>{group.title}</h3>
            <table>
              <tbody>
                {#each group.rows as row}
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <tr
                    class="legend-row"
                    class:capturing={capturingId === row.id}
                    class:overridden={hasOverride(row.id)}
                    role="button"
                    tabindex={0}
                    aria-label="Rebind {row.label}"
                    onclick={() => startCapture(row.id)}
                    onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') startCapture(row.id); }}
                  >
                    <td class="legend-keys">
                      {#if capturingId === row.id}
                        <span class="capture-prompt">Press new key…</span>
                      {:else}
                        {#each row.keys as k, i}
                          {#if i > 0}<span class="legend-plus">+</span>{/if}
                          <kbd>{k}</kbd>
                        {/each}
                        {#if hasOverride(row.id)}
                          <button
                            class="revert-btn"
                            title="Revert to default"
                            aria-label="Revert {row.label} to default"
                            onclick={(e) => { e.stopPropagation(); revertOverride(row.id); }}
                          >↺</button>
                        {/if}
                      {/if}
                    </td>
                    <td class="legend-label">{row.label}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </section>
        {/each}

        <!-- URL-param tips aren't keyboard shortcuts — static appendix -->
        <section class="legend-group">
          <h3>Tutorials</h3>
          <table>
            <tbody>
              <tr class="legend-row static">
                <td class="legend-keys"><kbd>?onboarding=t1</kbd></td>
                <td class="legend-label">Re-run T1 First Build (URL param)</td>
              </tr>
            </tbody>
          </table>
        </section>
      </div>

      <footer class="legend-footer">
        {#if capturingId}
          <span class="capture-hint">Press new chord — <kbd>Esc</kbd> to cancel</span>
        {:else}
          <span><kbd>Esc</kbd> to dismiss · click any row to rebind</span>
        {/if}
      </footer>
    </div>
  </div>
{/if}

<style>
  .legend-scrim {
    position: fixed;
    inset: 0;
    z-index: 80;
    background: var(--la-scrim-color);
    backdrop-filter: blur(var(--la-scrim-blur));
    display: grid;
    place-items: center;
    animation: legend-fade-in var(--la-transition-fast) ease-out;
  }
  .legend-modal {
    width: min(560px, 92vw);
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    background: var(--la-drawer-bg);
    border: 1px solid var(--la-drawer-border);
    border-radius: var(--la-radius-lg);
    box-shadow: var(--la-drawer-shadow);
    color: var(--la-text-body);
    font-family: var(--la-font-chrome);
  }
  .legend-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px 8px;
    border-bottom: 1px solid var(--la-drawer-border);
  }
  .legend-header h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: #FFD700;
    letter-spacing: 0.02em;
    flex: 1;
  }
  .legend-hint {
    font-size: 10px;
    color: var(--la-text-dim);
    letter-spacing: 0.04em;
  }
  .legend-close {
    background: transparent;
    border: none;
    color: var(--la-text-mute);
    font-size: 20px;
    line-height: 1;
    cursor: pointer;
    padding: 0 4px;
    border-radius: var(--la-radius-sm);
    transition: color var(--la-transition-fast), background var(--la-transition-fast);
  }
  .legend-close:hover { color: var(--la-text-body); background: #1e293b; }

  .legend-body {
    flex: 1;
    overflow-y: auto;
    padding: 8px 16px;
  }
  .legend-group {
    padding: 8px 0;
    border-bottom: 1px solid var(--la-drawer-border);
  }
  .legend-group:last-child { border-bottom: none; }
  .legend-group h3 {
    margin: 0 0 6px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--la-text-mute);
  }
  .legend-group table { width: 100%; border-collapse: collapse; }
  .legend-group td { padding: 3px 0; vertical-align: middle; }

  .legend-row {
    cursor: pointer;
    border-radius: var(--la-radius-sm);
    transition: background var(--la-transition-fast);
  }
  .legend-row:hover:not(.static) td { background: rgba(255, 215, 0, 0.04); }
  .legend-row.capturing td { background: rgba(255, 215, 0, 0.08); }
  .legend-row.overridden .legend-keys kbd { color: #4dff8e; border-color: rgba(77,255,142,0.4); }
  .legend-row.static { cursor: default; }

  .legend-keys {
    width: 150px;
    white-space: nowrap;
  }
  .legend-keys kbd {
    display: inline-block;
    padding: 1px 6px;
    background: #1e293b;
    border: 1px solid #334155;
    border-bottom-width: 2px;
    border-radius: var(--la-radius-sm);
    color: #FFD700;
    font-family: var(--la-font-mono);
    font-size: 10px;
    transition: color var(--la-transition-fast), border-color var(--la-transition-fast);
  }
  .legend-plus {
    margin: 0 4px;
    color: var(--la-text-dim);
    font-size: 10px;
  }
  .legend-label {
    color: var(--la-text-body);
    font-size: 11px;
  }

  .capture-prompt {
    font-size: 10px;
    color: #FFD700;
    font-family: var(--la-font-mono);
    letter-spacing: 0.06em;
    animation: blink 0.8s step-end infinite;
  }

  .revert-btn {
    background: transparent;
    border: none;
    color: var(--la-text-dim);
    font-size: 11px;
    cursor: pointer;
    padding: 0 2px;
    margin-left: 4px;
    border-radius: var(--la-radius-sm);
    vertical-align: middle;
    transition: color var(--la-transition-fast);
  }
  .revert-btn:hover { color: #FFD700; }

  .legend-footer {
    padding: 8px 16px;
    border-top: 1px solid var(--la-drawer-border);
    color: var(--la-text-mute);
    font-size: 10px;
    text-align: right;
  }
  .legend-footer kbd {
    background: #1e293b;
    padding: 1px 4px;
    border-radius: var(--la-radius-sm);
    font-family: var(--la-font-mono);
  }
  .capture-hint {
    color: #FFD700;
    font-size: 10px;
  }

  @keyframes legend-fade-in {
    from { opacity: 0; }
    to   { opacity: 1; }
  }
  @keyframes blink {
    50% { opacity: 0; }
  }
</style>
