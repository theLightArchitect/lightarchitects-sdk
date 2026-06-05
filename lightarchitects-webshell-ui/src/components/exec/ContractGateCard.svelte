<script lang="ts">
  // WHY: operator_experience_layer phase-4 widget — Q5 contract-gate card shows
  // 235/235 (or N/235) inline. AYIN span agent.skill.gate forwarded via BuildUpdate SSE.

  interface GateResult {
    passed: number;
    total: number;
    failedIds?: string[];
    runAt?: string;
  }

  let { result }: { result?: GateResult } = $props();

  const pct = $derived(
    result ? Math.round((result.passed / Math.max(result.total, 1)) * 100) : null,
  );
</script>

<div class="cg-card" data-testid="contract-gate-card">
  <span class="cg-label">CONTRACT GATE</span>
  {#if result}
    <span
      class="cg-count"
      class:cg-pass={result.passed === result.total}
      class:cg-fail={result.passed < result.total}
    >
      {result.passed}/{result.total}
    </span>
    {#if pct !== null}
      <span class="cg-pct" class:cg-pass={pct === 100}>{pct}%</span>
    {/if}
    {#if result.failedIds && result.failedIds.length > 0}
      <details class="cg-failures">
        <summary class="cg-fail-summary">↓ {result.failedIds.length} failing</summary>
        <ul class="cg-fail-list">
          {#each result.failedIds as id}
            <li class="cg-fail-id">{id}</li>
          {/each}
        </ul>
      </details>
    {/if}
  {:else}
    <span class="cg-pending">not run</span>
  {/if}
</div>

<style>
  .cg-card {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    background: var(--la-bg-elev-1, #111);
    border: 1px solid var(--la-border, #333);
    border-radius: 3px;
    font-size: 10px;
    font-family: var(--la-font-chrome, monospace);
  }

  .cg-label { color: var(--la-text-mute, #555); text-transform: uppercase; letter-spacing: 0.05em; }
  .cg-count { font-weight: 700; color: var(--la-text-bright); }
  .cg-pct { color: var(--la-text-dim, #888); }
  .cg-pass { color: var(--la-agent-knowledge, #4caf50) !important; }
  .cg-fail { color: var(--la-agent-security, #f55) !important; }
  .cg-pending { color: var(--la-text-mute, #555); font-style: italic; }

  .cg-failures { margin-left: 4px; }
  .cg-fail-summary { cursor: pointer; color: var(--la-agent-security, #f55); font-size: 9px; }
  .cg-fail-list { margin: 2px 0 0 12px; padding: 0; list-style: none; }
  .cg-fail-id { font-size: 9px; color: var(--la-text-dim, #888); white-space: nowrap; }
</style>
