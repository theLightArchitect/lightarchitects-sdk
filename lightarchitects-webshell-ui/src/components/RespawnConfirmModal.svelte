<script lang="ts">
  import { api } from '$lib/api';

  type Props = {
    targetKind: string;
    targetLabel: string;
    targetColor: string;
    currentKind: string;
    currentLabel: string;
    currentColor: string;
    onconfirm: () => void;
    oncancel: () => void;
  };

  let {
    targetKind,
    targetLabel,
    targetColor,
    currentKind,
    currentLabel,
    currentColor,
    onconfirm,
    oncancel,
  }: Props = $props();

  let loading = $state(false);
  let error = $state<string | null>(null);

  let dialogEl: HTMLElement;
  $effect(() => { dialogEl?.focus(); });

  async function confirm() {
    if (loading) return;
    loading = true;
    error = null;
    try {
      await api.respawnPty(targetKind);
      onconfirm();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Respawn failed';
      loading = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && !loading) oncancel();
    if (e.key === 'Enter' && !loading) void confirm();
  }
</script>

<div
  bind:this={dialogEl}
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
  role="dialog"
  aria-modal="true"
  aria-label="Confirm backend switch"
  tabindex="-1"
  onkeydown={handleKeydown}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="absolute inset-0" onclick={() => { if (!loading) oncancel(); }}></div>

  <div
    class="relative z-10 w-[360px] rounded-xl border border-[var(--la-hair-strong)]
           bg-[var(--la-surface-overlay)] shadow-2xl p-5 flex flex-col gap-4"
  >
    <h2 class="text-[13px] font-semibold text-[var(--la-text-primary)] font-mono">
      Switch backend agent
    </h2>

    <div class="flex items-center gap-3">
      <!-- Current -->
      <div class="flex-1 rounded-lg border border-[var(--la-hair)] bg-[var(--la-surface-input)] p-3">
        <p class="text-[10px] text-[var(--la-text-dim)] font-mono mb-1">Current</p>
        <div class="flex items-center gap-2">
          <span
            class="w-[7px] h-[7px] rounded-full shrink-0"
            style="background-color: {currentColor}; box-shadow: 0 0 4px {currentColor}"
          ></span>
          <span class="text-[12px] text-[var(--la-text-primary)] font-mono">{currentLabel}</span>
        </div>
      </div>

      <span class="text-[var(--la-text-dim)] text-lg">→</span>

      <!-- Target -->
      <div class="flex-1 rounded-lg border border-[var(--la-hair-strong)] bg-[var(--la-surface-input)] p-3">
        <p class="text-[10px] text-[var(--la-text-dim)] font-mono mb-1">Switching to</p>
        <div class="flex items-center gap-2">
          <span
            class="w-[7px] h-[7px] rounded-full shrink-0"
            style="background-color: {targetColor}; box-shadow: 0 0 4px {targetColor}"
          ></span>
          <span class="text-[12px] text-[var(--la-text-primary)] font-mono">{targetLabel}</span>
        </div>
      </div>
    </div>

    <p class="text-[11px] text-[var(--la-text-dim)] leading-relaxed">
      The current PTY process will receive SIGTERM (3 s grace), then the new
      agent will start. The browser tab stays open; the SSE stream reconnects
      automatically.
    </p>

    {#if error}
      <p class="text-[11px] text-red-400 font-mono">{error}</p>
    {/if}

    <div class="flex gap-2 justify-end">
      <button
        class="px-3 py-1.5 text-[11px] font-mono rounded border border-[var(--la-hair-strong)]
               text-[var(--la-text-label)] hover:bg-[var(--la-surface-hover)] transition-colors
               disabled:opacity-40 disabled:cursor-not-allowed"
        disabled={loading}
        onclick={oncancel}
      >
        Cancel
      </button>
      <button
        class="px-3 py-1.5 text-[11px] font-mono rounded
               bg-[var(--la-accent)] text-white
               hover:opacity-90 transition-opacity
               disabled:opacity-40 disabled:cursor-not-allowed
               flex items-center gap-1.5"
        disabled={loading}
        onclick={() => void confirm()}
      >
        {#if loading}
          <span class="w-3 h-3 border border-white/40 border-t-white rounded-full animate-spin"></span>
        {/if}
        Switch
      </button>
    </div>
  </div>
</div>
