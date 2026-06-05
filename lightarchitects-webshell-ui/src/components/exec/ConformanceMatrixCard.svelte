<script lang="ts">
  // WHY: operator_experience_layer phase-6 widget — per-contract per-provider
  // VERIFIED/UNTESTED grid; updates via SSE BuildUpdate with phase=conformance.

  type ProviderKey = string;
  type ConformanceStatus = 'VERIFIED' | 'UNTESTED' | 'FAIL' | 'SKIP';

  interface MatrixRow {
    contractId: string;
    statusByProvider: Record<ProviderKey, ConformanceStatus>;
  }

  let { rows = [], providers = [] }: { rows?: MatrixRow[]; providers?: ProviderKey[] } = $props();

  function statusClass(s: ConformanceStatus): string {
    if (s === 'VERIFIED') return 'cm-verified';
    if (s === 'FAIL') return 'cm-fail';
    if (s === 'SKIP') return 'cm-skip';
    return 'cm-untested';
  }

  function statusGlyph(s: ConformanceStatus): string {
    if (s === 'VERIFIED') return '✓';
    if (s === 'FAIL') return '✗';
    if (s === 'SKIP') return '—';
    return '?';
  }
</script>

<div class="cm-card" data-testid="conformance-matrix-card">
  <div class="cm-header">
    <span class="cm-title">CONFORMANCE MATRIX</span>
    {#if providers.length > 0}
      <div class="cm-providers">
        {#each providers as p}<span class="cm-prov-label">{p}</span>{/each}
      </div>
    {/if}
  </div>

  {#if rows.length === 0}
    <div class="cm-empty">Run /VERIFY V4 to populate conformance data</div>
  {:else}
    <table class="cm-table" aria-label="Conformance matrix">
      <thead>
        <tr>
          <th class="cm-th-contract">Contract</th>
          {#each providers as p}<th class="cm-th-prov">{p}</th>{/each}
        </tr>
      </thead>
      <tbody>
        {#each rows as row}
          <tr>
            <td class="cm-contract-id" title={row.contractId}>{row.contractId.split('.').slice(-1)[0]}</td>
            {#each providers as p}
              <td class="cm-cell {statusClass(row.statusByProvider[p] ?? 'UNTESTED')}"
                  title="{row.contractId} · {p}: {row.statusByProvider[p] ?? 'UNTESTED'}">
                {statusGlyph(row.statusByProvider[p] ?? 'UNTESTED')}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .cm-card {
    display: flex;
    flex-direction: column;
    background: var(--la-bg-elev-1, #111);
    border: 1px solid var(--la-border, #333);
    border-radius: 4px;
    font-family: var(--la-font-chrome, monospace);
    font-size: 10px;
    overflow: hidden;
  }

  .cm-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 8px;
    border-bottom: 1px solid var(--la-border, #333);
  }

  .cm-title { color: var(--la-text-mute, #555); text-transform: uppercase; letter-spacing: 0.05em; font-size: 9px; }

  .cm-providers { display: flex; gap: 4px; }
  .cm-prov-label { font-size: 8px; color: var(--la-text-dim, #888); background: var(--la-bg-void, #000); padding: 1px 4px; border-radius: 2px; }

  .cm-empty { padding: 8px 10px; color: var(--la-text-mute, #555); font-style: italic; }

  .cm-table { width: 100%; border-collapse: collapse; }

  .cm-th-contract, .cm-th-prov {
    padding: 3px 6px;
    text-align: left;
    font-size: 9px;
    color: var(--la-text-mute, #555);
    border-bottom: 1px solid var(--la-border, #333);
  }

  .cm-th-prov { text-align: center; }

  .cm-contract-id {
    padding: 3px 6px;
    font-size: 9px;
    color: var(--la-text-dim, #888);
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cm-cell {
    padding: 3px 6px;
    text-align: center;
    font-size: 10px;
  }

  .cm-verified { color: var(--la-agent-knowledge, #4caf50); }
  .cm-fail { color: var(--la-agent-security, #f55); }
  .cm-skip { color: var(--la-text-mute, #555); }
  .cm-untested { color: var(--la-text-dim, #888); }
</style>
