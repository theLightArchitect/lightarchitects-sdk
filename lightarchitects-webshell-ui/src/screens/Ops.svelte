<script lang="ts">
  import {
    siblingHealth, ayinStatus, platformHealth,
    conductorStats, alertStats, buildStats,
    siblingDispatchCounts, projectGroups, activityFeed, logEntries,
    helixEntries, vaultCounts,
  } from '$lib/stores';
  import { SIBLING_COLORS, STATUS_COLORS, SIBLINGS } from '$lib/design-tokens';
  import type { SiblingId } from '$lib/types';
  import ConductorPanel from '$lib/../components/ConductorPanel.svelte';
  import AlertPanel from '$lib/../components/AlertPanel.svelte';
  import CompactionPanel from '$lib/../components/CompactionPanel.svelte';
  import GitForest from '$lib/../components/topology/GitForest.svelte';
  import EventStream from '$lib/../components/EventStream.svelte';
  import { logLevelToSeverity } from '$lib/../components/EventStream.svelte';
  import { sourceColor } from '$lib/atmosphere';
  import type { StreamRow } from '$lib/../components/EventStream.svelte';

  // ── Reactive data ─────────────────────────────────────────────────────────

  let health        = $derived($siblingHealth);
  let status        = $derived($ayinStatus);
  let ph            = $derived($platformHealth);
  let dispatchCounts = $derived($siblingDispatchCounts);
  let phColor       = $derived(
    ph === 'healthy'  ? 'var(--la-agent-researcher)'  :
    ph === 'degraded' ? 'var(--la-agent-performance)' :
                        'var(--la-agent-security)',
  );
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
    if (age < 30)  return 'fresh';
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
    if (seconds < 60)    return `${seconds}s`;
    if (seconds < 3600)  return `${Math.floor(seconds / 60)}m`;
    if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`;
    return `${Math.floor(seconds / 86400)}d`;
  }

  let wallClock = $derived.by(() => {
    const d = new Date(now);
    const hh = d.getHours()  .toString().padStart(2, '0');
    const mm = d.getMinutes().toString().padStart(2, '0');
    const ss = d.getSeconds().toString().padStart(2, '0');
    return `${hh}:${mm}:${ss}`;
  });

  let expanded = $state<Record<string, boolean>>({});
  function toggle(sib: string) { expanded[sib] = !expanded[sib]; }

  // Tutorial hook
  $effect(() => {
    import('$lib/tutorial').then(m => m.runTutorial('t5')).catch(() => {});
  });

  // ── Git forest toggle (OPS-1) ─────────────────────────────────────────────

  let forestVisible = $state(true);

  // ── Header counts ─────────────────────────────────────────────────────────

  let projectCount = $derived($projectGroups.length);
  let onlineCount  = $derived(
    Object.values($siblingHealth).filter(h => h?.status === 'online').length,
  );

  // ── Memory metrics ────────────────────────────────────────────────────────

  let memSteps   = $derived($helixEntries.length);
  let memStrands = $derived($vaultCounts?.['strands'] ?? 0);
  let memHelixes = $derived($vaultCounts?.['helixes'] ?? 0);

  // ── Compact event stream ──────────────────────────────────────────────────

  const MAX_ROWS = 60;

  let eventRows = $derived.by((): StreamRow[] => {
    const out: StreamRow[] = [];
    const base = Date.now();
    for (let i = 0; i < $activityFeed.length; i++) {
      const e = $activityFeed[i];
      let text = ''; let source = ''; let ts: number;
      if (e.source === 'copilot') {
        text = e.event.summary ?? e.event.kind; source = 'copilot'; ts = base - i;
      } else if (e.source === 'ayin') {
        text = `${e.span.action} (${e.span.duration_ms}ms)`; source = e.span.actor;
        ts = new Date(e.span.timestamp).getTime();
      } else {
        text = e.alert.message; source = `${e.alert.sibling}/${e.alert.gate}`;
        ts = e.alert.timestamp;
      }
      out.push({ ts, time: new Date(ts).toTimeString().slice(0, 8), source, color: sourceColor(source), text, severity: 'info' });
    }
    for (const e of $logEntries.slice(-40)) {
      out.push({ ts: new Date(e.timestamp).getTime(), time: new Date(e.timestamp).toTimeString().slice(0, 8), source: e.source, color: sourceColor(e.source), text: e.message, severity: logLevelToSeverity(e.level) });
    }
    out.sort((a, b) => b.ts - a.ts);
    return out.slice(0, MAX_ROWS);
  });
</script>

<div class="ops-shell">

  <!-- Telemetry bar -->
  <div class="ops-tab-bar">
    <span class="ops-tab active">MISSION CONTROL</span>
    <div class="ops-header-tele">
      <span class="tele-stat" style="color:{phColor}">● {ph.toUpperCase()}</span>
      <span class="tele-stat" style="color:{stColor}">{status}</span>
      <span class="tele-div">·</span>
      <span class="tele-stat">{projectCount} PROJECTS</span>
      <span class="tele-stat tele-active">{$buildStats.inProgress} RUNNING</span>
      <span class="tele-stat">{onlineCount}/7 ONLINE</span>
      <span class="tele-div">·</span>
      <span class="tele-stat tele-queue">{$conductorStats.queueDepth} QUEUED</span>
      <span class="tele-stat tele-alert">{$alertStats.unacknowledged} ALERTS</span>
      <span class="tele-div">·</span>
      <span class="tele-stat tele-clock">{wallClock}</span>
    </div>
  </div>

  <!-- OPS-1: 2-column hero layout (55% mission-control / 45% git forest) -->
  <div class="ops-main" data-forest-hidden={!forestVisible || undefined}>

    <!-- LEFT — Mission Control (hero at 55%) -->
    <div class="mission-col">

      <!-- OPS-2: Squad health pills with color glow + heartbeat -->
      <div class="panel-section" data-onboarding="sitrep-squad-health">
        <div class="panel-head">
          <span class="panel-label">SQUAD HEALTH</span>
          <span class="panel-count">{onlineCount}/7 online</span>
        </div>
        <div class="squad-pills">
          {#each SIBLINGS as sib}
            {@const h = health[sib as SiblingId]}
            {@const color = SIBLING_COLORS[sib] ?? 'var(--la-text-mute)'}
            {@const stale = staleness(h?.lastHeartbeat)}
            {@const dc = dispatchCounts[sib as SiblingId] ?? 0}
            {@const pillState = h?.status === 'online' ? 'online' : h?.status === 'offline' ? 'offline' : 'never'}
            <button
              class="squad-pill"
              data-state={pillState}
              style:--color={color}
              onclick={() => toggle(sib)}
              aria-expanded={expanded[sib] ?? false}
              aria-label="Toggle {sib} details"
              data-testid="squad-health-toggle"
            >
              <span class="pill-dot"></span>
              <span class="pill-name">{sib.toUpperCase()}</span>
              <span class="pill-seen">{formatAgo(h?.lastHeartbeat)}</span>
              {#if dc > 0}<span class="pill-dc">{dc}</span>{/if}
              {#if stale !== 'fresh' && pillState !== 'never'}
                <span class="pill-stale" class:stale={stale === 'stale'} class:dead={stale === 'dead'}></span>
              {/if}
            </button>
            {#if expanded[sib]}
              <div class="pill-expanded" style:border-color={color}>
                <div class="pill-exp-row">
                  <span class="exp-label">uptime</span>
                  <span class="exp-val">{h?.uptime ? formatUptime(h.uptime) : '--'}</span>
                </div>
                <div class="pill-exp-row">
                  <span class="exp-label">hb</span>
                  <span class="exp-val hb-age" class:fresh={stale === 'fresh'} class:stale={stale === 'stale'} class:dead={stale === 'dead'}>{formatAgo(h?.lastHeartbeat)}</span>
                </div>
                {#if h?.capabilities?.length}
                  <div class="pill-caps">
                    {#each h.capabilities as cap}
                      <span class="cap-chip">{cap}</span>
                    {/each}
                  </div>
                {/if}
              </div>
            {/if}
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

      <!-- Live event stream (moved from right col) -->
      <div class="events-section">
        <div class="panel-head">
          <span class="panel-label">LIVE EVENTS</span>
          <span class="panel-count">{eventRows.length}</span>
        </div>
        <div class="events-stream">
          <EventStream rows={eventRows} newestFirst maxDisplay={MAX_ROWS} />
        </div>
      </div>
    </div>

    <!-- RIGHT — Git forest (GIT-1 through GIT-3) -->
    {#if forestVisible}
      <div class="forest-col">
        <div class="forest-header">
          <span class="panel-label">GIT FOREST</span>
          <div class="forest-legend">
            <span class="legend-dot" style="background:#22c55e"></span><span class="legend-txt">clean</span>
            <span class="legend-dot" style="background:#f59e0b"></span><span class="legend-txt">hitl</span>
            <span class="legend-dot" style="background:#ffd700"></span><span class="legend-txt">pr ready</span>
            <span class="legend-dot" style="background:#ef4444"></span><span class="legend-txt">failed</span>
          </div>
          <button class="forest-toggle" onclick={() => { forestVisible = false; }}>
            HIDE
          </button>
        </div>
        <GitForest />
      </div>
    {/if}
  </div>

  <!-- Show forest button when hidden -->
  {#if !forestVisible}
    <button class="forest-show-btn" onclick={() => { forestVisible = true; }}>
      SHOW GIT FOREST
    </button>
  {/if}

</div>

<style>
  .ops-shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    font-family: var(--la-font-chrome);
    position: relative;
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
    display: flex;
    align-items: center;
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
  .tele-stat   { color: var(--la-text-dim); }
  .tele-active { color: var(--la-agent-researcher); }
  .tele-queue  { color: var(--la-agent-performance); }
  .tele-alert  { color: var(--la-agent-security); }
  .tele-clock  { color: var(--la-text-mute); }
  .tele-div    { color: var(--la-hair-strong); }

  /* ── OPS-1: 2-col layout (55/45 split) ── */
  .ops-main {
    flex: 1;
    display: grid;
    grid-template-columns: minmax(0, 55fr) minmax(0, 45fr);
    min-height: 0;
    overflow: hidden;
  }
  .ops-main[data-forest-hidden] {
    grid-template-columns: 1fr;
  }

  /* ── Mission Control column ── */
  .mission-col {
    overflow-y: auto;
    border-right: 1px solid var(--la-hair-base);
    display: flex;
    flex-direction: column;
    gap: 0;
    scrollbar-width: none;
  }
  .mission-col::-webkit-scrollbar { display: none; }

  .panel-section {
    border-bottom: 1px solid var(--la-hair-base);
    flex-shrink: 0;
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

  /* ── OPS-2: Squad health pills ── */
  .squad-pills {
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .squad-pill {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--la-hair-base);
    cursor: pointer;
    text-align: left;
    transition: background 80ms;
    position: relative;
  }
  .squad-pill:hover { background: rgba(255,255,255,0.03); }

  /* Online: colored border + subtle box-shadow glow */
  .squad-pill[data-state="online"] {
    border-left: 2px solid var(--color);
    box-shadow: inset 3px 0 12px color-mix(in srgb, var(--color) 18%, transparent);
    padding-left: 8px;
  }
  .squad-pill[data-state="offline"] {
    border-left: 2px solid var(--la-semantic-offline, #475569);
    opacity: 0.55;
    padding-left: 8px;
  }
  .squad-pill[data-state="never"] {
    border-left: 2px solid var(--la-semantic-offline, #475569);
    opacity: 0.38;
    font-style: italic;
    padding-left: 8px;
  }

  .pill-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--color);
    flex-shrink: 0;
  }
  .squad-pill[data-state="online"] .pill-dot {
    animation: agent-pulse 2s ease-in-out infinite;
  }
  @keyframes agent-pulse {
    0%, 100% { box-shadow: 0 0 3px var(--color); }
    50%       { box-shadow: 0 0 10px var(--color), 0 0 18px color-mix(in srgb, var(--color) 50%, transparent); }
  }
  .squad-pill[data-state="offline"] .pill-dot,
  .squad-pill[data-state="never"] .pill-dot {
    background: var(--la-semantic-offline, #475569);
  }

  .pill-name {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--color);
    min-width: 38px;
    flex-shrink: 0;
  }
  .squad-pill[data-state="offline"] .pill-name,
  .squad-pill[data-state="never"]   .pill-name {
    color: var(--la-text-mute);
  }
  .pill-seen {
    font-size: 7px;
    color: var(--la-text-dim);
    font-variant-numeric: tabular-nums;
    flex: 1;
  }
  .pill-dc {
    font-size: 6px;
    color: var(--la-agent-engineer, #4d8eff);
    background: rgba(77, 142, 255, 0.12);
    padding: 1px 3px;
    border-radius: 2px;
  }
  .pill-stale {
    width: 4px; height: 4px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .pill-stale.stale { background: var(--la-agent-performance, #ff8e3c); }
  .pill-stale.dead  { background: var(--la-agent-security,    #ff4d4d); }

  .pill-expanded {
    padding: 5px 10px 6px 20px;
    border-left: 2px solid;
    border-bottom: 1px solid var(--la-hair-base);
    background: rgba(255,255,255,0.02);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .pill-exp-row { display: flex; gap: 6px; align-items: center; }
  .exp-label { font-size: 7px; color: var(--la-text-dim); min-width: 28px; }
  .exp-val { font-size: 7px; color: var(--la-text-base); font-variant-numeric: tabular-nums; }
  .hb-age.fresh { color: var(--la-agent-researcher, #4dffe6); }
  .hb-age.stale { color: var(--la-agent-performance, #ff8e3c); }
  .hb-age.dead  { color: var(--la-agent-security,    #ff4d4d); }
  .pill-caps { display: flex; flex-wrap: wrap; gap: 2px; padding-top: 2px; }
  .cap-chip { font-size: 5px; padding: 1px 3px; background: var(--la-bg-elev-2); color: var(--la-text-dim); border-radius: 1px; }

  /* Memory metrics */
  .memory-metrics { padding: 6px 10px; display: flex; flex-direction: column; gap: 3px; }
  .mem-row { display: flex; justify-content: space-between; }
  .mem-label { font-size: 8px; color: var(--la-text-dim); }
  .mem-val { font-size: 8px; color: var(--la-text-base); font-variant-numeric: tabular-nums; }

  /* Events section (fills remaining space) */
  .events-section {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 120px;
    border-bottom: none;
  }
  .events-stream {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  /* ── Git Forest column (OPS-1 right side) ── */
  .forest-col {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    background: #020408;
  }
  .forest-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 10px;
    border-bottom: 1px solid var(--la-hair-strong);
    background: var(--la-bg-elev-1, #111214);
    flex-shrink: 0;
  }
  .forest-legend {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-left: 8px;
    flex: 1;
  }
  .legend-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .legend-txt { font-size: 7px; color: var(--la-text-dim); margin-right: 4px; }
  .forest-toggle {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-dim);
    background: none;
    border: 1px solid var(--la-hair-strong);
    padding: 2px 6px;
    cursor: pointer;
    transition: color 80ms, border-color 80ms;
  }
  .forest-toggle:hover { color: var(--la-text-base); border-color: var(--la-text-mute); }

  /* Show forest floating button (when hidden) */
  .forest-show-btn {
    position: absolute;
    bottom: 12px;
    right: 14px;
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--la-focus-ring, #0ea5e9);
    background: rgba(14, 165, 233, 0.08);
    border: 1px solid var(--la-focus-ring, #0ea5e9);
    padding: 4px 10px;
    cursor: pointer;
    transition: background 80ms;
  }
  .forest-show-btn:hover { background: rgba(14, 165, 233, 0.16); }
</style>
