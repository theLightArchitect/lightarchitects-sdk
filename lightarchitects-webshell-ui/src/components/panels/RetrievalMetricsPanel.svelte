<script lang="ts">
  import type { RetrieveResult } from '$lib/helix';

  interface Props {
    result?: RetrieveResult | null;
    loading?: boolean;
    error?: string | null;
  }

  let { result = null, loading = false, error = null }: Props = $props();

  const MODE_LABELS: Record<string, string> = {
    keyword_dominated: 'KEYWORD',
    balanced:          'BALANCED',
    graph_weighted:    'GRAPH',
  };

  let modeLabel = $derived(result ? (MODE_LABELS[result.mode] ?? result.mode.toUpperCase()) : '—');
  let hitPct    = $derived(result ? Math.round(result.cache_hit_ratio * 100) : null);
  let count     = $derived(result?.count ?? null);
</script>

<div class="rm-panel" data-card-role="retrieval-metrics" data-testid="retrieval-metrics-panel">
  <header class="rm-header">
    <span class="rm-title">RETRIEVAL METRICS</span>
    {#if result}
      <span class="rm-mode-badge" data-testid="rm-mode-badge">{modeLabel}</span>
    {/if}
  </header>

  {#if loading}
    <div class="rm-state rm-loading" data-testid="rm-loading">
      <span class="rm-spinner" aria-label="Loading" aria-busy="true">◌</span>
    </div>

  {:else if error}
    <div class="rm-state rm-error" data-testid="rm-error">
      <span class="rm-error-icon" aria-hidden="true">⚠</span>
      <span class="rm-error-msg">{error}</span>
    </div>

  {:else if result}
    <div class="rm-stats" data-testid="rm-stats">
      <div class="rm-stat">
        <span class="rm-val" data-testid="rm-count">{count}</span>
        <span class="rm-label">RESULTS</span>
      </div>
      <div class="rm-sep" aria-hidden="true"></div>
      <div class="rm-stat">
        <span class="rm-val" class:rm-hit={hitPct !== null && hitPct > 50} data-testid="rm-cache-hit">{hitPct !== null ? `${hitPct}%` : '—'}</span>
        <span class="rm-label">CACHE HIT</span>
      </div>
      <div class="rm-sep" aria-hidden="true"></div>
      <div class="rm-stat">
        <span class="rm-val" data-testid="rm-mode">{modeLabel}</span>
        <span class="rm-label">MODE</span>
      </div>
    </div>

    {#if result.results.length > 0}
      <ol class="rm-results" aria-label="Top results" data-testid="rm-result-list">
        {#each result.results.slice(0, 5) as r, i (r.step_id)}
          <li class="rm-result-row">
            <span class="rm-rank" aria-hidden="true">{i + 1}</span>
            <span class="rm-step-id" title={r.step_id}>{r.step_id}</span>
            <span class="rm-score">{r.score.toFixed(3)}</span>
          </li>
        {/each}
      </ol>
    {/if}

  {:else}
    <div class="rm-state rm-empty" data-testid="rm-empty">
      <span class="rm-empty-hint">Submit a query to see results</span>
    </div>
  {/if}
</div>

<style>
  .rm-panel {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    background: var(--la-bg-base, #0a0a12);
    font-family: var(--la-font-mono, monospace);
    overflow: hidden;
  }

  .rm-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    border-bottom: 1px solid var(--la-hair-base, #2c3140);
    flex-shrink: 0;
  }

  .rm-title {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.14em;
    color: var(--la-struct-primary, #00bfff);
  }

  .rm-mode-badge {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-mute, #6e7681);
    background: var(--la-bg-elev-1, #111214);
    border: 1px solid var(--la-hair-strong, #3a3f4b);
    padding: 1px 5px;
    border-radius: 2px;
  }

  /* ── Stats row ────────────────────────────────────────────────── */
  .rm-stats {
    display: flex;
    align-items: stretch;
    padding: 0;
    border-bottom: 1px solid var(--la-hair-faint, #1c2028);
    flex-shrink: 0;
  }

  .rm-stat {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    padding: 8px 12px;
    flex: 1;
  }

  .rm-val {
    font-size: 14px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: var(--la-text-bright, #f1f5f9);
  }

  .rm-val.rm-hit { color: var(--la-agent-researcher, #17c3b2); }

  .rm-label {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-mute, #6e7681);
    text-transform: uppercase;
  }

  .rm-sep {
    width: 1px;
    background: var(--la-hair-faint, #1c2028);
    align-self: stretch;
    flex-shrink: 0;
  }

  /* ── Result list ──────────────────────────────────────────────── */
  .rm-results {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
    margin: 0;
    list-style: none;
  }

  .rm-result-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--la-hair-faint, #1c2028);
  }
  .rm-result-row:last-child { border-bottom: none; }

  .rm-rank {
    font-size: 9px;
    color: var(--la-text-mute, #6e7681);
    width: 12px;
    text-align: right;
    flex-shrink: 0;
  }

  .rm-step-id {
    flex: 1;
    font-size: 10px;
    color: var(--la-text-base, #c9d1d9);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .rm-score {
    font-size: 9px;
    font-variant-numeric: tabular-nums;
    color: var(--la-text-dim, #8b949e);
    flex-shrink: 0;
  }

  /* ── State views ──────────────────────────────────────────────── */
  .rm-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 16px;
  }

  .rm-spinner {
    font-size: 18px;
    color: var(--la-struct-primary, #00bfff);
    animation: spin 1.2s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .rm-error-icon {
    font-size: 18px;
    color: var(--la-semantic-error, #ef4444);
  }

  .rm-error-msg {
    font-size: 9px;
    color: var(--la-text-dim, #8b949e);
    text-align: center;
    max-width: 200px;
    line-height: 1.4;
  }

  .rm-empty-hint {
    font-size: 9px;
    color: var(--la-text-mute, #6e7681);
    letter-spacing: 0.04em;
  }
</style>
