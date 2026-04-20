<script lang="ts">
  import {
    step, settingsOpen, selectedBackend, selectedAgent, selectedModel,
    persistedConfig, availableModels, loadModels, saveSetup, setupLoading,
    ollamaBaseUrlInput,
  } from '$lib/setup';

  const backends = [
    { id: 'anthropic', agent: 'lightarchitects', label: 'Claude Code' },
    { id: 'openai', agent: 'codex', label: 'Codex' },
    { id: 'ollama-launch', agent: 'lightarchitects', label: 'Ollama' },
  ];

  let pickedBackend = $state($selectedBackend ?? $persistedConfig?.backend ?? 'anthropic');
  let pickedModel = $state<string | null>($selectedModel ?? $persistedConfig?.model ?? null);
  let toast = $state<string | null>(null);

  $effect(() => {
    if (pickedBackend) {
      loadModels(pickedBackend, pickedBackend.includes('ollama') ? $ollamaBaseUrlInput : undefined);
    }
  });

  $effect(() => {
    if ($availableModels.length > 0 && !pickedModel) {
      pickedModel = $availableModels[0].id;
    }
  });

  async function apply() {
    if (!pickedBackend || !pickedModel) return;
    const agent = backends.find(b => b.id === pickedBackend)?.agent ?? 'lightarchitects';
    selectedBackend.set(pickedBackend);
    selectedAgent.set(agent);
    selectedModel.set(pickedModel);
    await saveSetup();
    toast = `Switched to ${pickedBackend}`;
    setTimeout(() => { toast = null; settingsOpen.set(false); }, 4000);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') settingsOpen.set(false);
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if toast}
  <div class="toast">{toast}</div>
{/if}

<div class="overlay">
  <div class="panel">
    <div class="panel-header">
      <span class="panel-title">Backend Settings</span>
      <button class="close-btn" onclick={() => settingsOpen.set(false)}>✕</button>
    </div>

    {#if $persistedConfig}
      <div class="current-badge">
        Current: <strong>{$persistedConfig.backend}</strong>
        {#if $persistedConfig.model}/ {$persistedConfig.model}{/if}
      </div>
    {/if}

    <div class="section-label">Backend</div>
    <div class="backend-row">
      {#each backends as b}
        <button
          class="backend-btn"
          class:selected={pickedBackend === b.id}
          onclick={() => { pickedBackend = b.id; pickedModel = null; }}
        >{b.label}</button>
      {/each}
    </div>

    {#if $setupLoading}
      <div class="loading-row">Loading models…</div>
    {:else if $availableModels.length > 0}
      <div class="section-label">Model</div>
      <select class="model-select" bind:value={pickedModel}>
        {#each $availableModels as m}
          <option value={m.id}>{m.label || m.id}</option>
        {/each}
      </select>
    {/if}

    <div class="actions">
      <button class="btn-reset" onclick={() => { step.set('splash'); settingsOpen.set(false); }}>
        Reset setup
      </button>
      <button
        class="btn-apply"
        disabled={!pickedModel || $setupLoading}
        onclick={apply}
      >
        Save & Apply
      </button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: absolute; top: 2.5rem; right: 0.5rem; z-index: 200;
  }

  .panel {
    background: #0f172a; border: 1px solid #334155; border-radius: 8px;
    padding: 1rem; width: 280px; box-shadow: 0 8px 32px rgba(0,0,0,0.6);
    display: flex; flex-direction: column; gap: 0.75rem;
  }

  .panel-header { display: flex; align-items: center; justify-content: space-between; }
  .panel-title { font-family: 'IBM Plex Mono', monospace; font-size: 0.8rem; color: #94a3b8; letter-spacing: 0.05em; }
  .close-btn { background: none; border: none; color: #475569; cursor: pointer; font-size: 0.9rem; }
  .close-btn:hover { color: #94a3b8; }

  .current-badge { font-family: 'IBM Plex Mono', monospace; font-size: 0.7rem; color: #475569; }
  .current-badge strong { color: #94a3b8; }

  .section-label { font-family: 'IBM Plex Mono', monospace; font-size: 0.65rem; color: #475569; letter-spacing: 0.1em; text-transform: uppercase; }

  .backend-row { display: flex; gap: 0.5rem; }
  .backend-btn { flex: 1; background: #1e293b; border: 1px solid #334155; color: #64748b; border-radius: 6px; padding: 0.35rem; font-family: 'IBM Plex Mono', monospace; font-size: 0.7rem; cursor: pointer; transition: all 0.15s; }
  .backend-btn:hover { color: #94a3b8; }
  .backend-btn.selected { border-color: #ff6600; color: #ff6600; background: rgba(255,102,0,0.08); }

  .loading-row { font-family: 'IBM Plex Mono', monospace; font-size: 0.75rem; color: #475569; }
  .model-select { background: #1e293b; border: 1px solid #334155; color: #94a3b8; border-radius: 6px; padding: 0.4rem; font-family: 'IBM Plex Mono', monospace; font-size: 0.75rem; width: 100%; }

  .actions { display: flex; justify-content: space-between; margin-top: 0.25rem; }
  .btn-reset { background: none; border: 1px solid #334155; color: #475569; border-radius: 6px; padding: 0.4rem 0.75rem; font-family: 'IBM Plex Mono', monospace; font-size: 0.7rem; cursor: pointer; }
  .btn-reset:hover { color: #64748b; }
  .btn-apply { background: #ff6600; border: none; color: #fff; border-radius: 6px; padding: 0.4rem 1rem; font-family: 'IBM Plex Mono', monospace; font-size: 0.7rem; font-weight: 600; cursor: pointer; transition: opacity 0.15s; }
  .btn-apply:disabled { opacity: 0.35; cursor: not-allowed; }

  .toast {
    position: fixed; bottom: 1.5rem; left: 50%; transform: translateX(-50%);
    background: #1e293b; border: 1px solid #334155; color: #94a3b8;
    font-family: 'IBM Plex Mono', monospace; font-size: 0.75rem;
    padding: 0.5rem 1.25rem; border-radius: 6px;
    z-index: 9999; animation: fadein 0.2s ease;
  }
  @keyframes fadein { from { opacity: 0; transform: translateX(-50%) translateY(8px); } to { opacity: 1; transform: translateX(-50%) translateY(0); } }
</style>
