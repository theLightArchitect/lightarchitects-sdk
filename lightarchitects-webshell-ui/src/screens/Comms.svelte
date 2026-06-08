<script lang="ts">
  import { builds, conductorTasks, conductorStats } from '$lib/stores';
  import ConductorPanel from '$lib/../components/ConductorPanel.svelte';
  import type { Build, ConductorTask } from '$lib/types';
  import { goto } from '$app/navigation';

  let allBuilds    = $derived($builds);
  let allTasks     = $derived($conductorTasks);
  let stats        = $derived($conductorStats);

  function activeTasksFor(buildId: string): number {
    return allTasks.filter((t: ConductorTask) => t.buildId === buildId && t.status === 'running').length;
  }

  function runningAgentFor(buildId: string): string | null {
    const t = allTasks.find((t: ConductorTask) => t.buildId === buildId && t.status === 'running');
    return t ? t.sibling.toUpperCase() : null;
  }

  function statusColor(status: Build['status']): string {
    switch (status) {
      case 'in_progress': return 'var(--la-agent-engineer)';
      case 'completed':   return 'var(--la-agent-researcher)';
      case 'failed':      return 'var(--la-agent-security)';
      default:            return 'var(--la-text-dim)';
    }
  }
</script>

<div class="comms-dashboard" data-testid="comms-dashboard">
  <!-- Header row -->
  <div class="dash-header">
    <div class="dash-title">Activity</div>
    <div class="dash-meta">
      <span class="meta-chip">{stats.total} task{stats.total === 1 ? '' : 's'}</span>
      <span class="meta-chip running">{stats.running} running</span>
    </div>
  </div>

  <div class="dash-body">
    <!-- Build comms summary rail -->
    <div class="builds-rail">
      <div class="rail-head">BUILDS</div>
      {#if allBuilds.length === 0}
        <div class="rail-empty">— no builds —</div>
      {:else}
        <div class="build-list">
          {#each allBuilds as build (build.id)}
            {@const agent = runningAgentFor(build.id)}
            {@const taskCount = activeTasksFor(build.id)}
            <button
              class="build-row"
              onclick={() => goto(`/builds/${build.id}/comms`)}
            >
              <div class="build-name">{build.name}</div>
              <div class="build-meta">
                <span class="build-status" style="color: {statusColor(build.status)}">
                  {build.status.toUpperCase().replace('_', ' ')}
                </span>
                {#if build.currentPillar}
                  <span class="build-pillar">{build.currentPillar}</span>
                {/if}
                {#if agent}
                  <span class="build-agent">{agent}</span>
                {:else if taskCount === 0}
                  <span class="build-idle">IDLE</span>
                {/if}
              </div>
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Global conductor queue -->
    <div class="queue-panel">
      <div class="panel-head">
        <span class="panel-label">GLOBAL CONDUCTOR QUEUE</span>
        <span class="panel-count">{allTasks.length} task{allTasks.length === 1 ? '' : 's'}</span>
      </div>
      <div class="panel-body">
        <ConductorPanel maxDisplay={50} />
      </div>
    </div>
  </div>
</div>

<style>
  .comms-dashboard {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .dash-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-bottom: 1px solid var(--la-hair-strong);
    flex-shrink: 0;
    background: var(--la-bg-base);
  }

  .dash-title {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-stark);
  }

  .dash-meta {
    display: flex;
    gap: 8px;
  }

  .meta-chip {
    font-size: 9px;
    color: var(--la-text-dim);
    letter-spacing: 0.06em;
    padding: 2px 6px;
    border: 1px solid var(--la-hair-faint);
    border-radius: 3px;
  }

  .meta-chip.running {
    color: var(--la-agent-engineer);
    border-color: var(--la-agent-engineer);
  }

  .dash-body {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .builds-rail {
    width: 260px;
    flex-shrink: 0;
    border-right: 1px solid var(--la-hair-faint);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .rail-head {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-dim);
    padding: 6px 12px;
    border-bottom: 1px solid var(--la-hair-faint);
    flex-shrink: 0;
  }

  .rail-empty {
    font-size: 10px;
    color: var(--la-text-mute);
    padding: 12px;
    letter-spacing: 0.06em;
  }

  .build-list {
    flex: 1;
    overflow-y: auto;
  }

  .build-row {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 8px 12px;
    width: 100%;
    text-align: left;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--la-hair-faint);
    cursor: pointer;
    transition: background 80ms;
    font-family: inherit;
  }

  .build-row:hover {
    background: var(--la-bg-elev-1);
  }

  .build-name {
    font-size: 11px;
    font-weight: 600;
    color: var(--la-text-base);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 220px;
  }

  .build-meta {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .build-status {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.06em;
  }

  .build-pillar {
    font-size: 9px;
    color: var(--la-text-mute);
    letter-spacing: 0.04em;
  }

  .build-agent {
    font-size: 9px;
    font-weight: 700;
    color: var(--la-agent-engineer);
    letter-spacing: 0.04em;
  }

  .build-idle {
    font-size: 9px;
    color: var(--la-text-mute);
    letter-spacing: 0.04em;
    opacity: 0.6;
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

  .panel-count {
    font-size: 9px;
    color: var(--la-text-mute);
  }

  .panel-body {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
  }
</style>
