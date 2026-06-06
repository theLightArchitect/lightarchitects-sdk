<!--
  @component CostTicker
  Ambient cost accumulator for the current session / active build.
  Shows total inference + tool spend from GET /api/cost/session (§ future).
  Falls back to "—" when the cost-accounting plan endpoint is unavailable.
  This is a Phase 6.7 Tier 4 non-guarantee surface — the `—` fallback is
  explicitly documented as the valid state until the cost plan ships.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  const POLL_MS = 30_000;

  let display = $state<string>('—');
  let timer: ReturnType<typeof setInterval> | null = null;

  async function refresh() {
    try {
      const res = await fetch('/api/cost/session', { headers: { Accept: 'application/json' } });
      if (!res.ok) { display = '—'; return; }
      const data = await res.json() as { total_usd?: number };
      display = data.total_usd !== undefined
        ? `$${data.total_usd.toFixed(4)}`
        : '—';
    } catch {
      display = '—';
    }
  }

  onMount(() => {
    void refresh();
    timer = setInterval(refresh, POLL_MS);
  });

  onDestroy(() => {
    if (timer !== null) clearInterval(timer);
  });
</script>

<span class="cost-ticker" data-testid="cost-ticker" title="Session inference cost (updates every 30s)">{display}</span>

<style>
  .cost-ticker {
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    font-variant-numeric: tabular-nums;
    color: var(--la-text-mute, rgba(255,255,255,0.28));
    letter-spacing: 0.04em;
    cursor: default;
    white-space: nowrap;
  }
</style>
