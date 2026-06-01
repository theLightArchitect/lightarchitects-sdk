<!--
  @component
  ProviderSettings — unified inline provider switcher + credential form.
  Slide-in panel anchored below ProviderPill. Non-blocking; operator keeps work in focus.
-->
<script lang="ts">
  import { providerConfig, loadProvider, saveProvider } from '$lib/providerStore';
  import { resyncAuth } from '$lib/auth';

  let { onClose }: { onClose: () => void } = $props();

  // ── Preset catalogue ────────────────────────────────────────────────────────
  type Preset = { label: string; model: string; badge: string };
  type Group  = { group: string; color: string; items: Preset[] };

  const PRESETS: Group[] = [
    {
      group: 'Anthropic', color: '#e07060',
      items: [
        { label: 'claude-opus-4-7',   model: 'anthropic/claude-opus-4-7',   badge: 'flagship' },
        { label: 'claude-sonnet-4-6', model: 'anthropic/claude-sonnet-4-6', badge: 'balanced' },
        { label: 'claude-haiku-4-5',  model: 'anthropic/claude-haiku-4-5',  badge: 'fast'     },
      ],
    },
    {
      group: 'OpenAI', color: '#22c55e',
      items: [
        { label: 'gpt-4o',      model: 'openai/gpt-4o',      badge: 'flagship' },
        { label: 'gpt-4o-mini', model: 'openai/gpt-4o-mini', badge: 'fast'     },
        { label: 'o3-mini',     model: 'openai/o3-mini',     badge: 'reason'   },
      ],
    },
    {
      group: 'OpenRouter', color: '#a78bfa',
      items: [
        { label: 'llama-3.3-70b',   model: 'openrouter/meta-llama/llama-3.3-70b-instruct', badge: 'oss'  },
        { label: 'qwen3-coder-72b', model: 'openrouter/qwen/qwen3-coder',                  badge: 'code' },
      ],
    },
    {
      group: 'Ollama (local)', color: '#fb923c',
      items: [
        { label: 'qwen3-coder:32b', model: 'ollama/qwen3-coder:32b', badge: 'local' },
        { label: 'llama3.2:3b',    model: 'ollama/llama3.2:3b',    badge: 'local' },
      ],
    },
    {
      group: 'Groq', color: '#38bdf8',
      items: [
        { label: 'llama-3.3-70b', model: 'groq/llama-3.3-70b-versatile', badge: 'fast' },
        { label: 'gemma2-9b',     model: 'groq/gemma2-9b-it',            badge: 'fast' },
      ],
    },
    {
      group: 'Mistral', color: '#f472b6',
      items: [
        { label: 'mistral-large', model: 'mistral/mistral-large-latest', badge: 'flagship' },
        { label: 'codestral',     model: 'mistral/codestral-latest',     badge: 'code'     },
      ],
    },
  ];

  // ── Form state ───────────────────────────────────────────────────────────────
  let baseUrl = $state($providerConfig?.base_url ?? 'http://localhost:4000');
  let model   = $state($providerConfig?.model   ?? '');
  let apiKey  = $state('');
  let showKey = $state(false);
  let saving  = $state(false);
  let saveErr = $state<string | null>(null);
  let saveOk  = $state(false);

  $effect(() => {
    const cfg = $providerConfig;
    if (cfg) {
      if (!baseUrl || baseUrl === 'http://localhost:4000') baseUrl = cfg.base_url;
      if (!model) model = cfg.model;
    }
  });

  $effect(() => {
    if ($providerConfig === null) void loadProvider();
  });

  function pickPreset(m: string) {
    model = m;
    apiKey = '';
    saveErr = null;
    saveOk = false;
  }

  async function save() {
    if (!baseUrl.trim() || !model.trim() || !apiKey.trim()) return;
    saving = true;
    saveErr = null;
    saveOk = false;
    try {
      await saveProvider({ base_url: baseUrl, model, api_key: apiKey });
      saveOk = true;
      apiKey = '';
      showKey = false;
      await resyncAuth();
      setTimeout(() => { saveOk = false; }, 2500);
    } catch (e) {
      saveErr = e instanceof Error ? e.message : 'Network error';
    } finally {
      saving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) void save();
  }

  let canSave    = $derived(!saving && baseUrl.trim().length > 0 && model.trim().length > 0 && apiKey.trim().length > 0);
  let hasKey     = $derived($providerConfig?.has_key ?? false);
  let activeModel = $derived($providerConfig?.model    ?? '');
  let activeBase  = $derived($providerConfig?.base_url ?? '');
  let isLive      = $derived(!!$providerConfig?.has_key && !!$providerConfig?.model);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="provider-settings"
  data-testid="provider-settings"
  onkeydown={handleKeydown}
  onclick={(e) => e.stopPropagation()}
