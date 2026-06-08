<script lang="ts">
  import type { ProjectGroup, Build } from '$lib/types';
  import { SIBLING_COLORS } from '$lib/design-tokens';
  import { goto } from '$app/navigation';
  import { supervisorAlerts, findings } from '$lib/stores';

  interface Props {
    group: ProjectGroup;
    build?: Build | null;
    onClose: () => void;
  }

  let { group, build = null, onClose }: Props = $props();

  // Pick the most relevant build to display — passed-in build wins, else most active
  let displayBuild = $derived(
    build ?? group.plans.find(b => b.status === 'in_progress') ?? group.plans[0] ?? null
  );

  // Latest supervisor gate verdict for this build
  let latestGate = $derived.by(() => {
    if (!displayBuild) return null;
    const id = displayBuild.id;
    return $supervisorAlerts.filter(a => a.message.includes(id)).at(0) ?? null;
  });

  // Open findings for this build
  let openFindings = $derived(
    displayBuild ? $findings.filter(f => f.buildId === displayBuild!.id && !f.verified) : []
  );

  // Pillar gate summary
  let gateRows = $derived.by(() => {
    if (!displayBuild?.pillars) return [];
    return displayBuild.pillars.map(p => ({
      label: p.pillar,
      status: p.status,
    }));
  });

  function statusColor(status: string): string {
    if (status === 'passed')     return '#22c55e';
    if (status === 'failed')     return '#ef4444';
    if (status === 'in_progress') return '#FFD700';
    return '#475569';
  }

  function buildStatusLabel(status: string): string {
    if (status === 'in_progress') return 'ACTIVE';
    if (status === 'completed')   return 'COMPLETE';
    if (status === 'failed')      return 'FAILED';
    if (status === 'queued')      return 'QUEUED';
    return status.toUpperCase();
  }

  function buildStatusColor(status: string): string {
    if (status === 'in_progress') return '#22c55e';
    if (status === 'completed')   return '#4d8eff';
    if (status === 'failed')      return '#ef4444';
    return '#475569';
  }

  function relativeTime(iso: string): string {
    const diff = Date.now() - new Date(iso).getTime();
    const s = Math.floor(diff / 1000);
    if (s < 60)   return `${s}s ago`;
    if (s < 3600) return `${Math.floor(s / 60)}m ago`;
    return `${Math.floor(s / 3600)}h ago`;
  }
</script>

<!-- 260px right-side slide-in gutter inside the topology grid -->
<div
  class="flex flex-col border-l border-[#1e293b] bg-[#0a0a0f]/95 overflow-y-auto"
  style="width: 260px; min-height: 0;"
  data-testid="project-detail-card"
>
  <!-- Header -->
  <div class="flex items-start justify-between px-3 pt-3 pb-2 border-b border-[#1e293b] shrink-0">
    <div class="flex flex-col gap-0.5 min-w-0">
      <span class="text-[10px] font-mono font-bold text-[#e2e8f0] truncate">{group.name}</span>
      <span class="text-[9px] font-mono text-[#475569] truncate">{group.path}</span>
    </div>
    <button
      onclick={onClose}
      class="text-[#475569] hover:text-[#94a3b8] text-[11px] shrink-0 ml-2"
      aria-label="Close detail card"
    >✕</button>
  </div>

  {#if displayBuild}
    <!-- Build identity -->
    <div class="px-3 py-2 border-b border-[#0f172a]">
      <div class="flex items-center justify-between mb-1">
        <span class="text-[9px] font-mono text-[#475569] uppercase tracking-wider">Active Build</span>
        <span
          class="text-[8px] font-mono font-bold px-1 rounded"
          style="color: {buildStatusColor(displayBuild.status)}; background: {buildStatusColor(displayBuild.status)}18;"
        >{buildStatusLabel(displayBuild.status)}</span>
      </div>
      <p class="text-[10px] font-mono text-[#94a3b8] truncate">{displayBuild.codename ?? displayBuild.name}</p>
      {#if displayBuild.branch}
        <p class="text-[9px] font-mono text-[#334155] truncate mt-0.5">{displayBuild.branch}</p>
      {/if}
      {#if displayBuild.description}
        <p class="text-[9px] text-[#475569] mt-1 leading-relaxed line-clamp-2">{displayBuild.description}</p>
      {/if}
    </div>

    <!-- Agents engaged -->
    {#if displayBuild.siblings && displayBuild.siblings.length > 0}
      <div class="px-3 py-2 border-b border-[#0f172a]">
        <span class="text-[9px] font-mono text-[#475569] uppercase tracking-wider block mb-1">Agents Engaged</span>
        <div class="flex flex-wrap gap-1">
          {#each displayBuild.siblings as sib}
            <span
              class="text-[8px] font-mono font-bold px-1 rounded"
              style="color: {SIBLING_COLORS[sib] ?? '#64748b'}; background: {SIBLING_COLORS[sib] ?? '#64748b'}18;"
            >{sib.toUpperCase()}</span>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Gate status -->
    {#if gateRows.length > 0}
      <div class="px-3 py-2 border-b border-[#0f172a]">
        <span class="text-[9px] font-mono text-[#475569] uppercase tracking-wider block mb-1">Gates</span>
        <div class="flex flex-wrap gap-x-2 gap-y-0.5">
          {#each gateRows as gate}
            <span class="text-[8px] font-mono" style="color: {statusColor(gate.status)};">
              {gate.label}: {gate.status === 'passed' ? '✓' : gate.status === 'failed' ? '✗' : '…'}
            </span>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Last event + findings -->
    <div class="px-3 py-2 border-b border-[#0f172a]">
      <div class="flex items-center justify-between">
        <span class="text-[9px] font-mono text-[#475569]">Updated</span>
        <span class="text-[9px] font-mono text-[#64748b]">{relativeTime(displayBuild.updatedAt)}</span>
      </div>
      <div class="flex items-center justify-between mt-0.5">
        <span class="text-[9px] font-mono text-[#475569]">Open findings</span>
        <span
          class="text-[9px] font-mono"
          style="color: {openFindings.length > 0 ? '#f59e0b' : '#22c55e'};"
        >{openFindings.length}</span>
      </div>
    </div>

    <!-- Multiple builds stack indicator -->
    {#if group.plans.length > 1}
      <div class="px-3 py-1.5 border-b border-[#0f172a]">
        <span class="text-[9px] font-mono text-[#334155]">{group.plans.length} builds in this project</span>
      </div>
    {/if}

    <!-- Action buttons -->
    <div class="px-3 py-2 flex flex-col gap-1 shrink-0 mt-auto">
      <button
        onclick={() => { goto('/builds'); onClose(); }}
        class="w-full text-[9px] font-mono py-1 px-2 rounded border border-[#1e293b] text-[#94a3b8] hover:text-[#FFD700] hover:border-[#FFD700]/30 transition-colors text-left"
      >OPEN BUILD DETAIL</button>
      {#if displayBuild.branch}
        <div class="text-[9px] font-mono py-0.5 px-2 text-[#334155] truncate" title={displayBuild.branch}>
          {displayBuild.branch}
        </div>
      {/if}
    </div>
  {:else}
    <!-- No active builds -->
    <div class="flex-1 flex items-center justify-center px-3">
      <p class="text-[9px] font-mono text-[#334155] text-center">No builds in this project</p>
    </div>
  {/if}
</div>
