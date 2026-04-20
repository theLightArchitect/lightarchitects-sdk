<script lang="ts">
  import { builds, buildStats, currentBuildId } from '$lib/stores';
  import { SIBLING_COLORS, PILLAR_COLORS, PILLARS, getMetaSkillPolytope, getMetaSkillColor } from '$lib/design-tokens';
  import type { Build } from '$lib/types';
  import PillarRail from '$lib/../components/PillarRail.svelte';
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
</script>

<div class="h-full flex flex-col relative overflow-hidden">
  <!-- Ambient polytope decoration -->
  <div class="absolute inset-0 overflow-hidden pointer-events-none -z-10">
    <div class="absolute -top-20 -right-20">
      <PolytopeDecor type="icositetrachoron" color="#7C3AED" size={400} opacity={0.03} speed={0.05} />
    </div>
    <div class="absolute -bottom-20 -left-20">
      <PolytopeDecor type="tesseract" color="#FF1493" size={300} opacity={0.04} speed={0.08} />
    </div>
  </div>

  <!-- Header -->
  <header class="flex items-center justify-between flex-wrap gap-y-2 px-4 md:px-6 py-3 border-b border-[#1e293b]">
    <div class="flex items-center gap-3">
      <h1 class="text-lg font-semibold tracking-wide">Build Queue</h1>
      <span class="text-xs text-[#64748b]">{$buildStats.total} total · {$buildStats.inProgress} active</span>
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
        class="px-4 py-1.5 bg-[#7C3AED] text-white text-xs rounded hover:bg-[#6D28D9] transition-colors"
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
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {#each $builds as build}
          {@const polyType = getMetaSkillPolytope(build.metaSkill)}
          {@const polyColor = getMetaSkillColor(build.metaSkill)}
          {@const progress = build.pillars.filter(p => p.status === 'passed').length}
          <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
          <div
            class="bg-[#111827] border border-[#1e293b] rounded-lg p-4 cursor-pointer hover:border-[#334155] transition-colors"
            onclick={() => openBuild(build.id)}
            onkeydown={() => openBuild(build.id)}
          >
            <div class="flex items-start gap-3 mb-2">
              <!-- Polytope icon -->
              <div class="flex-shrink-0">
                <PolytopeIcon type={polyType} color={polyColor} size={48} />
              </div>
              <div class="flex-1 min-w-0">
                <div class="flex items-center justify-between">
                  <span class="font-semibold text-sm truncate">{build.name}</span>
                  <span
                    class="text-[10px] px-2 py-0.5 rounded-full"
                    style="background-color: {polyColor}20; color: {polyColor}"
                  >
                    {build.metaSkill}
                  </span>
                </div>
                <div class="text-xs text-[#64748b] mt-1">
                  {build.currentPillar} · {progress}/7 · confidence {Math.round(build.confidence * 100)}%
                </div>
              </div>
            </div>
            <PillarRail pillars={build.pillars} compact={true} />
          </div>
        {/each}
      </div>
    {:else}
      <div class="overflow-x-auto">
      <table class="w-full text-sm min-w-[600px]">
        <thead>
          <tr class="text-[#64748b] text-left border-b border-[#1e293b]">
            <th class="pb-2 font-medium w-10"></th>
            <th class="pb-2 font-medium">Name</th>
            <th class="pb-2 font-medium">Meta-Skill</th>
            <th class="pb-2 font-medium">Pillar</th>
            <th class="pb-2 font-medium">Confidence</th>
            <th class="pb-2 font-medium">Status</th>
          </tr>
        </thead>
        <tbody>
          {#each $builds as build}
            {@const polyType = getMetaSkillPolytope(build.metaSkill)}
            {@const polyColor = getMetaSkillColor(build.metaSkill)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <tr
              class="border-b border-[#1e293b] hover:bg-[#111827] cursor-pointer"
              onclick={() => openBuild(build.id)}
            >
              <td class="py-2">
                <PolytopeIcon type={polyType} color={polyColor} size={24} />
              </td>
              <td class="py-2">{build.name}</td>
              <td class="py-2" style="color: {polyColor}">{build.metaSkill}</td>
              <td class="py-2">{build.currentPillar}</td>
              <td class="py-2">{Math.round(build.confidence * 100)}%</td>
              <td class="py-2">
                <span class="text-xs px-2 py-0.5 rounded-full bg-[#1e293b]">{build.status}</span>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      </div>
    {/if}
  </div>
</div>