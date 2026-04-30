<script lang="ts">
  import { builds, buildStats, currentBuildId, projectGroups } from '$lib/stores';
  import { SIBLING_COLORS, getMetaSkillPolytope, getMetaSkillColor } from '$lib/design-tokens';
  import { downloadRoadmap } from '$lib/roadmap-export';
  import type { Build, ProjectGroup } from '$lib/types';
  import type { PlanPhaseStatus } from '$lib/types';
  import PhaseTimeline from '$lib/../components/PhaseTimeline.svelte';
  import QualityGateDash from '$lib/../components/QualityGateDash.svelte';
  import PolytopeIcon from '$lib/../components/PolytopeIcon.svelte';
  import PolytopeDecor from '$lib/../components/PolytopeDecor.svelte';

  // View mode
  let viewMode = $state<'list' | 'card'>('card');

  // Navigate to a build
  function openBuild(buildId: string) {
    currentBuildId.set(buildId);
    window.location.hash = `/workspace/${buildId}`;
  }

  // Navigate to intake
  function newBuild() {
    window.location.hash = '/intake';
  }

  // Navigate to project detail (roadmap drill-down)
  function openProject(projectId: string) {
    window.location.hash = `/project/${projectId}`;
  }

  // Map pillar index to LASDLC phase name for the PhaseTimeline
  const PILLAR_TO_LASDLC = ['Plan', 'Research', 'Implement', 'Harden', 'Verify', 'Ship', 'Learn'];

  function buildPhases(build: Build): { id: number; title: string; status: PlanPhaseStatus }[] {
    return build.pillars.map((p, i) => ({
      id: i + 1,
      title: PILLAR_TO_LASDLC[i] ?? p.pillar,
      status: p.status === 'passed'
        ? 'complete'
        : p.status === 'in_progress'
          ? 'active'
          : p.status === 'failed'
            ? 'failed'
            : 'pending',
    }));
  }

  // Priority dot color
  function priorityColor(priority: string | undefined): string {
    switch (priority) {
      case 'high': return '#ef4444';
      case 'medium': return '#f59e0b';
      case 'low': return '#22c55e';
      default: return '#475569';
    }
  }

  // Status label styling
  function statusStyle(status: string): { bg: string; fg: string } {
    switch (status) {
      case 'in_progress': return { bg: '#22c55e20', fg: '#22c55e' };
      case 'completed': return { bg: '#3b82f620', fg: '#3b82f6' };
      case 'failed': return { bg: '#ef444420', fg: '#ef4444' };
      case 'paused': return { bg: '#f59e0b20', fg: '#f59e0b' };
      default: return { bg: '#64748b20', fg: '#64748b' };
    }
  }

  // Progress fraction as pillar count
  function pillarProgress(build: Build): string {
    const passed = build.pillars.filter(p => p.status === 'passed').length;
    return `${passed}/${build.pillars.length}`;
  }
</script>

