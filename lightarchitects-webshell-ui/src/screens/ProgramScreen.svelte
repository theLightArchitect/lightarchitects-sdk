<script lang="ts">
  import { onMount } from 'svelte';
  import { authHeaders } from '$lib/auth';
  import { a2aFeedStore } from '$lib/a2aFeed';
  import A2aFeedPanel from '$lib/../components/A2aFeedPanel.svelte';

  // ── Program manifest types ────────────────────────────────────────────────

  type BuildState = 'pending' | 'running' | 'completed' | 'failed';

  interface ProgramBuild {
    codename: string;
    state: BuildState;
  }

  interface ProgramStatus {
    codenames: string[];
    current: string | null;
    state: 'idle' | 'running' | 'completed' | 'cancelled';
  }

  // ── State ─────────────────────────────────────────────────────────────────

  let status  = $state<ProgramStatus | null>(null);
  let loading = $state(true);
  let error   = $state<string | null>(null);

  // Selected codename for the feed panel — null = show all
  let selectedCodename = $state<string | null>(null);

  // Feed event counts per codename (derived from the store)
  let eventCounts = $derived.by(() => {
    const map = $a2aFeedStore;
    const counts: Record<string, number> = {};
    for (const [cn, evs] of map) counts[cn] = evs.length;
    return counts;
  });

  // ── Data fetch ────────────────────────────────────────────────────────────

  async function fetchStatus() {
    try {
      loading = true;
      error = null;
      const r = await fetch('/api/program/status', { headers: authHeaders() });
      if (r.ok) {
        status = await r.json() as ProgramStatus;
      } else if (r.status === 404) {
        // No active program — show idle state
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

  onMount(() => { fetchStatus(); });

  // ── Start controls ────────────────────────────────────────────────────────

  let startPending = $state(false);
  let startError   = $state<string | null>(null);

  // Input for comma-separated codenames to run as a program
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
        body: JSON.stringify({ codenames }),
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

  // ── State helpers ─────────────────────────────────────────────────────────

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
    <button class="ps-refresh" onclick={fetchStatus} title="Refresh status">↺</button>
  </div>

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

    <!-- Build codename pills -->
    {#if status.codenames.length > 0}
      <div class="ps-codenames" role="list" aria-label="Program codenames">
        {#each status.codenames as cn}
          <div
            class="ps-cn-pill"
            class:ps-cn-active={status.current === cn}
            role="listitem"
            title={cn}
          >
            <span class="cn-name">{cn}</span>
            {#if (eventCounts[cn] ?? 0) > 0}
              <span class="cn-count">{eventCounts[cn]}</span>
            {/if}
          </div>
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

  <!-- A2A Feed Panel -->
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

  /* Codename pills */
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
    color: var(--la-text-dim, #94a3b8);
    background: var(--la-bg-elev-1, #111214);
  }
  .ps-cn-pill.ps-cn-active {
    color: #38bdf8;
    border-color: #38bdf8;
    background: rgba(56, 189, 248, 0.06);
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
