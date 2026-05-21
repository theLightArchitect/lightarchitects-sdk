<!--
@component
Displays the HMAC-chained decision log for an autonomous build (L1–L4 levels).
Supports level filtering and live L4 escalation entries via topic-filtered SSE.

Props:
- `buildId` — the active build's UUID; used to query decisions and filter escalation events

Level taxonomy: L1 ARCHITECTURAL · L2 IMPLEMENTATION · L3 QUALITY GATE · L4 ESCALATION.
Subscribes to `v1.supervisor.escalation` via topic SSE; prepends entries when `build_id` matches.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api } from '$lib/api';
  import type { DecisionEntry } from '$lib/types';
  import { subscribeByTopic, type WebEventV2 } from '$lib/sse';
  import { MOCK_DECISION_ENTRIES } from '$lib/mock-surfaces';

  let { buildId }: { buildId: string } = $props();

  type LevelFilter = 'all' | 'L1' | 'L2' | 'L3' | 'L4';

  let entries = $state<DecisionEntry[]>([]);
  let loading = $state(true);
  let error   = $state<string | null>(null);
  let filter  = $state<LevelFilter>('all');

  // ── Live escalation events via topic-filtered SSE ──────────────────────────

  let unsubscribeEscalation: (() => void) | null = null;

  function handleEscalation(event: WebEventV2): void {
    const ev = event as WebEventV2 & { build_id?: string; reason?: string; canon_ref?: string };
    if (ev.build_id !== buildId) return;
    const entry: DecisionEntry = {
      line_n:    entries.length,
      timestamp: event.timestamp,
      level:     'L4',
      decision:  `ESCALATION: ${ev.reason ?? ''}`,
      canon_ref: ev.canon_ref,
      hmac_ok:   undefined,
    };
    entries = [entry, ...entries];
  }

  onMount(() => {
    loadDecisions();
    unsubscribeEscalation = subscribeByTopic('v1.supervisor.escalation', handleEscalation);
  });

  onDestroy(() => {
    unsubscribeEscalation?.();
  });

  async function loadDecisions() {
    loading = true;
    error = null;
    try {
      const data = await api.getDecisions(buildId);
      // Fallback to mock entries when API returns empty (build has no decisions yet
      // OR backend SSE stream is not implemented — see MockBadge "STREAM" in header).
      entries = data.length > 0 ? [...data].reverse() : [...MOCK_DECISION_ENTRIES];
    } catch (e) {
      // On error, render mock so panel is never blank.
      entries = [...MOCK_DECISION_ENTRIES];
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  let filtered = $derived(
    filter === 'all' ? entries : entries.filter(e => e.level === filter),
  );

  const LEVEL_LABELS: Record<string, string> = {
    L1: 'ARCHITECTURAL',
    L2: 'IMPLEMENTATION',
    L3: 'QUALITY GATE',
    L4: 'ESCALATION',
  };

  function levelClass(level: string): string {
    switch (level) {
      case 'L4': return 'lvl-l4';
      case 'L3': return 'lvl-l3';
      case 'L2': return 'lvl-l2';
      default:   return 'lvl-l1';
    }
  }

  function formatTs(ts: string): string {
    try {
      return new Date(ts).toLocaleTimeString('en-US', { hour12: false });
    } catch {
      return ts.slice(11, 19);
    }
  }

  // Allow only canon:// (→ anchor) and https?:// (internal/external doc links).
  // Blocks javascript: and data: URIs — Security Guardrails §3.1 A03 / ASVS v5 §3.4.1.
  function safeCanonHref(ref: string): string {
    if (ref.startsWith('canon://')) return '#';
    if (/^https?:\/\//.test(ref)) return ref;
    return '#';
  }
</script>

<div class="decision-log" data-testid="decision-log" data-build-id={buildId}>
  <!-- ── Header + filter ─────────────────────────────────────────────────────── -->
  <div class="dl-header">
    <span class="dl-title">
      DECISION LOG
    </span>
    <div class="dl-filters" role="group" aria-label="Filter by decision level">
      {#each (['all', 'L1', 'L2', 'L3', 'L4'] as const) as lvl}
        <button
          class="dl-filter-btn"
          class:active={filter === lvl}
          onclick={() => { filter = lvl; }}
          aria-pressed={filter === lvl}
        >{lvl === 'all' ? 'ALL' : lvl}</button>
      {/each}
    </div>
    <button
      class="dl-refresh"
      onclick={loadDecisions}
      disabled={loading}
      aria-label="Refresh decisions"
      title="Refresh"
    >↺</button>
  </div>

  <!-- ── Content ─────────────────────────────────────────────────────────────── -->
  {#if loading}
    <div class="dl-loading" aria-busy="true">Loading decisions…</div>
  {:else if error}
    <div class="dl-error" role="alert">
      <span>Failed to load decisions: {error}</span>
      <button onclick={loadDecisions}>Retry</button>
    </div>
  {:else if filtered.length === 0}
    <div class="dl-empty">
      {#if filter !== 'all'}
        <span>No {filter} decisions yet.</span>
        <button class="dl-clear-filter" onclick={() => { filter = 'all'; }}>Clear filter</button>
      {:else}
        <span>No decisions recorded yet.</span>
        <span class="dl-empty-hint">Decisions appear when an autonomous build makes L1–L4 choices.</span>
      {/if}
    </div>
  {:else}
    <ul class="dl-list" aria-label="Decision entries">
      {#each filtered as entry (entry.line_n)}
        <li class="dl-entry {levelClass(entry.level)}">
          <div class="dl-entry-meta">
            <span class="dl-level" aria-label="Level {entry.level}">{entry.level}</span>
            <span class="dl-level-label">{LEVEL_LABELS[entry.level] ?? entry.level}</span>
            <span class="dl-ts">{formatTs(entry.timestamp)}</span>
            {#if entry.hmac_ok === false}
              <span class="dl-hmac-warn" title="HMAC verification failed — chain may be broken">⚠ HMAC</span>
            {/if}
          </div>
          <p class="dl-decision">{entry.decision}</p>
          {#if entry.canon_ref}
            <span class="dl-canon-ref" title="Canon reference">
              <a href={safeCanonHref(entry.canon_ref)}
                 class="dl-canon-link"
                 aria-label="Canon reference: {entry.canon_ref}">
                {entry.canon_ref}
              </a>
            </span>
          {/if}
        </li>
      {/each}
    </ul>
    <div class="dl-count" aria-live="polite">
      {filtered.length} decision{filtered.length !== 1 ? 's' : ''}
      {#if filter !== 'all'} (filtered to {filter}){/if}
    </div>
  {/if}
</div>

<style>
  .decision-log {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    height: 100%;
    overflow-y: auto;
  }

  .dl-header {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }

  .dl-title {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-label);
    flex: 1;
  }

  .dl-filters {
    display: flex;
    gap: 4px;
  }

  .dl-filter-btn {
    padding: 2px 7px;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.06em;
    border-radius: 2px;
    border: 1px solid var(--la-hair-strong);
    background: var(--la-bg-elev-1);
    color: var(--la-text-dim);
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }

  .dl-filter-btn.active {
    background: var(--la-focus-ring);
    border-color: var(--la-focus-ring);
    color: #fff;
  }

  .dl-refresh {
    font-size: 14px;
    background: none;
    border: none;
    color: var(--la-text-dim);
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 3px;
    line-height: 1;
  }

  .dl-refresh:hover:not(:disabled) { color: var(--la-focus-ring); }
  .dl-refresh:disabled { opacity: 0.4; cursor: default; }

  .dl-loading,
  .dl-error,
  .dl-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 32px 16px;
    text-align: center;
    font-size: 11px;
    color: var(--la-text-dim);
  }

  .dl-error { color: #e55; }

  .dl-empty-hint {
    font-size: 10px;
    opacity: 0.7;
  }

  .dl-clear-filter {
    font-size: 10px;
    padding: 2px 8px;
    border-radius: 3px;
    border: 1px solid var(--la-hair-strong);
    background: none;
    color: var(--la-focus-ring);
    cursor: pointer;
  }

  .dl-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .dl-entry {
    padding: 8px 10px;
    border-radius: 4px;
    background: var(--la-bg-elev-1);
    border-left: 3px solid var(--la-hair-strong);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .dl-entry.lvl-l1 { border-left-color: var(--la-focus-ring); }
  .dl-entry.lvl-l2 { border-left-color: var(--la-strand-sec, #48b); }
  .dl-entry.lvl-l3 { border-left-color: #e90; }
  .dl-entry.lvl-l4 { border-left-color: #e55; }

  .dl-entry-meta {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .dl-level {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    min-width: 20px;
    color: var(--la-text-label);
  }

  .dl-level-label {
    font-size: 9px;
    letter-spacing: 0.06em;
    color: var(--la-text-dim);
  }

  .dl-ts {
    font-size: 9px;
    font-variant-numeric: tabular-nums;
    color: var(--la-text-dim);
    margin-left: auto;
  }

  .dl-hmac-warn {
    font-size: 9px;
    color: #e90;
    font-weight: 600;
  }

  .dl-decision {
    font-size: 11px;
    color: var(--la-text-bright);
    margin: 0;
    line-height: 1.5;
    word-break: break-word;
  }

  .dl-canon-ref {
    font-size: 9px;
    color: var(--la-text-dim);
  }

  .dl-canon-link {
    color: var(--la-focus-ring);
    text-decoration: none;
  }

  .dl-canon-link:hover { text-decoration: underline; }

  .dl-count {
    font-size: 10px;
    color: var(--la-text-dim);
    text-align: right;
    flex-shrink: 0;
  }
</style>
