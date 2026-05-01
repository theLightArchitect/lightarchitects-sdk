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

<div class="bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded-lg overflow-hidden">
  <!-- Pillar header -->
  <div class="px-4 py-3 border-b border-[var(--la-drawer-border)] flex items-center justify-between">
    <div class="flex items-center gap-3">
      <div
        class="w-2 h-8 rounded-sm"
        style="background-color: {colorStatus}"
      ></div>
      <div>
        <div class="flex items-center gap-2">
          <span class="text-sm font-semibold" style="color: {color}">{gate.pillar}</span>
          <span class="text-xs text-[var(--la-text-dim)] font-mono">{actionName}</span>
        </div>
        <div class="text-[10px] text-[var(--la-text-dim)]">{metaSkill} phase</div>
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
  <div class="px-4 py-2 border-b border-[var(--la-drawer-border)]">
    <div class="flex items-center justify-between mb-1">
      <span class="text-[10px] text-[var(--la-text-dim)]">Confidence</span>
      <span class="text-[10px] font-mono" style="color: {colorStatus}">{Math.round(gate.confidence * 100)}%</span>
    </div>
    <div class="h-1.5 bg-[var(--la-drawer-border)] rounded-full overflow-hidden">
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
      <div class="text-[10px] text-[var(--la-text-dim)] mb-1">ENTRY GATE</div>
      {#if gate.entryGate}
        <div class="flex items-center gap-1.5">
          <div class="w-1.5 h-1.5 rounded-full" style="background-color: {gate.entryGate.passed ? '#22c55e' : '#ef4444'}"></div>
          <span class="text-[10px] text-[var(--la-text-label)]">
            {gate.entryGate.passed ? 'Passed' : 'Failed'}
            {gate.entryGate.hitl ? '(HITL)' : ''}
          </span>
        </div>
      {:else}
        <span class="text-[10px] text-[var(--la-hair-strong)]">Not reached</span>
      {/if}
    </div>

    <!-- Exit gate -->
    <div>
      <div class="text-[10px] text-[var(--la-text-dim)] mb-1">EXIT GATE</div>
      {#if gate.exitGate}
        <div class="flex items-center gap-1.5">
          <div class="w-1.5 h-1.5 rounded-full" style="background-color: {gate.exitGate.passed ? '#22c55e' : '#ef4444'}"></div>
          <span class="text-[10px] text-[var(--la-text-label)]">
            {gate.exitGate.passed ? 'Passed' : 'Failed'}
            {gate.exitGate.hitl ? '(HITL)' : ''}
          </span>
        </div>
      {:else}
        <span class="text-[10px] text-[var(--la-hair-strong)]">Not reached</span>
      {/if}
    </div>
  </div>

  <!-- Recovery options (shown when failed/blocked) -->
  {#if gate.status === 'failed' || gate.status === 'blocked'}
    <div class="px-4 py-3 border-t border-[var(--la-drawer-border)]">
      <div class="text-[10px] text-[var(--la-text-dim)] mb-2">RECOVERY</div>
      <div class="flex gap-2">
        <button class="text-[10px] px-2 py-1 rounded border border-[var(--la-drawer-border)] hover:border-[var(--la-hair-strong)] text-[var(--la-text-label)] transition-colors">
          Loop back
        </button>
        <button class="text-[10px] px-2 py-1 rounded border border-[var(--la-drawer-border)] hover:border-[var(--la-hair-strong)] text-[var(--la-text-label)] transition-colors">
          Upload evidence
        </button>
        <button class="text-[10px] px-2 py-1 rounded border border-[var(--la-drawer-border)] hover:border-[var(--la-agent-performance)] text-[var(--la-agent-performance)] transition-colors">
          Override
        </button>
        <button class="text-[10px] px-2 py-1 rounded border border-[var(--la-drawer-border)] hover:border-[var(--la-danger-stroke)] text-[var(--la-danger-stroke)] transition-colors">
          Escalate
        </button>
      </div>
    </div>
  {/if}
</div>