<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { navigate } from '$lib/routes';
  import { activityFeed } from '$lib/stores';
  import { api } from '$lib/api';
  import { CONTEXT_TIER_DEFAULTS } from '$lib/WavePipelineView.contract';
  import type { AyinSpanEvent } from '$lib/types';
  import EventStream from '$lib/../components/EventStream.svelte';
  import type { StreamRow } from '$lib/../components/EventStream.svelte';

  interface Props {
    buildId: string;
    phaseId: string;
    waveId: string;
    agentKey: string;
    taskId: string;
  }

  let { buildId, phaseId, waveId, agentKey, taskId }: Props = $props();

  // ── Historical traces (replayed on mount) ────────────────────────────────
  let historicalTraces = $state<AyinSpanEvent[]>([]);
  let tracesLoading = $state(true);
  let tracesError = $state<string | null>(null);

  async function loadTraces() {
    tracesLoading = true;
    tracesError = null;
    try {
      historicalTraces = await api.getAgentTraces(buildId, agentKey, { limit: 200 });
    } catch (e) {
      tracesError = e instanceof Error ? e.message : String(e);
    } finally {
      tracesLoading = false;
    }
  }

  onMount(() => { void loadTraces(); });

  // ── Live spans from SSE activity feed (filtered by agentKey) ─────────────
  let liveSpans = $derived(
    $activityFeed
      .filter((e): e is { source: 'ayin'; span: AyinSpanEvent } =>
        e.source === 'ayin' && e.span.actor === agentKey
      )
      .map(e => e.span)
      .slice(-200)
  );

  // Merge historical + live, deduplicate by id, sort newest-first.
  let allSpans = $derived.by(() => {
    const seen = new Set<string>();
    const merged: AyinSpanEvent[] = [];
    for (const s of [...liveSpans, ...historicalTraces]) {
      if (!seen.has(s.id)) { seen.add(s.id); merged.push(s); }
    }
    return merged.sort((a, b) =>
      new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
    );
  });

  // Whether there is at least one span with no duration (still running).
  let hasActiveSpan = $derived(allSpans.some(s => s.duration_ms === 0));

  // ── Convert spans → EventStream rows ─────────────────────────────────────
  let streamRows = $derived<StreamRow[]>(
    allSpans.map(s => ({
      ts:       new Date(s.timestamp).getTime(),
      time:     s.timestamp.slice(11, 19),
      source:   s.actor,
      color:    'var(--la-agent-researcher)',
      text:     `${s.action}${s.duration_ms > 0 ? ` (${s.duration_ms}ms)` : ' ●'}`,
      severity: s.duration_ms === 0 ? 'warn' : 'info',
    }))
  );

  // ── Keyboard: Esc → parent route ─────────────────────────────────────────
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      navigate('/builds/:buildId/phase/:phaseId/wave/:waveId/agent/:agentKey',
        { buildId, phaseId, waveId, agentKey });
    }
  }

  onMount(() => {
    window.addEventListener('keydown', handleKeydown);
  });
  onDestroy(() => {
    window.removeEventListener('keydown', handleKeydown);
  });
</script>

