<script lang="ts">
  import {
    siblingHealth, ayinStatus, authStatus, platformHealth,
    conductorStats, conductorTasks, alertStats, buildStats,
    projectGroups, activityFeed, logEntries,
    mailboxUnread,
  } from '$lib/stores';
  import { STATUS_COLORS } from '$lib/design-tokens';
  import GitForest2D from '$lib/../components/topology/GitForest2D.svelte';
  import WorktreePanel from '$lib/../components/WorktreePanel.svelte';
  import AgentCommsPanel from '$lib/../components/AgentCommsPanel.svelte';
  import TokenVault  from '$lib/../components/TokenVault.svelte';
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
  import SharedSlotBar from '$lib/../components/SharedSlotBar.svelte';

  // ── Reactive data ─────────────────────────────────────────────────────────

  let health        = $derived($siblingHealth);
  let status        = $derived($ayinStatus);
  // P0-1: surface auth failures distinctly from genuine network outages
  let displayStatus = $derived(
    status === 'offline' && $authStatus !== 'ok'
      ? ($authStatus === 'forbidden' ? 'forbidden' : 'auth fail')
      : status,
  );
  let ph            = $derived($platformHealth);
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

  let wallClock = $derived.by(() => {
    const d = new Date(now);
    const hh = d.getHours()  .toString().padStart(2, '0');
    const mm = d.getMinutes().toString().padStart(2, '0');
    const ss = d.getSeconds().toString().padStart(2, '0');
    return `${hh}:${mm}:${ss}`;
  });

  // Tutorial hook
  $effect(() => {
    import('$lib/tutorial').then(m => m.runTutorial('t5')).catch(() => {});
  });

  let vaultOpen = $state(false);

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
    ops:        'Dashboard — git branches | agent console | build status',
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

  let projectCount  = $derived($projectGroups.length);
  let onlineCount   = $derived(
    Object.values($siblingHealth).filter(h => h?.status === 'online').length,
  );
  let pendingTasks  = $derived($conductorTasks.filter(t => t.status === 'pending'));

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
    <span class="ops-tab active">Dashboard</span>
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
  <!-- Monitor layout: 3-column (agent comms | gitforest | worktrees) -->
  <div class="ops-main">

    <!-- LEFT — Structured agent comms (~22%) -->
    <div class="comms-col">
      <div class="panel-head">
        <span class="panel-label">AGENT COMMS</span>
        <span class="panel-count">{eventRows.length}</span>
      </div>
      <AgentCommsPanel rows={eventRows} maxDisplay={MAX_ROWS} />
    </div>

    <!-- CENTER — 2D Git Forest (~50%) -->
    <div class="forest-col">
      <div class="forest-header">
        <span class="panel-label">GIT FOREST</span>
        <SharedSlotBar />
        <div class="forest-legend">
          <span class="legend-dot" style="background:#f5a623"></span><span class="legend-txt">active</span>
          <span class="legend-dot" style="background:#22c55e"></span><span class="legend-txt">merged</span>
          <span class="legend-dot" style="background:#1e3a52"></span><span class="legend-txt">idle</span>
        </div>
        <button class="forest-connect" onclick={() => { vaultOpen = true; }}>CONNECT</button>
      </div>

      <!-- Conductor queue strip — only visible when builds are queued -->
      {#if pendingTasks.length > 0}
        <div class="queue-strip" aria-label="Queued builds">
          <span class="queue-label">QUEUE</span>
          {#each pendingTasks.slice(0, 5) as task}
            <span class="queue-chip queue-chip--{task.priority}" title="{task.buildId} · {task.taskType}">
              <span class="queue-chip-sib">{task.sibling.toUpperCase()}</span>
              <span class="queue-chip-name">{task.buildId.replace(/^feat\//, '').slice(0, 18)}</span>
              <span class="queue-chip-type">{task.taskType}</span>
            </span>
          {/each}
          {#if pendingTasks.length > 5}
            <span class="queue-more">+{pendingTasks.length - 5}</span>
          {/if}
        </div>
      {/if}

      <div class="forest-body">
        <GitForest2D />
      </div>
    </div>

    <!-- RIGHT — Worktree hierarchy (~28%) -->
    <div class="worktree-col">
      <div class="panel-head">
        <span class="panel-label">WORKTREES</span>
      </div>
      <div class="worktree-body">
        <WorktreePanel />
      </div>
    </div>

  </div>

  <TokenVault bind:open={vaultOpen} />
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

  /* ── Monitor: 3-col layout (comms | forest | worktrees) ── */
  .ops-main {
    flex: 1;
    display: grid;
    grid-template-columns: minmax(0, 22fr) minmax(0, 50fr) minmax(0, 28fr);
    min-height: 0;
    overflow: hidden;
  }

  /* ── Agent comms column ── */
  .comms-col {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--la-hair-base);
    overflow: hidden;
  }

  /* ── Conductor queue strip ── */
  .queue-strip {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 10px;
    background: rgba(245, 158, 11, 0.05);
    border-bottom: 1px solid rgba(245, 158, 11, 0.2);
    flex-shrink: 0;
    overflow-x: auto;
    scrollbar-width: none;
    font-family: var(--la-font-chrome, 'JetBrains Mono', monospace);
  }
  .queue-strip::-webkit-scrollbar { display: none; }
  .queue-label {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: #f59e0b;
    flex-shrink: 0;
  }
  .queue-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 7px;
    border-radius: 3px;
    padding: 2px 6px;
    border: 1px solid;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .queue-chip--high   { border-color: rgba(248,113,113,0.4); background: rgba(248,113,113,0.07); color: #f87171; }
  .queue-chip--normal { border-color: rgba(245,158,11,0.35); background: rgba(245,158,11,0.06); color: #f59e0b; }
  .queue-chip--low    { border-color: var(--la-hair-base); background: transparent; color: var(--la-text-mute); }
  .queue-chip-sib  { font-weight: 700; opacity: 0.8; }
  .queue-chip-name { font-weight: 600; }
  .queue-chip-type { opacity: 0.6; }
  .queue-more {
    font-size: 7px;
    color: var(--la-text-mute);
    flex-shrink: 0;
  }

  /* ── Worktree column ── */
  .worktree-col {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }
  .worktree-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
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

  /* ── Git Forest column ── */
  .forest-col {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    background: #020408;
    border-right: 1px solid var(--la-hair-base);
  }
  .forest-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
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
    margin-left: 4px;
    flex: 1;
  }
  .legend-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .legend-txt { font-size: 7px; color: var(--la-text-dim); margin-right: 4px; }
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
