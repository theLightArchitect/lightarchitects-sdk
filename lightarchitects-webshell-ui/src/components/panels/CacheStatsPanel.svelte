<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { getCacheStats, type CacheStats } from '$lib/helix';

  const POLL_MS = 15_000;

  let stats  = $state<CacheStats | null>(null);
  let status = $state<'loading' | 'ok' | 'error'>('loading');
  let errMsg = $state('');

  let timer: ReturnType<typeof setInterval> | null = null;

  async function poll() {
    try {
      stats  = await getCacheStats();
      status = 'ok';
    } catch (e) {
      errMsg = e instanceof Error ? e.message : String(e);
      status = 'error';
    }
  }

  onMount(() => {
    void poll();
    timer = setInterval(() => { void poll(); }, POLL_MS);
  });

  onDestroy(() => {
    if (timer !== null) clearInterval(timer);
  });

  function fmtBytes(b: number): string {
    if (b >= 1_048_576) return `${(b / 1_048_576).toFixed(1)} MiB`;
    if (b >= 1_024)     return `${(b / 1_024).toFixed(1)} KiB`;
    return `${b} B`;
  }
</script>

<div class="cs-panel" data-card-role="cache-stats" data-testid="cache-stats-panel">
  <header class="cs-header">
    <span class="cs-title">HELIX CACHE</span>
    <span class="cs-poll-hint" aria-label="Polls every 15 seconds">15s</span>
  </header>

  {#if status === 'loading'}
    <div class="cs-state" data-testid="cs-loading">
      <span class="cs-spinner" aria-label="Loading" aria-busy="true">◌</span>
    </div>

  {:else if status === 'error'}
    <div class="cs-state cs-error" data-testid="cs-error">
      <span class="cs-err-icon" aria-hidden="true">⚠</span>
      <span class="cs-err-msg">{errMsg}</span>
    </div>

  {:else if stats}
    <div class="cs-stats" data-testid="cs-stats">
      <div class="cs-stat">
        <span class="cs-val" data-testid="cs-entry-count">{stats.entry_count}</span>
        <span class="cs-label">ENTRIES</span>
      </div>
      <div class="cs-sep" aria-hidden="true"></div>
      <div class="cs-stat">
        <span class="cs-val" data-testid="cs-size">{fmtBytes(stats.weighted_size_bytes)}</span>
        <span class="cs-label">WEIGHT</span>
      </div>
    </div>
  {/if}
</div>

<style>
  .cs-panel {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    background: var(--la-bg-base, #0a0a12);
    font-family: var(--la-font-mono, monospace);
    overflow: hidden;
  }

  .cs-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    border-bottom: 1px solid var(--la-hair-base, #2c3140);
    flex-shrink: 0;
  }

  .cs-title {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.14em;
    color: var(--la-struct-primary, #00bfff);
  }

  .cs-poll-hint {
    font-size: 8px;
    color: var(--la-text-mute, #6e7681);
  }

  .cs-stats {
    display: flex;
    align-items: stretch;
    flex-shrink: 0;
  }

  .cs-stat {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    padding: 12px 16px;
    flex: 1;
  }

  .cs-val {
    font-size: 16px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: var(--la-text-bright, #f1f5f9);
  }

  .cs-label {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-mute, #6e7681);
    text-transform: uppercase;
  }

  .cs-sep {
    width: 1px;
    background: var(--la-hair-faint, #1c2028);
    align-self: stretch;
    flex-shrink: 0;
  }

  .cs-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 16px;
  }

  .cs-spinner {
    font-size: 18px;
    color: var(--la-struct-primary, #00bfff);
    animation: spin 1.2s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .cs-err-icon {
    font-size: 18px;
    color: var(--la-semantic-error, #ef4444);
  }

  .cs-err-msg {
    font-size: 9px;
    color: var(--la-text-dim, #8b949e);
    text-align: center;
    max-width: 180px;
    line-height: 1.4;
  }
</style>
