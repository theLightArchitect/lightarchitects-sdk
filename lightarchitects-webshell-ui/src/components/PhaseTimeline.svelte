<script lang="ts">
  import type { PlanPhaseStatus } from '$lib/types';

  interface PhaseItem {
    id: number;
    title: string;
    status: PlanPhaseStatus;
  }

  interface Props {
    phases: PhaseItem[];
    currentPhase?: number;
    compact?: boolean;
  }

  let { phases, currentPhase, compact = false }: Props = $props();

  const PHASE_ABBREV: Record<string, string> = {
    'Plan': 'PLN',
    'Research': 'RSH',
    'Implement': 'IMP',
    'Harden': 'HRD',
    'Verify': 'VFY',
    'Ship': 'SHP',
    'Learn': 'LRN',
  };

  function statusIcon(status: PlanPhaseStatus): string {
    switch (status) {
      case 'complete': return '\u2713';
      case 'active': return '\u25CF';
      case 'failed': return '\u2717';
      case 'skipped': return '\u2014';
      default: return '\u25CB';
    }
  }

  function statusColor(status: PlanPhaseStatus): string {
    switch (status) {
      case 'complete': return '#22c55e';
      case 'active': return '#FFD700';
      case 'failed': return '#ef4444';
      case 'skipped': return '#64748b';
      default: return '#475569';
    }
  }

  function separatorColor(index: number): string {
    if (index >= phases.length - 1) return '#475569';
    const current = phases[index];
    if (current.status === 'complete') return '#22c55e';
    if (current.status === 'active') return '#FFD700';
    return '#475569';
  }

  function isActive(phase: PhaseItem): boolean {
    return phase.status === 'active' || phase.id === currentPhase;
  }
</script>

{#if compact}
  <!-- Compact: icons + abbreviation, single line -->
  <div class="flex items-center gap-0.5 text-[10px] font-mono">
    {#each phases as phase, i}
      <span
        class="inline-flex items-center gap-0.5 px-1 rounded"
        class:phase-active-glow={isActive(phase)}
        style="color: {statusColor(phase.status)}"
        title="{phase.title}: {phase.status}"
      >
        <span class="text-[11px]" class:pulse={isActive(phase)}>
          {statusIcon(phase.status)}
        </span>
        <span class="opacity-80">{PHASE_ABBREV[phase.title] ?? phase.title.slice(0, 3).toUpperCase()}</span>
      </span>
      {#if i < phases.length - 1}
        <span style="color: {separatorColor(i)}; font-size: 8px;">\u2192</span>
      {/if}
    {/each}
  </div>
{:else}
  <!-- Expanded: icons + full title, wrapping allowed -->
  <div class="flex flex-wrap items-center gap-1">
    {#each phases as phase, i}
      <div
        class="inline-flex items-center gap-1.5 px-2 py-1 rounded-md border transition-all"
        class:phase-active-glow={isActive(phase)}
        style="
          border-color: {isActive(phase) ? '#FFD700' + '60' : '#1e293b'};
          background-color: {isActive(phase) ? '#FFD700' + '08' : 'transparent'};
        "
        title="{phase.title}: {phase.status}"
      >
        <span
          class="text-sm font-bold"
          class:pulse={isActive(phase)}
          style="color: {statusColor(phase.status)}"
        >
          {statusIcon(phase.status)}
        </span>
        <span
          class="text-[11px] font-mono"
          style="color: {statusColor(phase.status)}"
        >
          {phase.title}
        </span>
      </div>
      {#if i < phases.length - 1}
        <span
          class="text-[10px] mx-0.5"
          style="color: {separatorColor(i)}"
          aria-hidden="true"
        >&rarr;</span>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .phase-active-glow {
    box-shadow: 0 0 8px #FFD70040;
  }

  .pulse {
    animation: phase-pulse 1.5s ease-in-out infinite;
  }

  @keyframes phase-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }
</style>
