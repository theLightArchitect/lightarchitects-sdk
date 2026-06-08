<script lang="ts">
  import { goto } from '$app/navigation';

  interface Props {
    turnSpanId: string;
  }

  let { turnSpanId }: Props = $props();

  // Show last 12 chars of span UUID for compactness — enough to distinguish turns.
  const shortId = turnSpanId.slice(-12);

  function openAyin() {
    // Navigate to observability panel. Gap 3 (?span= deeplink filter) deferred
    // pending AYIN binary change — operators can use shortId for manual lookup.
    goto('/observability'); // was '/#/observability' — hash-routing artifact removed
  }
</script>

<div class="lineage-strip" data-testid="turn-lineage-strip" data-span-id={turnSpanId}>
  <span class="lineage-icon" aria-hidden="true">◈</span>
  <span class="lineage-label">span</span>
  <span class="lineage-id">{shortId}</span>
  <button class="lineage-link" onclick={openAyin} title="View in AYIN Lineage Circuit">
    View in AYIN →
  </button>
</div>

<style>
  .lineage-strip {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-top: 6px;
    padding: 2px 6px;
    border-left: 2px solid rgba(255, 215, 0, 0.35);
    font-family: var(--la-font-mono, 'JetBrains Mono', 'Fira Code', monospace);
    font-size: 9px;
    color: rgba(255, 215, 0, 0.55);
    user-select: none;
  }

  .lineage-icon {
    font-size: 8px;
    color: rgba(255, 215, 0, 0.7);
    flex-shrink: 0;
  }

  .lineage-label {
    color: rgba(255, 215, 0, 0.4);
    letter-spacing: 0.05em;
    text-transform: uppercase;
    font-size: 8px;
  }

  .lineage-id {
    color: rgba(255, 215, 0, 0.8);
    letter-spacing: 0.08em;
    font-size: 9px;
  }

  .lineage-link {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    color: rgba(255, 215, 0, 0.45);
    font-family: inherit;
    font-size: 8px;
    letter-spacing: 0.04em;
    text-decoration: none;
    transition: color 0.15s ease;
    margin-left: 2px;
  }

  .lineage-link:hover {
    color: rgba(255, 215, 0, 0.9);
  }
</style>
