<!--
  @component BranchLaneCard
  @description LASDLC phase ladder — parallel agent exploration lanes.
    Shows 1-3 lanes with agent type, state, progress bar, and committed indicator.
  @contract EventType 'merge_agent_status' → MergeAgentStatusEvent
  @reads mergeAgentEvents (existing store)
  @mutates lightspaceLasdlcStore.branchLanes
  @api none — data arrives via existing SSE pipeline
-->
<script lang="ts">
  import type { BranchLane } from '$lib/lightspace-types';
  let { data }: { data: unknown } = $props();
  const d = $derived(data as { lanes?: BranchLane[] } ?? {});
</script>
<div class="ls-lanes">
  {#each d.lanes ?? [] as lane}
    <div class="ls-lane" class:ls-lane-committed={lane.state === 'committed'}>
      <div class="ls-lane-head">
        <span class="ls-lane-agent">{lane.agentKey}</span>
        <span class="ls-lane-state">{lane.state}</span>
      </div>
      <p class="ls-lane-task">{lane.taskDesc}</p>
      <div class="ls-lane-bar">
        <div class="ls-lane-fill" style="width: {lane.progress}%"></div>
      </div>
    </div>
  {/each}
  {#if !d.lanes?.length}
    <span class="ls-lanes-empty">no active exploration lanes</span>
  {/if}
</div>
<style>
.ls-lanes { display: flex; flex-direction: column; gap: 8px; }
.ls-lane { padding: 8px 10px; background: var(--ls-sunken); border: 1px solid var(--ls-border); display: flex; flex-direction: column; gap: 4px; transition: background var(--ls-fast); }
.ls-lane-committed { border-color: var(--ls-acc-green); background: rgba(62,207,142,0.06); }
.ls-lane-head { display: flex; align-items: center; gap: 8px; }
.ls-lane-agent { font-family: var(--ls-font-display); font-weight: 700; font-size: 9px; letter-spacing: var(--ls-tk-mid); color: var(--ls-acc); text-transform: uppercase; }
.ls-lane-state { font-size: 8px; color: var(--ls-text-mute); text-transform: uppercase; margin-left: auto; }
.ls-lane-task { font-size: 9px; color: var(--ls-text-dim); margin: 0; line-height: 1.4; }
.ls-lane-bar { height: 3px; background: var(--ls-border); border-radius: 2px; overflow: hidden; }
.ls-lane-fill { height: 100%; background: linear-gradient(90deg, var(--ls-acc), var(--ls-acc-green)); transition: width 0.6s ease; }
.ls-lane-committed .ls-lane-fill { box-shadow: 0 0 10px var(--ls-acc-green); }
.ls-lanes-empty { font-style: italic; color: var(--ls-text-ghost); font-size: 10px; }
</style>