<div class="h-full flex flex-col relative overflow-hidden">
  <!-- Ambient polytope decoration -->
  <div class="absolute inset-0 overflow-hidden pointer-events-none -z-10">
    <div class="absolute -top-20 -right-20">
      <PolytopeDecor type="icositetrachoron" color="#f0c040" size={400} opacity={0.03} speed={0.05} />
    </div>
    <div class="absolute -bottom-20 -left-20">
      <PolytopeDecor type="tesseract" color="#FF6B9D" size={300} opacity={0.04} speed={0.08} />
    </div>
  </div>

  <!-- Header -->
  <header class="flex items-center justify-between flex-wrap gap-y-2 px-4 md:px-6 py-3 border-b border-[#1e293b]">
    <div class="flex items-center gap-3">
      <h1 class="text-lg font-semibold tracking-wide">Build Queue</h1>
      <span class="text-xs text-[#64748b]">
        {$projectGroups.length} {$projectGroups.length === 1 ? 'project' : 'projects'}
        ·
        {$buildStats.total} {$buildStats.total === 1 ? 'build' : 'builds'}
      </span>
    </div>
    <div class="flex items-center gap-3">
      <!-- View toggle -->
      <div class="flex bg-[#1e293b] rounded overflow-hidden">
        <button
          class="px-3 py-1 text-xs {viewMode === 'card' ? 'bg-[#334155] text-white' : 'text-[#64748b]'}"
          onclick={() => { viewMode = 'card'; }}
        >
          Cards
        </button>
        <button
          class="px-3 py-1 text-xs {viewMode === 'list' ? 'bg-[#334155] text-white' : 'text-[#64748b]'}"
          onclick={() => { viewMode = 'list'; }}
        >
          List
        </button>
      </div>
      <button
        class="px-3 py-1.5 bg-[#1e293b] text-[#94a3b8] text-xs rounded hover:bg-[#334155] hover:text-white transition-all"
        onclick={() => downloadRoadmap($builds)}
        title="Export roadmap as standalone HTML"
      >
        Export
      </button>
      <button
        class="px-4 py-1.5 bg-[#d4a017] text-[#0a0a0f] text-xs font-semibold rounded hover:bg-[#f0c040] hover:shadow-[0_0_10px_rgba(255,215,0,0.4)] transition-all"
        onclick={newBuild}
      >
        + New Build
      </button>
    </div>
  </header>

  <!-- Stat strip -->
  <div class="flex items-center flex-wrap gap-x-4 gap-y-1 px-4 md:px-6 py-2 bg-[#0d0d14] border-b border-[#1e293b] text-xs">
    <span class="text-[#22c55e]">{$buildStats.inProgress} in progress</span>
    <span class="text-[#3b82f6]">{$buildStats.pending} queued</span>
    <span class="text-[#94a3b8]">{$buildStats.completed} completed</span>
    <span class="text-[#ef4444]">{$buildStats.failed} failed</span>
  </div>

  <!-- Build list/cards -->
  <div class="flex-1 overflow-y-auto p-6">
    {#if $builds.length === 0}
      <div class="flex flex-col items-center justify-center h-full text-[#475569]">
        <p class="text-lg mb-2">No active builds</p>
        <p class="text-sm">Start a new build with <kbd class="bg-[#1e293b] px-2 py-0.5 rounded text-xs">/build</kbd></p>
      </div>
    {:else if viewMode === 'card'}
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
        {#each $projectGroups as group}
          <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
          <div
            class="bg-[#111827] border border-[#1e293b] rounded-lg p-3 cursor-pointer hover:border-[#334155] transition-colors"
            onclick={() => group.plans.length > 1 ? openProject(group.id) : openBuild(group.plans[0]?.id ?? group.id)}
            onkeydown={() => group.plans.length > 1 ? openProject(group.id) : openBuild(group.plans[0]?.id ?? group.id)}
          >
            <!-- Project name + plan count -->
            <div class="flex items-center justify-between mb-1">
              <span class="font-semibold text-sm truncate">{group.name}</span>
              <span class="text-[9px] px-1.5 py-0.5 rounded-full bg-[#1e293b] text-[#94a3b8]">
                {group.planCount} {group.planCount === 1 ? 'build' : 'plans'}
              </span>
            </div>

            <!-- Path -->
            <p class="text-[9px] text-[#475569] font-mono truncate mb-2">~/{group.path}</p>

            <!-- Plan list (first 3) -->
            <div class="space-y-1 mb-2">
              {#each group.plans.slice(0, 3) as plan}
                {@const sstyle = statusStyle(plan.status)}
                <div class="flex items-center gap-1.5 text-[10px]">
                  <span class="w-1.5 h-1.5 rounded-full flex-shrink-0" style="background-color: {sstyle.fg}"></span>
                  <span class="text-[#94a3b8] truncate flex-1">{plan.name}</span>
                  <span class="text-[#475569]">{plan.status === 'in_progress' ? 'active' : plan.status === 'completed' ? 'done' : plan.status === 'queued' ? 'planned' : plan.status}</span>
                </div>
              {/each}
              {#if group.plans.length > 3}
                <div class="text-[9px] text-[#475569]">+{group.plans.length - 3} more</div>
              {/if}
            </div>

            <!-- Progress bar -->
            <div class="h-1 bg-[#1e293b] rounded-full overflow-hidden">
              <div
                class="h-full rounded-full transition-all"
                style="width: {Math.round(group.progress * 100)}%; background-color: {group.progress >= 1 ? '#22c55e' : group.activePlanCount > 0 ? '#f0c040' : '#475569'}"
              ></div>
            </div>

            <!-- Stats footer -->
            <div class="flex items-center justify-between mt-1.5 text-[9px] text-[#475569]">
              <span>{group.activePlanCount > 0 ? `${group.activePlanCount} active` : 'no active plans'}</span>
              <span>{Math.round(group.progress * 100)}%</span>
            </div>
          </div>
        {/each}

      </div>

      <!-- Hidden: individual build cards available via project drill-down -->
      {#if false}{#each $builds as build}
          {@const polyType = getMetaSkillPolytope(build.metaSkill)}
          {@const polyColor = getMetaSkillColor(build.metaSkill)}
          {@const phases = buildPhases(build)}
          {@const sstyle = statusStyle(build.status)}
          <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
          <div
            class="bg-[#111827] border border-[#1e293b] rounded-lg p-4 cursor-pointer hover:border-[#334155] transition-colors group"
            onclick={() => openBuild(build.id)}
            onkeydown={() => openBuild(build.id)}
          >
            <!-- Row 1: Polytope + Name + Priority dot + Status badge -->
            <div class="flex items-start gap-3 mb-1.5">
              <div class="flex-shrink-0 mt-0.5">
                <PolytopeIcon type={polyType} color={polyColor} size={36} />
              </div>
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="font-semibold text-sm truncate flex-1">{build.name}</span>
                  <!-- Priority dot -->
                  <span
                    class="inline-block w-2 h-2 rounded-full flex-shrink-0"
                    style="background-color: {priorityColor(build.priority)}"
                    title="Priority: {build.priority ?? 'unset'}"
                  ></span>
                  <!-- Status badge -->
                  <span
                    class="text-[10px] px-2 py-0.5 rounded-full flex-shrink-0 font-mono"
                    style="background-color: {sstyle.bg}; color: {sstyle.fg}"
                  >
                    {build.status}
                  </span>
                </div>
                <!-- Description line -->
                <p class="text-[11px] text-[#64748b] mt-0.5 truncate">
                  {build.description ?? 'No description'}
                </p>
              </div>
            </div>

            <!-- Row 2: PhaseTimeline compact -->
            <div class="mt-1.5 mb-1">
              <PhaseTimeline {phases} compact={true} />
            </div>

            <!-- Row 3: Siblings + status -->
            <div class="flex items-center justify-between text-[9px]">
              <div class="flex items-center gap-0.5">
                {#if (build.siblings?.length ?? 0) > 0}
                  {#each (build.siblings ?? []).slice(0, 3) as sib}
                    <span
                      class="px-1 py-0 rounded font-mono uppercase"
                      style="color: {SIBLING_COLORS[sib] ?? '#8B5CF6'}; opacity: 0.8"
                    >
                      {sib.slice(0, 2)}
                    </span>
                  {/each}
                  {#if (build.siblings?.length ?? 0) > 3}
                    <span class="text-[#475569]">+{(build.siblings?.length ?? 0) - 3}</span>
                  {/if}
                {/if}
              </div>
              {#if (build.blockedBy?.length ?? 0) > 0}
                <span class="text-[#ef4444] truncate max-w-[120px]" title="blocked by: {(build.blockedBy ?? []).join(', ')}">
                  blocked
                </span>
              {:else}
                <span class="text-[#475569]">{build.status === 'in_progress' ? 'active' : build.status === 'completed' ? 'done' : 'planned'}</span>
              {/if}
            </div>
          </div>
        {/each}{/if}
    {:else}
      <!-- List view -->
      <div class="overflow-x-auto">
      <table class="w-full text-sm min-w-[700px]">
        <thead>
          <tr class="text-[#64748b] text-left border-b border-[#1e293b]">
            <th class="pb-2 font-medium w-10"></th>
            <th class="pb-2 font-medium">Name</th>
            <th class="pb-2 font-medium">Phase</th>
            <th class="pb-2 font-medium">Progress</th>
            <th class="pb-2 font-medium">Status</th>
            <th class="pb-2 font-medium">Priority</th>
            <th class="pb-2 font-medium">Updated</th>
          </tr>
        </thead>
        <tbody>
          {#each $builds as build}
            {@const polyType = getMetaSkillPolytope(build.metaSkill)}
            {@const polyColor = getMetaSkillColor(build.metaSkill)}
            {@const sstyle = statusStyle(build.status)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <tr
              class="border-b border-[#1e293b] hover:bg-[#111827] cursor-pointer"
              onclick={() => openBuild(build.id)}
            >
              <td class="py-2">
                <PolytopeIcon type={polyType} color={polyColor} size={24} />
              </td>
              <td class="py-2">
                <div class="font-medium">{build.name}</div>
                <div class="text-[10px] text-[#64748b] truncate max-w-[200px]">{build.description ?? ''}</div>
              </td>
              <td class="py-2">
                <span class="text-xs font-mono" style="color: {polyColor}">{build.currentPillar}</span>
              </td>
              <td class="py-2">
                <span class="text-xs font-mono">{pillarProgress(build)}</span>
                <span class="text-[10px] text-[#64748b] ml-1">{Math.round(build.confidence * 100)}%</span>
              </td>
              <td class="py-2">
                <span
                  class="text-[10px] px-2 py-0.5 rounded-full font-mono"
                  style="background-color: {sstyle.bg}; color: {sstyle.fg}"
                >{build.status}</span>
              </td>
              <td class="py-2">
                <span
                  class="inline-block w-2 h-2 rounded-full"
                  style="background-color: {priorityColor(build.priority)}"
                  title="{build.priority ?? 'unset'}"
                ></span>
              </td>
              <td class="py-2 text-[10px] text-[#64748b] font-mono">
                {build.status === 'in_progress' ? 'active' : build.status === 'completed' ? 'completed' : 'planned'}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      </div>
    {/if}
  </div>
</div>
