<script lang="ts">
  import { PILLAR_COLORS } from '$lib/design-tokens';
  import { PILLAR_ACTIONS, type Pillar, type PillarGate, type MetaSkill } from '$lib/types';

  interface Props {
    gate: PillarGate;
    metaSkill: MetaSkill;
    action?: string; // Override for custom action name
  }

  let { gate, metaSkill, action }: Props = $props();

  function statusLabel(status: PillarGate['status']): string {
    switch (status) {
      case 'passed': return 'PASSED';
      case 'in_progress': return 'IN PROGRESS';
      case 'failed': return 'FAILED';
      case 'blocked': return 'BLOCKED';
      default: return 'PENDING';
    }
  }

  function statusColor(status: PillarGate['status']): string {
    switch (status) {
      case 'passed': return '#22c55e';
      case 'in_progress': return '#3b82f6';
      case 'failed': return '#ef4444';
      case 'blocked': return '#f59e0b';
      default: return '#6b7280';
    }
  }

  const actionName = $derived(action ?? PILLAR_ACTIONS[metaSkill]?.[gate.pillar] ?? gate.pillar);
  const color = $derived(PILLAR_COLORS[gate.pillar] ?? '#8B5CF6');
  const colorStatus = $derived(statusColor(gate.status));
</script>

<div class="bg-[#111827] border border-[#1e293b] rounded-lg overflow-hidden">
  <!-- Pillar header -->
  <div class="px-4 py-3 border-b border-[#1e293b] flex items-center justify-between">
    <div class="flex items-center gap-3">
      <div
        class="w-2 h-8 rounded-sm"
        style="background-color: {colorStatus}"
      ></div>
      <div>
        <div class="flex items-center gap-2">
          <span class="text-sm font-semibold" style="color: {color}">{gate.pillar}</span>
          <span class="text-xs text-[#64748b] font-mono">{actionName}</span>
        </div>
        <div class="text-[10px] text-[#475569]">{metaSkill} phase</div>
      </div>
    </div>
    <span
      class="text-[10px] font-semibold px-2 py-0.5 rounded-full"
      style="background-color: {colorStatus}20; color: {colorStatus}"
    >
      {statusLabel(gate.status)}
    </span>
  </div>

  <!-- Confidence bar -->
  <div class="px-4 py-2 border-b border-[#1e293b]">
    <div class="flex items-center justify-between mb-1">
      <span class="text-[10px] text-[#64748b]">Confidence</span>
      <span class="text-[10px] font-mono" style="color: {colorStatus}">{Math.round(gate.confidence * 100)}%</span>
    </div>
    <div class="h-1.5 bg-[#1e293b] rounded-full overflow-hidden">
      <div
        class="h-full rounded-full transition-all"
        style="width: {Math.round(gate.confidence * 100)}%; background-color: {colorStatus}"
      ></div>
    </div>
  </div>

  <!-- Gate status -->
  <div class="px-4 py-3 grid grid-cols-2 gap-3">
    <!-- Entry gate -->
    <div>
      <div class="text-[10px] text-[#64748b] mb-1">ENTRY GATE</div>
      {#if gate.entryGate}
        <div class="flex items-center gap-1.5">
          <div class="w-1.5 h-1.5 rounded-full" style="background-color: {gate.entryGate.passed ? '#22c55e' : '#ef4444'}"></div>
          <span class="text-[10px] text-[#94a3b8]">
            {gate.entryGate.passed ? 'Passed' : 'Failed'}
            {gate.entryGate.hitl ? '(HITL)' : ''}
          </span>
        </div>
      {:else}
        <span class="text-[10px] text-[#334155]">Not reached</span>
      {/if}
    </div>

    <!-- Exit gate -->
    <div>
      <div class="text-[10px] text-[#64748b] mb-1">EXIT GATE</div>
      {#if gate.exitGate}
        <div class="flex items-center gap-1.5">
          <div class="w-1.5 h-1.5 rounded-full" style="background-color: {gate.exitGate.passed ? '#22c55e' : '#ef4444'}"></div>
          <span class="text-[10px] text-[#94a3b8]">
            {gate.exitGate.passed ? 'Passed' : 'Failed'}
            {gate.exitGate.hitl ? '(HITL)' : ''}
          </span>
        </div>
      {:else}
        <span class="text-[10px] text-[#334155]">Not reached</span>
      {/if}
    </div>
  </div>

  <!-- Recovery options (shown when failed/blocked) -->
  {#if gate.status === 'failed' || gate.status === 'blocked'}
    <div class="px-4 py-3 border-t border-[#1e293b]">
      <div class="text-[10px] text-[#64748b] mb-2">RECOVERY</div>
      <div class="flex gap-2">
        <button class="text-[10px] px-2 py-1 rounded border border-[#1e293b] hover:border-[#334155] text-[#94a3b8] transition-colors">
          Loop back
        </button>
        <button class="text-[10px] px-2 py-1 rounded border border-[#1e293b] hover:border-[#334155] text-[#94a3b8] transition-colors">
          Upload evidence
        </button>
        <button class="text-[10px] px-2 py-1 rounded border border-[#1e293b] hover:border-[#f59e0b] text-[#f59e0b] transition-colors">
          Override
        </button>
        <button class="text-[10px] px-2 py-1 rounded border border-[#1e293b] hover:border-[#ef4444] text-[#ef4444] transition-colors">
          Escalate
        </button>
      </div>
    </div>
  {/if}
</div>