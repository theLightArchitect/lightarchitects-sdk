<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { builds, activityFeed, slotAssignments, staleBuilds, ayinStatus, terminalConnected, authStatus } from '$lib/stores';
  import { STATUS_COLORS } from '$lib/design-tokens';
  import { api } from '$lib/api';
  import type { BuildStatus, OverallStatus } from '$lib/types';
  import { navigate } from '$lib/routes';
  import { subscribeByTopic, type WebEventV2 } from '$lib/sse';

  // ── Counters ──────────────────────────────────────────────────────────────

  let totalBuilds = $derived($builds.length);

  let activeBuilds = $derived(
    $builds.filter((b): boolean =>
      (b.status as BuildStatus) === 'in_progress' || (b.status as BuildStatus) === 'queued',
    ).length,
  );

  let activeAgents = $derived(
    [...$slotAssignments.values()].flat().filter(w => w.state === 'writing' || w.state === 'gate').length,
  );

  let recentGates = $derived(
    $activityFeed.filter(e => {
      if (e.source !== 'copilot') return false;
      const ev = (e as { source: 'copilot'; event: import('$lib/types').CopilotActivityEvent }).event;
      return ev.kind === 'gate';
    }).length,
  );

  let hitlPending = $derived(
    $builds.filter((b): boolean => (b.status as BuildStatus) === 'paused').length,
  );

  let staleCount = $derived($staleBuilds.length);

  // ── Service health dots ───────────────────────────────────────────────────

  const statusColor = (s: string) => STATUS_COLORS[s as keyof typeof STATUS_COLORS] ?? '#6b7280';

  let auth = $derived($authStatus);
  let ayinState = $derived($ayinStatus);
  let buildCount = $derived($builds.length);
  let ptyColor = $derived($terminalConnected ? STATUS_COLORS.connected : STATUS_COLORS.offline);

  // AYIN dot: auth failures surface as red (more urgent than "reconnecting")
  let ayinColor = $derived(auth !== 'ok' ? '#DC2626' : statusColor(ayinState));
  let ayinTooltip = $derived(
    auth === 'unauthorized'  ? 'AYIN — auth expired'        :
    auth === 'forbidden'     ? 'AYIN — auth denied'         :
    ayinState === 'connected'    ? `AYIN live · ${buildCount} builds` :
    ayinState === 'reconnecting' ? 'AYIN reconnecting…'              :
                                   'AYIN offline',
  );

  let preflightOverall = $state<OverallStatus | null>(null);

  function preflightColor(o: OverallStatus | null): string {
    if (o === 'Ready')    return '#22c55e';
    if (o === 'Degraded') return '#f59e0b';
    if (o === 'Blocked')  return '#ef4444';
    return '#6b7280';
  }
  function preflightTooltip(o: OverallStatus | null): string {
    if (o === 'Ready')    return 'INFRA ready';
    if (o === 'Degraded') return 'INFRA degraded';
    if (o === 'Blocked')  return 'INFRA blocked';
    return 'INFRA …';
  }

  async function pollPreflight() {
    try { preflightOverall = (await api.fetchPreflight()).overall; } catch { /* retain last */ }
  }

  let unsubscribeAyin: (() => void) | null = null;
  function handleAyinEvent(event: WebEventV2): void {
    if (event.topic.endsWith('.connected'))    ayinStatus.set('connected');
    else if (event.topic.endsWith('.disconnected')) ayinStatus.set('offline');
    else if (event.topic.endsWith('.reconnecting')) ayinStatus.set('reconnecting');
  }

  let pollInterval: ReturnType<typeof setInterval> | undefined;
  onMount(() => {
    void pollPreflight();
    pollInterval = setInterval(() => { void pollPreflight(); }, 30_000);
    unsubscribeAyin = subscribeByTopic('v1.agent.ayin.*', handleAyinEvent);
  });
  onDestroy(() => {
    clearInterval(pollInterval);
    unsubscribeAyin?.();
  });
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

  <!-- Major separator before service health cluster -->
  <div class="stat-sep stat-sep--major" aria-hidden="true"></div>

  <!-- Service health dots — compact live indicators -->
  <div class="health-cluster" aria-label="Service health">
    <!-- AYIN -->
    <div class="health-dot-wrap" title={ayinTooltip}>
      <div class="health-dot" style="background:{ayinColor};box-shadow:0 0 4px {ayinColor}80"></div>
      <span class="health-label">AY</span>
    </div>
    <!-- HELIX — always gold (structural, not polled) -->
    <div class="health-dot-wrap" title="HELIX knowledge graph">
      <div class="health-dot" style="background:#FFD700;box-shadow:0 0 5px #FFD70060"></div>
      <span class="health-label">HX</span>
    </div>
    <!-- BUILD gateway -->
    <div class="health-dot-wrap" title="BUILD gateway">
      <div class="health-dot" style="background:var(--la-agent-engineer,#3B82F6);box-shadow:0 0 4px #3B82F680"></div>
      <span class="health-label">BL</span>
    </div>
    <!-- PTY -->
    <div class="health-dot-wrap" title={$terminalConnected ? 'PTY connected' : 'PTY offline'}>
      <div class="health-dot" style="background:{ptyColor};box-shadow:0 0 4px {ptyColor}80"></div>
      <span class="health-label">PT</span>
    </div>
    <!-- INFRA preflight -->
    <div class="health-dot-wrap" title={preflightTooltip(preflightOverall)}>
      <div class="health-dot" style="background:{preflightColor(preflightOverall)};box-shadow:0 0 4px {preflightColor(preflightOverall)}80"></div>
      <span class="health-label">IF</span>
    </div>
  </div>
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
    border-radius: 0;
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

  /* Hairline vertical separator */
  .stat-sep {
    width: 1px;
    height: 12px;
    background: var(--la-hair-faint, #1c2028);
    flex-shrink: 0;
    align-self: center;
  }

  /* Major separator — taller, slightly brighter — demarcates health cluster */
  .stat-sep--major {
    height: 18px;
    background: var(--la-hair-base, #2c3140);
    margin: 0 2px;
  }

  /* ── Service health cluster ─────────────────────────────────────────────── */
  .health-cluster {
    display: flex;
    align-items: center;
    padding: 0 4px;
    height: 100%;
    gap: 0;
  }

  .health-dot-wrap {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 0 5px;
    height: 100%;
    cursor: default;
    transition: background 80ms;
  }
  .health-dot-wrap:hover {
    background: var(--la-bg-elev-1, #111214);
  }
  .health-dot-wrap:hover .health-label {
    color: var(--la-text-base, #c9d1d9);
  }

  .health-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .health-label {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-mute, #6e7681);
    font-family: var(--la-font-mono, monospace);
    text-transform: uppercase;
  }
</style>
