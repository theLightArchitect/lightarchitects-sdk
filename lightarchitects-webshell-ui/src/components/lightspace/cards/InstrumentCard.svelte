<!--
  @component InstrumentCard
  @description Metrics KPI strip — phase output, loop budget progress bars.
  @contract EventType 'pillar_update' → PillarUpdatePayload (via pillarStream store)
  @reads pillarStream (existing store), lightspaceMetricsStore.loopBudget
  @mutates none
  @api none
-->
<script lang="ts">
  let { data }: { data: unknown } = $props();
  const d = $derived(data as { label?: string; value?: number; max?: number } ?? {});
</script>
<div class="ls-inst-wrap">
  <div class="ls-inst-label">{d.label ?? 'metrics'}</div>
  {#if d.max}
    <div class="ls-inst-bar"><div class="ls-inst-fill" style="width: {Math.round((d.value ?? 0) / d.max * 100)}%"></div></div>
    <div class="ls-inst-nums">{d.value ?? 0} / {d.max}</div>
  {:else}
    <div class="ls-inst-val">{d.value ?? '—'}</div>
  {/if}
</div>
<style>
.ls-inst-wrap { display: flex; flex-direction: column; gap: 6px; padding: 4px 0; }
.ls-inst-label { font-size: 8px; text-transform: uppercase; letter-spacing: var(--ls-tk-loose); color: var(--ls-text-mute); }
.ls-inst-bar { height: 3px; background: var(--ls-sunken); border-radius: 2px; overflow: hidden; }
.ls-inst-fill { height: 100%; background: linear-gradient(90deg, var(--ls-acc), var(--ls-acc-green)); transition: width 0.6s ease; }
.ls-inst-nums { font-size: 9px; color: var(--ls-text-dim); }
.ls-inst-val { font-size: 20px; font-family: var(--ls-font-display); font-weight: 700; color: var(--ls-text-bright); }
</style>
