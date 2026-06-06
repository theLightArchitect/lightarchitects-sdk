<script lang="ts">
  import { selection } from '$lib/cockpit/stores/selection';
  import BuildFocusPanel      from './BuildFocusPanel.svelte';
  import WorkerFocusPanel     from './WorkerFocusPanel.svelte';
  import EscalationFocusPanel from './EscalationFocusPanel.svelte';
  import SpanFocusPanel       from './SpanFocusPanel.svelte';
  import GateFocusPanel       from './GateFocusPanel.svelte';
  import DecisionFocusPanel   from './DecisionFocusPanel.svelte';
  import PrFocusPanel         from './PrFocusPanel.svelte';
  import CrateFocusPanel      from './CrateFocusPanel.svelte';

  const sel = $derived($selection);
</script>

<div class="focus-router" data-card-role="focus-router" aria-live="polite">
  {#if sel.kind === 'build'}
    <BuildFocusPanel codename={sel.codename} />
  {:else if sel.kind === 'worker'}
    <WorkerFocusPanel worker_id={sel.worker_id} build_codename={sel.build_codename} />
  {:else if sel.kind === 'escalation'}
    <EscalationFocusPanel source={sel.source} id={sel.id} />
  {:else if sel.kind === 'span'}
    <SpanFocusPanel turn_span_id={sel.turn_span_id} />
  {:else if sel.kind === 'gate'}
    <GateFocusPanel codename={sel.codename} phase={sel.phase} gate={sel.gate} />
  {:else if sel.kind === 'decision'}
    <DecisionFocusPanel decision_id={sel.decision_id} build_codename={sel.build_codename} />
  {:else if sel.kind === 'pr'}
    <PrFocusPanel owner={sel.owner} repo={sel.repo} number={sel.number} />
  {:else if sel.kind === 'crate'}
    <CrateFocusPanel name={sel.name} />
  {:else}
    <div class="focus-idle" aria-label="No item selected">
      <span class="focus-idle-label">SELECT TO FOCUS</span>
    </div>
  {/if}
</div>

<style>
  .focus-router {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }
  .focus-idle {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--la-text-mute, #444);
  }
  .focus-idle-label {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    letter-spacing: 0.12em;
    opacity: 0.4;
  }
</style>
