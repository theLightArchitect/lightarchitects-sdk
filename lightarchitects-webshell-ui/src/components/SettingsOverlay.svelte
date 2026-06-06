<script lang="ts">
  import {
    step, settingsOpen, selectedBackend, selectedAgent, selectedModel,
    persistedConfig, availableModels, loadModels, saveSetup, setupLoading,
    ollamaBaseUrlInput,
  } from '$lib/setup';
  import { saveSettingsDebounced } from '$lib/settings-persistence';
  import { listSkills, resolveSkill, type SkillOverlay } from '$lib/skill-resolver';
  import { listPersonas, resolvePersona, type PersonaOverlay } from '$lib/persona-resolver';
  import { devModeEnabled } from '$lib/stores';
  import CredentialsPanel from './CredentialsPanel.svelte';

  const backends = [
    { id: 'lightarchitects', agent: 'light_architect', label: 'LA Native' },
    { id: 'ollama-launch', agent: 'lightarchitects', label: 'Ollama' },
    { id: 'anthropic', agent: 'lightarchitects', label: 'Claude Code' },
    { id: 'openai', agent: 'codex', label: 'Codex' },
    { id: 'mistral-vibe', agent: 'mistral_vibe', label: 'Mistral Vibe' },
  ];

  let pickedBackend = $state($selectedBackend ?? $persistedConfig?.backend ?? 'lightarchitects');
  let pickedModel = $state<string | null>($selectedModel ?? $persistedConfig?.model ?? null);
  let toast = $state<string | null>(null);

  // Tab state
  let activeTab = $state<'backend' | 'personas' | 'skills' | 'credentials'>('backend');
  let skillsList = $state<SkillOverlay[]>([]);
  let personasList = $state<PersonaOverlay[]>([]);
  let loadingSkills = $state(false);
  let loadingPersonas = $state(false);

  $effect(() => {
    if (pickedBackend === 'mistral-vibe') {
      availableModels.set([{ id: '', label: 'default (from ~/.vibe/config.toml)', tier: '' }]);
    } else if (pickedBackend) {
      loadModels(pickedBackend, pickedBackend.includes('ollama') ? $ollamaBaseUrlInput : undefined);
    }
  });

  $effect(() => {
    if ($availableModels.length > 0 && !pickedModel) {
      pickedModel = $availableModels[0].id;
    }
  });

  // Load skills when tab opens
  $effect(() => {
    if (activeTab === 'skills') {
      loadingSkills = true;
      listSkills(50).then((skills) => {
        skillsList = skills.map(s => ({ ...s, source: 'platform' as const, is_override: false }));
        loadingSkills = false;
      });
    }
  });

  // Load personas when tab opens
  $effect(() => {
    if (activeTab === 'personas') {
      loadingPersonas = true;
      listPersonas(50).then((personas) => {
        personasList = personas.map(p => ({ ...p, source: 'platform' as const, is_override: false }));
        loadingPersonas = false;
      });
    }
  });

  async function apply() {
    if (!pickedBackend || pickedModel === undefined || pickedModel === null) return;
    const agent = backends.find(b => b.id === pickedBackend)?.agent ?? 'lightarchitects';
    selectedBackend.set(pickedBackend);
    selectedAgent.set(agent);
    selectedModel.set(pickedModel || null);
    await saveSetup();
    saveSettingsDebounced();
    toast = `Switched to ${pickedBackend}`;
    setTimeout(() => { toast = null; settingsOpen.set(false); }, 4000);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') settingsOpen.set(false);
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if toast}
  <div class="toast" role="status" aria-live="polite">{toast}</div>
{/if}

