<script lang="ts">
  import { onMount } from 'svelte';
  import { activeBuild, currentBuildId, activityFeed } from '$lib/stores';
  import { matchRoute, navigate } from '$lib/routes';
  import KanbanView  from '$lib/../components/views/KanbanView.svelte';
  import ListView    from '$lib/../components/views/ListView.svelte';
  import OperatorView from '$lib/../components/views/OperatorView.svelte';
  import ManifestView from '$lib/../components/views/ManifestView.svelte';
  import PlanView    from '$lib/../components/PlanView.svelte';
  import CommsView   from '$lib/../components/views/CommsView.svelte';

  type ViewMode = 'kanban' | 'list' | 'operator' | 'manifest' | 'plan' | 'comms';

  const VIEW_MODES: ViewMode[] = ['kanban', 'list', 'operator', 'manifest', 'plan', 'comms'];

  let viewMode = $state<ViewMode>('kanban');
  let build = $derived($activeBuild);

  /** Thinking entries from the activity feed for the active build (P1-2). */
  let thinkingEntries = $derived(
    $activityFeed.filter(
      (e): e is { source: 'copilot'; event: import('$lib/types').CopilotActivityEvent } =>
        e.source === 'copilot' && (e as { source: 'copilot'; event: import('$lib/types').CopilotActivityEvent }).event.kind === 'thinking'
    )
  );

  function syncFromHash() {
    const { params } = matchRoute(window.location.hash.slice(1) || '/');
    if (params.buildId) currentBuildId.set(params.buildId);
    if (params.view && (VIEW_MODES as string[]).includes(params.view)) {
      viewMode = params.view as ViewMode;
    }
  }

  // Hydrate currentBuildId + viewMode when navigating directly to /builds/:id/:view
  onMount(syncFromHash);

  // Sync view on back/forward navigation
  $effect(() => {
    window.addEventListener('hashchange', syncFromHash);
    return () => window.removeEventListener('hashchange', syncFromHash);
  });

  const VIEW_TABS: { key: ViewMode; label: string; desc: string }[] = [
    { key: 'kanban',   label: 'KANBAN',   desc: 'LASDLC pillar board — findings sorted by severity per gate' },
    { key: 'list',     label: 'LIST',     desc: 'Flat phase list with status, confidence, and findings counts' },
    { key: 'operator', label: 'OPERATOR', desc: 'Live log stream, agent dispatch, and artifact panel' },
    { key: 'manifest', label: 'MANIFEST', desc: 'Raw YAML manifest — codename, tier, phase set, assumptions' },
    { key: 'plan',     label: 'PLAN',     desc: 'Full LASDLC plan document with phases, exit criteria, and deliverables' },
    { key: 'comms',    label: 'COMMS',    desc: 'Agent communication stream — messages, handoffs, and coordination events' },
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
            title={t.desc}
            onclick={() => navigate('/builds/:buildId/:view', { buildId: build.id, view: t.key })}
          >
            {t.label}
          </button>
        {/each}
      </div>
    </div>

    <!-- P1-2: Collapsible thinking entries from copilot activity feed -->
    {#if thinkingEntries.length > 0}
      <div class="thinking-feed">
        {#each thinkingEntries.slice(-5) as entry}
          <details class="thinking-entry">
            <summary class="thinking-summary">Reasoning · {entry.event.timestamp.slice(11, 19)}</summary>
            <pre class="thinking-content">{entry.event.summary ?? '—'}</pre>
          </details>
        {/each}
      </div>
    {/if}

    <div class="view-content" data-mode={viewMode}>
      {#if viewMode === 'kanban'}
        <KanbanView />
      {:else if viewMode === 'list'}
        <ListView />
      {:else if viewMode === 'operator'}
        <OperatorView />
      {:else if viewMode === 'manifest'}
        <ManifestView />
      {:else if viewMode === 'comms'}
        <CommsView />
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

  /* P1-2: Thinking feed */
  .thinking-feed {
    border-bottom: 1px solid var(--la-hair-faint);
    background: var(--la-bg-panel);
    padding: 2px 0;
    flex-shrink: 0;
  }
  .thinking-entry {
    border-bottom: 1px solid var(--la-hair-faint);
  }
  .thinking-entry:last-child {
    border-bottom: none;
  }
  .thinking-summary {
    font-size: 10px;
    color: var(--la-text-dim);
    padding: 2px 12px;
    cursor: pointer;
    user-select: none;
    font-family: var(--la-font-mono);
    letter-spacing: 0.04em;
  }
  .thinking-summary:hover {
    color: var(--la-text-base);
  }
  .thinking-content {
    font-size: 10px;
    color: var(--la-text-base);
    padding: 4px 16px 6px;
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--la-font-mono);
    max-height: 120px;
    overflow-y: auto;
    background: var(--la-bg-card);
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
