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
    <div class="w-[420px] bg-[#0f172a] border border-[#1e293b] rounded-lg shadow-2xl p-6 flex flex-col gap-5">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold text-[#e2e8f0]">Ollama Cloud Config</h2>
        <button
          onclick={onClose}
          class="text-[#475569] hover:text-[#e2e8f0] transition-colors text-lg leading-none"
          aria-label="Close"
        >×</button>
      </div>

      <div class="flex flex-col gap-4">
        <label class="flex flex-col gap-1">
          <span class="text-[10px] font-medium text-[#64748b] uppercase tracking-wider">Base URL</span>
          <input
            type="text"
            bind:value={baseUrl}
            placeholder="http://localhost:11434"
            class="bg-[#111827] border border-[#1e293b] rounded px-3 py-2 text-sm text-[#e2e8f0] placeholder-[#475569] outline-none focus:border-[#6366F1] transition-colors"
          />
        </label>

        <label class="flex flex-col gap-1">
          <span class="text-[10px] font-medium text-[#64748b] uppercase tracking-wider">Model</span>
          <input
            type="text"
            bind:value={model}
            placeholder="qwen3-coder:480b-cloud"
            class="bg-[#111827] border border-[#1e293b] rounded px-3 py-2 text-sm text-[#e2e8f0] placeholder-[#475569] outline-none focus:border-[#6366F1] transition-colors"
          />
        </label>

        <label class="flex flex-col gap-1">
          <span class="text-[10px] font-medium text-[#64748b] uppercase tracking-wider">API Key</span>
          <input
            type="password"
            bind:value={apiKey}
            placeholder="sk-…"
            class="bg-[#111827] border border-[#1e293b] rounded px-3 py-2 text-sm text-[#e2e8f0] placeholder-[#475569] outline-none focus:border-[#6366F1] transition-colors"
          />
        </label>
      </div>

      <div class="flex gap-2 justify-end">
        <button
          onclick={onClose}
          class="px-4 py-2 text-sm text-[#64748b] hover:text-[#e2e8f0] rounded border border-[#1e293b] hover:border-[#334155] transition-colors"
        >
          Cancel
        </button>
        <button
          onclick={save}
          class="px-4 py-2 bg-[#6366F1] text-white text-sm rounded hover:bg-[#4F46E5] transition-colors"
        >
          Save
        </button>
      </div>
    </div>
  </div>
{/if}
