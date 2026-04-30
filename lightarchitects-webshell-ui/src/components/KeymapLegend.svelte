<script lang="ts">
  /**
   * KeymapLegend — modal listing every global keybinding (#4).
   *
   * Triggered by Cmd+/ (or Ctrl+/ on non-mac). Listens for the
   * `la:open-keymap-legend` custom event, dispatched by app.svelte's
   * keydown handler. Esc dismisses.
   *
   * Adding a new keybinding? Add a row here so it stays discoverable.
   */
  let open = $state(false);

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

  function onEsc(e: KeyboardEvent) {
    if (open && e.key === 'Escape') {
      e.preventDefault();
      open = false;
    }
  }

  // Single source of truth for the legend rows. Group order = onboarding
  // order: navigation first (most-used), drawers second, dispatch+input
  // third, system last.
  const groups: { title: string; rows: { keys: string[]; label: string }[] }[] = [
    {
      title: 'Navigation',
      rows: [
        { keys: ['⌘', 'K'], label: 'Open Squad Dispatch' },
        { keys: ['⌘', '/'], label: 'Open this keymap legend' },
        { keys: ['Esc'],    label: 'Close any modal / dismiss banner' },
      ],
    },
    {
      title: 'Drawers',
      rows: [
        { keys: ['⌃', '`'], label: 'Toggle Copilot drawer' },
        { keys: ['⌘', 'M'], label: 'Toggle Memory drawer (Hot · Cold · Convergences)' },
      ],
    },
    {
      title: 'Dispatch + Plan',
      rows: [
        { keys: ['↑'], label: 'Previous suggestion (CommandPalette / autocomplete)' },
        { keys: ['↓'], label: 'Next suggestion' },
        { keys: ['↵'], label: 'Select / submit' },
      ],
    },
    {
      title: 'Tutorials',
      rows: [
        { keys: ['?onboarding=t1'], label: 'Re-run T1 First Build (URL param)' },
      ],
    },
  ];
</script>

<svelte:window onkeydown={onEsc} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="legend-scrim"
    role="dialog"
    aria-modal="true"
    aria-labelledby="legend-title"
    data-testid="keymap-legend"
    tabindex={-1}
    onclick={() => { open = false; }}
  >
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="legend-modal" onclick={(e) => e.stopPropagation()} role="presentation">
      <header class="legend-header">
        <h2 id="legend-title">Keyboard shortcuts</h2>
        <button class="legend-close" aria-label="Close" onclick={() => { open = false; }}>×</button>
      </header>

      <div class="legend-body">
        {#each groups as group (group.title)}
          <section class="legend-group">
            <h3>{group.title}</h3>
            <table>
              <tbody>
                {#each group.rows as row}
                  <tr>
                    <td class="legend-keys">
                      {#each row.keys as k, i}
                        {#if i > 0}<span class="legend-plus">+</span>{/if}
                        <kbd>{k}</kbd>
                      {/each}
                    </td>
                    <td class="legend-label">{row.label}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </section>
        {/each}
      </div>

      <footer class="legend-footer">
        <span><kbd>Esc</kbd> to dismiss</span>
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
    width: min(540px, 92vw);
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
    justify-content: space-between;
    padding: 12px 16px 8px;
    border-bottom: 1px solid var(--la-drawer-border);
  }
  .legend-header h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: #FFD700;
    letter-spacing: 0.02em;
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
  .legend-keys {
    width: 130px;
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

  @keyframes legend-fade-in {
    from { opacity: 0; }
    to   { opacity: 1; }
  }
</style>
