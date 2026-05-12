<script lang="ts">
  import {
    siblingHealth, ayinStatus, platformHealth,
    conductorStats, alertStats, buildStats,
    siblingDispatchCounts, projectGroups, activityFeed, logEntries,
    helixEntries, vaultCounts,
  } from '$lib/stores';
  import { SIBLING_COLORS, STATUS_COLORS, SIBLINGS } from '$lib/design-tokens';
  import type { SiblingId } from '$lib/types';
  import type { ProjectGroup, Build } from '$lib/types';
  import ConductorPanel from '$lib/../components/ConductorPanel.svelte';
  import AlertPanel from '$lib/../components/AlertPanel.svelte';
  import CompactionPanel from '$lib/../components/CompactionPanel.svelte';
  import VoxelProjects3D from '$lib/../components/topology/VoxelProjects3D.svelte';
  import type { VoxelHoverData } from '$lib/../components/topology/VoxelProjects3D.svelte';
  import BlueprintHUD from '$lib/../components/topology/BlueprintHUD.svelte';
  import ProjectDetailCard from '$lib/../components/topology/ProjectDetailCard.svelte';
  import AgentTaskStrip from '$lib/../components/topology/AgentTaskStrip.svelte';
  import EventStream from '$lib/../components/EventStream.svelte';
  import { logLevelToSeverity } from '$lib/../components/EventStream.svelte';
  import { sourceColor } from '$lib/atmosphere';
  import type { StreamRow } from '$lib/../components/EventStream.svelte';

  // ── Existing Ops state (preserved) ───────────────────────────────────────

  let health = $derived($siblingHealth);
  let status = $derived($ayinStatus);
  let ph = $derived($platformHealth);
  let dispatchCounts = $derived($siblingDispatchCounts);
  let phColor = $derived(ph === 'healthy' ? 'var(--la-agent-researcher)' : ph === 'degraded' ? 'var(--la-agent-performance)' : 'var(--la-agent-security)');
  let stColor = $derived(STATUS_COLORS[status] ?? 'var(--la-text-mute)');

  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => { now = Date.now(); }, 10_000);
    return () => clearInterval(id);
  });

  function heartbeatAge(lastHeartbeat: string | undefined): number {
    if (!lastHeartbeat) return Infinity;
    return (now - new Date(lastHeartbeat).getTime()) / 1000;
  }

  function staleness(lastHeartbeat: string | undefined): 'fresh' | 'stale' | 'dead' {
    const age = heartbeatAge(lastHeartbeat);
    if (age < 30) return 'fresh';
    if (age < 120) return 'stale';
    return 'dead';
  }

  function formatAgo(lastHeartbeat: string | undefined): string {
    const age = heartbeatAge(lastHeartbeat);
    if (!isFinite(age)) return 'never';
    if (age < 60)   return `${Math.floor(age)}s ago`;
    if (age < 3600) return `${Math.floor(age / 60)}m ago`;
    return `${Math.floor(age / 3600)}h ago`;
  }

  function formatUptime(seconds: number): string {
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
    if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`;
    return `${Math.floor(seconds / 86400)}d`;
  }

  let wallClock = $derived.by(() => {
    const d = new Date(now);
    return `${d.getHours().toString().padStart(2,'0')}:${d.getMinutes().toString().padStart(2,'0')}:${d.getSeconds().toString().padStart(2,'0')}`;
  });

  let expanded = $state<Record<string, boolean>>({});
  function toggle(sib: string) { expanded[sib] = !expanded[sib]; }

  // Tutorial hook — preserved
  $effect(() => {
    import('$lib/tutorial').then(m => m.runTutorial('t5')).catch(() => {});
  });

  // ── Topology selection state ──────────────────────────────────────────────

  let selectedGroup = $state<ProjectGroup | null>(null);
  let selectedBuild = $state<Build | null>(null);
  let hoverData = $state<VoxelHoverData | null>(null);
  let detailOpen = $state(false);
  let topoW = $state(0);
  let topoH = $state(0);

  function handleVoxelClick(group: ProjectGroup, build: Build) {
    selectedGroup = group;
    selectedBuild = build;
    detailOpen = true;
  }

  function handleClusterClick(group: ProjectGroup) {
    selectedGroup = group;
    selectedBuild = null;
    detailOpen = true;
  }

  function handleDetailClose() {
    detailOpen = false;
  }

  // Listen for topology-close-gutter from resetTopologyView()
  $effect(() => {
    const close = () => { detailOpen = false; };
    window.addEventListener('la:topology-close-gutter', close);
    return () => window.removeEventListener('la:topology-close-gutter', close);
  });

  // ── Compact right-column event rows ──────────────────────────────────────

  const MAX_RIGHT_ROWS = 80;

  let rightRows = $derived.by((): StreamRow[] => {
    const out: StreamRow[] = [];
    const feedBase = Date.now();
    for (let i = 0; i < $activityFeed.length; i++) {
      const e = $activityFeed[i];
      let text = '';
      let source = '';
      let ts: number;
      if (e.source === 'copilot') {
        text = e.event.summary ?? e.event.kind;
        source = 'copilot';
        ts = feedBase - i; // CopilotActivityEvent has no timestamp; preserve feed order
      } else if (e.source === 'ayin') {
        text = `${e.span.action} (${e.span.duration_ms}ms)`;
        source = e.span.actor;
        ts = new Date(e.span.timestamp).getTime();
      } else {
        text = e.alert.message;
        source = `${e.alert.sibling}/${e.alert.gate}`;
        ts = e.alert.timestamp;
      }
      out.push({
        ts,
        time: new Date(ts).toTimeString().slice(0, 8),
        source,
        color: sourceColor(source),
        text,
        severity: 'info',
      });
    }
    for (const e of $logEntries.slice(-40)) {
      out.push({
        ts: new Date(e.timestamp).getTime(),
        time: new Date(e.timestamp).toTimeString().slice(0, 8),
        source: e.source,
        color: sourceColor(e.source),
        text: e.message,
        severity: logLevelToSeverity(e.level),
      });
    }
    out.sort((a, b) => b.ts - a.ts);
    return out.slice(0, MAX_RIGHT_ROWS);
  });

  // ── Memory metrics ────────────────────────────────────────────────────────

  let memSteps   = $derived($helixEntries.length);
  let memStrands = $derived($vaultCounts?.['strands'] ?? 0);
  let memHelixes = $derived($vaultCounts?.['helixes'] ?? 0);

  // ── Header counts ─────────────────────────────────────────────────────────

  let projectCount = $derived($projectGroups.length);
  let onlineCount  = $derived(Object.values($siblingHealth).filter(h => h?.status === 'online').length);
</script>

<div class="ops-shell">

  <!-- Telemetry bar -->
  <div class="ops-tab-bar">
    <span class="ops-tab active">MISSION CONTROL</span>
    <div class="ops-header-tele">
      <span class="tele-stat" style="color:{phColor}">● {ph.toUpperCase()}</span>
      <span class="tele-stat" style="color:{stColor}">{status}</span>
      <span class="tele-divider">·</span>
      <span class="tele-stat">{projectCount} PROJECTS</span>
      <span class="tele-stat tele-active">{$buildStats.inProgress} RUNNING</span>
      <span class="tele-stat">{onlineCount}/7 ONLINE</span>
      <span class="tele-divider">·</span>
      <span class="tele-stat tele-queue">{$conductorStats.queueDepth} QUEUED</span>
      <span class="tele-stat tele-alert">{$alertStats.unacknowledged} ALERTS</span>
      <span class="tele-divider">·</span>
      <span class="tele-stat tele-clock">{wallClock}</span>
    </div>
  </div>

  <!-- 3-column main area -->
  <div class="ops-main">

    <!-- LEFT — agent roster + memory + accordion -->
    <div class="left-col">

      <!-- Squad health compact roster -->
      <div class="panel-section" data-onboarding="sitrep-squad-health">
        <div class="panel-head">
          <span class="panel-label">SQUAD HEALTH</span>
          <span class="panel-count">{onlineCount}/7 agents online</span>
        </div>
        <div class="squad-grid">
          {#each SIBLINGS as sib}
            {@const h = health[sib as SiblingId]}
            {@const color = SIBLING_COLORS[sib] ?? 'var(--la-text-mute)'}
            {@const stale = staleness(h?.lastHeartbeat)}
            {@const dc = dispatchCounts[sib as SiblingId] ?? 0}
            <div class="agent-card">
              <button
                class="agent-card-btn"
                onclick={() => toggle(sib)}
                aria-expanded={expanded[sib] ?? false}
                aria-label="Toggle {sib} details"
                data-testid="squad-health-toggle"
              >
                <div class="agent-card-top">
                  <div class="agent-pip" style="background: {STATUS_COLORS[h?.status ?? ''] ?? 'var(--la-text-dim)'}"></div>
                  <span class="agent-name" style="color: {color}">{sib.toUpperCase()}</span>
                </div>
                <div class="agent-status">{h?.status ?? 'unknown'}</div>
                <div class="agent-uptime">{h?.uptime ? formatUptime(h.uptime) : '--'}</div>
                {#if stale !== 'fresh'}
                  <span class="stale-badge" class:stale={stale === 'stale'} class:dead={stale === 'dead'}>
                    {formatAgo(h?.lastHeartbeat)}
                  </span>
                {/if}
                {#if dc > 0}
                  <span class="dispatch-badge">{dc}</span>
                {/if}
              </button>
              {#if expanded[sib]}
                <div class="agent-expanded">
                  <div class="agent-hb">hb: <span class="hb-age" class:fresh={stale === 'fresh'} class:stale={stale === 'stale'} class:dead={stale === 'dead'}>{formatAgo(h?.lastHeartbeat)}</span></div>
                  {#if h?.capabilities?.length}
                    <div class="agent-caps">
                      {#each h.capabilities as cap}
                        <span class="cap-chip">{cap}</span>
                      {/each}
                    </div>
                  {/if}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      </div>

      <!-- Memory metrics -->
      <div class="panel-section">
        <div class="panel-head">
          <span class="panel-label">MEMORY</span>
        </div>
        <div class="memory-metrics">
          <div class="mem-row"><span class="mem-label">Steps</span><span class="mem-val">{memSteps.toLocaleString()}</span></div>
          <div class="mem-row"><span class="mem-label">Strands</span><span class="mem-val">{memStrands.toLocaleString()}</span></div>
          <div class="mem-row"><span class="mem-label">Helixes</span><span class="mem-val">{memHelixes.toLocaleString()}</span></div>
        </div>
      </div>

      <!-- Accordion panels -->
      <ConductorPanel />
      <AlertPanel />
      <CompactionPanel />
    </div>

    <!-- CENTER — VoxelProjects3D + BlueprintHUD + detail gutter -->
    <div class="center-col">
      <!-- Topology viewport: topology canvas + hud overlay side-by-side with detail card -->
      <div class="topology-viewport">
        <!-- 3D scene + HUD overlay -->
        <div
          class="la-topology-container"
          style="flex: 1; min-width: 0;"
          bind:clientWidth={topoW}
          bind:clientHeight={topoH}
        >
          <VoxelProjects3D
            onVoxelHover={(d) => { hoverData = d; }}
            onVoxelClick={handleVoxelClick}
            onClusterClick={handleClusterClick}
          />
          <BlueprintHUD
            groups={$projectGroups}
            selectedGroup={selectedGroup}
            width={topoW}
            height={topoH}
          />
          <!-- Hover tooltip -->
          {#if hoverData && !detailOpen}
            <div
              class="hover-tooltip"
              style="left: {hoverData.screenX + 12}px; top: {hoverData.screenY - 8}px; position: fixed; z-index: 50;"
            >
              <span class="text-[9px] font-mono text-[#e2e8f0]">{hoverData.group.name}</span>
              {#if hoverData.build}
                <span class="text-[8px] font-mono text-[#94a3b8] block">{hoverData.build.codename ?? hoverData.build.name}</span>
                <span class="text-[8px] font-mono block" style="color: {hoverData.build.status === 'in_progress' ? '#22c55e' : hoverData.build.status === 'failed' ? '#ef4444' : '#475569'};">{hoverData.build.status}</span>
              {:else}
                <span class="text-[8px] font-mono text-[#475569] block">{hoverData.group.activePlanCount} active builds</span>
              {/if}
            </div>
          {/if}
        </div>

        <!-- ProjectDetailCard gutter — slides in when a voxel/cluster is selected -->
        {#if detailOpen && selectedGroup}
          <ProjectDetailCard
            group={selectedGroup}
            build={selectedBuild}
            onClose={handleDetailClose}
          />
        {/if}
      </div>
    </div>

    <!-- RIGHT — compact live event stream -->
    <div class="right-col">
      <div class="panel-head">
        <span class="panel-label">LIVE EVENTS</span>
        <span class="panel-count">{rightRows.length}</span>
      </div>
      <div class="right-stream">
        <EventStream rows={rightRows} newestFirst maxDisplay={MAX_RIGHT_ROWS} />
      </div>
    </div>
  </div>

  <!-- Bottom: AgentTaskStrip -->
  <AgentTaskStrip />
</div>

<style>
  .ops-shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    font-family: var(--la-font-chrome);
  }

  /* ── Telemetry bar ── */
  .ops-tab-bar {
    display: flex;
    align-items: center;
    border-bottom: 1px solid var(--la-hair-strong);
    flex-shrink: 0;
    background: var(--la-bg-base);
  }
  .ops-tab {
    padding: 0 16px;
    height: 36px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--la-text-stark);
    border-bottom: 2px solid var(--la-focus-ring);
    flex-shrink: 0;
  }
  .ops-header-tele {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 14px;
    font-size: 9px;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.06em;
  }
  .tele-stat  { color: var(--la-text-dim); }
  .tele-active { color: var(--la-agent-researcher); }
  .tele-queue  { color: var(--la-agent-performance); }
  .tele-alert  { color: var(--la-agent-security); }
  .tele-clock  { color: var(--la-text-mute); }
  .tele-divider { color: var(--la-hair-strong); }

  /* ── 3-column main ── */
  .ops-main {
    flex: 1;
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr) 240px;
    min-height: 0;
    overflow: hidden;
  }

  /* ── Left column ── */
  .left-col {
    overflow-y: auto;
    border-right: 1px solid var(--la-hair-base);
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .panel-section {
    border-bottom: 1px solid var(--la-hair-base);
  }
  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 5px 10px;
    border-bottom: 1px solid var(--la-hair-strong);
    background: var(--la-bg-elev-1, #111214);
  }
  .panel-label {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.14em;
    color: var(--la-text-mute);
    text-transform: uppercase;
  }
  .panel-count { font-size: 8px; color: var(--la-text-dim); }

  /* Squad health grid — 7 compact cells */
  .squad-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 1px;
    background: var(--la-hair-base);
  }
  .agent-card { background: var(--la-bg-frame); overflow: hidden; }
  .agent-card-btn {
    width: 100%;
    padding: 5px 3px;
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    transition: background 80ms;
  }
  .agent-card-btn:hover { background: var(--la-bg-elev-1, #111214); }
  .agent-card-top { display: flex; align-items: center; gap: 3px; }
  .agent-pip { width: 5px; height: 5px; border-radius: 50%; flex-shrink: 0; }
  .agent-name { font-size: 7px; font-weight: 700; letter-spacing: 0.06em; }
  .agent-status { font-size: 6px; color: var(--la-text-mute); text-transform: capitalize; }
  .agent-uptime { font-size: 6px; color: var(--la-text-dim); font-variant-numeric: tabular-nums; }
  .stale-badge { font-size: 6px; padding: 1px 2px; }
  .stale-badge.stale { color: var(--la-agent-performance); }
  .stale-badge.dead  { color: var(--la-agent-security); }
  .dispatch-badge { font-size: 6px; color: var(--la-agent-engineer); }
  .agent-expanded {
    padding: 3px 5px 5px;
    border-top: 1px solid var(--la-hair-base);
    display: flex; flex-direction: column; gap: 2px;
  }
  .agent-hb { font-size: 6px; color: var(--la-text-dim); }
  .hb-age.fresh { color: var(--la-agent-researcher); }
  .hb-age.stale { color: var(--la-agent-performance); }
  .hb-age.dead  { color: var(--la-agent-security); }
  .agent-caps { display: flex; flex-wrap: wrap; gap: 2px; }
  .cap-chip { font-size: 5px; padding: 1px 2px; background: var(--la-bg-elev-2); color: var(--la-text-dim); }

  /* Memory metrics */
  .memory-metrics { padding: 6px 10px; display: flex; flex-direction: column; gap: 3px; }
  .mem-row { display: flex; justify-content: space-between; }
  .mem-label { font-size: 8px; color: var(--la-text-dim); }
  .mem-val { font-size: 8px; color: var(--la-text-base); font-variant-numeric: tabular-nums; }

  /* ── Center column ── */
  .center-col {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .topology-viewport {
    flex: 1;
    display: flex;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }
  .hover-tooltip {
    background: rgba(10,10,15,0.92);
    border: 1px solid #1e293b;
    padding: 5px 8px;
    border-radius: 4px;
    pointer-events: none;
    white-space: nowrap;
  }

  /* ── Right column ── */
  .right-col {
    border-left: 1px solid var(--la-hair-base);
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }
  .right-stream {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
</style>
