<script lang="ts">
  import { builds } from '$lib/stores';
  import { goto } from '$app/navigation';

  let count = $derived($builds.filter(b => b.status === 'in_progress').length);
</script>

{#if count > 0}
  <button
    class="chip"
    onclick={() => goto('/dashboard')}
    title="{count} build{count !== 1 ? 's' : ''} running. Click to view."
    data-testid="active-builds-chip"
  >
    <span class="chip-dot" aria-hidden="true"></span>
    <span class="chip-count">{count}</span>
    <span class="chip-label">active</span>
  </button>
{/if}

<style>
  .chip {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 2px 8px;
    background: color-mix(in srgb, var(--la-agent-researcher) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--la-agent-researcher) 30%, transparent);
    color: var(--la-agent-researcher);
    font-family: var(--la-font-mono);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    cursor: pointer;
    transition: background 80ms, border-color 80ms;
    flex-shrink: 0;
  }
  .chip:hover {
    background: color-mix(in srgb, var(--la-agent-researcher) 18%, transparent);
    border-color: color-mix(in srgb, var(--la-agent-researcher) 50%, transparent);
  }

  .chip-dot {
    display: inline-block;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--la-agent-researcher);
    box-shadow: 0 0 4px var(--la-agent-researcher);
    animation: pulse 2s ease-in-out infinite;
    flex-shrink: 0;
  }

  .chip-count {
    font-variant-numeric: tabular-nums;
  }

  .chip-label {
    font-weight: 400;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.5; }
  }

  @supports not (color: color-mix(in srgb, red 50%, blue)) {
    .chip         { background: rgba(77, 255, 142, 0.10); border-color: rgba(77, 255, 142, 0.30); }
    .chip:hover   { background: rgba(77, 255, 142, 0.18); }
  }
</style>
