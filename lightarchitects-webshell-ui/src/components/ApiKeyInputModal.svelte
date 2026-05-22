<script lang="ts">
  import { authHeaders } from '$lib/auth';

  let {
    provider,
    label,
    prompt,
    isOpen,
    onClose,
    onConnected,
  }: {
    provider: string;
    label: string;
    prompt: string;
    isOpen: boolean;
    onClose: () => void;
    onConnected: () => void;
  } = $props();

  let key = $state('');
  let saving = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (!isOpen) {
      key = '';
      error = null;
    }
  });

  async function save() {
    if (!key.trim() || saving) return;
    saving = true;
    error = null;
    try {
      const res = await fetch(`/api/auth/credential/${provider}/key`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ key }),
      });
      if (!res.ok) {
        const body = await res.json().catch(() => ({})) as { error?: string };
        error = body.error ?? `HTTP ${res.status}`;
      } else {
        key = '';
        onConnected();
        onClose();
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Network error';
    } finally {
      saving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') { onClose(); }
    if (e.key === 'Enter' && key.trim() && !saving) { void save(); }
  }
</script>

{#if isOpen}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    role="dialog"
    aria-modal="true"
    aria-label="Enter {label} API key"
    onkeydown={handleKeydown}
  >
    <div class="w-[440px] bg-[var(--la-bg-frame)] border border-[var(--la-drawer-border)] rounded-lg shadow-2xl p-6 flex flex-col gap-5">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold text-[var(--la-text-bright)]">{label} API Key</h2>
        <button
          onclick={onClose}
          class="text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)] transition-colors text-lg leading-none"
          aria-label="Close"
        >×</button>
      </div>

      <label class="flex flex-col gap-1">
        <span class="text-[10px] font-medium text-[var(--la-text-dim)] uppercase tracking-wider">{prompt}</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          type="password"
          bind:value={key}
          autofocus
          autocomplete="off"
          class="bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded px-3 py-2 text-sm text-[var(--la-text-bright)] placeholder-[var(--la-text-dim)] outline-none focus:border-[var(--la-agent-testing)] transition-colors font-mono"
          placeholder="Paste key here…"
        />
      </label>

      {#if error}
        <p class="text-xs text-red-400">{error}</p>
      {/if}

      <div class="flex gap-2 justify-end">
        <button
          onclick={onClose}
          class="px-4 py-2 text-sm text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)] rounded border border-[var(--la-drawer-border)] hover:border-[var(--la-hair-strong)] transition-colors"
        >
          Cancel
        </button>
        <button
          onclick={() => void save()}
          disabled={!key.trim() || saving}
          class="px-4 py-2 bg-[var(--la-agent-testing)] text-white text-sm rounded transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        >
          {saving ? 'Saving…' : 'Save Key'}
        </button>
      </div>
    </div>
  </div>
{/if}
