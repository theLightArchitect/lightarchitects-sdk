<script lang="ts">
  import { onMount } from 'svelte';
  import { activeBuild, currentBuildId } from '$lib/stores';
  import { matchRoute } from '$lib/routes';
  import KanbanView  from '$lib/../components/views/KanbanView.svelte';
  import ListView    from '$lib/../components/views/ListView.svelte';
  import OperatorView from '$lib/../components/views/OperatorView.svelte';
  import ManifestView from '$lib/../components/views/ManifestView.svelte';
  import PlanView    from '$lib/../components/PlanView.svelte';

  type ViewMode = 'kanban' | 'list' | 'operator' | 'manifest' | 'plan';

  let viewMode = $state<ViewMode>('kanban');
  let build = $derived($activeBuild);

  // Hydrate currentBuildId when navigating directly to /builds/:id
  onMount(() => {
    const route = window.location.hash.slice(1) || '/';
    const { params } = matchRoute(route);
    if (params.buildId && !$currentBuildId) {
      currentBuildId.set(params.buildId);
    }
  });

  const VIEW_TABS: { key: ViewMode; label: string }[] = [
    { key: 'kanban',   label: 'KANBAN' },
    { key: 'list',     label: 'LIST' },
    { key: 'operator', label: 'OPERATOR' },
    { key: 'manifest', label: 'MANIFEST' },
    { key: 'plan',     label: 'PLAN' },
  ];
</script>

<div class="build-detail-shell">
  {#if build}
    <!-- View mode tab bar -->
    <div class="view-tab-bar">
      <div class="build-crumb">
        <span class="crumb-name">{build.name}</span>
        <span class="crumb-id">{build.id.slice(0, 8)}</span>
      </div>
      <div class="view-tabs">
        {#each VIEW_TABS as t}
          <button
            class="view-tab"
            class:active={viewMode === t.key}
            onclick={() => { viewMode = t.key; }}
          >
            {t.label}
          </button>
        {/each}
      </div>
    </div>

    <div class="view-content" data-mode={viewMode}>
      {#if viewMode === 'kanban'}
        <KanbanView />
      {:else if viewMode === 'list'}
        <ListView />
      {:else if viewMode === 'operator'}
        <OperatorView />
      {:else if viewMode === 'manifest'}
        <ManifestView />
      {:else}
        <PlanView />
      {/if}
    </div>
  {:else}
    <div class="build-detail-empty">
      <span>— no build selected —</span>
    </div>
  {/if}
</div>

<style>
  .build-detail-shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .view-tab-bar {
    display: flex;
    align-items: center;
    border-bottom: 1px solid var(--la-hair-strong);
    flex-shrink: 0;
    background: var(--la-bg-base);
    gap: 0;
  }

  .build-crumb {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 0 16px;
    border-right: 1px solid var(--la-hair-faint);
    height: 36px;
    flex-shrink: 0;
  }

  .crumb-name {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--la-text-base);
    white-space: nowrap;
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .crumb-id {
    font-size: 9px;
    color: var(--la-text-mute);
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.04em;
  }

  .view-tabs {
    display: flex;
    flex: 1;
    align-items: stretch;
  }

  .view-tab {
    padding: 0 16px;
    height: 36px;
    font-family: inherit;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--la-text-mute);
    cursor: pointer;
    transition: color 80ms, border-color 80ms;
    white-space: nowrap;
  }

  .view-tab:hover { color: var(--la-text-base); }
  .view-tab.active {
    color: var(--la-text-stark);
    border-bottom-color: var(--la-focus-ring);
  }

  .view-content {
    flex: 1;
    overflow: hidden;
    min-height: 0;
  }

  .build-detail-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--la-text-mute);
    font-size: 11px;
    letter-spacing: 0.12em;
    font-style: italic;
  }
</style>
