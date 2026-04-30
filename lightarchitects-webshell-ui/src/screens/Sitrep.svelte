<script lang="ts">
  import {
    siblingHealth, builds, ayinStatus, platformHealth,
    conductorStats, arenaStats, alertStats, buildStats,
    siblingDispatchCounts,
  } from '$lib/stores';
  import { SIBLING_COLORS, STATUS_COLORS, SIBLINGS } from '$lib/design-tokens';
  import type { SiblingId } from '$lib/types';
  import PolytopeDecor from '$lib/../components/PolytopeDecor.svelte';
  import BuildPortfolio from '$lib/../components/BuildPortfolio.svelte';
  import ConductorPanel from '$lib/../components/ConductorPanel.svelte';
  import ArenaPanel from '$lib/../components/ArenaPanel.svelte';
  import AlertPanel from '$lib/../components/AlertPanel.svelte';
  import CompactionPanel from '$lib/../components/CompactionPanel.svelte';

  let health = $derived($siblingHealth);
  let status = $derived($ayinStatus);
  let dispatchCounts = $derived($siblingDispatchCounts);
  let ph = $derived($platformHealth);
  let phColor = $derived(ph === 'healthy' ? '#22c55e' : ph === 'degraded' ? '#f59e0b' : '#ef4444');
  let stColor = $derived(STATUS_COLORS[status] ?? '#6b7280');

  // Navigate to build
  function openBuild(buildId: string) {
    window.location.hash = `/workspace/${buildId}`;
  }

  // Acknowledge alert (store update)
  function acknowledgeAlert(alertId: string) {
    // Would call API in production; for now, update store
  }

  function formatUptime(seconds: number): string {
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
    if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`;
    return `${Math.floor(seconds / 86400)}d`;
  }
</script>

<div class="h-full flex flex-col relative overflow-hidden">
  <!-- Ambient polytope decoration -->
  <div class="absolute inset-0 overflow-hidden pointer-events-none -z-10">
    <div class="absolute -top-20 -right-20">
      <PolytopeDecor type="icositetrachoron" color="#FFD700" size={400} opacity={0.03} speed={0.05} />
    </div>
    <div class="absolute -bottom-20 -left-20">
      <PolytopeDecor type="hexadecachoron" color="#00BFFF" size={300} opacity={0.04} speed={0.08} />
    </div>
  </div>

  <!-- Header (#38 — fixed 56px band shared across all top-level screens) -->
  <header class="la-screen-header flex items-center justify-between px-6 border-b border-[#1e293b]">
    <div class="flex items-center gap-3">
      <h1 class="text-lg font-semibold tracking-wide">SITREP</h1>
      <span class="text-xs text-[#64748b]">Platform Situation Report</span>
    </div>
    <div class="flex items-center gap-4">
      <!-- Platform health indicator -->
      <div class="flex items-center gap-2">
        <span class="text-[10px] text-[#64748b]">Platform:</span>
        <div class="flex items-center gap-1.5">
          <div
            class="w-2 h-2 rounded-full animate-pulse"
            style="background-color: {phColor}; box-shadow: 0 0 6px {phColor}"
          ></div>
          <span class="text-[10px]" style="color: {phColor}">{ph.toUpperCase()}</span>
        </div>
      </div>

      <!-- Connection status -->
      <div class="flex items-center gap-1.5">
        <div
          class="w-2 h-2 rounded-full"
          style="background-color: {stColor}; box-shadow: 0 0 4px {stColor}"
        ></div>
        <span class="text-[10px] text-[#64748b]">{status}</span>
      </div>

      <!-- Stats summary -->
      <div class="flex items-center gap-3 text-[10px]">
        <span class="text-[#3b82f6]">{$buildStats.inProgress} active</span>
        <span class="text-[#f59e0b]">{$conductorStats.queueDepth} queued</span>
        <span class="text-[#ef4444]">{$alertStats.unacknowledged} alerts</span>
      </div>
    </div>
  </header>

  <!-- Main content grid -->
  <div class="flex-1 overflow-y-auto p-6">
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">

      <!-- Left column: Build Portfolio + Sibling Health -->
      <div class="lg:col-span-2 space-y-6">
        <!-- Build Portfolio -->
        <BuildPortfolio onBuildClick={openBuild} />

        <!-- Sibling Health Cards (7 siblings) -->
        <div class="bg-[#111827] border border-[#1e293b] rounded-lg overflow-hidden">
          <div class="px-4 py-2 border-b border-[#1e293b] flex items-center justify-between">
            <h3 class="text-xs font-medium text-[#64748b]">SQUAD HEALTH</h3>
            <span class="text-[10px] text-[#6b7280]">
              {Object.values(health).filter(h => h.status === 'online').length}/7 online
            </span>
          </div>
          <div class="p-3 grid grid-cols-2 md:grid-cols-4 lg:grid-cols-7 gap-2">
            {#each SIBLINGS as sib}
              {@const h = health[sib as SiblingId]}
              {@const color = SIBLING_COLORS[sib] ?? '#6b7280'}
              {@const statusColor = h?.status ? STATUS_COLORS[h.status] : '#6b7280'}
              {@const dispatchCount = dispatchCounts[sib as SiblingId] ?? 0}

              <div class="bg-[#0d1117] border border-[#1e293b] rounded-lg p-3 text-center">
                <div class="flex items-center justify-center gap-2 mb-2">
                  <div class="relative">
                    <div
                      class="w-2 h-2 rounded-full"
                      style="background-color: {statusColor}; {h?.status === 'online' ? `box-shadow: 0 0 4px ${statusColor}` : ''}"
                    ></div>
                    {#if h?.status === 'online'}
                      <div
                        class="absolute inset-0 w-2 h-2 rounded-full animate-ping"
                        style="background-color: {statusColor}; opacity: 0.4"
                      ></div>
                    {/if}
                  </div>
                  <span class="text-[10px] font-semibold" style="color: {color}">{sib.toUpperCase()}</span>
                </div>
                <div class="text-[9px] text-[#64748b] mb-1">{h?.status ?? 'unknown'}</div>
                <div class="text-[9px] text-[#475569]">
                  uptime: {h?.uptime ? formatUptime(h.uptime) : '--'}
                </div>
                {#if dispatchCount > 0}
                  <div class="mt-1">
                    <span class="text-[8px] px-1.5 py-0.5 rounded-full bg-[#3b82f6]/20 text-[#3b82f6]">
                      {dispatchCount} active
                    </span>
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      </div>

      <!-- Right column: Conductor + Arena + Alerts -->
      <div class="space-y-6">
        <!-- Conductor Panel -->
        <ConductorPanel />

        <!-- Arena Panel -->
        <ArenaPanel />

        <!-- Alert Panel -->
        <AlertPanel onAcknowledge={acknowledgeAlert} />

        <!-- Phase 16b — Compaction Panel (retention policy + preview + apply) -->
        <CompactionPanel />
      </div>
    </div>
  </div>
</div>