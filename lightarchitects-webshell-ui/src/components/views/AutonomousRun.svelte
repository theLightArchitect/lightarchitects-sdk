<!--
@component
Displays real-time autonomous-run orchestration for a given build: worker slot gauge,
conductor heartbeat, MergeAgent events, and FixAgent iteration feed.

Props:
- `buildId` — the active build's UUID; used to filter store events by build

Consumed stores: `workerSlots` (slot capacity + active count + wave index),
`conductorState` (queue depth + heartbeat), `mergeAgentEvents`, `fixAgentEvents`.
-->
<script lang="ts">
  import { workerSlots, conductorState, mergeAgentEvents, fixAgentEvents, implCompleteEvents } from '$lib/stores';
  import type { MergeAgentStatusEvent, FixAgentIterationEvent } from '$lib/types';
  import { authHeaders } from '$lib/auth';
  import HitlEscalationModal from '../HitlEscalationModal.svelte';
  import AttestationCard from '../AttestationCard.svelte';

  let { buildId }: { buildId: string } = $props();

  // ── Autonomous status polling ─────────────────────────────────────────────

  interface PendingHitlSummary {
    call_id:     string;
    task_id:     string;
    reason:      string;
    wave_index:  number;
    worker_slot: number;
    created_at:  string;
    build_id:    string;
  }

  let buildStatus    = $state<'running' | 'completed' | null>(null);
  let pendingHitl    = $state<PendingHitlSummary[]>([]);
  let cancelPending  = $state(false);
  let cancelError    = $state('');

  async function pollStatus() {
    try {
      const res = await fetch(`/api/builds/${buildId}/autonomous/status`, {
        headers: authHeaders(),
      });
      if (!res.ok) return;
      const data = await res.json() as { status: string; pending_hitl: Omit<PendingHitlSummary, 'build_id'>[] };
      buildStatus = data.status as 'running' | 'completed';
      pendingHitl = data.pending_hitl.map(it => ({ ...it, build_id: buildId }));
    } catch {
      // network error; keep last known state
    }
  }

  $effect(() => {
    void pollStatus();
    const interval = setInterval(() => { void pollStatus(); }, 8_000);
    return () => clearInterval(interval);
  });

  async function cancelBuild() {
    if (cancelPending) return;
    cancelPending = true;
    cancelError = '';
    try {
      const res = await fetch(`/api/builds/${buildId}/autonomous`, {
        method:  'DELETE',
        headers: authHeaders(),
      });
      if (!res.ok && res.status !== 204) {
        cancelError = `${res.status}`;
      } else {
        buildStatus = 'completed';
      }
    } catch (e) {
      cancelError = e instanceof Error ? e.message : 'cancel failed';
    } finally {
      cancelPending = false;
    }
  }

  const CAPACITY = 7;

  // ── Derived slot display ────────────────────────────────────────────────────

  let slots = $derived($workerSlots);
  let tick  = $derived($conductorState);
  let merges      = $derived($mergeAgentEvents.filter(e => e.build_id === buildId));
  let fixes       = $derived($fixAgentEvents.filter(e => e.build_id === buildId));
  let attestations = $derived($implCompleteEvents.filter(e => e.build_id === buildId));

  let activeSlots  = $derived(slots?.active     ?? 0);
  let capacity     = $derived(slots?.capacity   ?? CAPACITY);
  let queueDepth   = $derived(tick?.queue_depth ?? 0);
  let waveIndex    = $derived(slots?.wave_index ?? 0);

  let gatePassRate = $derived.by(() => {
    const total = merges.length;
    if (total === 0) return null;
    const passed = merges.filter(e => e.phase === 'merged').length;
    return Math.round((passed / total) * 100);
  });

  function mergePhaseClass(phase: string): string {
    if (phase === 'merged') return 'merge-merged';
    if (phase === 'failed') return 'merge-failed';
    if (phase === 'running') return 'merge-running';
    return 'merge-started';
  }

  function fixSeverityClass(iter: number): string {
    if (iter >= 3) return 'fix-deep';
    if (iter === 2) return 'fix-mid';
    return 'fix-shallow';
  }
</script>

<HitlEscalationModal {buildId} initialItems={pendingHitl} />

