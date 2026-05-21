<script lang="ts">
  import { builds, conductorTasks } from '$lib/stores';
  import { selectedTarget } from '$lib/cockpit/stores';
  import { needsActionItems } from '$lib/cockpit/engineerLens';
  import { navigate } from '$lib/routes';

  const items = $derived(needsActionItems($selectedTarget, $builds, $conductorTasks));
</script>

<div class="zone">
  <div class="zone-label">NEEDS ACTION</div>

  {#if items.length === 0}
    <div class="zone-empty">all clear</div>
  {:else}
    <div class="zone-list">
      {#each items as item (item.id)}
        <div class="action-row" class:urgency-critical={item.urgency === 'critical'} class:urgency-high={item.urgency === 'high'}>
          <span class="action-dot" class:dot-critical={item.urgency === 'critical'} class:dot-high={item.urgency === 'high'}></span>
          <span class="action-label">{item.label}</span>
          {#if item.buildId}
            <button
              class="action-verb"
              onclick={() => navigate('/builds/:buildId/:view', { buildId: item.buildId!, view: 'comms' })}
            >{item.verb}</button>
          {:else}
            <span class="action-verb-static">{item.verb}</span>
          {/if}
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
    color: var(--la-semantic-ok);
    font-family: var(--la-font-mono, monospace);
    font-style: italic;
  }

  .zone-list {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .action-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 0;
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    border-bottom: 1px solid var(--la-hair-faint, var(--la-hair-base));
  }

  .action-row:last-child { border-bottom: none; }

  .action-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--la-text-mute);
    flex-shrink: 0;
  }

  .dot-critical { background: var(--la-semantic-error); }
  .dot-high     { background: var(--la-semantic-warn); }

  .action-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--la-text-base);
  }

  .urgency-critical .action-label { color: var(--la-semantic-error); }
  .urgency-high .action-label     { color: var(--la-semantic-warn); }

  .action-verb {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.07em;
    padding: 1px 5px;
    border: 1px solid var(--la-struct-primary);
    background: transparent;
    color: var(--la-struct-primary);
    cursor: pointer;
    font-family: var(--la-font-mono, monospace);
    flex-shrink: 0;
  }

  .action-verb:hover {
    background: color-mix(in srgb, var(--la-struct-primary) 10%, transparent);
  }

  .action-verb-static {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.07em;
    color: var(--la-text-mute);
    flex-shrink: 0;
  }
</style>
