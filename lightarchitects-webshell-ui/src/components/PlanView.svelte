<script lang="ts">
  import { activePlan } from '$lib/stores';
  import type { PlanPhase, PlanPhaseStatus } from '$lib/types';

  let plan = $derived($activePlan);

  // Track which phases are expanded (by id)
  let expandedPhases = $state(new Set<number>());

  function togglePhase(id: number) {
    expandedPhases = new Set(expandedPhases);
    if (expandedPhases.has(id)) {
      expandedPhases.delete(id);
    } else {
      expandedPhases.add(id);
    }
  }

  function statusColor(status: PlanPhaseStatus): string {
    switch (status) {
      case 'pending': return '#475569';
      case 'active': return '#FFD700';
      case 'complete': return '#22c55e';
      case 'failed': return '#ef4444';
    }
  }

  function statusIcon(status: PlanPhaseStatus): string {
    switch (status) {
      case 'pending': return '\u25CB';   // open circle
      case 'active': return '\u25CF';    // filled circle
      case 'complete': return '\u2713';  // check mark
      case 'failed': return '\u2717';    // x mark
    }
  }
</script>

{#if plan}
  <div class="bg-[#111827] border border-[#1e293b] rounded-lg p-3 mb-4">
    <!-- Plan header -->
    <div class="flex items-center gap-2 mb-2">
      <span class="text-[10px] font-semibold tracking-wider text-[#FFD700] uppercase">Plan</span>
      <span class="text-[11px] text-[#e2e8f0] font-medium">{plan.title}</span>
    </div>

    <!-- Phase list -->
    <div class="space-y-1">
      {#each plan.phases as phase (phase.id)}
        {@const color = statusColor(phase.status)}
        {@const icon = statusIcon(phase.status)}
        {@const isExpanded = expandedPhases.has(phase.id)}
        {@const isActive = phase.status === 'active'}

        <div
          class="rounded border transition-colors"
          style="border-color: {isActive ? '#FFD700' + '40' : '#1e293b'}; {isActive ? 'box-shadow: 0 0 8px #FFD70020;' : ''}"
        >
          <!-- Phase header (clickable) -->
          <button
            class="w-full flex items-center gap-2 px-2 py-1.5 text-left hover:bg-[#1e293b]/40 transition-colors rounded"
            onclick={() => togglePhase(phase.id)}
          >
            <!-- Number badge -->
            <span
              class="flex-shrink-0 w-5 h-5 rounded-full flex items-center justify-center text-[9px] font-bold"
              style="background-color: {color}20; color: {color}"
            >
              {phase.id}
            </span>

            <!-- Status indicator -->
            <span
              class="flex-shrink-0 text-[10px]"
              class:plan-pulse={isActive}
              style="color: {color}"
            >
              {icon}
            </span>

            <!-- Title -->
            <span class="text-[10px] text-[#e2e8f0] flex-1 truncate">{phase.title}</span>

            <!-- Expand chevron -->
            <span class="text-[9px] text-[#475569] flex-shrink-0 transition-transform" class:rotate-90={isExpanded}>
              &#9654;
            </span>
          </button>

          <!-- Expanded content -->
          {#if isExpanded}
            <div class="px-2 pb-2 pl-9 space-y-1">
              {#if phase.description}
                <p class="text-[9px] text-[#94a3b8] leading-relaxed">{phase.description}</p>
              {/if}
              {#if phase.files?.length > 0}
                <div class="space-y-0.5">
                  {#each phase.files as file}
                    <div class="text-[9px] font-mono text-[#64748b] truncate" title={file}>{file}</div>
                  {/each}
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .plan-pulse {
    animation: plan-glow 2s ease-in-out infinite;
  }
  @keyframes plan-glow {
    0%, 100% { opacity: 0.6; }
    50% { opacity: 1; }
  }
</style>
