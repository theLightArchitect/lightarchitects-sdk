<!--
@component
Renders a single IMPLEMENTATION_COMPLETE attestation (Agents Playbook §3.5).

Three-layer display:
 1. Self-report  — commit SHA, gates passed/skipped, confidence score
 2. AYIN witness — spans dropped (P gate signal; red badge when > 0)
 3. Trust boundary — rendered as amber "unverified" badge; NEVER as "signed"

Security invariants (mirroring backend attestation_routes.rs):
 - trust_boundary starting with "unverified" → amber badge
 - ayin_spans_dropped_total > 0 → red P-gate warning badge
 - file_content_span_id is a UUID reference only; never rendered as a path

Props:
 - `ev` — the ImplCompleteEvent to display
-->
<script lang="ts">
  import type { ImplCompleteEvent } from '$lib/types';

  let { ev }: { ev: ImplCompleteEvent } = $props();

  let isUnverified    = $derived(ev.trust_boundary.startsWith('unverified'));
  let ayinDropped     = $derived(ev.ayin_spans_dropped_total > 0);
  let confidencePct   = $derived(Math.round(ev.confidence * 100));
  let isWaveBoundary  = $derived(ev.task_id.endsWith('-boundary'));
</script>

<div
  class="attest-card"
  class:attest-wave-boundary={isWaveBoundary}
  data-testid="attestation-card"
  data-build-id={ev.build_id}
  data-wave={ev.wave}
>
  <!-- ── Header row ──────────────────────────────────────────────────────── -->
  <div class="attest-header">
    {#if isWaveBoundary}
      <span class="attest-label attest-label-boundary">WAVE BOUNDARY</span>
    {:else}
      <span class="attest-label">IMPL COMPLETE</span>
    {/if}
    <span class="attest-wave">W{ev.wave}</span>
    {#if !isWaveBoundary}
      <span class="attest-task-id" title={ev.task_id}>{ev.task_id.slice(0, 16)}</span>
    {/if}
    <span class="attest-agent">{ev.agent_id.slice(0, 12)}</span>
    <span class="attest-sha">{ev.commit_sha.slice(0, 7)}</span>

    <!-- Trust boundary badge — amber when unverified, NEVER shows "signed" -->
    {#if isUnverified}
      <span class="attest-trust-badge attest-unverified" title={ev.trust_boundary}>UNVERIFIED</span>
    {:else}
      <span class="attest-trust-badge attest-boundary" title={ev.trust_boundary}>
        {ev.trust_boundary.slice(0, 14).toUpperCase()}
      </span>
    {/if}

    <!-- P gate warning — red when AYIN dropped spans -->
    {#if ayinDropped}
      <span class="attest-p-warn" title="AYIN dropped {ev.ayin_spans_dropped_total} span(s) — P gate signal">
        P⚠ {ev.ayin_spans_dropped_total}
      </span>
    {/if}
  </div>

  <!-- ── Self-report layer ───────────────────────────────────────────────── -->
  <div class="attest-body">
    <!-- Gates -->
    {#if ev.gates_passed.length > 0}
      <div class="attest-gates attest-gates-passed">
        {#each ev.gates_passed as g}
          <span class="gate-pill gate-pass">{g}</span>
        {/each}
      </div>
    {/if}
    {#if ev.gates_skipped.length > 0}
      <div class="attest-gates attest-gates-skipped">
        {#each ev.gates_skipped as g}
          <span class="gate-pill gate-skip">{g}</span>
        {/each}
      </div>
    {/if}

    <!-- Confidence -->
    <div class="attest-confidence">
      <span class="attest-conf-label">CONFIDENCE</span>
      <span
        class="attest-conf-val"
        class:conf-high={confidencePct >= 90}
        class:conf-mid={confidencePct >= 70 && confidencePct < 90}
        class:conf-low={confidencePct < 70}
      >{confidencePct}%</span>
    </div>

    <!-- Spec compliance claim (optional) -->
    {#if ev.spec_compliance_claim}
      <div class="attest-claim">{ev.spec_compliance_claim.slice(0, 80)}</div>
    {/if}
  </div>
</div>

<style>
  .attest-card {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 5px 8px;
    border-radius: 3px;
    background: var(--la-bg-elev-1);
    border-left: 3px solid var(--la-focus-ring);
  }

  .attest-header {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .attest-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-focus-ring);
  }

  .attest-wave {
    font-size: 9px;
    font-weight: 600;
    color: var(--la-text-label);
    min-width: 20px;
  }

  .attest-agent {
    font-size: 9px;
    color: var(--la-text-dim);
    font-style: italic;
  }

  .attest-sha {
    font-family: monospace;
    font-size: 9px;
    color: var(--la-text-dim);
  }

  .attest-trust-badge {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 1px 5px;
    border-radius: 2px;
  }

  .attest-unverified {
    background: color-mix(in srgb, #e90 18%, transparent);
    color: #e90;
    border: 1px solid color-mix(in srgb, #e90 35%, transparent);
  }

  .attest-boundary {
    background: color-mix(in srgb, var(--la-text-dim) 12%, transparent);
    color: var(--la-text-dim);
    border: 1px solid color-mix(in srgb, var(--la-text-dim) 25%, transparent);
  }

  .attest-p-warn {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.06em;
    padding: 1px 5px;
    border-radius: 2px;
    background: color-mix(in srgb, #e55 18%, transparent);
    color: #e55;
    border: 1px solid color-mix(in srgb, #e55 35%, transparent);
    animation: pulse-warn 2s ease-in-out infinite;
  }

  @keyframes pulse-warn {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.65; }
  }

  .attest-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .attest-gates {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }

  .gate-pill {
    font-size: 8px;
    padding: 1px 5px;
    border-radius: 2px;
    font-family: monospace;
  }

  .gate-pass {
    background: color-mix(in srgb, var(--la-strand-sec) 14%, transparent);
    color: var(--la-strand-sec);
    border: 1px solid color-mix(in srgb, var(--la-strand-sec) 28%, transparent);
  }

  .gate-skip {
    background: color-mix(in srgb, #e90 10%, transparent);
    color: #e90;
    border: 1px solid color-mix(in srgb, #e90 22%, transparent);
    opacity: 0.75;
  }

  .attest-confidence {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .attest-conf-label {
    font-size: 8px;
    letter-spacing: 0.08em;
    color: var(--la-text-dim);
  }

  .attest-conf-val {
    font-size: 11px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }

  .conf-high { color: var(--la-strand-sec); }
  .conf-mid  { color: #e90; }
  .conf-low  { color: #e55; }

  .attest-claim {
    font-size: 9px;
    color: var(--la-text-dim);
    font-style: italic;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Wave-boundary marker — muted teal border vs task's focus-ring blue */
  .attest-wave-boundary {
    border-left-color: var(--la-text-label);
    opacity: 0.82;
  }

  .attest-label-boundary {
    color: var(--la-text-label);
    letter-spacing: 0.08em;
  }

  .attest-task-id {
    font-family: monospace;
    font-size: 9px;
    color: var(--la-text-dim);
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
