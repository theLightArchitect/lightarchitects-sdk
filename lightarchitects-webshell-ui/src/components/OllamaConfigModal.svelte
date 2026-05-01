<script lang="ts">
  import { ollamaConfig } from '$lib/stores';
  import type { OllamaConfig } from '$lib/types';

  let { isOpen, onClose }: { isOpen: boolean; onClose: () => void } = $props();

  let baseUrl = $state($ollamaConfig?.baseUrl ?? 'http://localhost:11434');
  let model = $state($ollamaConfig?.model ?? 'qwen3-coder:480b-cloud');
  let apiKey = $state($ollamaConfig?.apiKey ?? '');

  function save() {
    const cfg: OllamaConfig = { baseUrl, model, apiKey };
    ollamaConfig.set(cfg);
    onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

{#if isOpen}
  <!-- Overlay -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    onkeydown={handleKeydown}
  >
    <!-- Modal card -->
    <div class="w-[420px] bg-[var(--la-bg-frame)] border border-[var(--la-drawer-border)] rounded-lg shadow-2xl p-6 flex flex-col gap-5">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold text-[var(--la-text-bright)]">Ollama Cloud Config</h2>
        <button
          onclick={onClose}
          class="text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)] transition-colors text-lg leading-none"
          aria-label="Close"
        >×</button>
      </div>

      <div class="flex flex-col gap-4">
        <label class="flex flex-col gap-1">
          <span class="text-[10px] font-medium text-[var(--la-text-dim)] uppercase tracking-wider">Base URL</span>
          <input
            type="text"
            bind:value={baseUrl}
            placeholder="http://localhost:11434"
            class="bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded px-3 py-2 text-sm text-[var(--la-text-bright)] placeholder-[var(--la-text-dim)] outline-none focus:border-[var(--la-agent-testing)] transition-colors"
          />
        </label>

        <label class="flex flex-col gap-1">
          <span class="text-[10px] font-medium text-[var(--la-text-dim)] uppercase tracking-wider">Model</span>
          <input
            type="text"
            bind:value={model}
            placeholder="qwen3-coder:480b-cloud"
            class="bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded px-3 py-2 text-sm text-[var(--la-text-bright)] placeholder-[var(--la-text-dim)] outline-none focus:border-[var(--la-agent-testing)] transition-colors"
          />
        </label>

        <label class="flex flex-col gap-1">
          <span class="text-[10px] font-medium text-[var(--la-text-dim)] uppercase tracking-wider">API Key</span>
          <input
            type="password"
            bind:value={apiKey}
            placeholder="sk-…"
            class="bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded px-3 py-2 text-sm text-[var(--la-text-bright)] placeholder-[var(--la-text-dim)] outline-none focus:border-[var(--la-agent-testing)] transition-colors"
          />
        </label>
      </div>

      <div class="flex gap-2 justify-end">
        <button
          onclick={onClose}
          class="px-4 py-2 text-sm text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)] rounded border border-[var(--la-drawer-border)] hover:border-[var(--la-hair-strong)] transition-colors"
        >
          Cancel
        </button>
        <button
          onclick={save}
          class="px-4 py-2 bg-[var(--la-agent-testing)] text-white text-sm rounded hover:bg-[var(--la-agent-testing)] transition-colors"
        >
          Save
        </button>
      </div>
    </div>
  </div>
{/if}
