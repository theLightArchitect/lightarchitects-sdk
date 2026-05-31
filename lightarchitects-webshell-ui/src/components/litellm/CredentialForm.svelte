<script lang="ts">
  import { authHeaders } from '$lib/auth';

  let { isOpen, initialModel = '', onClose, onSaved }: {
    isOpen: boolean;
    initialModel?: string;
    onClose: () => void;
    onSaved?: () => void;
  } = $props();

  let baseUrl = $state('http://localhost:4000');
  let model = $state('');
  let apiKey = $state('');
  let hasExistingKey = $state(false);
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let saveOk = $state(false);

  async function loadCurrent() {
    try {
      const resp = await fetch('/api/litellm/config', { headers: authHeaders() });
      if (!resp.ok) return;
      const data: { base_url: string; model: string; has_key: boolean } = await resp.json();
      baseUrl = data.base_url || 'http://localhost:4000';
      model = initialModel || data.model || '';
      hasExistingKey = data.has_key;
    } catch { /* best-effort */ }
  }

  $effect(() => {
    if (isOpen) {
      saveError = null;
      saveOk = false;
      apiKey = '';
      loadCurrent();
    }
  });

  // When a preset model is injected after open, sync it.
  $effect(() => {
    if (isOpen && initialModel) model = initialModel;
  });

  async function save() {
    if (!baseUrl.trim() || !model.trim() || !apiKey.trim()) return;
    saving = true;
    saveError = null;
    saveOk = false;
    try {
      const resp = await fetch('/api/litellm/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ base_url: baseUrl.trim(), api_key: apiKey, model: model.trim() }),
      });
      if (resp.status === 204) {
        saveOk = true;
        onSaved?.();
        setTimeout(() => onClose(), 900);
      } else {
        const text = await resp.text();
        saveError = text || `HTTP ${resp.status}`;
      }
    } catch (e) {
      saveError = e instanceof Error ? e.message : 'Network error';
    } finally {
      saving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) void save();
  }

  let canSave = $derived(
    !saving && baseUrl.trim().length > 0 && model.trim().length > 0 && apiKey.trim().length > 0,
  );
</script>

{#if isOpen}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    onkeydown={handleKeydown}
    onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
  >
    <div class="w-[460px] bg-[var(--la-bg-frame)] border border-[var(--la-drawer-border)] rounded-lg shadow-2xl p-6 flex flex-col gap-5">
      <div class="flex items-center justify-between">
        <div>
          <h2 class="text-sm font-semibold text-[var(--la-text-bright)]">LiteLLM Provider Config</h2>
          <p class="text-[10px] text-[var(--la-text-dim)] mt-0.5">Changes apply to all surfaces on the next request — no restart needed.</p>
        </div>
        <button
          onclick={onClose}
          class="text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)] transition-colors text-lg leading-none"
          aria-label="Close"
        >×</button>
      </div>

      <div class="flex flex-col gap-4">
        <label class="flex flex-col gap-1">
          <span class="text-[10px] font-medium text-[var(--la-text-dim)] uppercase tracking-wider">Proxy Base URL</span>
          <input
            type="text"
            bind:value={baseUrl}
            placeholder="http://localhost:4000"
            class="bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded px-3 py-2 text-sm text-[var(--la-text-bright)] placeholder-[var(--la-text-dim)] outline-none focus:border-[var(--la-focus-ring)]/60 transition-colors font-mono"
          />
          <span class="text-[9px] text-[var(--la-text-dim)]">https:// for remote · http://localhost for local proxy</span>
        </label>

        <label class="flex flex-col gap-1">
          <span class="text-[10px] font-medium text-[var(--la-text-dim)] uppercase tracking-wider">Model</span>
          <input
            type="text"
            bind:value={model}
            placeholder="anthropic/claude-opus-4-7"
            class="bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded px-3 py-2 text-sm text-[var(--la-text-bright)] placeholder-[var(--la-text-dim)] outline-none focus:border-[var(--la-focus-ring)]/60 transition-colors font-mono"
          />
        </label>

        <label class="flex flex-col gap-1">
          <span class="text-[10px] font-medium text-[var(--la-text-dim)] uppercase tracking-wider">API Key</span>
          <input
            type="password"
            bind:value={apiKey}
            placeholder={hasExistingKey ? 'Re-enter to update stored key' : 'Enter API key (required)'}
            class="bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded px-3 py-2 text-sm text-[var(--la-text-bright)] placeholder-[var(--la-text-dim)] outline-none focus:border-[var(--la-focus-ring)]/60 transition-colors"
          />
          <span class="text-[9px] text-[var(--la-text-dim)]">
            {hasExistingKey ? 'Key stored in macOS Keychain — never returned by the API.' : 'Will be stored in macOS Keychain.'}
          </span>
        </label>
      </div>

      {#if saveOk}
        <div class="px-3 py-2 rounded border border-[var(--la-agent-testing)]/40 bg-[var(--la-agent-testing)]/10 text-[10px] text-[var(--la-agent-testing)]">
          ✓ Provider configured. All LLM surfaces now use the new config.
        </div>
      {:else if saveError}
        <div class="px-3 py-2 rounded border border-red-500/40 bg-red-500/10 text-[10px] text-red-300">
          {saveError}
        </div>
      {/if}

      <div class="flex gap-2 justify-end">
        <button
          onclick={onClose}
          class="px-4 py-2 text-sm text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)] rounded border border-[var(--la-drawer-border)] hover:border-[var(--la-hair-strong)] transition-colors"
        >Cancel</button>
        <button
          onclick={save}
          disabled={!canSave}
          class="px-4 py-2 bg-[var(--la-focus-ring)] text-[var(--la-bg-frame)] text-sm font-semibold rounded hover:opacity-90 disabled:opacity-40 transition-opacity"
        >{saving ? 'Saving…' : 'Save'}</button>
      </div>
    </div>
  </div>
{/if}
