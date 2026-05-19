<script lang="ts">
  import { onMount } from 'svelte';
  import { activeBuild, currentBuildId, activityFeed, ensureBuildInStore } from '$lib/stores';
  import { matchRoute, navigate } from '$lib/routes';
  import { api } from '$lib/api';
  import KanbanView  from '$lib/../components/views/KanbanView.svelte';
  import ListView    from '$lib/../components/views/ListView.svelte';
  import OperatorView from '$lib/../components/views/OperatorView.svelte';
  import ManifestView from '$lib/../components/views/ManifestView.svelte';
  import PlanView    from '$lib/../components/PlanView.svelte';
  import CommsView   from '$lib/../components/views/CommsView.svelte';
  import TaskDrillView from '$lib/../components/views/TaskDrillView.svelte';
  import WavePipelineView from '$lib/../components/views/WavePipelineView.svelte';
  import ProposalCard from '$lib/../components/ProposalCard.svelte';
  import type { Phase } from '$lib/WavePipelineView.contract';
  import type { SupervisorState } from '$lib/types';

  // 'pipeline' is the FOLD-4 hook for ironclaw-spine (6th view mode).
  type ViewMode = 'kanban' | 'list' | 'operator' | 'manifest' | 'plan' | 'comms' | 'pipeline';

  const VIEW_MODES: ViewMode[] = ['kanban', 'list', 'operator', 'manifest', 'plan', 'comms', 'pipeline'];

  let viewMode = $state<ViewMode>('kanban');
  let build = $derived($activeBuild);

  // helix-viz-remap: Turn-zoom deep-link params (P6 check 7)
  let phaseId  = $state<string | null>(null);
  let waveId   = $state<string | null>(null);
  let agentKey = $state<string | null>(null);
  // Phase 5: L3 task drill-down
  let taskId   = $state<string | null>(null);
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
    // Phase 5: L3 task drill-down
    taskId   = params.taskId   ?? null;
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
    { key: 'pipeline', label: 'PIPELINE', desc: 'Wave pipeline — phase/wave/task timeline with gate verdicts' },
  ];

  // Stub phases for WavePipelineView — populated from manifest in Phase 7.
  const STUB_PHASES: Phase[] = [];

  // ── Supervisor state ──────────────────────────────────────────────────────
  let supervisorState = $state<SupervisorState | null>(null);

  async function fetchSupervisorState(buildId: string) {
    try {
      supervisorState = await api.getSupervisorState(buildId);
    } catch (e: unknown) {
      // 404 = no northstar captured for this build yet — silent
      if (e instanceof Error && e.message.includes('404')) return;
      // other errors: leave prior state intact, don't crash the panel
    }
  }

  async function handleAcknowledge() {
    const id = build?.id;
    if (!id) return;
    // Optimistic clear so the card disappears immediately
    if (supervisorState) supervisorState = { ...supervisorState, proposal_pending: false };
    try {
      await api.acknowledgeProposal(id);
      // SSE will broadcast the confirmed state; re-fetch for drift counter
      await fetchSupervisorState(id);
    } catch {
      // Re-fetch to restore real state if the POST failed
      await fetchSupervisorState(id);
    }
  }

  // Open SSE on build mount; re-fetch full state on every evaluation event.
  // Uses {#if} (not CSS hide) per api.ts comment — onDestroy fires on unmount.
  $effect(() => {
    const id = build?.id;
    if (!id) return;
    fetchSupervisorState(id);
    const es = api.supervisorEvents(id, () => { fetchSupervisorState(id); });
    return () => es.close();
  });
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

    <!-- Supervisor northstar strip — only when northstar has been captured -->
    {#if supervisorState?.northstar_text}
      <div
        class="supervisor-strip"
        class:sv-drifting={supervisorState.consecutive_drifts > 0}
        class:sv-advancing={supervisorState.last_evaluation?.status === 'advancing'}
      >
        <span class="sv-label">NORTHSTAR</span>
        <span class="sv-text" title={supervisorState.northstar_text}>
          {supervisorState.northstar_text}
        </span>
        {#if supervisorState.consecutive_drifts > 0}
          <span class="sv-badge sv-badge--drift" aria-label="Consecutive drift count">
            DRIFT {supervisorState.consecutive_drifts}/{supervisorState.drift_threshold}
          </span>
        {:else if supervisorState.last_evaluation?.status === 'advancing'}
          <span class="sv-badge sv-badge--ok">ADVANCING</span>
        {/if}
      </div>

      <!-- ProposalCard — mounted only while proposal is pending (SSE cleans up) -->
      {#if supervisorState.proposal_pending && supervisorState.last_evaluation}
        <div class="proposal-wrap" role="status">
          <ProposalCard
            evaluation={supervisorState.last_evaluation}
            onAcknowledge={handleAcknowledge}
          />
        </div>
      {/if}
    {/if}

    <!-- Phase 5: L3 task drill-down — renders instead of turn/build content -->
    <!-- Use {#if} (not visibility) so EventStream SSE cleans up on navigation -->
    {#if taskId && phaseId && waveId && agentKey}
      <TaskDrillView
        buildId={build.id}
        phaseId={phaseId}
        waveId={waveId}
        agentKey={agentKey}
        taskId={taskId}
      />
    <!-- helix-viz-remap: Turn zoom replaces tab content (P6 check 7) -->
    {:else if zoomLevel === 'turn'}
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
    <!-- Phase 6: phaseId drill — split-pane (left: phase info, right: WavePipelineView) -->
    {:else if phaseId && !waveId && !agentKey}
      <div class="phase-split">
        <div class="phase-split-left">
          <span class="phase-split-label">PHASE</span>
          <span class="phase-split-id">{phaseId}</span>
          <span class="phase-split-build">{build.name}</span>
        </div>
        <div class="phase-split-right">
          <WavePipelineView
            mode="split"
            phases={STUB_PHASES}
            onTaskClick={(taskId) => navigate('/builds/:buildId/phase/:phaseId/wave/stub/agent/stub/task/:taskId', { buildId: build.id, phaseId: phaseId ?? '', taskId })}
            onGateClick={(pid, wid) => navigate('/builds/:buildId/phase/:phaseId', { buildId: build.id, phaseId: pid })}
          />
        </div>
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
        {:else if viewMode === 'pipeline'}
          <WavePipelineView mode="full" phases={STUB_PHASES} />
        {:else}
          <PlanView />
        {/if}
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

  /* Phase 6: phaseId split-pane */
  .phase-split {
    flex: 1;
    display: grid;
    grid-template-columns: 200px 1fr;
    overflow: hidden;
    min-height: 0;
  }

  .phase-split-left {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 16px 12px;
    border-right: 1px solid var(--la-hair-faint);
    background: var(--la-bg-panel);
    font-family: var(--la-font-mono);
  }

  .phase-split-label {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--la-text-mute);
    text-transform: uppercase;
  }

  .phase-split-id {
    font-size: 11px;
    color: var(--la-text-stark);
    font-weight: 600;
    letter-spacing: 0.02em;
    word-break: break-all;
  }

  .phase-split-build {
    font-size: 9px;
    color: var(--la-text-dim);
    letter-spacing: 0.04em;
    margin-top: 2px;
  }

  .phase-split-right {
    overflow: hidden;
    display: flex;
    flex-direction: column;
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

  /* ── Supervisor northstar strip ─────────────────────────────────────── */
  .supervisor-strip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 14px;
    height: 28px;
    flex-shrink: 0;
    border-bottom: 1px solid var(--la-hair-faint);
    background: var(--la-bg-panel);
    font-family: var(--la-font-mono);
    overflow: hidden;
    transition: background 200ms;
  }
  .sv-drifting {
    background: rgba(248, 113, 113, 0.05);
    border-bottom-color: rgba(248, 113, 113, 0.25);
  }
  .sv-advancing {
    background: rgba(34, 197, 94, 0.04);
    border-bottom-color: rgba(34, 197, 94, 0.2);
  }

  .sv-label {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--la-text-mute);
    flex-shrink: 0;
  }
  .sv-text {
    font-size: 9px;
    color: var(--la-text-dim);
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    letter-spacing: 0.01em;
  }

  .sv-badge {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    padding: 1px 6px;
    border-radius: 2px;
    flex-shrink: 0;
  }
  .sv-badge--drift {
    color: #f87171;
    background: rgba(248, 113, 113, 0.12);
    border: 1px solid rgba(248, 113, 113, 0.3);
  }
  .sv-badge--ok {
    color: #22c55e;
    background: rgba(34, 197, 94, 0.1);
    border: 1px solid rgba(34, 197, 94, 0.25);
  }

  .proposal-wrap {
    padding: 10px 14px;
    flex-shrink: 0;
    border-bottom: 1px solid var(--la-hair-faint);
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
