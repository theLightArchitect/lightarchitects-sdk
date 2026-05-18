<script lang="ts">
  /**
   * StatsTopbar — persistent global status ribbon.
   *
   * Counters live-update via Svelte store subscriptions driven by the SSE
   * activity feed and builds portfolio store. Mounted once in `app.svelte`.
   *
   * Counters: Builds · Active · Agents · Gates · HITL · Stale
   */

  import { builds, activityFeed, slotAssignments } from '$lib/stores';
  import type { BuildStatus } from '$lib/types';

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

  // Gate events = GATE_REVIEW entries in the activity feed in the last 60s.
  let recentGates = $derived(
    $activityFeed.filter(e => {
      if (e.source !== 'copilot') return false;
      const ev = (e as { source: 'copilot'; event: import('$lib/types').CopilotActivityEvent }).event;
      return ev.kind === 'gate' && Date.now() - new Date(ev.timestamp).getTime() < 60_000;
    }).length,
  );

  // HITL = builds in 'paused' state (waiting for operator approval).
  let hitlPending = $derived(
    $builds.filter((b): boolean => (b.status as BuildStatus) === 'paused').length,
  );

  // Stale = builds in_progress with no activity feed event in the last 10 minutes.
  let staleBuilds = $derived(
    $builds.filter((b): boolean => {
      if ((b.status as BuildStatus) !== 'in_progress') return false;
      const lastActivity = $activityFeed.findLast(e => {
        if (e.source !== 'copilot') return false;
        const ev = (e as { source: 'copilot'; event: import('$lib/types').CopilotActivityEvent }).event;
        return 'build_id' in ev && (ev as unknown as Record<string, unknown>).build_id === b.id;
      });
      if (!lastActivity) return true;
      const ts = (lastActivity as { source: 'copilot'; event: import('$lib/types').CopilotActivityEvent }).event.timestamp;
      return Date.now() - new Date(ts).getTime() > 10 * 60_000;
    }).length,
  );
</script>

<div class="stats-topbar" role="status" aria-label="Build fleet status">
  <span class="stat">
    <span class="stat-val">{totalBuilds}</span>
    <span class="stat-label">BUILDS</span>
  </span>
  <span class="stat-sep" aria-hidden="true">·</span>

  <span class="stat" class:stat-hot={activeBuilds > 0}>
    <span class="stat-val">{activeBuilds}</span>
    <span class="stat-label">ACTIVE</span>
  </span>
  <span class="stat-sep" aria-hidden="true">·</span>

  <span class="stat" class:stat-hot={activeAgents > 0}>
    <span class="stat-val">{activeAgents}</span>
    <span class="stat-label">AGENTS</span>
  </span>
  <span class="stat-sep" aria-hidden="true">·</span>

  <span class="stat">
    <span class="stat-val">{recentGates}</span>
    <span class="stat-label">GATES</span>
  </span>
  <span class="stat-sep" aria-hidden="true">·</span>

  <span class="stat" class:stat-warn={hitlPending > 0}>
    <span class="stat-val">{hitlPending}</span>
    <span class="stat-label">HITL</span>
  </span>
  <span class="stat-sep" aria-hidden="true">·</span>

  <span class="stat" class:stat-warn={staleBuilds > 0}>
    <span class="stat-val">{staleBuilds}</span>
    <span class="stat-label">STALE</span>
  </span>
</div>

<style>
  .stats-topbar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 10px;
    height: 24px;
    background: var(--la-bg-base, #0d1117);
    border-bottom: 1px solid var(--la-hair-faint, rgba(255,255,255,0.06));
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    flex-shrink: 0;
  }

  .stat {
    display: flex;
    align-items: baseline;
    gap: 3px;
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

  .stat-sep {
    color: var(--la-text-mute, #6e7681);
    font-size: 8px;
    user-select: none;
    flex-shrink: 0;
  }
</style>