<div class="overlay">
  <div class="panel">
    <div class="panel-header">
      <span class="panel-title">Settings</span>
      <button class="close-btn" aria-label="Close settings" onclick={() => settingsOpen.set(false)}>✕</button>
    </div>

    <!-- Tab navigation -->
    <div class="tabs" role="tablist">
      <button
        role="tab"
        id="tab-backend"
        aria-controls="panel-backend"
        class="tab-btn"
        class:selected={activeTab === 'backend'}
        aria-selected={activeTab === 'backend'}
        onclick={() => { activeTab = 'backend'; }}
      >
        Backend
      </button>
      <button
        role="tab"
        id="tab-personas"
        aria-controls="panel-personas"
        class="tab-btn"
        class:selected={activeTab === 'personas'}
        aria-selected={activeTab === 'personas'}
        onclick={() => { activeTab = 'personas'; }}
      >
        Personas
      </button>
      <button
        role="tab"
        id="tab-skills"
        aria-controls="panel-skills"
        class="tab-btn"
        class:selected={activeTab === 'skills'}
        aria-selected={activeTab === 'skills'}
        onclick={() => { activeTab = 'skills'; }}
      >
        Skills
      </button>
      <button
        role="tab"
        id="tab-credentials"
        aria-controls="panel-credentials"
        class="tab-btn"
        class:selected={activeTab === 'credentials'}
        aria-selected={activeTab === 'credentials'}
        onclick={() => { activeTab = 'credentials'; }}
      >
        Credentials
      </button>
      {#if import.meta.env.DEV}
        <button
          role="tab"
          id="tab-dev"
          aria-controls="panel-dev"
          class="tab-btn"
          class:selected={activeTab === 'dev'}
          aria-selected={activeTab === 'dev'}
          onclick={() => { activeTab = 'dev'; }}
        >
          Dev
        </button>
      {/if}
    </div>

    <!-- Backend tab -->
    {#if activeTab === 'backend'}
      <div role="tabpanel" id="panel-backend" aria-labelledby="tab-backend">
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
            disabled={pickedModel === null || pickedModel === undefined || $setupLoading}
            onclick={apply}
          >
            Save & Apply
          </button>
        </div>
      </div>
    {/if}

    <!-- Personas tab -->
    {#if activeTab === 'personas'}
      <div role="tabpanel" id="panel-personas" aria-labelledby="tab-personas">
        <div class="tab-content">
          {#if loadingPersonas}
            <div class="loading-state">Loading personas…</div>
          {:else if personasList.length === 0}
            <div class="empty-state">No personas available</div>
          {:else}
            <div class="list">
              {#each personasList as persona (persona.name)}
                <div class="list-item">
                  <div class="list-item-header">
                    <span class="list-item-name">{persona.name}</span>
                    <span class="list-item-source">{persona.source}</span>
                  </div>
                  {#if persona.description}
                    <div class="list-item-desc">{persona.description}</div>
                  {/if}
                  <div class="list-item-meta">
                    <span class="meta-sibling">{persona.sibling}</span>
                    <span class="meta-version">v{persona.version}</span>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </div>
    {/if}

    <!-- Credentials tab -->
    {#if activeTab === 'credentials'}
      <div role="tabpanel" id="panel-credentials" aria-labelledby="tab-credentials">
        <CredentialsPanel />
      </div>
    {/if}

    <!-- Dev tab (dev-mode only) -->
    {#if import.meta.env.DEV && activeTab === 'dev'}
      <div role="tabpanel" id="panel-dev" aria-labelledby="tab-dev">
        <div class="tab-content">
          <div class="section-label">Development</div>
          <label class="flex items-center gap-2 cursor-pointer select-none text-xs text-[var(--la-text-dim)]">
            <input
              type="checkbox"
              bind:checked={$devModeEnabled}
              class="accent-[var(--la-focus-ring)] w-3.5 h-3.5"
            />
            <span>Enable dev-mode features (Playwright CDP, screenshot, DOM inspection)</span>
          </label>
          <p class="text-[10px] text-[var(--la-text-dim)] mt-2 leading-snug">
            Dev mode enables browser inspection tools in the copilot header.
            Requires the backend to be running with <code class="text-[var(--la-focus-ring)]">--dev-mode</code>
            and the <code class="text-[var(--la-focus-ring)]">playwright</code> feature flag.
          </p>
        </div>
      </div>
    {/if}

    <!-- Skills tab -->
    {#if activeTab === 'skills'}
      <div role="tabpanel" id="panel-skills" aria-labelledby="tab-skills">
        <div class="tab-content">
          {#if loadingSkills}
            <div class="loading-state">Loading skills…</div>
          {:else if skillsList.length === 0}
            <div class="empty-state">No skills available</div>
          {:else}
            <div class="list">
              {#each skillsList as skill (skill.name)}
                <div class="list-item">
                  <div class="list-item-header">
                    <span class="list-item-name">{skill.name}</span>
                    {#if skill.is_override}
                      <span class="badge-override">CUSTOM</span>
                    {/if}
                  </div>
                  {#if skill.description}
                    <div class="list-item-desc">{skill.description}</div>
                  {/if}
                  {#if skill.trigger_patterns && skill.trigger_patterns.length > 0}
                    <div class="list-item-meta">
                      <span class="meta-triggers">Triggers: {skill.trigger_patterns.join(', ')}</span>
                    </div>
                  {/if}
                  <div class="list-item-meta">
                    <span class="meta-version">v{skill.version}</span>
                    <span class="meta-source">{skill.source}</span>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed; bottom: 2.75rem; right: 0.5rem; z-index: 200;
  }

  .panel {
    background: #0f172a; border: 1px solid #334155; border-radius: 8px;
    padding: 1rem; width: 360px; max-height: calc(100vh - 5rem); overflow-y: auto;
    box-shadow: 0 8px 32px rgba(0,0,0,0.6); display: flex; flex-direction: column; gap: 0.75rem;
  }

  .panel-header { display: flex; align-items: center; justify-content: space-between; }
  .panel-title { font-family: 'IBM Plex Mono', monospace; font-size: 0.8rem; color: #94a3b8; letter-spacing: 0.05em; }
  .close-btn { background: none; border: none; color: #475569; cursor: pointer; font-size: 0.9rem; }
  .close-btn:hover { color: #94a3b8; }

  .tabs { display: flex; gap: 0.25rem; border-bottom: 1px solid #334155; padding-bottom: 0.5rem; }
  .tab-btn { flex: 1; background: #1e293b; border: 1px solid #334155; color: #64748b; border-radius: 4px; padding: 0.35rem 0.5rem; font-family: 'IBM Plex Mono', monospace; font-size: 0.65rem; cursor: pointer; transition: all 0.15s; }
  .tab-btn:hover { color: #94a3b8; }
  .tab-btn.selected { border-color: #ff6600; color: #ff6600; background: rgba(255,102,0,0.08); }

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

  .tab-content { padding: 0.5rem 0; }
  .loading-state, .empty-state { font-family: 'IBM Plex Mono', monospace; font-size: 0.75rem; color: #475569; text-align: center; padding: 1rem; }

  .list { display: flex; flex-direction: column; gap: 0.5rem; }
  .list-item { background: #1e293b; border: 1px solid #334155; border-radius: 6px; padding: 0.5rem 0.75rem; }
  .list-item-header { display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; }
  .list-item-name { font-family: 'IBM Plex Mono', monospace; font-size: 0.75rem; color: #94a3b8; font-weight: 600; }
  .list-item-desc { font-size: 0.7rem; color: #64748b; margin-top: 0.25rem; }
  .list-item-meta { display: flex; gap: 0.5rem; margin-top: 0.25rem; font-size: 0.65rem; color: #475569; }
  .meta-sibling, .meta-version, .meta-source, .meta-triggers { font-family: 'IBM Plex Mono', monospace; }

  .badge-override { background: rgba(255,102,0,0.15); border: 1px solid #ff6600; color: #ff6600; border-radius: 3px; padding: 0.1rem 0.35rem; font-family: 'IBM Plex Mono', monospace; font-size: 0.6rem; font-weight: 600; }

  .toast {
    position: fixed; bottom: 1.5rem; left: 50%; transform: translateX(-50%);
    background: #1e293b; border: 1px solid #334155; color: #94a3b8;
    font-family: 'IBM Plex Mono', monospace; font-size: 0.75rem;
    padding: 0.5rem 1.25rem; border-radius: 6px;
    z-index: 9999; animation: fadein 0.2s ease;
  }
  @keyframes fadein { from { opacity: 0; transform: translateX(-50%) translateY(8px); } to { opacity: 1; transform: translateX(-50%) translateY(0); } }
</style>
