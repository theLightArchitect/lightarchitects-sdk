<!--
  @component
  SmartDispatchCard — reasons-aware action suggestions sourced from
  `GET /api/wave/suggestions?scope=platform` (§2.53).
  Priority-true items get an elevated arrow.  Polls every 45 seconds.
  Click → emits a `la:smart-dispatch-request` CustomEvent the parent
  can listen for to open the dispatch confirmation modal.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, type WaveSuggestion } from '$lib/api';

  const POLL_MS = 45_000;

  let suggestions: WaveSuggestion[] = $state([]);
  let loading: boolean = $state(true);
  let error: string | null = $state(null);
  let timer: ReturnType<typeof setInterval> | null = null;

  async function refresh() {
    try {
      const res = await api.getWaveSuggestions('platform');
      suggestions = res.suggestions;
      error = null;
    } catch (err) {
      error = err instanceof Error ? err.message : 'wave fetch failed';
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    refresh();
    timer = setInterval(refresh, POLL_MS);
  });

  onDestroy(() => {
    if (timer !== null) clearInterval(timer);
  });

  function clickSuggestion(s: WaveSuggestion) {
    // SECURITY: action slug + reason originate server-side; treat as text only.
    window.dispatchEvent(new CustomEvent('la:smart-dispatch-request', {
      detail: { action: s.action, scope: 'platform', reason: s.reason, domain: s.domain },
    }));
  }
</script>

<div class="sd-card">
  {#if loading && suggestions.length === 0}
    <div class="sd-empty">loading suggestions…</div>
  {:else if error && suggestions.length === 0}
    <div class="sd-error">dispatch unavailable — {error}</div>
  {:else if suggestions.length === 0}
    <div class="sd-empty">no suggestions</div>
  {:else}
    <div class="sd-list" role="listbox" aria-label="Smart dispatch suggestions">
      {#each suggestions as s (s.action)}
        <button
          type="button"
          class="sd-row"
          class:sd-row-priority={s.priority}
          onclick={() => clickSuggestion(s)}
          data-action={s.action}
        >
          <span class="sd-prio" aria-hidden="true">{s.priority ? '↑' : ''}</span>
          <span class="sd-action">{s.action}</span>
          <span class="sd-reason">
            <span class="sd-domain sd-domain-{s.domain}">{s.domain}</span>
            {s.reason}
          </span>
          <span class="sd-go" aria-hidden="true">›</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .sd-card {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-family: var(--la-font-mono, monospace);
  }

  .sd-empty, .sd-error {
    font-size: 9px;
    color: var(--la-text-mute);
    font-style: italic;
    padding: 12px 0;
    text-align: center;
  }
  .sd-error { color: var(--la-err, #ff4d6a); }

  .sd-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .sd-row {
    display: grid;
    grid-template-columns: 14px 64px 1fr 16px;
    gap: 8px;
    align-items: center;
    padding: 7px 4px;
    border: none;
    background: none;
    text-align: left;
    cursor: pointer;
    border-bottom: 1px solid var(--la-hair-faint, rgba(255,255,255,0.04));
    color: inherit;
    font-family: inherit;
  }
  .sd-row:last-child { border-bottom: none; }
  .sd-row:hover .sd-action { color: var(--la-struct-primary, #4d8eff); }
  .sd-row:hover .sd-go     { color: var(--la-struct-primary, #4d8eff); transform: translateX(2px); }
  .sd-row:focus-visible { outline: 1px solid var(--la-struct-primary, #4d8eff); outline-offset: 2px; }

  .sd-prio {
    font-family: var(--la-font-display, 'Syne', sans-serif);
    font-size: 13px;
    font-weight: 800;
    color: transparent;
    text-align: center;
    line-height: 1;
  }
  .sd-row-priority .sd-prio { color: var(--la-warn, #ffad2e); }

  .sd-action {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--la-text-bright, rgba(255,255,255,0.95));
    text-transform: uppercase;
  }

  .sd-reason {
    font-size: 9px;
    color: var(--la-text-dim, rgba(255,255,255,0.5));
    font-style: italic;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sd-domain {
    display: inline-block;
    font-size: 7px;
    font-weight: 700;
    padding: 1px 4px;
    border-radius: 2px;
    background: var(--la-text-mute, rgba(255,255,255,0.28));
    color: var(--la-bg-base, #07080f);
    margin-right: 4px;
    letter-spacing: 0;
    font-style: normal;
  }
  .sd-domain-S { background: var(--la-err, #ff4d6a); color: white; }
  .sd-domain-T { background: var(--la-warn, #ffad2e); }
  .sd-domain-Q { background: var(--la-warn, #ffad2e); }
  .sd-domain-A { background: var(--la-ok, #39ff8a); }
  .sd-domain-K { background: #7a6cff; color: white; }
  .sd-domain-O { background: #5f6e84; color: var(--la-text-bright, rgba(255,255,255,0.95)); }
  .sd-domain-D { background: #8aa9ff; }
  .sd-domain-P { background: #d44df0; color: white; }

  .sd-go {
    color: var(--la-text-mute, rgba(255,255,255,0.28));
    font-family: var(--la-font-display, 'Syne', sans-serif);
    font-weight: 800;
    font-size: 12px;
    text-align: right;
    transition: color 0.15s ease, transform 0.15s ease;
  }
</style>
