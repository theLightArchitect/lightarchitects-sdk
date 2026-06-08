<!--
  @component MonitorCard
  @description Status KPI strip — compact 4-col metric grid.
    Displays agent health indicators, loop budget, northstar ledger.
  @contract EventType 'supervisor_update' → NorthstarEvaluationEvent (via northstarState store)
  @reads lightspaceSessionStore.northstarState (for northstar status metrics)
  @mutates none
  @api none — data arrives via SSE supervisor_update → northstarState
-->
<script lang="ts">
  let { data }: { data: unknown } = $props();
  const d = $derived(data as Record<string, string | number> ?? {});
</script>
<div class="ls-mon-grid">
  {#each Object.entries(d) as [k, v]}
    <div class="ls-mon-cell">
      <span class="ls-mon-key">{k}</span>
      <span class="ls-mon-val">{v}</span>
    </div>
  {/each}
  {#if Object.keys(d).length === 0}
    <div class="ls-mon-cell ls-mon-empty"><span class="ls-mon-key">status</span><span class="ls-mon-val">awaiting…</span></div>
  {/if}
</div>
<style>
.ls-mon-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
.ls-mon-cell { display: flex; flex-direction: column; gap: 2px; padding: 5px 7px; background: var(--ls-sunken); border: 1px solid var(--ls-border); }
.ls-mon-key { font-size: 7px; text-transform: uppercase; letter-spacing: var(--ls-tk-loose); color: var(--ls-text-ghost); }
.ls-mon-val { font-size: 11px; color: var(--ls-text-bright); font-family: var(--ls-font-display); font-weight: 700; }
</style>
