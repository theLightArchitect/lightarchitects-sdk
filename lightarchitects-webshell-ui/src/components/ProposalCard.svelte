<script lang="ts">
  import type { NorthstarEvaluationEvent } from '$lib/types';

  interface Props {
    /** The evaluation event that triggered this proposal. */
    evaluation: NorthstarEvaluationEvent;
    /** Called when the operator clicks "Acknowledge & Redirect". */
    onAcknowledge?: () => void;
  }

  let { evaluation, onAcknowledge }: Props = $props();

  function confidencePct(c: number): string {
    return `${Math.round(c * 100)}%`;
  }

  function statusColor(status: NorthstarEvaluationEvent['status']): string {
    switch (status) {
      case 'drifting':  return 'var(--la-danger-stroke)';
      case 'advancing': return 'var(--la-agent-testing)';
      case 'neutral':   return 'var(--la-text-dim)';
    }
  }

  function statusLabel(status: NorthstarEvaluationEvent['status']): string {
    switch (status) {
      case 'drifting':  return 'DRIFTING';
      case 'advancing': return 'ADVANCING';
      case 'neutral':   return 'NEUTRAL';
    }
  }
</script>

<!--
  ProposalCard — shown when supervisor detects consecutive drift from northstar.
  Accessibility: uses role="alert" so screen readers announce it immediately.
  Interactive: single "Acknowledge" button is keyboard-reachable.
-->
<div
  role="alert"
  aria-live="assertive"
  class="rounded-lg border border-[var(--la-danger-stroke)] bg-[var(--la-danger-stroke)]/5
         px-4 py-3 space-y-2"
>
  <!-- Header row -->
  <div class="flex items-center justify-between gap-3">
    <div class="flex items-center gap-2">
      <!-- Drift indicator dot -->
      <span
        class="inline-block w-2 h-2 rounded-full flex-shrink-0 animate-pulse"
        style="background-color: {statusColor(evaluation.status)}"
        aria-hidden="true"
      ></span>
      <span
        class="text-[10px] font-semibold tracking-widest"
        style="color: {statusColor(evaluation.status)}"
      >
        NORTHSTAR {statusLabel(evaluation.status)}
      </span>
    </div>
    <span class="text-[9px] text-[var(--la-text-dim)] flex-shrink-0">
      Wave {evaluation.wave_num} · {confidencePct(evaluation.confidence)} confidence
    </span>
  </div>

  <!-- Recommended action -->
  <p class="text-[11px] text-[var(--la-text-label)] leading-relaxed">
    {evaluation.recommended_next}
  </p>

  <!-- Acknowledge button -->
  {#if onAcknowledge}
    <div class="pt-1">
      <button
        class="w-full rounded px-3 py-1.5 text-[10px] font-medium
               bg-[var(--la-danger-stroke)]/10 text-[var(--la-danger-stroke)]
               hover:bg-[var(--la-danger-stroke)]/20 focus-visible:outline
               focus-visible:outline-2 focus-visible:outline-[var(--la-danger-stroke)]
               transition-colors"
        onclick={onAcknowledge}
      >
        Acknowledge &amp; Redirect
      </button>
    </div>
  {/if}
</div>
