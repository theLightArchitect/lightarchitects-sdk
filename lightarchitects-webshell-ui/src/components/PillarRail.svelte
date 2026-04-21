<script lang="ts">
  import { PILLAR_COLORS, PILLARS } from '$lib/design-tokens';
  import type { PillarGate, Pillar } from '$lib/types';

  interface Props {
    pillars: PillarGate[];
    compact?: boolean;
    selected?: Pillar | null;
    onPillarClick?: (pillar: Pillar) => void;
  }

  let { pillars, compact = false, selected = null, onPillarClick }: Props = $props();

  function pillarColor(pillar: Pillar): string {
    return PILLAR_COLORS[pillar] ?? '#6b7280';
  }

  function statusColor(status: string): string {
    switch (status) {
      case 'passed': return '#22c55e';
      case 'in_progress': return '#3b82f6';
      case 'failed': return '#ef4444';
      case 'blocked': return '#f59e0b';
      default: return '#6b7280';
    }
  }
</script>

{#if compact}
  <!-- Compact horizontal rail for cards -->
  <div class="flex items-center gap-1">
    {#each pillars as gate}
      <div
        class="flex-1 h-2 rounded-sm"
        style="background-color: {statusColor(gate.status)}30; border: 1px solid {statusColor(gate.status)}60"
        title="{gate.pillar}: {gate.status}"
      >
        <div
          class="h-full rounded-sm transition-all"
          style="width: {gate.status === 'passed' ? '100' : gate.status === 'in_progress' ? Math.round(gate.confidence * 100) : '0'}%; background-color: {statusColor(gate.status)}"
        ></div>
      </div>
    {/each}
  </div>
{:else}
  <!-- Full rail with labels (clickable) -->
  <div class="flex items-stretch gap-2">
    {#each pillars as gate, i}
      <button
        class="flex-1 text-center rounded-lg transition-all {selected === gate.pillar ? 'ring-1 ring-[#FFD700] bg-[#FFD700]/5' : 'hover:bg-[#111827]'}"
        onclick={() => onPillarClick?.(gate.pillar)}
      >
        <!-- Pillar label -->
        <div
          class="text-[10px] font-semibold tracking-wider mb-1"
          style="color: {pillarColor(gate.pillar)}"
        >
          {gate.pillar}
        </div>
        <!-- Pillar block -->
        <div
          class="h-8 rounded border transition-colors"
          style="background-color: {statusColor(gate.status)}15; border-color: {statusColor(gate.status)}60"
        >
          <div
            class="h-full rounded transition-all"
            style="width: {gate.status === 'passed' ? '100' : gate.status === 'in_progress' ? Math.round(gate.confidence * 100) : '0'}%; background-color: {statusColor(gate.status)}40; min-width: {gate.status !== 'pending' ? '2px' : '0'}"
          ></div>
        </div>
        <!-- Confidence -->
        <div class="text-[9px] text-[#475569] mt-1">
          {gate.status === 'pending' ? '—' : `${Math.round(gate.confidence * 100)}%`}
        </div>
      </button>
      {#if i < pillars.length - 1}
        <div class="flex items-center text-[#334155]">
          →
        </div>
      {/if}
    {/each}
  </div>
{/if}