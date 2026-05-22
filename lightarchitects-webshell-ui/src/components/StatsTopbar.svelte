<script lang="ts">
  /**
   * StatsTopbar — persistent global status ribbon.
   *
   * Counters live-update via Svelte store subscriptions driven by the SSE
   * activity feed and builds portfolio store. Mounted once in `app.svelte`.
   *
   * Counters: Builds · Active · Agents · Gates · HITL · Stale
   */

  import { builds, activityFeed, slotAssignments, staleBuilds } from '$lib/stores';
  import type { BuildStatus } from '$lib/types';
  import { navigate } from '$lib/routes';

  // ── Derived counters ──────────────────────────────────────────────────────

  let totalBuilds = $derived($builds.length);

  let activeBuilds = $derived(
    $builds.filter((b): boolean =>
      (b.status as BuildStatus) === 'in_progress' || (b.status as BuildStatus) === 'queued',
    ).length,
  );

  let activeAgents = $derived(
    [...$slotAssignments.values()].flat().filter(w => w.state === 'writing' || w.state === 'gate').length,
  );

  // Gate events — all session gate events (monotonically increasing, never resets).
  let recentGates = $derived(
    $activityFeed.filter(e => {
      if (e.source !== 'copilot') return false;
      const ev = (e as { source: 'copilot'; event: import('$lib/types').CopilotActivityEvent }).event;
      return ev.kind === 'gate';
    }).length,
  );

  // HITL = builds in 'paused' state (waiting for operator approval).
  let hitlPending = $derived(
    $builds.filter((b): boolean => (b.status as BuildStatus) === 'paused').length,
  );

  // Stale count — sourced from the shared staleBuilds store (single source of truth).
  let staleCount = $derived($staleBuilds.length);
</script>

<div class="stats-topbar" role="status" aria-label="Build fleet status">
  <button class="stat stat-btn" onclick={() => navigate('/builds')} title="All builds">
    <span class="stat-val">{totalBuilds}</span>
    <span class="stat-label">BUILDS</span>
  </button>
  <div class="stat-sep" aria-hidden="true"></div>

  <button class="stat stat-btn" class:stat-hot={activeBuilds > 0} onclick={() => navigate('/builds?filter=active')} title="Active builds">
    <span class="stat-val">{activeBuilds}</span>
    <span class="stat-label">ACTIVE</span>
  </button>
  <div class="stat-sep" aria-hidden="true"></div>

  <button class="stat stat-btn" class:stat-hot={activeAgents > 0} onclick={() => navigate('/ops')} title="Agent fleet">
    <span class="stat-val">{activeAgents}</span>
    <span class="stat-label">AGENTS</span>
  </button>
  <div class="stat-sep" aria-hidden="true"></div>

  <button class="stat stat-btn" onclick={() => navigate('/builds?filter=gates')} title="Recent gate verdicts">
    <span class="stat-val">{recentGates}</span>
    <span class="stat-label">GATES</span>
  </button>
  <div class="stat-sep" aria-hidden="true"></div>

  <button class="stat stat-btn" class:stat-warn={hitlPending > 0} onclick={() => navigate('/run?tab=approval')} title="Pending approvals">
    <span class="stat-val">{hitlPending}</span>
    <span class="stat-label">Approval</span>
  </button>
  <div class="stat-sep" aria-hidden="true"></div>

  <button class="stat stat-btn" class:stat-warn={staleCount > 0} onclick={() => navigate('/builds?filter=stale')} title="Stale / idle builds">
    <span class="stat-val">{staleCount}</span>
    <span class="stat-label">Idle</span>
  </button>
</div>

<style>
  .stats-topbar {
    display: flex;
    align-items: stretch;
    gap: 0;
    padding: 0;
    height: 24px;
    background: var(--la-bg-void, #08090a);
    border-bottom: 1px solid var(--la-hair-faint, #1c2028);
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    flex-shrink: 0;
  }

  .stat {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .stat-val {
    font-size: 10px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: var(--la-text-base, #c9d1d9);
    letter-spacing: 0.02em;
  }

  .stat-label {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-mute, #6e7681);
    text-transform: uppercase;
  }

  .stat-hot .stat-val { color: var(--la-agent-researcher, #17c3b2); }
  .stat-warn .stat-val { color: var(--la-agent-performance, #f97316); }

  .stat-btn {
    background: none;
    border: none;
    padding: 0 8px;
    cursor: pointer;
    border-radius: 0;   /* zero-radius: angular surfaces */
    height: 100%;
    transition: background-color 80ms;
    display: flex;
    align-items: center;
    gap: 3px;
  }
  .stat-btn:hover {
    background: var(--la-bg-elev-1, #111214);
  }
  .stat-btn:hover .stat-label {
    color: var(--la-text-base, #c9d1d9);
  }

  /* Hairline vertical separator — 1px structural line, not a glyph */
  .stat-sep {
    width: 1px;
    height: 12px;
    background: var(--la-hair-faint, #1c2028);
    flex-shrink: 0;
    align-self: center;
  }
</style>
