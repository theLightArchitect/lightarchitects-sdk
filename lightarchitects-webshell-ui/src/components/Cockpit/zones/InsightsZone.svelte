<script lang="ts">
  import { builds, conductorTasks } from '$lib/stores';
  import { selectedTarget } from '$lib/cockpit/stores';
  import { deriveInsights } from '$lib/cockpit/engineerLens';

  const insights = $derived(deriveInsights($selectedTarget, $builds, $conductorTasks));

  const TREND_ICON: Record<string, string> = { up: '▲', down: '▼', stable: '—' };
  const TREND_CLS:  Record<string, string> = {
    up: 'trend-up', down: 'trend-down', stable: 'trend-stable',
  };
</script>

<div class="zone">
  <div class="zone-label">INSIGHTS <span class="zone-cache">30s cache</span></div>

  {#if insights.length === 0}
    <div class="zone-empty">collecting signals…</div>
  {:else}
    <div class="insight-list">
      {#each insights as ins (ins.id)}
        <div class="insight-row">
          {#if ins.trend}
            <span class="insight-trend {TREND_CLS[ins.trend]}">{TREND_ICON[ins.trend]}</span>
          {:else}
            <span class="insight-trend trend-stable">·</span>
          {/if}
          <div class="insight-body">
            <span class="insight-signal">{ins.signal}</span>
            <span class="insight-value">{ins.value}</span>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .zone {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .zone-label {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-mute);
    font-family: var(--la-font-mono, monospace);
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .zone-cache {
    font-size: 7px;
    font-weight: 400;
    color: var(--la-text-mute);
    opacity: 0.7;
  }

  .zone-empty {
    font-size: 9px;
    color: var(--la-text-mute);
    font-family: var(--la-font-mono, monospace);
    font-style: italic;
  }

  .insight-list {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .insight-row {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    font-family: var(--la-font-mono, monospace);
  }

  .insight-trend {
    font-size: 10px;
    flex-shrink: 0;
    width: 10px;
    line-height: 1.3;
  }

  .trend-up     { color: var(--la-semantic-ok); }
  .trend-down   { color: var(--la-semantic-error); }
  .trend-stable { color: var(--la-text-mute); }

  .insight-body {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    min-width: 0;
  }

  .insight-signal {
    font-size: 8px;
    font-weight: 700;
    color: var(--la-text-mute);
    letter-spacing: 0.04em;
  }

  .insight-value {
    font-size: 9px;
    color: var(--la-text-base);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
