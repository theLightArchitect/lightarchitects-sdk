<!--
  @component GateMatrix
  @description 10-cell [A,S,Q,C,O,P,K,D,T,R] gate status grid derived from impl_complete events.
  @contract EventType 'impl_complete' → ImplCompleteEvent.gates_passed/gates_skipped
  @reads lightspaceLasdlcStore.gateMatrix (populated by lasdlcUpdateGates in Phase 5 wiring)
  @mutates none
  @api none
  @mockup-ref arch/lightspace-mockup.html → .la-gate-matrix, gate status colors
-->
<script lang="ts">
  import { lightspaceLasdlcStore } from '$lib/lightspace-stores';

  const STATUS_COLOR: Record<string, string> = {
    pass:    'var(--ls-acc-green)',
    fail:    'var(--ls-acc-red)',
    active:  'var(--ls-acc)',
    skip:    'var(--ls-text-ghost)',
    pending: 'var(--ls-border-strong)',
  };
</script>

{#if $lightspaceLasdlcStore.gateMatrix.length > 0}
  <div class="ls-gate-matrix">
    {#each $lightspaceLasdlcStore.gateMatrix as gate}
      <div class="ls-gate-cell" title={`[${gate.id}] ${gate.status}`} style="border-color: {STATUS_COLOR[gate.status] ?? STATUS_COLOR.pending}">
        <span class="ls-gate-id">{gate.id}</span>
        <span class="ls-gate-status" style="color: {STATUS_COLOR[gate.status] ?? STATUS_COLOR.pending}">{gate.status.slice(0,4).toUpperCase()}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
.ls-gate-matrix {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--ls-border);
}
.ls-gate-cell {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  padding: 4px 6px;
  border: 1px solid var(--ls-border);
  min-width: 34px;
  transition: border-color var(--ls-fast);
}
.ls-gate-id {
  font-family: var(--ls-font-display);
  font-weight: 700;
  font-size: 9px;
  color: var(--ls-text-bright);
  letter-spacing: var(--ls-tk-loose);
}
.ls-gate-status {
  font-size: 7px;
  text-transform: uppercase;
  letter-spacing: var(--ls-tk-mid);
}
</style>
