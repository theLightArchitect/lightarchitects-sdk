<script lang="ts">
  import { activeBuild, conductorTasks } from '$lib/stores';
  import ConductorPanel from '$lib/../components/ConductorPanel.svelte';
  import type { ConductorTask } from '$lib/types';

  let build = $derived($activeBuild);

  // Build-scoped tasks — falls back to all tasks when no build is selected
  let buildTasks = $derived(
    build
      ? $conductorTasks.filter((t: ConductorTask) => !t.buildId || t.buildId === build.id)
      : $conductorTasks
  );

  let activeTask = $derived(buildTasks.find((t: ConductorTask) => t.status === 'running') ?? null);
</script>

<div class="comms-wrap" data-testid="comms-view">
  {#if !build}
    <div class="comms-empty">— no build selected —</div>
  {:else}
    <div class="comms-main">

      <!-- Phase handoff card -->
      <div class="handoff-rail">
        <div class="rail-head">PHASE STATUS</div>
        <div class="handoff-card">
          <div class="phase-crumb">
            <span class="phase-label">STATUS</span>
            <span class="phase-name">{build.status.toUpperCase()}</span>
          </div>
          <div class="phase-crumb">
            <span class="phase-label">PILLAR</span>
            <span class="phase-name">{build.currentPillar ?? '—'}</span>
          </div>
          {#if activeTask}
            <div class="task-claim">
              <span class="claim-label">ACTIVE AGENT</span>
              <span class="claim-sibling">{activeTask.sibling.toUpperCase()}</span>
              <span class="claim-type">{activeTask.taskType}</span>
            </div>
          {:else}
            <div class="task-claim idle">
              <span class="claim-label">NO AGENT RUNNING</span>
            </div>
          {/if}
        </div>
      </div>

      <!-- Conductor queue scoped to this build -->
      <div class="queue-panel">
        <div class="panel-head">
          <span class="panel-label">CONDUCTOR QUEUE</span>
          <span class="panel-meta">{buildTasks.length} task{buildTasks.length === 1 ? '' : 's'}</span>
        </div>
        <div class="panel-body">
          <ConductorPanel maxDisplay={12} />
        </div>
      </div>

    </div>
  {/if}
</div>

<style>
  .comms-wrap {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .comms-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    font-size: 11px;
    color: var(--la-text-dim);
    letter-spacing: 0.06em;
  }

  .comms-main {
    display: flex;
    flex-direction: column;
    gap: 0;
    height: 100%;
    overflow: hidden;
  }

  .handoff-rail {
    flex-shrink: 0;
    border-bottom: 1px solid var(--la-hair-faint);
    padding: 10px 16px;
    background: var(--la-bg-elev-1);
  }

  .rail-head {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-dim);
    margin-bottom: 8px;
  }

  .handoff-card {
    display: flex;
    align-items: center;
    gap: 24px;
  }

  .phase-crumb {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .phase-label {
    font-size: 9px;
    color: var(--la-text-dim);
    letter-spacing: 0.06em;
  }

  .phase-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--la-text-bright);
    letter-spacing: 0.04em;
  }

  .task-claim {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px;
    border-radius: 4px;
    background: var(--la-drawer-bg);
    border: 1px solid var(--la-hair-faint);
  }

  .task-claim.idle {
    opacity: 0.5;
  }

  .claim-label {
    font-size: 9px;
    color: var(--la-text-dim);
    letter-spacing: 0.06em;
  }

  .claim-sibling {
    font-size: 10px;
    font-weight: 700;
    color: var(--la-agent-engineer);
    letter-spacing: 0.04em;
  }

  .claim-type {
    font-size: 10px;
    color: var(--la-text-label);
  }

  .queue-panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 16px;
    border-bottom: 1px solid var(--la-hair-faint);
    flex-shrink: 0;
  }

  .panel-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-dim);
  }

  .panel-meta {
    font-size: 9px;
    color: var(--la-text-mute);
  }

  .panel-body {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
  }
</style>
