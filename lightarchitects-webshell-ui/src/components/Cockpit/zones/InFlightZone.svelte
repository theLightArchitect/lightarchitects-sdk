<script lang="ts">
  import { builds, conductorTasks } from '$lib/stores';
  import { inFlightItems } from '$lib/cockpit/engineerLens';

  const items = $derived(inFlightItems($builds, $conductorTasks));

  function fmtElapsed(ms: number): string {
    const s = Math.floor(ms / 1000);
    if (s < 60)  return `${s}s`;
    const m = Math.floor(s / 60);
    if (m < 60)  return `${m}m`;
    return `${Math.floor(m / 60)}h${m % 60}m`;
  }
</script>

<div class="zone">
  <div class="zone-label">IN FLIGHT</div>

  {#if items.length === 0}
    <div class="zone-empty">nothing running</div>
  {:else}
    <div class="zone-list">
      {#each items as item (item.id)}
        <div class="flight-row">
          <span class="flight-spinner">◌</span>
          <span class="flight-label">{item.label}</span>
          {#if item.sibling}
            <span class="flight-sibling">{item.sibling}</span>
          {/if}
          {#if item.confidence !== undefined}
            <span class="flight-conf">{Math.round(item.confidence * 100)}%</span>
          {/if}
          <span class="flight-elapsed">{fmtElapsed(item.elapsedMs)}</span>
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
  }

  .zone-empty {
    font-size: 9px;
    color: var(--la-text-mute);
    font-family: var(--la-font-mono, monospace);
    font-style: italic;
  }

  .zone-list {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .flight-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 0;
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    border-bottom: 1px solid var(--la-hair-faint, var(--la-hair-base));
  }

  .flight-row:last-child { border-bottom: none; }

  .flight-spinner {
    font-size: 10px;
    color: var(--la-struct-primary);
    animation: spin 2s linear infinite;
    flex-shrink: 0;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }

  .flight-label {
    flex: 1;
    color: var(--la-text-base);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .flight-sibling {
    font-size: 8px;
    color: var(--la-struct-primary);
    flex-shrink: 0;
  }

  .flight-conf {
    font-size: 8px;
    color: var(--la-semantic-ok);
    flex-shrink: 0;
  }

  .flight-elapsed {
    font-size: 8px;
    color: var(--la-text-mute);
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }
</style>
