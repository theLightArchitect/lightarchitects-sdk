<!--
  @component BranchLane — LASDLC phase ladder / speculative lane viewer.
  @reads content as { lanes: Array<{ id: string; label: string; state: string; progress: number }>;
                      committed_lane_id?: string }
-->
<script lang="ts">
  let { data }: { data: unknown } = $props();
  type Lane = { id: string; label: string; state: string; progress: number };
  type D = { lanes?: Lane[]; committed_lane_id?: string };
  const d = $derived((data as D | null) ?? {});
  const lanes = $derived(d.lanes ?? []);

  const STATE_COLOR: Record<string, string> = {
    exploring:   'var(--ls-acc)',
    committed:   'var(--ls-acc-green)',
    rolled_back: 'var(--ls-text-ghost)',
  };
</script>
<div class="ls-bl-wrap">
  {#each lanes as lane}
    <div
      class="ls-bl-lane"
      class:ls-bl-committed={lane.id === d.committed_lane_id}
      style="--lane-color: {STATE_COLOR[lane.state] ?? 'var(--ls-border)'}"
    >
      <div class="ls-bl-track">
        <div class="ls-bl-fill" style="width: {Math.min(100, lane.progress ?? 0)}%"></div>
      </div>
      <span class="ls-bl-label">{lane.label}</span>
    </div>
  {/each}
  {#if lanes.length === 0}
    <div class="ls-bl-empty">no lanes yet</div>
  {/if}
</div>
<style>
.ls-bl-wrap  { display: flex; flex-direction: column; gap: 5px; }
.ls-bl-lane  { display: flex; align-items: center; gap: 8px; }
.ls-bl-track { flex: 1; height: 5px; background: var(--ls-sunken); border-radius: 3px; overflow: hidden; }
.ls-bl-fill  { height: 100%; background: var(--lane-color); transition: width 0.5s ease; }
.ls-bl-label { font-size: 9px; color: var(--ls-text-dim); min-width: 60px; }
.ls-bl-committed .ls-bl-fill { box-shadow: 0 0 6px var(--ls-acc-green); }
.ls-bl-empty { font-size: 9px; color: var(--ls-text-ghost); }
</style>
