<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { authHeaders } from '$lib/auth';
  import { a2aFeedStore } from '$lib/a2aFeed';
  import A2aFeedPanel from '$lib/../components/A2aFeedPanel.svelte';
  import type { IronclawHitlEscalationEvent } from '$lib/types';
  import { autoModeActive } from '$lib/stores';

  // ── Program manifest types ────────────────────────────────────────────────

  interface ProgramStatus {
    codenames: string[];
    current: string | null;
    state: 'idle' | 'running' | 'completed' | 'cancelled';
  }

  // ── State ─────────────────────────────────────────────────────────────────

  let status  = $state<ProgramStatus | null>(null);
  let loading = $state(true);
  let error   = $state<string | null>(null);

  // Codename filter for the feed panel — null = show all
  let selectedCodename = $state<string | null>(null);

  // Feed event counts per codename (derived from store)
  let eventCounts = $derived.by(() => {
    const map = $a2aFeedStore;
    const counts: Record<string, number> = {};
    for (const [cn, evs] of map) counts[cn] = evs.length;
    return counts;
  });

  // ── Pending HITL escalations (keyed by nonce) ─────────────────────────────

  let pendingEscalations = $state<Map<string, IronclawHitlEscalationEvent>>(new Map());
  let pendingCount = $derived(pendingEscalations.size);

  // ── Data fetch ────────────────────────────────────────────────────────────

  async function fetchStatus() {
    try {
      loading = true;
      error = null;
      const r = await fetch('/api/program/status', { headers: authHeaders() });
      if (r.ok) {
        status = await r.json() as ProgramStatus;
      } else if (r.status === 404) {
        status = { codenames: [], current: null, state: 'idle' };
      } else {
        error = `HTTP ${r.status}`;
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  // Auto-refresh status when feed events arrive (program state may have advanced)
  let lastFeedSize = 0;
  $effect(() => {
    let total = 0;
    for (const evs of $a2aFeedStore.values()) total += evs.length;
    if (total !== lastFeedSize) {
      lastFeedSize = total;
      // Only re-fetch when program is running (avoid spurious calls when idle)
      if (status?.state === 'running') fetchStatus();
    }
  });

  // ── HITL window event listeners ────────────────────────────────────────────

  function onEscalation(e: Event) {
    const detail = (e as CustomEvent).detail as IronclawHitlEscalationEvent;
    pendingEscalations = new Map(pendingEscalations).set(detail.nonce, detail);
  }

  function onResolution(e: Event) {
    const detail = (e as CustomEvent).detail as { nonce: string };
    const m = new Map(pendingEscalations);
    m.delete(detail.nonce);
    pendingEscalations = m;
  }

  onMount(() => {
    fetchStatus();
    window.addEventListener('la:ironclaw_hitl_escalation', onEscalation);
    window.addEventListener('la:ironclaw_hitl_resolution', onResolution);
  });

  onDestroy(() => {
    window.removeEventListener('la:ironclaw_hitl_escalation', onEscalation);
    window.removeEventListener('la:ironclaw_hitl_resolution', onResolution);
  });

  // ── Start controls ────────────────────────────────────────────────────────

  let startPending = $state(false);
  let startError   = $state<string | null>(null);
  let codenameInput = $state('');

  async function startProgram() {
    const codenames = codenameInput
      .split(',')
      .map(s => s.trim())
      .filter(Boolean);
    if (codenames.length === 0) { startError = 'Enter at least one codename'; return; }
    startPending = true;
    startError = null;
    try {
      const r = await fetch('/api/program/start', {
        method: 'POST',
        headers: { ...authHeaders(), 'content-type': 'application/json' },
        body: JSON.stringify({ codenames, auto_mode: $autoModeActive }),
      });
      if (!r.ok) { startError = `HTTP ${r.status}`; return; }
      codenameInput = '';
      await fetchStatus();
    } catch (e) {
      startError = String(e);
    } finally {
      startPending = false;
    }
  }

  async function cancelProgram() {
    try {
      await fetch('/api/program/cancel', { method: 'POST', headers: authHeaders() });
      await fetchStatus();
    } catch { /* status will auto-refresh */ }
  }

  // ── HITL resolution ────────────────────────────────────────────────────────

  let resolvePending = $state<string | null>(null); // nonce being resolved

  async function resolveHitl(nonce: string, approved: boolean) {
    resolvePending = nonce;
    try {
      await fetch('/api/control', {
        method: 'POST',
        headers: { ...authHeaders(), 'content-type': 'application/json' },
        body: JSON.stringify({
          command: 'ironclaw_hitl_resolution',
          escalation_nonce: nonce,
          approved,
          operator_reason: null,
        }),
      });
      // Optimistically remove — server broadcasts resolution event which also removes it
      const m = new Map(pendingEscalations);
      m.delete(nonce);
      pendingEscalations = m;
    } catch {
      /* server broadcast will update state */
    } finally {
      resolvePending = null;
    }
  }

  // ── Helpers ────────────────────────────────────────────────────────────────

  const STATE_COLOR: Record<string, string> = {
    idle:      '#94a3b8',
    running:   '#38bdf8',
    completed: '#22c55e',
    cancelled: '#f87171',
  };
</script>

<div class="program-screen" data-testid="program-screen">

  <!-- Header -->
  <div class="ps-header">
    <span class="ps-title">Supervisor — Program</span>
    {#if pendingCount > 0}
      <span class="ps-hitl-badge" title="{pendingCount} HITL escalation{pendingCount > 1 ? 's' : ''} pending">
        ⚠ {pendingCount}
      </span>
    {/if}
    <button class="ps-refresh" onclick={fetchStatus} title="Refresh status">↺</button>
  </div>

  <!-- HITL escalation banner -->
  {#each [...pendingEscalations.values()] as esc (esc.nonce)}
    <div class="ps-hitl-banner">
      <span class="hitl-icon">⚠</span>
      <div class="hitl-body">
        <span class="hitl-topic">{esc.decision_topic}</span>
        <span class="hitl-question">{esc.escalation_question}</span>
      </div>
      <div class="hitl-actions">
        <button
          class="hitl-btn hitl-approve"
          disabled={resolvePending === esc.nonce}
          onclick={() => resolveHitl(esc.nonce, true)}
        >{resolvePending === esc.nonce ? '…' : 'Approve'}</button>
        <button
          class="hitl-btn hitl-reject"
          disabled={resolvePending === esc.nonce}
          onclick={() => resolveHitl(esc.nonce, false)}
        >{resolvePending === esc.nonce ? '…' : 'Reject'}</button>
      </div>
    </div>
  {/each}

  <!-- Program status -->
  {#if loading}
    <div class="ps-state-msg">Loading…</div>
  {:else if error}
    <div class="ps-state-msg ps-error">{error}</div>
  {:else if status}
    <div class="ps-status-row">
      <span class="ps-state-badge" style="color:{STATE_COLOR[status.state]};border-color:{STATE_COLOR[status.state]}">
        {status.state.toUpperCase()}
      </span>
      {#if status.current}
        <span class="ps-current">running: <strong>{status.current}</strong></span>
      {/if}
    </div>

    <!-- Build codename pills — clickable to filter feed -->
    {#if status.codenames.length > 0}
      <div class="ps-codenames" role="list" aria-label="Program codenames">
        {#each status.codenames as cn}
          <button
            class="ps-cn-pill"
            class:ps-cn-active={status.current === cn}
            class:ps-cn-selected={selectedCodename === cn}
            role="listitem"
            title="{cn} — click to filter feed"
            onclick={() => { selectedCodename = selectedCodename === cn ? null : cn; }}
          >
            <span class="cn-name">{cn}</span>
            {#if (eventCounts[cn] ?? 0) > 0}
              <span class="cn-count">{eventCounts[cn]}</span>
            {/if}
          </button>
        {/each}
      </div>
    {/if}

    <!-- Start controls (idle only) -->
    {#if status.state === 'idle'}
      <div class="ps-controls">
        <input
          class="ps-input"
          type="text"
          placeholder="codename-1, codename-2, …"
          bind:value={codenameInput}
          onkeydown={e => { if (e.key === 'Enter') startProgram(); }}
          aria-label="Codenames to start"
        />
        <button
          class="ps-btn ps-btn-start"
          onclick={startProgram}
          disabled={startPending}
        >{startPending ? '…' : 'Start Program'}</button>
        {#if startError}
          <span class="ps-input-error">{startError}</span>
        {/if}
      </div>
    {/if}

    <!-- Cancel (running only) -->
    {#if status.state === 'running'}
      <div class="ps-controls">
        <button class="ps-btn ps-btn-cancel" onclick={cancelProgram}>Cancel Program</button>
      </div>
    {/if}
  {/if}

  <!-- A2A Feed Panel — filtered by selected codename pill -->
  <div class="ps-feed">
    <A2aFeedPanel codename={selectedCodename} />
  </div>

</div>

<style>
  .program-screen {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    font-family: var(--la-font-chrome, 'JetBrains Mono', monospace);
    font-size: 11px;
  }

  .ps-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    background: var(--la-bg-elev-1, #111214);
    border-bottom: 1px solid var(--la-hair-base, #1e2128);
    flex-shrink: 0;
  }
  .ps-title { color: var(--la-text-base, #e2e8f0); font-weight: 600; }
  .ps-refresh {
    margin-left: auto;
    background: transparent;
    border: none;
    color: var(--la-text-mute, #475569);
    cursor: pointer;
    font-size: 13px;
    padding: 0 4px;
  }
  .ps-refresh:hover { color: var(--la-text-dim, #94a3b8); }

  .ps-hitl-badge {
    padding: 1px 6px;
    border-radius: 3px;
    background: rgba(245, 166, 35, 0.15);
    border: 1px solid #f5a623;
    color: #f5a623;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    animation: hitl-pulse 1.5s ease-in-out infinite;
  }

  @keyframes hitl-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.55; }
  }

  /* HITL escalation banner */
  .ps-hitl-banner {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 7px 10px;
    background: rgba(245, 166, 35, 0.08);
    border-bottom: 1px solid rgba(245, 166, 35, 0.3);
    flex-shrink: 0;
  }
  .hitl-icon { color: #f5a623; font-size: 13px; flex-shrink: 0; padding-top: 1px; }
  .hitl-body { flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .hitl-topic {
    color: #f5a623;
    font-weight: 600;
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hitl-question {
    color: var(--la-text-dim, #94a3b8);
    font-size: 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hitl-actions { display: flex; gap: 4px; flex-shrink: 0; }
  .hitl-btn {
    padding: 2px 8px;
    border-radius: 3px;
    border: 1px solid;
    cursor: pointer;
    font-family: inherit;
    font-size: 10px;
    font-weight: 600;
    transition: opacity 80ms;
  }
  .hitl-btn:disabled { opacity: 0.5; cursor: default; }
  .hitl-approve { color: #22c55e; border-color: #22c55e; background: rgba(34, 197, 94, 0.07); }
  .hitl-reject  { color: #f87171; border-color: #f87171; background: rgba(248, 113, 113, 0.07); }

  .ps-state-msg {
    padding: 8px 10px;
    color: var(--la-text-mute, #475569);
    flex-shrink: 0;
  }
  .ps-error { color: #f87171; }

  .ps-status-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 10px;
    flex-shrink: 0;
    border-bottom: 1px solid var(--la-hair-base, #1e2128);
  }
  .ps-state-badge {
    padding: 1px 6px;
    border: 1px solid;
    border-radius: 3px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }
  .ps-current { color: var(--la-text-dim, #94a3b8); }
  .ps-current strong { color: var(--la-text-base, #e2e8f0); }

  /* Codename pills — now interactive buttons */
  .ps-codenames {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 5px 10px;
    border-bottom: 1px solid var(--la-hair-base, #1e2128);
    flex-shrink: 0;
  }
  .ps-cn-pill {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 1px 7px;
    border: 1px solid var(--la-hair-base, #1e2128);
    border-radius: 10px;
    font-size: 10px;
    font-family: inherit;
    color: var(--la-text-dim, #94a3b8);
    background: var(--la-bg-elev-1, #111214);
    cursor: pointer;
    transition: border-color 80ms, color 80ms, background 80ms;
  }
  .ps-cn-pill:hover {
    border-color: var(--la-hair-hi, #2e3440);
    color: var(--la-text-base, #e2e8f0);
  }
  .ps-cn-pill.ps-cn-active {
    color: #38bdf8;
    border-color: #38bdf8;
    background: rgba(56, 189, 248, 0.06);
  }
  /* Selected filter highlight overrides active (toggle-on click) */
  .ps-cn-pill.ps-cn-selected {
    color: #e2e8f0;
    border-color: var(--la-hair-hi, #2e3440);
    background: var(--la-bg-elev-2, #161a1f);
    box-shadow: inset 0 0 0 1px var(--la-hair-hi, #2e3440);
  }
  .cn-count {
    padding: 0 4px;
    border-radius: 8px;
    background: var(--la-bg-elev-2, #161a1f);
    color: var(--la-text-mute, #475569);
    font-size: 9px;
  }

  /* Controls */
  .ps-controls {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--la-hair-base, #1e2128);
    flex-shrink: 0;
  }
  .ps-input {
    flex: 1;
    padding: 3px 7px;
    background: var(--la-bg-elev-2, #161a1f);
    border: 1px solid var(--la-hair-base, #1e2128);
    border-radius: 4px;
    color: var(--la-text-base, #e2e8f0);
    font-family: inherit;
    font-size: 11px;
    outline: none;
  }
  .ps-input:focus { border-color: var(--la-hair-hi, #2e3440); }
  .ps-btn {
    padding: 3px 10px;
    border-radius: 4px;
    border: 1px solid;
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    font-weight: 600;
    transition: opacity 80ms;
  }
  .ps-btn:disabled { opacity: 0.5; cursor: default; }
  .ps-btn-start {
    color: #22c55e;
    border-color: #22c55e;
    background: rgba(34, 197, 94, 0.07);
  }
  .ps-btn-cancel {
    color: #f87171;
    border-color: #f87171;
    background: rgba(248, 113, 113, 0.07);
  }
  .ps-input-error { color: #f87171; font-size: 10px; }

  /* Feed */
  .ps-feed {
    flex: 1;
    overflow: hidden;
  }
</style>
