<!--
  @component AgentSpawnCard
  @description Agent lifecycle card — shows agent type, status, progress bar.
  @contract Fleet SSE GET /api/builds/:id/fleet → FleetEvent (primary); a2a_envelope secondary
  @reads lightspaceMetricsStore.fleet (populated by subscribeFleet in Lightspace.svelte)
  @mutates none
  @api GET /api/builds/:id/fleet (fleet SSE stream)
-->
<script lang="ts">
  let { data }: { data: unknown } = $props();
  const d = $derived(data as { agentType?: string; status?: string; progress?: number; task?: string } ?? {});
</script>
<div class="ls-agent">
  <div class="ls-agent-header">
    <span class="ls-agent-type">{d.agentType ?? 'agent'}</span>
    <span class="ls-agent-status">{d.status ?? 'spawning'}</span>
  </div>
  {#if d.progress !== undefined}
    <div class="ls-agent-bar"><div class="ls-agent-fill" style="width: {d.progress}%"></div></div>
  {/if}
  {#if d.task}
    <p class="ls-agent-task">{d.task}</p>
  {/if}
</div>
<style>
.ls-agent { display: flex; flex-direction: column; gap: 6px; }
.ls-agent-header { display: flex; align-items: center; gap: 8px; }
.ls-agent-type { font-family: var(--ls-font-display); font-weight: 700; font-size: 10px; color: var(--ls-acc); letter-spacing: var(--ls-tk-loose); text-transform: uppercase; }
.ls-agent-status { font-size: 9px; color: var(--ls-acc-green); text-transform: uppercase; letter-spacing: var(--ls-tk-mid); }
.ls-agent-bar { height: 3px; background: var(--ls-sunken); border-radius: 2px; overflow: hidden; }
.ls-agent-fill { height: 100%; background: linear-gradient(90deg, var(--ls-acc), var(--ls-acc-green)); transition: width 0.6s ease; }
.ls-agent-task { font-size: 9px; color: var(--ls-text-dim); line-height: 1.4; margin: 0; }
</style>
