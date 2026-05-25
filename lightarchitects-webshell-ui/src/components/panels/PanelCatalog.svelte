<script lang="ts">
  import { layoutTree, addPanel, saveCustomPreset, collectPanelIds } from '$lib/layout';
  import type { PanelId } from '$lib/types';

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  const PANELS: { id: PanelId; label: string; desc: string }[] = [
    { id: 'terminal',      label: 'Terminal',        desc: 'Run commands, tests, git push' },
    { id: 'agent-console', label: 'Agent Console',   desc: 'Live agent output and events' },
    { id: 'file-diff',     label: 'Diff Viewer',     desc: 'What changed in each file' },
    { id: 'file-explorer', label: 'File Explorer',   desc: 'Navigate and open project files' },
    { id: 'build-status',  label: 'Build Status',    desc: 'CI runs, test results, lint' },
    { id: 'findings',      label: 'Findings',        desc: 'Quality and security issues' },
    { id: 'git-forest',    label: 'Git Branches',    desc: 'Branch topology and commits' },
    { id: 'helix',         label: 'Knowledge Graph', desc: '3D project memory graph' },
    { id: 'ayin-traces',      label: 'AYIN Traces',        desc: 'Live trace dataflow diagrams from AYIN at :3742' },
    { id: 'helix-retrieve',   label: 'Helix Retrieve',     desc: 'Cached hybrid-retrieval results and mode metrics' },
    { id: 'helix-cache-stats', label: 'Helix Cache Stats', desc: 'TinyLFU cache entry count and byte weight' },
  ];

  let inLayout = $derived(collectPanelIds($layoutTree));
  let saveName = $state('');
  let showSaveInput = $state(false);

  function handleSave() {
    const name = saveName.trim();
    if (!name) return;
    saveCustomPreset(name);
    saveName = '';
    showSaveInput = false;
  }
</script>

<div class="catalog" role="dialog" aria-label="Panel catalog">
  <div class="catalog-header">
    <span class="catalog-title">ADD PANEL</span>
    <button class="catalog-close" onclick={onClose} aria-label="Close" data-testid="catalog-close-btn">×</button>
  </div>

  <div class="panel-list">
    {#each PANELS as p}
      {@const active = inLayout.has(p.id)}
      <button
        class="panel-item"
        class:in-layout={active}
        onclick={() => { if (!active) addPanel(p.id); }}
        disabled={active}
        data-testid="catalog-add-{p.id}"
      >
        <span class="panel-label">{p.label}</span>
        <span class="panel-desc">{p.desc}</span>
        {#if active}
          <span class="badge-in">IN LAYOUT</span>
        {:else}
          <span class="badge-add">+ ADD</span>
        {/if}
      </button>
    {/each}
  </div>

  <div class="catalog-footer">
    {#if showSaveInput}
      <div class="save-row">
        <!-- svelte-ignore a11y_autofocus -->
        <input
          type="text"
          bind:value={saveName}
          placeholder="Preset name…"
          class="save-input"
          autofocus
          onkeydown={(e) => {
            if (e.key === 'Enter') handleSave();
            if (e.key === 'Escape') { showSaveInput = false; saveName = ''; }
          }}
        />
        <button class="save-confirm" onclick={handleSave} disabled={!saveName.trim()}>SAVE</button>
      </div>
    {:else}
      <button class="save-btn" onclick={() => showSaveInput = true}>
        Save layout as preset…
      </button>
    {/if}
  </div>
</div>

<style>
  .catalog {
    width: 224px;
    background: var(--la-bg-elev-2, #16181b);
    border: 1px solid var(--la-hair-strong);
    display: flex;
    flex-direction: column;
    font-family: var(--la-font-mono);
    box-shadow: -4px 0 16px rgba(0, 0, 0, 0.5);
  }

  .catalog-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 10px;
    border-bottom: 1px solid var(--la-hair-base);
    flex-shrink: 0;
  }
  .catalog-title {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.14em;
    color: var(--la-struct-primary);
  }
  .catalog-close {
    background: none;
    border: none;
    color: var(--la-text-mute);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    padding: 0 2px;
  }
  .catalog-close:hover { color: var(--la-text-bright); }

  .panel-list {
    flex: 1;
    overflow-y: auto;
  }

  .panel-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 7px 10px;
    background: none;
    border: none;
    border-bottom: 1px solid var(--la-hair-faint);
    cursor: pointer;
    position: relative;
  }
  .panel-item:hover:not(.in-layout) { background: var(--la-bg-elev-1); }
  .panel-item.in-layout { opacity: 0.4; cursor: default; }

  .panel-label {
    display: block;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: var(--la-text-bright);
    margin-bottom: 2px;
  }
  .panel-desc {
    display: block;
    font-size: 8px;
    color: var(--la-text-mute);
    line-height: 1.4;
  }

  .badge-add {
    position: absolute;
    right: 10px;
    top: 50%;
    transform: translateY(-50%);
    font-size: 8px;
    font-weight: 700;
    color: var(--la-struct-primary);
    opacity: 0;
    transition: opacity 80ms;
  }
  .panel-item:hover:not(.in-layout) .badge-add { opacity: 1; }

  .badge-in {
    position: absolute;
    right: 10px;
    top: 50%;
    transform: translateY(-50%);
    font-size: 7px;
    letter-spacing: 0.08em;
    color: var(--la-text-mute);
  }

  .catalog-footer {
    padding: 8px 10px;
    border-top: 1px solid var(--la-hair-base);
    flex-shrink: 0;
  }
  .save-btn {
    width: 100%;
    text-align: left;
    background: none;
    border: 1px solid var(--la-hair-strong);
    color: var(--la-text-dim);
    font-size: 8px;
    font-family: var(--la-font-mono);
    letter-spacing: 0.06em;
    padding: 5px 8px;
    cursor: pointer;
    transition: color 80ms, border-color 80ms;
  }
  .save-btn:hover { color: var(--la-struct-primary); border-color: var(--la-struct-primary); }

  .save-row { display: flex; gap: 4px; }
  .save-input {
    flex: 1;
    background: var(--la-bg-elev-1);
    border: 1px solid var(--la-hair-strong);
    color: var(--la-text-bright);
    font-size: 9px;
    font-family: var(--la-font-mono);
    padding: 4px 6px;
    outline: none;
    min-width: 0;
  }
  .save-input:focus { border-color: var(--la-struct-primary); }
  .save-confirm {
    background: var(--la-struct-primary);
    border: none;
    color: var(--la-bg-void);
    font-size: 8px;
    font-family: var(--la-font-mono);
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 4px 8px;
    cursor: pointer;
    flex-shrink: 0;
  }
  .save-confirm:disabled { opacity: 0.4; cursor: default; }
</style>
