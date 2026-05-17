<script lang="ts">
  import { onMount } from 'svelte';
  import { activeBuild, currentBuildId, activityFeed, ensureBuildInStore } from '$lib/stores';
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

  // helix-viz-remap: Turn-zoom deep-link params (P6 check 7)
  let phaseId  = $state<string | null>(null);
  let waveId   = $state<string | null>(null);
  let agentKey = $state<string | null>(null);
  // 'turn' when all three deep-link params are present, 'build' otherwise
  let zoomLevel = $derived<'build' | 'turn'>(
    phaseId !== null && waveId !== null && agentKey !== null ? 'turn' : 'build'
  );

  /** Thinking entries from the activity feed for the active build (P1-2). */
  let thinkingEntries = $derived(
    $activityFeed.filter(
      (e): e is { source: 'copilot'; event: import('$lib/types').CopilotActivityEvent } =>
        e.source === 'copilot' && (e as { source: 'copilot'; event: import('$lib/types').CopilotActivityEvent }).event.kind === 'thinking'
    )
  );

  function syncFromHash() {
    const { params } = matchRoute(window.location.hash.slice(1) || '/');
    if (params.buildId) {
      currentBuildId.set(params.buildId);
      // Session builds (POST /api/builds) are not in the portfolio snapshot.
      // Fetch and insert so activeBuild resolves and AgentConsole can render.
      ensureBuildInStore(params.buildId);
    }
    if (params.view && (VIEW_MODES as string[]).includes(params.view)) {
      viewMode = params.view as ViewMode;
    }
    // helix-viz-remap: sync Turn-zoom deep-link params
    phaseId  = params.phaseId  ?? null;
    waveId   = params.waveId   ?? null;
    agentKey = params.agentKey ?? null;
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
    <!-- ZoomBreadcrumb + view-mode tab bar (helix-viz-remap P6 check 7) -->
    <div class="view-tab-bar">
      <nav aria-label="breadcrumb" class="zoom-breadcrumb">
        <button
          class="crumb-seg"
          onclick={() => navigate('/builds', {})}
        >PORTFOLIO</button>
        <span class="crumb-sep" aria-hidden="true">›</span>
        {#if zoomLevel === 'turn'}
          <button
            class="crumb-seg"
            onclick={() => navigate('/builds/:buildId', { buildId: build.id })}
          >{build.name}</button>
          <span class="crumb-sep" aria-hidden="true">›</span>
          <span class="crumb-seg crumb-current" aria-current="page">{agentKey}</span>
        {:else}
          <span class="crumb-seg crumb-current" aria-current="page">{build.name}</span>
          <span class="crumb-id">{build.id.slice(0, 8)}</span>
        {/if}
      </nav>
      {#if zoomLevel === 'build'}
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
      {/if}
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

    <!-- helix-viz-remap: Turn zoom replaces tab content (P6 check 7) -->
    <!-- Use {#if} (not CSS visibility) so child SSE connections close on nav -->
    {#if zoomLevel === 'turn'}
      <div class="turn-panel">
        <dl class="turn-meta">
          <dt class="turn-label">AGENT</dt>
          <dd class="turn-value">{agentKey}</dd>
          <dt class="turn-label">PHASE</dt>
          <dd class="turn-value">{phaseId}</dd>
          <dt class="turn-label">WAVE</dt>
          <dd class="turn-value">{waveId}</dd>
        </dl>
      </div>
    {:else}
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
        <!-- SUPERVISOR_PANEL_SLOT -->
      </div>
    {/if}
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

  /* helix-viz-remap: ZoomBreadcrumb (replaces build-crumb) */
  .zoom-breadcrumb {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 12px;
    border-right: 1px solid var(--la-hair-faint);
    height: 36px;
    flex-shrink: 0;
  }

  .crumb-seg {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: var(--la-text-dim);
    background: transparent;
    border: none;
    padding: 0 2px;
    cursor: pointer;
    font-family: inherit;
    white-space: nowrap;
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .crumb-seg:hover { color: var(--la-text-base); }
  .crumb-seg:not(button) { cursor: default; }

  .crumb-current {
    color: var(--la-text-base);
    cursor: default;
  }

  .crumb-sep {
    font-size: 10px;
    color: var(--la-text-mute);
    user-select: none;
    flex-shrink: 0;
  }

  /* keep .crumb-id for the id badge at build zoom */
  .crumb-id {
    font-size: 9px;
    color: var(--la-text-mute);
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.04em;
  }

  /* .build-crumb kept for backwards-compat (unused in template, CSS only) */
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

  /* helix-viz-remap: Turn zoom panel (P6 check 7) */
  .turn-panel {
    flex: 1;
    overflow: auto;
    padding: 24px;
    background: var(--la-bg-base);
    min-height: 0;
  }

  .turn-meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 16px;
    margin: 0;
  }

  .turn-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.14em;
    color: var(--la-text-mute);
    text-transform: uppercase;
    padding-top: 2px;
  }

  .turn-value {
    font-size: 12px;
    color: var(--la-text-base);
    font-family: var(--la-font-mono);
    letter-spacing: 0.03em;
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
