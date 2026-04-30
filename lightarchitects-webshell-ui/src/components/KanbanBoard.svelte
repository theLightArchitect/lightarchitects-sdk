<!-- Origin: scope-bleed from radiant-weaving-phoenix; landed via squishy-tome (commit 42bb840) merge. See merge commit message. Phoenix is the canonical owner. -->
<script lang="ts">
  import type { Build } from '$lib/types';
  import { ROADMAP } from '$lib/design-tokens';
  import KanbanCard from './KanbanCard.svelte';

  let { builds, onOpenBuild, onSelectBuild }: {
    builds: Build[];
    onOpenBuild: (id: string) => void;
    onSelectBuild?: (build: Build) => void;
  } = $props();

  const KANBAN_COLUMNS = [
    { key: 'queued',      label: 'Planned',     color: '#64748b', icon: '○' },
    { key: 'in_progress', label: 'In Progress', color: '#22c55e', icon: '●' },
    { key: 'paused',      label: 'Blocked',     color: '#f59e0b', icon: '⛔' },
    { key: 'completed',   label: 'Completed',   color: '#3b82f6', icon: '✓' },
    { key: 'failed',      label: 'Failed',      color: '#ef4444', icon: '✗' },
  ] as const;

  let columns = $derived(
    KANBAN_COLUMNS.map(col => ({
      ...col,
      builds: builds
        .filter(b => b.status === col.key)
        .sort((a, b) => {
          const prioOrder: Record<string, number> = { high: 0, medium: 1, low: 2 };
          const pa = prioOrder[a.priority ?? ''] ?? 3;
          const pb = prioOrder[b.priority ?? ''] ?? 3;
          if (pa !== pb) return pa - pb;
          return a.name.localeCompare(b.name);
        }),
    }))
  );

  let totalBuilds = $derived(builds.length);
  let completedCount = $derived(columns.find(c => c.key === 'completed')?.builds.length ?? 0);
  let progressPct = $derived(totalBuilds > 0 ? Math.round((completedCount / totalBuilds) * 100) : 0);

  function handleCardClick(build: Build) {
    if (onSelectBuild) {
      onSelectBuild(build);
    } else {
      onOpenBuild(build.id);
    }
  }
</script>

<div class="kanban-wrapper" data-testid="kanban-board">
  <!-- Board columns -->
  <div class="kanban-columns">
    {#each columns as col, colIdx}
      <div
        class="kanban-column"
        style="animation-delay: {colIdx * 0.05}s;"
        data-testid="kanban-column-{col.key}"
      >
        <!-- Column header -->
        <div
          class="column-header"
          style="border-bottom: 2px solid {col.color}40;"
        >
          <div class="flex items-center gap-2">
            <span class="text-xs" style="color: {col.color}">{col.icon}</span>
            <span class="text-xs font-semibold text-[#e2e8f0]">{col.label}</span>
          </div>
          <span
            class="text-[9px] font-mono px-1.5 py-0.5 rounded-full"
            style="background: {col.color}15; color: {col.color};"
          >
            {col.builds.length}
          </span>
        </div>

        <!-- Column body -->
        <div class="column-body">
          {#if col.builds.length === 0}
            <div class="empty-state">
              <span class="text-lg text-[#334155] mb-1">{col.icon}</span>
              <span class="text-[9px] text-[#475569]">No {col.label.toLowerCase()} builds</span>
            </div>
          {:else}
            {#each col.builds as build, cardIdx (build.id)}
              <div style="animation-delay: {0.1 + cardIdx * 0.05}s;">
                <KanbanCard {build} onOpen={() => handleCardClick(build)} />
              </div>
            {/each}
          {/if}
        </div>
      </div>
    {/each}
  </div>

  <!-- Progress bar (fixed bottom) -->
  <div class="progress-track">
    <div class="progress-fill" style="width: {progressPct}%;"></div>
  </div>

  <!-- Stat summary bar -->
  <div class="stat-bar">
    <div class="flex items-center gap-3">
      {#each columns as col}
        <span class="text-[9px]">
          {col.label}: <strong style="color: {col.color}; font-family: monospace;">{col.builds.length}</strong>
        </span>
      {/each}
    </div>
    <span class="text-[9px] text-[#475569] font-mono">{progressPct}% complete</span>
  </div>
</div>

<style>
  .kanban-wrapper {
    display: flex;
    flex-direction: column;
    height: 100%;
    position: relative;
  }

  .kanban-columns {
    display: flex;
    gap: 14px;
    flex: 1;
    overflow-x: auto;
    padding-bottom: 48px; /* space for progress bar + stats */
  }

  .kanban-column {
    flex: 1;
    min-width: 240px;
    display: flex;
    flex-direction: column;
    animation: columnSlideIn 0.4s ease both;
  }

  .column-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    margin-bottom: 10px;
    border-radius: 10px 10px 0 0;
    background: rgba(18, 18, 30, 0.4);
    backdrop-filter: blur(12px);
    border: 1px solid rgba(42, 42, 58, 0.4);
    flex-shrink: 0;
  }

  .column-body {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 0 2px;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 32px 12px;
    border: 1px dashed #1e293b;
    border-radius: 8px;
    text-align: center;
  }

  /* Progress bar — bottom of board */
  .progress-track {
    position: absolute;
    bottom: 28px;
    left: 0;
    right: 0;
    height: 3px;
    background: #1e293b;
    border-radius: 2px;
    overflow: hidden;
    z-index: 2;
  }

  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, #ef4444, #f59e0b, #22c55e);
    border-radius: 2px;
    transition: width 0.6s ease;
  }

  /* Stat summary row */
  .stat-bar {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 8px;
    color: #94a3b8;
    z-index: 2;
  }

  @keyframes columnSlideIn {
    from {
      opacity: 0;
      transform: translateY(30px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