<!-- L3 Task Drill-Down View — Phase 5 -->
<div class="task-drill" role="region" aria-label="Task detail: {taskId}">

  <!-- Breadcrumb -->
  <nav class="drill-breadcrumb" aria-label="breadcrumb">
    <button class="crumb-seg" onclick={() => navigate('/builds', {})}>FOREST</button>
    <span class="crumb-sep" aria-hidden="true">›</span>
    <button class="crumb-seg"
      onclick={() => navigate('/builds/:buildId', { buildId })}
    >{buildId.slice(0, 10)}</button>
    <span class="crumb-sep" aria-hidden="true">›</span>
    <button class="crumb-seg"
      onclick={() => navigate('/builds/:buildId/phase/:phaseId', { buildId, phaseId })}
    >{phaseId}</button>
    <span class="crumb-sep" aria-hidden="true">›</span>
    <button class="crumb-seg"
      onclick={() => navigate('/builds/:buildId/phase/:phaseId/wave/:waveId', { buildId, phaseId, waveId })}
    >{waveId}</button>
    <span class="crumb-sep" aria-hidden="true">›</span>
    <button class="crumb-seg"
      onclick={() => navigate('/builds/:buildId/phase/:phaseId/wave/:waveId/agent/:agentKey', { buildId, phaseId, waveId, agentKey })}
    >{agentKey}</button>
    <span class="crumb-sep" aria-hidden="true">›</span>
    <span class="crumb-seg crumb-current" aria-current="page">{taskId}</span>
    <span class="crumb-esc" title="Press Esc to go up">[Esc]</span>
  </nav>

  <!-- Status header -->
  <div class="drill-header">
    <span class="task-label">TASK</span>
    <h2 class="task-name">{taskId}</h2>
    {#if hasActiveSpan}
      <span class="active-dot" aria-label="Agent actively working" title="Agent running">●</span>
    {/if}
  </div>

  <!-- Two-panel body -->
  <div class="drill-body">

    <!-- Left: Context tiers -->
    <section class="context-panel" aria-label="Context loaded">
      <h3 class="panel-heading">CONTEXT LOADED</h3>
      <ul class="tier-list">
        {#each Object.values(CONTEXT_TIER_DEFAULTS) as tier}
          <li class="tier-row">
            <span class="tier-icon" aria-hidden="true">{tier.icon}</span>
            <span class="tier-label">{tier.tier}</span>
            <span class="tier-desc">{tier.label}</span>
            <span class="tier-tokens">{tier.token_count.toLocaleString()}t</span>
          </li>
        {/each}
      </ul>
      {#if !tracesLoading && allSpans.length === 0}
        <p class="empty-hint">Context not loaded yet — agent hasn't started.</p>
      {/if}
    </section>

    <!-- Right: Activity log -->
    <section class="activity-panel" aria-label="Activity log">
      <h3 class="panel-heading">ACTIVITY LOG
        {#if tracesLoading}
          <span class="loading-dot" aria-label="Loading" aria-busy="true">…</span>
        {/if}
      </h3>

      {#if tracesError}
        <p class="error-hint" role="alert">Failed to load traces: {tracesError}</p>
      {:else if !tracesLoading && allSpans.length === 0}
        <p class="empty-hint" role="status">Agent hasn't started yet — no traces recorded.</p>
      {:else}
        <EventStream
          rows={streamRows}
          emptyMessage="Waiting for first span…"
          newestFirst={true}
        />
      {/if}
    </section>

  </div>
</div>

<style>
  .task-drill {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 0 1rem 1rem;
    overflow: hidden;
    font-family: var(--la-mono, monospace);
    background: var(--la-surface, #0b0f14);
    color: var(--la-text, #c9d1d9);
  }

  /* Breadcrumb */
  .drill-breadcrumb {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.5rem 0;
    font-size: 0.7rem;
    letter-spacing: 0.04em;
    color: var(--la-text-mute, #6e7681);
    flex-wrap: wrap;
  }

  .crumb-seg {
    background: none;
    border: none;
    padding: 0.1rem 0.25rem;
    color: inherit;
    cursor: pointer;
    font: inherit;
    border-radius: 2px;
  }

  .crumb-seg:hover {
    color: var(--la-text, #c9d1d9);
    background: var(--la-hover, rgba(255,255,255,0.06));
  }

  .crumb-current {
    color: var(--la-text, #c9d1d9);
    cursor: default;
  }

  .crumb-sep { user-select: none; }

  .crumb-esc {
    margin-left: auto;
    opacity: 0.4;
    font-size: 0.65rem;
  }

  /* Header */
  .drill-header {
    display: flex;
    align-items: baseline;
    gap: 0.75rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid var(--la-border, rgba(255,255,255,0.08));
  }

  .task-label {
    font-size: 0.65rem;
    letter-spacing: 0.1em;
    color: var(--la-text-mute, #6e7681);
  }

  .task-name {
    font-size: 0.9rem;
    font-weight: 600;
    margin: 0;
    color: var(--la-text-bright, #e6edf3);
  }

  .active-dot {
    font-size: 0.7rem;
    color: var(--la-agent-researcher, #17c3b2);
    animation: pulse-dot 1.2s ease-in-out infinite;
  }

  @keyframes pulse-dot {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.3; }
  }

  /* Body */
  .drill-body {
    display: grid;
    grid-template-columns: 280px 1fr;
    gap: 1rem;
    flex: 1;
    overflow: hidden;
    margin-top: 0.75rem;
  }

  .context-panel,
  .activity-panel {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--la-border, rgba(255,255,255,0.08));
    border-radius: 4px;
    padding: 0.75rem;
  }

  .panel-heading {
    font-size: 0.65rem;
    letter-spacing: 0.1em;
    color: var(--la-text-mute, #6e7681);
    margin: 0 0 0.75rem;
    text-transform: uppercase;
  }

  /* Tier list */
  .tier-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .tier-row {
    display: grid;
    grid-template-columns: 1.5rem 2rem 1fr auto;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.78rem;
  }

  .tier-icon {
    color: var(--la-agent-researcher, #17c3b2);
    font-size: 1rem;
  }

  .tier-label {
    font-weight: 600;
    color: var(--la-text-bright, #e6edf3);
  }

  .tier-desc {
    color: var(--la-text-mute, #6e7681);
  }

  .tier-tokens {
    font-variant-numeric: tabular-nums;
    color: var(--la-agent-performance, #f97316);
    font-size: 0.72rem;
  }

  /* Empty / error states */
  .empty-hint,
  .error-hint {
    font-size: 0.75rem;
    color: var(--la-text-mute, #6e7681);
    margin-top: 1rem;
    line-height: 1.5;
  }

  .error-hint { color: var(--la-agent-security, #ef4444); }

  .loading-dot {
    font-size: 0.7rem;
    opacity: 0.6;
    margin-left: 0.25rem;
  }

  /* Reduced-motion */
  @media (prefers-reduced-motion: reduce) {
    .active-dot { animation: none; }
  }

  @media (max-width: 1023px) {
    .drill-body { grid-template-columns: 1fr; }
    .context-panel { max-height: 200px; }
  }
</style>
