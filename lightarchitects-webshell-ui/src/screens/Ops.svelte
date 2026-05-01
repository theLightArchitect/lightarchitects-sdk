<script lang="ts">
  import {
    siblingHealth, ayinStatus, platformHealth,
    conductorStats, arenaStats, alertStats, buildStats,
    siblingDispatchCounts, logEntries,
  } from '$lib/stores';
  import { SIBLING_COLORS, STATUS_COLORS, SIBLINGS } from '$lib/design-tokens';
  import type { SiblingId } from '$lib/types';
  import BuildPortfolio from '$lib/../components/BuildPortfolio.svelte';
  import ConductorPanel from '$lib/../components/ConductorPanel.svelte';
  import ArenaPanel from '$lib/../components/ArenaPanel.svelte';
  import AlertPanel from '$lib/../components/AlertPanel.svelte';
  import CompactionPanel from '$lib/../components/CompactionPanel.svelte';
  import LogStream from '$lib/../components/LogStream.svelte';

  type OpsTab = 'health' | 'trace';
  let tab = $state<OpsTab>('health');

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

  let expanded = $state<Record<string, boolean>>({});
  function toggle(sib: string) { expanded[sib] = !expanded[sib]; }

  $effect(() => {
    import('$lib/tutorial').then(m => m.runTutorial('t5')).catch(() => {});
  });
</script>

<div class="ops-shell">
  <div class="ops-tab-bar">
    <button
      class="ops-tab"
      class:active={tab === 'health'}
      onclick={() => { tab = 'health'; }}
    >
      SQUAD HEALTH
    </button>
    <button
      class="ops-tab"
      class:active={tab === 'trace'}
      onclick={() => { tab = 'trace'; }}
    >
      LIVE TRACE
    </button>

    <div class="ops-header-tele">
      <span style="color: {phColor}">● {ph.toUpperCase()}</span>
      <span style="color: {stColor}">{status}</span>
      <span class="tele-stat tele-active">{$buildStats.inProgress} active</span>
      <span class="tele-stat tele-queue">{$conductorStats.queueDepth} queued</span>
      <span class="tele-stat tele-alert">{$alertStats.unacknowledged} alerts</span>
    </div>
  </div>

  <div class="ops-content">
    {#if tab === 'health'}
      <div class="health-layout">
        <!-- Left column -->
        <div class="health-col-main">
          <BuildPortfolio />

          <!-- Squad health cards -->
          <div class="squad-health-panel" data-onboarding="sitrep-squad-health">
            <div class="panel-head">
              <span class="panel-label">SQUAD HEALTH</span>
              <span class="panel-count">
                {Object.values(health).filter(h => h?.status === 'online').length}/7 agents online
              </span>
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
                    aria-expanded={expanded[sib]}
                    aria-label="Toggle {sib} details"
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
                      <span class="dispatch-badge">{dc} active</span>
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
        </div>

        <!-- Right column -->
        <div class="health-col-side">
          <ConductorPanel />
          <ArenaPanel />
          <AlertPanel />
          <CompactionPanel />
        </div>
      </div>
    {:else}
      <div class="trace-layout">
        <LogStream entries={$logEntries} autoScroll={true} />
      </div>
    {/if}
  </div>
</div>

<style>
  .ops-shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    font-family: var(--la-font-chrome);
  }

  .ops-tab-bar {
    display: flex;
    align-items: center;
    gap: 0;
    border-bottom: 1px solid var(--la-hair-strong);
    flex-shrink: 0;
    background: var(--la-bg-base);
  }

  .ops-tab {
    padding: 0 20px;
    height: 36px;
    font-family: inherit;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--la-text-mute);
    cursor: pointer;
    transition: color 80ms, border-color 80ms;
    flex-shrink: 0;
  }
  .ops-tab:hover { color: var(--la-text-base); }
  .ops-tab.active {
    color: var(--la-text-stark);
    border-bottom-color: var(--la-focus-ring);
  }

  .ops-header-tele {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 0 16px;
    font-size: 9px;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.06em;
  }
  .tele-stat { color: var(--la-text-dim); }
  .tele-active { color: var(--la-agent-researcher); }
  .tele-queue  { color: var(--la-agent-performance); }
  .tele-alert  { color: var(--la-agent-security); }

  .ops-content {
    flex: 1;
    overflow: hidden;
    min-height: 0;
  }

  /* ── health layout ── */
  .health-layout {
    display: grid;
    grid-template-columns: 1fr 280px;
    height: 100%;
    overflow: hidden;
  }
  .health-col-main {
    overflow-y: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    border-right: 1px solid var(--la-hair-base);
  }
  .health-col-side {
    overflow-y: auto;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  /* ── squad health panel ── */
  .squad-health-panel {
    border: 1px solid var(--la-hair-strong);
    overflow: hidden;
  }
  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 12px;
    border-bottom: 1px solid var(--la-hair-strong);
    background: var(--la-bg-elev-1, #111214);
  }
  .panel-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.14em;
    color: var(--la-text-mute);
    text-transform: uppercase;
  }
  .panel-count {
    font-size: 9px;
    color: var(--la-text-dim);
  }
  .squad-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 1px;
    background: var(--la-hair-base);
  }
  .agent-card {
    background: var(--la-bg-frame);
    overflow: hidden;
  }
  .agent-card-btn {
    width: 100%;
    padding: 8px 6px;
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    transition: background 80ms;
  }
  .agent-card-btn:hover { background: var(--la-bg-elev-1, #111214); }
  .agent-card-top {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .agent-pip {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .agent-name {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
  }
  .agent-status {
    font-size: 7px;
    color: var(--la-text-mute);
    text-transform: capitalize;
  }
  .agent-uptime {
    font-size: 7px;
    color: var(--la-text-dim);
    font-variant-numeric: tabular-nums;
  }
  .stale-badge {
    font-size: 7px;
    padding: 1px 4px;
  }
  .stale-badge.stale { color: var(--la-agent-performance); }
  .stale-badge.dead  { color: var(--la-agent-security); }
  .dispatch-badge {
    font-size: 7px;
    color: var(--la-agent-engineer);
    padding: 1px 4px;
  }
  .agent-expanded {
    padding: 4px 6px 6px;
    border-top: 1px solid var(--la-hair-base);
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .agent-hb { font-size: 7px; color: var(--la-text-dim); }
  .hb-age.fresh { color: var(--la-agent-researcher); }
  .hb-age.stale { color: var(--la-agent-performance); }
  .hb-age.dead  { color: var(--la-agent-security); }
  .agent-caps { display: flex; flex-wrap: wrap; gap: 2px; }
  .cap-chip {
    font-size: 6px;
    padding: 1px 3px;
    background: var(--la-bg-elev-2);
    color: var(--la-text-dim);
  }

  /* ── trace layout ── */
  .trace-layout {
    height: 100%;
    overflow: hidden;
  }
</style>