>
  <!-- ── Status bar ──────────────────────────────────────────────────────── -->
  <div class="ps-status" class:ps-status--live={isLive}>
    <span class="ps-status-dot" class:ps-status-dot--live={isLive}></span>

    {#if isLive}
      <span class="ps-status-model">{activeModel}</span>
      <span class="ps-status-sep">·</span>
      <span class="ps-status-url">{activeBase}</span>
    {:else if $providerConfig === null}
      <span class="ps-status-dim">Loading…</span>
    {:else}
      <span class="ps-status-warn">No provider configured — enter credentials below</span>
    {/if}

    <div class="ps-status-spacer"></div>

    <button class="ps-close" onclick={onClose} aria-label="Close provider settings">
      <kbd class="ps-close-esc">ESC</kbd>
      <span class="ps-close-x">×</span>
    </button>
  </div>

  <!-- ── Body ───────────────────────────────────────────────────────────── -->
  <div class="ps-body">
    <!-- Left: preset list -->
    <div class="ps-presets" role="listbox" aria-label="Provider presets">
      {#each PRESETS as grp}
        <div class="ps-group" style="--accent: {grp.color}">
          <div class="ps-group-header">
            <span class="ps-group-rule"></span>
            <span class="ps-group-name">{grp.group}</span>
            <span class="ps-group-rule"></span>
          </div>

          {#each grp.items as item}
            {@const isActive = activeModel === item.model}
            <button
              class="ps-preset"
              class:ps-preset--active={isActive}
              role="option"
              aria-selected={isActive}
              onclick={() => pickPreset(item.model)}
              title={item.model}
            >
              <span class="ps-bullet">{isActive ? '●' : '○'}</span>
              <span class="ps-preset-label">{item.label}</span>
              <span class="ps-badge ps-badge--{item.badge}">{item.badge}</span>
            </button>
          {/each}
        </div>
      {/each}
    </div>

    <!-- Right: credential form -->
    <div class="ps-form">
      <p class="ps-form-note">Changes apply immediately — no restart needed.</p>

      <label class="ps-field">
        <span class="ps-label">Base URL</span>
        <input
          type="text"
          bind:value={baseUrl}
          placeholder="http://localhost:4000"
          class="ps-input ps-input--mono"
          spellcheck="false"
          autocomplete="off"
        />
        <span class="ps-hint">https:// for remote · http://localhost for local proxy</span>
      </label>

      <label class="ps-field">
        <span class="ps-label">Model</span>
        <input
          type="text"
          bind:value={model}
          placeholder="anthropic/claude-opus-4-7"
          class="ps-input ps-input--mono"
          spellcheck="false"
          autocomplete="off"
        />
      </label>

      <div class="ps-field">
        <div class="ps-key-row">
          <span class="ps-label">API Key</span>
          <button
            class="ps-toggle"
            onclick={() => (showKey = !showKey)}
            type="button"
            tabindex="-1"
          >{showKey ? 'hide' : 'show'}</button>
        </div>
        <input
          type={showKey ? 'text' : 'password'}
          bind:value={apiKey}
          placeholder={hasKey ? '•••••••• (re-enter to update)' : 'Enter API key (required)'}
          class="ps-input"
          spellcheck="false"
          autocomplete="new-password"
        />
        <span class="ps-hint">
          {#if hasKey}
            Stored in macOS Keychain — never returned by the API.
          {:else}
            Will be stored in macOS Keychain.
          {/if}
        </span>
      </div>

      <div class="ps-feedback-area">
        {#if saveOk}
          <div class="ps-feedback ps-feedback--ok">✓ Provider saved — all surfaces updated.</div>
        {:else if saveErr}
          <div class="ps-feedback ps-feedback--err">⚠ {saveErr}</div>
        {/if}
      </div>

      <div class="ps-actions">
        <span class="ps-shortcut">⌘↵ to save</span>
        <button onclick={save} disabled={!canSave} class="ps-save">
          {#if saving}
            <span class="ps-spin">⟳</span>Saving
          {:else}
            Save
          {/if}
        </button>
      </div>
    </div>
  </div>
</div>

<style>
  /* ── Panel shell ─────────────────────────────────────────────────────── */
  .provider-settings {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 50;
    width: 580px;
    background: var(--la-bg-void, #0a0c0f);
    border: 1px solid color-mix(in srgb, var(--la-focus-ring, #FFD700) 18%, transparent);
    border-radius: 6px;
    box-shadow:
      0 16px 56px rgba(0, 0, 0, 0.75),
      0 0 0 1px rgba(255, 215, 0, 0.04) inset;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    font-family: var(--la-font-sans, sans-serif);
    animation: ps-reveal 0.18s cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  @keyframes ps-reveal {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.985);
      transform-origin: top right;
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  /* ── Status bar ──────────────────────────────────────────────────────── */
  .ps-status {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 7px 10px 7px 12px;
    background: rgba(0, 0, 0, 0.3);
    border-bottom: 1px solid var(--la-drawer-border, #1c2028);
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    min-height: 32px;
    transition: background 0.2s, border-color 0.2s;
  }

  .ps-status--live {
    background: rgba(34, 197, 94, 0.04);
    border-bottom-color: rgba(34, 197, 94, 0.14);
  }

  .ps-status-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--la-text-dim, #6b7280);
    transition: background 0.3s;
  }

  .ps-status-dot--live {
    background: #22c55e;
    box-shadow: 0 0 5px rgba(34, 197, 94, 0.5);
  }

  .ps-status-model {
    color: var(--la-focus-ring, #FFD700);
    letter-spacing: 0.03em;
  }

  .ps-status-sep {
    color: var(--la-text-dim, #6b7280);
    opacity: 0.4;
  }

  .ps-status-url {
    color: var(--la-text-dim, #6b7280);
    font-size: 8px;
    opacity: 0.65;
  }

  .ps-status-warn {
    color: var(--la-agent-performance, #f59e0b);
    font-size: 9px;
    letter-spacing: 0.03em;
  }

  .ps-status-dim {
    color: var(--la-text-dim, #6b7280);
  }

  .ps-status-spacer { flex: 1; }

  .ps-close {
    display: flex;
    align-items: center;
    gap: 5px;
    background: none;
    border: none;
    cursor: pointer;
    padding: 3px 5px;
    border-radius: 3px;
    transition: background 0.1s;
  }

  .ps-close:hover { background: rgba(255, 255, 255, 0.06); }

  .ps-close-esc {
    font-family: var(--la-font-mono, monospace);
    font-size: 7.5px;
    letter-spacing: 0.07em;
    color: var(--la-text-dim, #6b7280);
    background: rgba(255, 255, 255, 0.05);
    padding: 1px 4px;
    border-radius: 2px;
    border: 1px solid var(--la-drawer-border, #1c2028);
  }

  .ps-close-x {
    color: var(--la-text-dim, #6b7280);
    font-size: 16px;
    line-height: 1;
    transition: color 0.1s;
  }

  .ps-close:hover .ps-close-x { color: var(--la-text-bright, #f8fafc); }

  /* ── Body ────────────────────────────────────────────────────────────── */
  .ps-body {
    display: flex;
    max-height: 380px;
  }

  /* ── Preset list ─────────────────────────────────────────────────────── */
  .ps-presets {
    width: 198px;
    flex-shrink: 0;
    border-right: 1px solid var(--la-drawer-border, #1c2028);
    overflow-y: auto;
    padding: 6px 0;
    scrollbar-width: thin;
    scrollbar-color: var(--la-drawer-border, #1c2028) transparent;
  }

  .ps-group { --accent: #6b7280; }

  .ps-group-header {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px 3px;
  }

  .ps-group-rule {
    flex: 1;
    height: 1px;
    background: var(--la-drawer-border, #1c2028);
  }

  .ps-group-name {
    font-size: 7.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--accent);
    white-space: nowrap;
    opacity: 0.85;
  }

  .ps-preset {
    display: flex;
    align-items: center;
    gap: 5px;
    width: 100%;
    padding: 4px 10px 4px 11px;
    background: none;
    border: none;
    border-left: 2px solid transparent;
    cursor: pointer;
    font-size: 10px;
    color: var(--la-text-label, #9ca3af);
    transition: background 0.08s, color 0.08s, border-color 0.08s;
    text-align: left;
  }

  .ps-preset:hover {
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    color: var(--la-text-bright, #f8fafc);
    border-left-color: color-mix(in srgb, var(--accent) 40%, transparent);
  }

  .ps-preset--active {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    color: var(--accent);
    border-left-color: var(--accent);
  }

  .ps-bullet {
    font-size: 8px;
    width: 10px;
    flex-shrink: 0;
    opacity: 0.7;
    font-family: var(--la-font-mono, monospace);
  }

  .ps-preset-label {
    flex: 1;
    font-family: var(--la-font-mono, monospace);
    font-size: 9.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Badges */
  .ps-badge {
    font-size: 7px;
    padding: 1px 4px;
    border-radius: 2px;
    border: 1px solid;
    font-family: var(--la-font-mono, monospace);
    flex-shrink: 0;
    letter-spacing: 0.04em;
    text-transform: lowercase;
  }

  .ps-badge--flagship { color: var(--la-focus-ring, #FFD700);            border-color: color-mix(in srgb, var(--la-focus-ring, #FFD700) 30%, transparent); }
  .ps-badge--balanced { color: #86efac;                                   border-color: rgba(134, 239, 172, 0.3); }
  .ps-badge--fast     { color: #67e8f9;                                   border-color: rgba(103, 232, 249, 0.3); }
  .ps-badge--reason   { color: #c4b5fd;                                   border-color: rgba(196, 181, 253, 0.3); }
  .ps-badge--code     { color: var(--la-agent-performance, #f59e0b);      border-color: rgba(245, 158, 11, 0.3); }
  .ps-badge--oss      { color: var(--la-text-dim, #6b7280);               border-color: var(--la-drawer-border, #1c2028); }
  .ps-badge--local    { color: #fb923c;                                   border-color: rgba(251, 146, 60, 0.3); }

  /* ── Credential form ─────────────────────────────────────────────────── */
  .ps-form {
    flex: 1;
    padding: 12px 14px 10px;
    display: flex;
    flex-direction: column;
    gap: 9px;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--la-drawer-border, #1c2028) transparent;
  }

  .ps-form-note {
    font-size: 9px;
    color: var(--la-text-dim, #6b7280);
    font-family: var(--la-font-mono, monospace);
    letter-spacing: 0.02em;
    margin: 0;
  }

  .ps-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .ps-key-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .ps-label {
    font-size: 8px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--la-text-dim, #6b7280);
  }

  .ps-toggle {
    background: none;
    border: 1px solid var(--la-drawer-border, #1c2028);
    border-radius: 2px;
    cursor: pointer;
    font-family: var(--la-font-mono, monospace);
    font-size: 7.5px;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--la-text-dim, #6b7280);
    padding: 1px 5px;
    transition: color 0.1s, border-color 0.1s, background 0.1s;
  }

  .ps-toggle:hover {
    color: var(--la-focus-ring, #FFD700);
    border-color: color-mix(in srgb, var(--la-focus-ring, #FFD700) 30%, transparent);
    background: color-mix(in srgb, var(--la-focus-ring, #FFD700) 4%, transparent);
  }

  .ps-input {
    background: rgba(0, 0, 0, 0.35);
    border: 1px solid var(--la-drawer-border, #1c2028);
    border-radius: 3px;
    padding: 7px 9px;
    font-size: 11px;
    color: var(--la-text-bright, #f8fafc);
    outline: none;
    transition: border-color 0.12s, background 0.12s, box-shadow 0.12s;
    width: 100%;
    box-sizing: border-box;
  }

  .ps-input--mono {
    font-family: var(--la-font-mono, monospace);
    font-size: 10.5px;
  }

  .ps-input:focus {
    border-color: color-mix(in srgb, var(--la-focus-ring, #FFD700) 55%, transparent);
    background: rgba(255, 215, 0, 0.02);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--la-focus-ring, #FFD700) 10%, transparent);
  }

  .ps-hint {
    font-size: 8px;
    color: var(--la-text-dim, #6b7280);
    opacity: 0.7;
  }

  /* ── Feedback ────────────────────────────────────────────────────────── */
  .ps-feedback-area {
    min-height: 26px;
    display: flex;
    align-items: center;
  }

  .ps-feedback {
    font-family: var(--la-font-mono, monospace);
    font-size: 9.5px;
    padding: 5px 8px;
    border-radius: 3px;
    border: 1px solid;
    width: 100%;
    letter-spacing: 0.02em;
  }

  .ps-feedback--ok {
    border-color: rgba(34, 197, 94, 0.3);
    background: rgba(34, 197, 94, 0.06);
    color: #86efac;
  }

  .ps-feedback--err {
    border-color: rgba(239, 68, 68, 0.35);
    background: rgba(239, 68, 68, 0.06);
    color: #fca5a5;
  }

  /* ── Actions ─────────────────────────────────────────────────────────── */
  .ps-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 2px;
  }

  .ps-shortcut {
    font-family: var(--la-font-mono, monospace);
    font-size: 8px;
    color: var(--la-text-dim, #6b7280);
    opacity: 0.5;
    letter-spacing: 0.04em;
  }

  .ps-save {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 18px;
    background: var(--la-focus-ring, #FFD700);
    color: #0a0c0f;
    font-size: 10px;
    font-weight: 700;
    font-family: var(--la-font-sans, sans-serif);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    border: none;
    border-radius: 3px;
    cursor: pointer;
    transition: opacity 0.12s, transform 0.08s;
  }

  .ps-save:not(:disabled):hover  { opacity: 0.88; transform: translateY(-1px); }
  .ps-save:not(:disabled):active { transform: translateY(0); }
  .ps-save:disabled { opacity: 0.28; cursor: not-allowed; }

  .ps-spin {
    display: inline-block;
    animation: spin 0.75s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
