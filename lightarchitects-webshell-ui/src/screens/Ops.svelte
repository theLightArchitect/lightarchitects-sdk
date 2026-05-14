<script lang="ts">
  import {
    siblingHealth, ayinStatus, authStatus, platformHealth,
    conductorStats, alertStats, buildStats,
    siblingDispatchCounts, projectGroups, activityFeed, logEntries,
    helixEntries, vaultCounts, mailboxUnread,
    activeBuild,
  } from '$lib/stores';
  import type { PillarGate } from '$lib/types';
  import { SIBLING_COLORS, STATUS_COLORS, SIBLINGS } from '$lib/design-tokens';
  import type { SiblingId } from '$lib/types';
  import ConductorPanel from '$lib/../components/ConductorPanel.svelte';
  import AlertPanel from '$lib/../components/AlertPanel.svelte';
  import CompactionPanel from '$lib/../components/CompactionPanel.svelte';
  import GitForest   from '$lib/../components/topology/GitForest.svelte';
  import TokenVault  from '$lib/../components/TokenVault.svelte';
  import EventStream from '$lib/../components/EventStream.svelte';
  import { logLevelToSeverity } from '$lib/../components/EventStream.svelte';
  import { sourceColor } from '$lib/atmosphere';
  import type { StreamRow } from '$lib/../components/EventStream.svelte';
  import PanelRoot from '$lib/../components/panels/PanelRoot.svelte';
  import PanelCatalog from '$lib/../components/panels/PanelCatalog.svelte';
  import PolytopeIcon from '$lib/../components/PolytopeIcon.svelte';
  import { activePreset, applyPreset, customPresets, deleteCustomPreset, applyCustomPreset, editMode } from '$lib/layout';
  import type { LayoutPreset } from '$lib/types';
  import type { Polytope4DType } from '$lib/polytopes4d-canvas2d';
  import { registerHotkey } from '$lib/hotkeyRegistry';

  // ── Reactive data ─────────────────────────────────────────────────────────

  /** P1-6: 10 gatekeeper gate labels → pillar indices for the active build. */
  const GATE_LABELS: Array<{ key: string; short: string }> = [
    { key: 'A', short: 'A' }, { key: 'S', short: 'S' }, { key: 'Q', short: 'Q' },
    { key: 'C', short: 'C' }, { key: 'O', short: 'O' }, { key: 'P', short: 'P' },
    { key: 'K', short: 'K' }, { key: 'D', short: 'D' }, { key: 'T', short: 'T' },
    { key: 'R', short: 'R' },
  ];

  /** Map LASDLC gate key to pillar status for the active build's pillar array. */
  function gateColor(gate: string, pillars: PillarGate[]): string {
    // Map A→ARCH, S→SEC, Q→QUAL, O→OPS, P→PERF, T→TEST, D→DOC
    const GATE_TO_PILLAR: Record<string, string> = {
      A: 'ARCH', S: 'SEC', Q: 'QUAL', O: 'OPS', P: 'PERF', T: 'TEST', D: 'DOC',
    };
    const pillar = pillars.find(p => p.pillar === GATE_TO_PILLAR[gate]);
    if (!pillar) return 'var(--la-text-mute)';
    switch (pillar.status) {
      case 'passed':      return 'var(--la-semantic-ok)';
      case 'failed':      return 'var(--la-semantic-error)';
      case 'in_progress': return 'var(--la-semantic-warn)';
      case 'blocked':     return 'var(--la-semantic-error)';
      default:            return 'var(--la-text-mute)';
    }
  }

  let build = $derived($activeBuild);
  let health        = $derived($siblingHealth);
  let status        = $derived($ayinStatus);
  // P0-1: surface auth failures distinctly from genuine network outages
  let displayStatus = $derived(
    status === 'offline' && $authStatus !== 'ok'
      ? ($authStatus === 'forbidden' ? 'forbidden' : 'auth fail')
      : status,
  );
  let ph            = $derived($platformHealth);
  let dispatchCounts = $derived($siblingDispatchCounts);
  let phColor       = $derived(
    ph === 'healthy'  ? 'var(--la-agent-researcher)'  :
    ph === 'degraded' ? 'var(--la-agent-performance)' :
                        'var(--la-agent-security)',
  );
  let stColor = $derived(
    displayStatus === 'auth fail' || displayStatus === 'forbidden'
      ? 'var(--la-agent-performance, #f59e0b)'
      : (STATUS_COLORS[status as keyof typeof STATUS_COLORS] ?? 'var(--la-text-mute)'),
  );

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

  // ── Git forest toggle + vault (OPS-1) ────────────────────────────────────

  let forestVisible = $state(true);
  let vaultOpen     = $state(false);

  // ── Mosaic layout mode ────────────────────────────────────────────────────
  let mosaicMode = $state(
    (() => { try { return localStorage.getItem('la_mosaic_mode') === 'true'; } catch { return false; } })()
  );
  let currentPreset = $derived(mosaicMode ? $activePreset : 'ops');

  $effect(() => {
    try { localStorage.setItem('la_mosaic_mode', String(mosaicMode)); } catch { /* ignore */ }
  });

  const PRESET_POLYTOPES: Record<LayoutPreset, Polytope4DType> = {
    ops:        'tesseract',
    ide:        'pentachoron',
    debug:      'hexadecachoron',
    'pr-review': 'icositetrachoron',
    focus:      'duoprism55',
    observe:    'hexacosichoron',
  };

  const PRESET_SHORT: Record<LayoutPreset, string> = {
    ops:        'MONITOR',
    ide:        'WORKSPACE',
    debug:      'DEBUG',
    'pr-review': 'SHIP',
    focus:      'AGENT',
    observe:    'OBSERVE',
  };

  const PRESET_TOOLTIP: Record<LayoutPreset, string> = {
    ops:        'Monitor — git branches | agent console | build status',
    ide:        'Workspace — file explorer | diff viewer | terminal to run tests',
    debug:      'Debug — agent console | findings | terminal to reproduce and fix',
    'pr-review': 'Ship — diff what changed | terminal to push | build status confirm',
    focus:      'Agent — full-screen console, watch a single run live',
    observe:    'Observe — agent console | live AYIN trace dataflow diagrams',
  };

  const PRESET_TITLES: Record<LayoutPreset, string> = {
    ops:        'Monitor layout   Ctrl+Shift+1',
    ide:        'Workspace layout Ctrl+Shift+2',
    debug:      'Debug layout     Ctrl+Shift+3',
    'pr-review': 'Ship layout      Ctrl+Shift+4',
    focus:      'Agent layout     Ctrl+Shift+5',
    observe:    'Observe layout   Ctrl+Shift+6',
  };

  $effect(() => {
    const presets: Array<[string, string, LayoutPreset, boolean]> = [
      ['layout-ops',       '1', 'ops',       false],
      ['layout-ide',       '2', 'ide',       true],
      ['layout-debug',     '3', 'debug',     true],
      ['layout-pr-review', '4', 'pr-review', true],
      ['layout-focus',     '5', 'focus',     true],
      ['layout-observe',   '6', 'observe',   true],
    ];
    const unreg = presets.map(([id, num, preset, mosaic]) =>
      registerHotkey({
        id,
        keys: [`Ctrl`, `Shift`, num],
        label: `Layout: ${preset}`,
        group: 'Mosaic',
        scope: 'global',
        matches: e => e.ctrlKey && e.shiftKey && e.key === num,
        handler: () => { applyPreset(preset); mosaicMode = mosaic; },
      })
    );
    return () => unreg.forEach(fn => fn());
  });

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
      <span class="tele-stat" style="color:{stColor}">{displayStatus}</span>
      <span class="tele-div">·</span>
      <span class="tele-stat">{projectCount} PROJECTS</span>
      <span class="tele-stat tele-active">{$buildStats.inProgress} RUNNING</span>
      <span class="tele-stat">{onlineCount}/7 ONLINE</span>
      <span class="tele-div">·</span>
      <span class="tele-stat tele-queue">{$conductorStats.queueDepth} QUEUED</span>
      <span class="tele-stat tele-alert">{$alertStats.unacknowledged} ALERTS</span>
      {#if $mailboxUnread > 0}
        <span class="tele-stat tele-mailbox">{$mailboxUnread} MAILBOX</span>
      {/if}
      <span class="tele-div">·</span>
      <span class="tele-stat tele-clock">{wallClock}</span>
    </div>

    <!-- Preset switcher — right-anchored in telemetry bar -->
    <div class="preset-switcher" role="toolbar" aria-label="Layout presets">
      <span class="preset-label">PRESETS</span>

      {#each Object.entries(PRESET_POLYTOPES) as [preset, polyType]}
        {@const isActive = currentPreset === preset}
        <button
          class="preset-btn"
          class:active={isActive}
          onclick={() => {
            applyPreset(preset as LayoutPreset);
            mosaicMode = preset !== 'ops';
          }}
          data-tooltip={PRESET_TOOLTIP[preset as LayoutPreset]}
          aria-label={PRESET_TITLES[preset as LayoutPreset]}
          data-testid="preset-btn-{preset}"
        >
          <span class="preset-polytope" class:preset-polytope-active={isActive}>
            <PolytopeIcon type={polyType} color={isActive ? '#00c8ff' : '#3a4450'} size={18} />
          </span>
          <span class="preset-short">{PRESET_SHORT[preset as LayoutPreset]}</span>
        </button>
      {/each}

      <!-- Custom preset tabs -->
      {#each $customPresets as cp}
        <div class="custom-preset-wrapper">
          <button
            class="preset-btn"
            onclick={() => { applyCustomPreset(cp); mosaicMode = true; }}
            data-tooltip="Custom — {cp.name}"
            aria-label="Apply custom preset: {cp.name}"
          >
            <span class="preset-polytope">
              <PolytopeIcon type="doubleHelix4D" color="#a78bfa" size={18} />
            </span>
            <span class="preset-short custom">{cp.name.toUpperCase().slice(0, 7)}</span>
          </button>
          <button
            class="custom-delete"
            onclick={() => deleteCustomPreset(cp.id)}
            aria-label="Delete preset {cp.name}"
            title="Delete"
          >×</button>
        </div>
      {/each}

      <!-- EDIT toggle -->
      <button
        class="edit-btn"
        class:active={$editMode}
        onclick={() => { const next = !$editMode; editMode.set(next); if (next) mosaicMode = true; }}
        title={$editMode ? 'Close panel editor' : 'Edit layout — add or remove panels'}
        aria-label={$editMode ? 'Close panel editor' : 'Edit layout'}
        data-testid="edit-mode-btn"
      >EDIT</button>
    </div>
  </div>

  <!-- Mosaic layout (PanelRoot) — replaces legacy grid when mosaicMode is on -->
  {#if mosaicMode}
    <div class="ops-mosaic" data-testid="mosaic-container">
      <PanelRoot />
      {#if $editMode}
        <div class="catalog-overlay" data-testid="catalog-overlay">
          <PanelCatalog onClose={() => editMode.set(false)} />
        </div>
      {/if}
    </div>

  {:else}
  <!-- OPS-1: 2-column hero layout (55% mission-control / 45% git forest) -->
  <div class="ops-main" data-forest-hidden={!forestVisible || undefined}>

    <!-- LEFT — Mission Control (hero at 55%) -->
    <div class="mission-col">

      <!-- P1-6: 10-column gatekeeper gate row for active build -->
      {#if build}
        <div class="panel-section gate-row-section">
          <div class="panel-head">
            <span class="panel-label">GATEKEEPER — {build.name.slice(0, 24)}</span>
          </div>
          <div class="gate-row" role="row" aria-label="LASDLC gatekeeper status">
            {#each GATE_LABELS as gate}
              <span
                class="gate-cell"
                style="color: {gateColor(gate.key, build.pillars)}"
                title="Gate [{gate.key}]"
                role="cell"
              >[{gate.short}]</span>
            {/each}
          </div>
        </div>
      {/if}

      <!-- OPS-2: Squad health pills with color glow + heartbeat -->
      <div class="panel-section" data-onboarding="sitrep-squad-health">
        <div class="panel-head">
          <span class="panel-label">SQUAD HEALTH</span>
          <span class="panel-count">{onlineCount}/7 agents online</span>
        </div>
        <div class="squad-pills">
          {#each SIBLINGS as sib}
            {@const h = health[sib as SiblingId]}
            {@const color = SIBLING_COLORS[sib] ?? 'var(--la-text-mute)'}
            {@const stale = staleness(h?.lastHeartbeat)}
            {@const dc = dispatchCounts[sib as SiblingId] ?? 0}
            {@const pillState = h?.status === 'online' ? 'online' : h?.status === 'offline' || h?.status === 'degraded' ? 'offline' : h?.status === 'unconfigured' ? 'unconfigured' : 'never'}
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
              {#if stale !== 'fresh' && pillState !== 'never' && pillState !== 'unconfigured'}
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
          <button class="forest-connect" onclick={() => { vaultOpen = true; }}>
            CONNECT
          </button>
          <button class="forest-toggle" onclick={() => { forestVisible = false; }}>
            HIDE
          </button>
        </div>
        <GitForest />
      </div>
    {/if}
  </div>

  <TokenVault bind:open={vaultOpen} />

  <!-- Show forest button when hidden -->
  {#if !forestVisible}
    <button class="forest-show-btn" onclick={() => { forestVisible = true; }}>
      SHOW GIT FOREST
    </button>
  {/if}
  {/if}<!-- /mosaicMode -->

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
  .tele-alert   { color: var(--la-agent-security); }
  .tele-mailbox { color: var(--la-struct-primary, #00c8ff); }
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

  /* P1-6: gatekeeper row */
  .gate-row {
    display: flex;
    gap: 0;
    padding: 4px 10px;
    font-family: var(--la-font-mono);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  .gate-cell {
    flex: 1;
    text-align: center;
    transition: color 200ms ease;
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
    color: var(--la-text-dim);
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
    /* amber: was running, now crashed */
    border-left: 2px solid var(--la-agent-performance, #f59e0b);
    opacity: 0.72;
    padding-left: 8px;
  }
  .squad-pill[data-state="unconfigured"] {
    /* dim: never set up — no alarm, no color */
    border-left: 2px solid var(--la-hair-base, #334155);
    opacity: 0.28;
    font-style: italic;
    padding-left: 8px;
  }
  .squad-pill[data-state="never"] {
    border-left: 2px solid var(--la-hair-base, #334155);
    opacity: 0.32;
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
  .squad-pill[data-state="offline"] .pill-dot {
    background: var(--la-agent-performance, #f59e0b);
  }
  .squad-pill[data-state="unconfigured"] .pill-dot,
  .squad-pill[data-state="never"] .pill-dot {
    background: var(--la-hair-base, #334155);
  }

  .pill-name {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--color);
    min-width: 38px;
    flex-shrink: 0;
  }
  .squad-pill[data-state="offline"] .pill-name {
    color: color-mix(in srgb, var(--la-agent-performance, #f59e0b) 70%, var(--la-text-mute));
  }
  .squad-pill[data-state="never"] .pill-name,
  .squad-pill[data-state="unconfigured"] .pill-name {
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
  .forest-connect {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-struct-primary);
    background: rgba(0, 200, 255, 0.06);
    border: 1px solid rgba(0, 200, 255, 0.3);
    padding: 2px 7px;
    cursor: pointer;
    transition: background var(--la-t-snap), box-shadow var(--la-t-snap);
  }
  .forest-connect:hover {
    background: rgba(0, 200, 255, 0.14);
    box-shadow: 0 0 0 1px rgba(0, 200, 255, 0.2);
  }

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

  /* ── Mosaic layout container ── */
  .ops-mosaic {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    position: relative; /* required for .catalog-overlay absolute positioning */
  }

  /* ── Preset switcher (right-anchored in telemetry bar) ── */
  .preset-switcher {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 8px 0 8px;
    flex-shrink: 0;
    border-left: 1px solid var(--la-hair-base);
  }

  .preset-label {
    font-size: 8px;
    font-family: var(--la-font-mono, 'JetBrains Mono', monospace);
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--la-text-mute);
    padding-right: 6px;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .preset-btn {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1px;
    font-family: var(--la-font-mono, 'JetBrains Mono', monospace);
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    padding: 2px 6px 0;
    height: 100%;
    cursor: pointer;
    transition: border-bottom-color 120ms;
    white-space: nowrap;
  }

  .preset-polytope {
    display: block;
    opacity: 0.45;
    transition: opacity 120ms, filter 120ms;
  }
  .preset-btn:hover .preset-polytope {
    opacity: 0.75;
  }
  .preset-polytope-active {
    opacity: 1;
    filter: drop-shadow(0 0 4px rgba(0, 200, 255, 0.6));
    animation: preset-pulse 2.8s ease-in-out infinite;
  }
  @keyframes preset-pulse {
    0%, 100% { filter: drop-shadow(0 0 3px rgba(0, 200, 255, 0.5)); }
    50%       { filter: drop-shadow(0 0 8px rgba(0, 200, 255, 0.9)) drop-shadow(0 0 16px rgba(0, 200, 255, 0.3)); }
  }

  .preset-short {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-mute);
    transition: color 120ms;
  }
  .preset-btn:hover .preset-short { color: var(--la-text-dim); }
  .preset-btn.active .preset-short {
    color: var(--la-struct-primary);
  }
  .preset-btn.active {
    border-bottom-color: var(--la-struct-primary);
  }

  /* ── EDIT button ──────────────────────────────────────────────────────────── */
  .edit-btn {
    font-size: 8px;
    font-family: var(--la-font-mono, 'JetBrains Mono', monospace);
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--la-text-mute);
    background: none;
    border: 1px solid var(--la-hair-base);
    padding: 2px 7px;
    height: 20px;
    cursor: pointer;
    transition: color 120ms, border-color 120ms;
    margin-left: 6px;
    flex-shrink: 0;
  }
  .edit-btn:hover { color: var(--la-text-dim); border-color: var(--la-hair-strong); }
  .edit-btn.active {
    color: var(--la-struct-primary);
    border-color: var(--la-struct-primary);
    box-shadow: 0 0 6px rgba(0, 200, 255, 0.25);
  }

  /* ── Custom preset tab ────────────────────────────────────────────────────── */
  .custom-preset-wrapper {
    position: relative;
    display: flex;
    align-items: center;
  }
  .preset-short.custom { color: #a78bfa; }
  .preset-btn.active .preset-short.custom { color: #a78bfa; }
  .custom-delete {
    position: absolute;
    top: 2px;
    right: -2px;
    background: none;
    border: none;
    color: var(--la-text-mute);
    font-size: 10px;
    line-height: 1;
    cursor: pointer;
    padding: 1px 2px;
    opacity: 0;
    transition: opacity 80ms, color 80ms;
    z-index: 1;
  }
  .custom-preset-wrapper:hover .custom-delete { opacity: 1; }
  .custom-delete:hover { color: var(--la-semantic-error); }

  /* ── Panel catalog overlay ────────────────────────────────────────────────── */
  .catalog-overlay {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: var(--z-panel);
    display: flex;
    flex-direction: column;
  }

  /* Custom tooltip — immediate, no OS delay, styled with design tokens */
  .preset-btn[data-tooltip] {
    position: relative;
  }
  .preset-btn[data-tooltip]::after {
    content: attr(data-tooltip);
    position: absolute;
    bottom: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
    background: var(--la-bg-elevated, #1a2030);
    border: 1px solid var(--la-hair-strong);
    color: var(--la-text-bright);
    font-size: 9px;
    font-family: var(--la-font-mono, 'JetBrains Mono', monospace);
    letter-spacing: 0.04em;
    white-space: nowrap;
    padding: 4px 8px;
    pointer-events: none;
    z-index: var(--z-tooltip);
    opacity: 0;
    transition: opacity 80ms ease-out;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.5);
  }
  .preset-btn[data-tooltip]:hover::after {
    opacity: 1;
  }
</style>