<div class="autonomous-run" data-testid="autonomous-run" data-build-id={buildId}>
  <!-- ── Header ─────────────────────────────────────────────────────────────── -->
  <div class="ar-header">
    <span class="ar-label">AUTONOMOUS RUN</span>
    {#if buildStatus}
      <span class="ar-status" class:ar-status-done={buildStatus === 'completed'}>{buildStatus}</span>
    {/if}
    {#if tick}
      <span class="ar-seq">tick #{tick.tick_seq}</span>
    {:else}
      <span class="ar-idle">waiting for conductor…</span>
    {/if}
    {#if pendingHitl.length > 0}
      <span class="ar-hitl-badge">{pendingHitl.length} HITL</span>
    {/if}
    {#if buildStatus === 'running'}
      <button
        class="ar-cancel-btn"
        disabled={cancelPending}
        onclick={cancelBuild}
        aria-label="Cancel autonomous build"
      >
        {cancelPending ? '…' : 'CANCEL'}
      </button>
    {/if}
    {#if cancelError}
      <span class="ar-cancel-error">{cancelError}</span>
    {/if}
  </div>

  <!-- ── Wave / Queue summary ────────────────────────────────────────────────── -->
  <div class="ar-summary-row">
    <div class="ar-stat">
      <span class="ar-stat-val">{waveIndex}</span>
      <span class="ar-stat-label">WAVE</span>
    </div>
    <div class="ar-stat">
      <span class="ar-stat-val">{activeSlots}/{capacity}</span>
      <span class="ar-stat-label">SLOTS</span>
    </div>
    <div class="ar-stat">
      <span class="ar-stat-val">{queueDepth}</span>
      <span class="ar-stat-label">QUEUED</span>
    </div>
    {#if gatePassRate !== null}
      <div class="ar-stat">
        <span class="ar-stat-val" class:ar-pass={gatePassRate >= 80} class:ar-fail={gatePassRate < 60}>{gatePassRate}%</span>
        <span class="ar-stat-label">GATE PASS</span>
      </div>
    {/if}
  </div>

  <!-- ── 7-slot occupancy bar ─────────────────────────────────────────────────── -->
  <div class="ar-section">
    <span class="ar-section-label">WORKER SLOTS</span>
    <div class="slot-bar" role="meter" aria-label="Worker slot occupancy" aria-valuenow={activeSlots} aria-valuemax={capacity}>
      {#each { length: capacity } as _, i}
        <div
          class="slot"
          class:slot-active={i < activeSlots}
          aria-label="Slot {i + 1}: {i < activeSlots ? 'active' : 'idle'}"
        ></div>
      {/each}
    </div>
    <span class="ar-slot-label">{activeSlots} of {capacity} active</span>
  </div>

  <!-- ── Merge agent events ────────────────────────────────────────────────────── -->
  {#if merges.length > 0}
    <div class="ar-section">
      <span class="ar-section-label">MERGE AGENT</span>
      <ul class="ar-event-list">
        {#each merges.slice(0, 8) as ev (ev.wave_index + ev.phase)}
          <li class="ar-event-item {mergePhaseClass(ev.phase)}">
            <span class="ev-wave">W{ev.wave_index}</span>
            <span class="ev-phase">{ev.phase.toUpperCase()}</span>
            {#if ev.commit_sha}
              <span class="ev-sha">{ev.commit_sha.slice(0, 7)}</span>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  <!-- ── Fix agent iterations ──────────────────────────────────────────────────── -->
  {#if fixes.length > 0}
    <div class="ar-section">
      <span class="ar-section-label">FIX AGENT</span>
      <ul class="ar-event-list">
        {#each fixes.slice(0, 6) as ev (ev.wave_index + '-' + ev.worker_slot + '-' + ev.iteration)}
          <li class="ar-event-item {fixSeverityClass(ev.iteration)}">
            <span class="ev-wave">W{ev.wave_index}/S{ev.worker_slot}</span>
            <span class="ev-iter">iter {ev.iteration}</span>
            <span class="ev-issue">{ev.issue_summary.slice(0, 60)}</span>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  <!-- ── IMPLEMENTATION_COMPLETE attestations (Agents Playbook §3.5) ──────── -->
  {#if attestations.length > 0}
    <div class="ar-section">
      <span class="ar-section-label">ATTESTATIONS</span>
      <ul class="ar-event-list">
        {#each attestations.slice(0, 6) as ev (ev.wave + '-' + ev.task_id)}
          <li class="ar-event-item-bare">
            <AttestationCard {ev} />
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  <!-- ── Empty state ──────────────────────────────────────────────────────────── -->
  {#if !slots && !tick && merges.length === 0 && attestations.length === 0}
    <div class="ar-empty">
      <span>No autonomous run in progress for this build.</span>
      <span class="ar-empty-hint">Start a build in autonomous mode to see live worker activity.</span>
    </div>
  {/if}
</div>

<style>
  .autonomous-run {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px;
    height: 100%;
    overflow-y: auto;
  }

  .ar-header {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .ar-label {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-label);
  }

  .ar-seq {
    font-size: 10px;
    color: var(--la-focus-ring);
    font-variant-numeric: tabular-nums;
  }

  .ar-idle {
    font-size: 10px;
    color: var(--la-text-dim);
    font-style: italic;
  }

  .ar-status {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    padding: 1px 6px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--la-focus-ring) 18%, transparent);
    color: var(--la-focus-ring);
    border: 1px solid color-mix(in srgb, var(--la-focus-ring) 35%, transparent);
    text-transform: uppercase;
  }

  .ar-status.ar-status-done {
    background: color-mix(in srgb, var(--la-strand-sec) 14%, transparent);
    color: var(--la-strand-sec);
    border-color: color-mix(in srgb, var(--la-strand-sec) 30%, transparent);
  }

  .ar-hitl-badge {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 1px 6px;
    border-radius: 2px;
    background: var(--la-semantic-warn);
    color: var(--la-bg-base);
    animation: pulse-warn 2s ease-in-out infinite;
  }

  @keyframes pulse-warn {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.7; }
  }

  .ar-cancel-btn {
    margin-left: auto;
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    padding: 2px 10px;
    border: 1px solid var(--la-semantic-error);
    border-radius: 2px;
    color: var(--la-semantic-error);
    background: transparent;
    cursor: pointer;
    transition: background 150ms;
  }

  .ar-cancel-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--la-semantic-error) 12%, transparent);
  }

  .ar-cancel-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .ar-cancel-error {
    font-size: 9px;
    color: var(--la-semantic-error);
  }

  .ar-summary-row {
    display: flex;
    gap: 20px;
    flex-wrap: wrap;
  }

  .ar-stat {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
  }

  .ar-stat-val {
    font-size: 20px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: var(--la-text-bright);
    line-height: 1;
  }

  .ar-stat-val.ar-pass { color: var(--la-strand-sec); }
  .ar-stat-val.ar-fail { color: var(--la-strand-sec-alt, #e55); }

  .ar-stat-label {
    font-size: 9px;
    letter-spacing: 0.1em;
    color: var(--la-text-dim);
  }

  .ar-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .ar-section-label {
    font-size: 9px;
    letter-spacing: 0.1em;
    color: var(--la-text-label);
    font-weight: 600;
  }

  .slot-bar {
    display: flex;
    gap: 4px;
  }

  .slot {
    width: 28px;
    height: 14px;
    border-radius: 2px;
    background: var(--la-bg-elev-2);
    border: 1px solid var(--la-hair-strong);
    transition: background 0.2s, border-color 0.2s;
  }

  .slot.slot-active {
    background: var(--la-focus-ring);
    border-color: var(--la-focus-ring);
  }

  .ar-slot-label {
    font-size: 10px;
    color: var(--la-text-dim);
  }

  .ar-event-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .ar-event-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 6px;
    border-radius: 3px;
    font-size: 10px;
    background: var(--la-bg-elev-1);
    border-left: 3px solid var(--la-hair-strong);
  }

  /* Wrapper for AttestationCard — no extra padding; card owns its own layout. */
  .ar-event-item-bare {
    list-style: none;
  }

  .ar-event-item.merge-merged  { border-left-color: var(--la-strand-sec); }
  .ar-event-item.merge-failed  { border-left-color: #e55; }
  .ar-event-item.merge-running { border-left-color: var(--la-focus-ring); }
  .ar-event-item.merge-started { border-left-color: var(--la-text-dim); }

  .ar-event-item.fix-deep    { border-left-color: #e55; }
  .ar-event-item.fix-mid     { border-left-color: #e90; }
  .ar-event-item.fix-shallow { border-left-color: var(--la-text-dim); }

  .ev-wave  { font-weight: 600; color: var(--la-text-label); min-width: 28px; }
  .ev-phase { font-weight: 500; color: var(--la-text-bright); }
  .ev-iter  { color: var(--la-text-label); }
  .ev-sha   { font-family: monospace; color: var(--la-text-dim); }
  .ev-issue { color: var(--la-text-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .ar-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 32px 16px;
    text-align: center;
    color: var(--la-text-dim);
    font-size: 11px;
  }

  .ar-empty-hint {
    font-size: 10px;
    color: var(--la-text-dim);
    opacity: 0.7;
  }
</style>
