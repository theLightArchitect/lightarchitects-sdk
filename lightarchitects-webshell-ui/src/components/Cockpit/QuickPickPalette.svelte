<script lang="ts">
  import { tick } from 'svelte';
  import { quickPickOpen, selectedTarget, selectedPreset, type CockpitTarget } from '$lib/cockpit/stores';
  import { registerHotkey } from '$lib/hotkeyRegistry';
  import { rank } from '$lib/cockpit/fuzzyMatch';
  import { getBuildList, getFileList, getBranchList } from '$lib/cockpit/localTargets';
  import { onMount } from 'svelte';

  type SourceMode = 'local' | 'github';

  let query = $state('');
  let sourceMode = $state<SourceMode>('local');
  let activeIdx = $state(-1);
  let results = $state<CockpitTarget[]>([]);
  let loading = $state(false);
  let inputEl: HTMLInputElement | undefined;

  const TYPE_ICON: Record<string, string> = {
    project: '⬡', build: '◈', phase: '◇', wave: '∿',
    file: '▣', commit: '◉', branch: '⎇', pr: '⌥',
  };

  function close() {
    quickPickOpen.set(false);
    query = '';
    activeIdx = -1;
    results = [];
  }

  function select(target: CockpitTarget) {
    selectedTarget.set(target);
    close();
  }

  function moveActive(delta: 1 | -1) {
    if (results.length === 0) return;
    activeIdx = (activeIdx + delta + results.length) % results.length;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); close(); return; }
    if (e.key === 'ArrowDown') { e.preventDefault(); moveActive(1); return; }
    if (e.key === 'ArrowUp') { e.preventDefault(); moveActive(-1); return; }
    if (e.key === 'Enter') {
      e.preventDefault();
      const t = activeIdx >= 0 ? results[activeIdx] : results[0];
      if (t) select(t);
    }
  }

  async function loadResults(q: string) {
    loading = true;
    activeIdx = -1;
    try {
      const [builds, files, branches] = await Promise.all([
        Promise.resolve(getBuildList()),
        getFileList(q),
        getBranchList(),
      ]);
      const all: CockpitTarget[] = [...builds, ...files, ...branches];
      results = q ? rank(q, all, t => t.label) : all.slice(0, 50);
    } catch {
      results = [];
    } finally {
      loading = false;
    }
  }

  // Debounce: reload on query change 120ms after last keystroke
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const q = query;
    const mode = sourceMode;
    void mode; // track mode reactively
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => { void loadResults(q); }, 120);
    return () => clearTimeout(debounceTimer);
  });

  // Load initial results when palette opens
  $effect(() => {
    if ($quickPickOpen) {
      void loadResults('');
      tick().then(() => inputEl?.focus());
    }
  });

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
    <!-- Search row -->
    <div class="qp-search-row">
      <span class="qp-hint" aria-hidden="true">⌘T</span>
      <input
        class="qp-input"
        type="text"
        placeholder="Build, file, branch, phase…"
        bind:value={query}
        bind:this={inputEl}
        onkeydown={handleKeydown}
        autocomplete="off"
        spellcheck="false"
        data-testid="quick-pick-input"
      />
      {#if loading}
        <span class="qp-loading" aria-hidden="true">…</span>
      {/if}
      <button class="qp-esc" onclick={close} aria-label="Close picker">ESC</button>
    </div>

    <!-- Source toggle -->
    <div class="qp-modes" role="tablist" aria-label="Target source">
      <button
        role="tab"
        class="qp-mode"
        class:qp-mode-active={sourceMode === 'local'}
        aria-selected={sourceMode === 'local'}
        onclick={() => { sourceMode = 'local'; }}
        data-testid="qp-mode-local"
      >Local</button>
      <button
        role="tab"
        class="qp-mode"
        class:qp-mode-active={sourceMode === 'github'}
        aria-selected={sourceMode === 'github'}
        onclick={() => { sourceMode = 'github'; }}
        data-testid="qp-mode-github"
      >GitHub</button>
    </div>

    <!-- Results -->
    <div class="qp-body" role="listbox" aria-label="Target results">
      {#if sourceMode === 'github'}
        <p class="qp-empty">GitHub sources connect in Phase 3.</p>
      {:else if results.length === 0 && !loading}
        <p class="qp-empty">{query ? 'No matches.' : 'Start typing to search.'}</p>
      {:else}
        {#each results.slice(0, 50) as target, i (target.id)}
          <!-- svelte-ignore a11y_interactive_supports_focus -->
          <div
            class="qp-item"
            class:qp-item-active={i === activeIdx}
            role="option"
            aria-selected={i === activeIdx}
            onclick={() => select(target)}
            onmouseenter={() => { activeIdx = i; }}
            data-testid="qp-item"
          >
            <span class="qp-type-icon" aria-hidden="true">{TYPE_ICON[target.type] ?? '◌'}</span>
            <span class="qp-type-label">{target.type}</span>
            <span class="qp-item-label">{target.label}</span>
          </div>
        {/each}
      {/if}
    </div>

    <!-- Footer: active preset context -->
    <div class="qp-footer" aria-live="polite">
      Preset: <strong>{$selectedPreset}</strong>
      {#if $selectedTarget}
        · current target: <span class="qp-current">{$selectedTarget.label}</span>
      {/if}
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
    max-height: 70vh;
    border-radius: 3px;
  }

  .qp-search-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--la-hair-base);
    flex-shrink: 0;
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

  .qp-loading {
    font-size: 10px;
    color: var(--la-text-mute);
    animation: pulse 1s infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }

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

  .qp-modes {
    display: flex;
    gap: 0;
    padding: 4px 14px;
    border-bottom: 1px solid var(--la-hair-base);
    flex-shrink: 0;
  }

  .qp-mode {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.06em;
    padding: 3px 8px;
    border: 1px solid var(--la-hair-base);
    background: transparent;
    color: var(--la-text-mute);
    cursor: pointer;
    font-family: var(--la-font-mono, monospace);
  }

  .qp-mode:first-child { border-radius: 2px 0 0 2px; }
  .qp-mode:last-child  { border-radius: 0 2px 2px 0; border-left: none; }

  .qp-mode-active {
    background: var(--la-struct-primary);
    color: var(--la-bg-base);
    border-color: var(--la-struct-primary);
  }

  .qp-body {
    overflow-y: auto;
    flex: 1;
    min-height: 60px;
    max-height: 300px;
  }

  .qp-empty {
    font-size: 10px;
    color: var(--la-text-mute);
    font-style: italic;
    text-align: center;
    padding: 20px 0;
    margin: 0;
  }

  .qp-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 14px;
    cursor: pointer;
    font-family: var(--la-font-mono, monospace);
    font-size: 11px;
    color: var(--la-text-base);
    border-bottom: 1px solid var(--la-hair-faint, var(--la-hair-base));
  }

  .qp-item:last-child { border-bottom: none; }

  .qp-item:hover, .qp-item-active {
    background: var(--la-bg-hover, color-mix(in srgb, var(--la-struct-primary) 10%, transparent));
    color: var(--la-text-bright);
  }

  .qp-type-icon {
    font-size: 10px;
    color: var(--la-struct-primary);
    flex-shrink: 0;
    width: 12px;
    text-align: center;
  }

  .qp-type-label {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--la-text-mute);
    flex-shrink: 0;
    width: 44px;
  }

  .qp-item-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }

  .qp-footer {
    padding: 4px 14px;
    font-size: 9px;
    color: var(--la-text-mute);
    border-top: 1px solid var(--la-hair-base);
    font-family: var(--la-font-mono, monospace);
    flex-shrink: 0;
  }

  .qp-current {
    color: var(--la-struct-primary);
  }
</style>
