<script lang="ts">
  import { gitforestTree } from '$lib/stores';
  import { goto } from '$app/navigation';
  import type { BranchNode } from '$lib/gitforest';

  interface Props {
    nodeId: string;
    /** Bounding rect of the hitbox button — used for fixed positioning. */
    anchor: DOMRect;
    onclose: () => void;
  }

  let { nodeId, anchor, onclose }: Props = $props();

  // Derived from the gitforestTree store — null if topology not yet loaded.
  let node = $derived<BranchNode | null>(
    $gitforestTree?.nodes[nodeId] ?? null,
  );

  // Position the card: prefer above the anchor; flip below if too close to top.
  const GAP = 10;
  const VIEWPORT_PAD = 8;

  let cardLeft = $derived(Math.max(VIEWPORT_PAD, anchor.left + anchor.width / 2 - 120));
  let cardTop = $derived(
    anchor.top - GAP > 180
      ? anchor.top - GAP          // above
      : anchor.bottom + GAP,      // flip below
  );
  let flipBelow = $derived(anchor.top - GAP <= 180);

  // ── Helpers ──────────────────────────────────────────────────────────────

  function ciLabel(status: string): string {
    if (status === 'success') return '✓';
    if (status === 'failure') return '✗';
    if (status === 'pending') return '…';
    return '—';
  }

  function ciClass(status: string): string {
    if (status === 'success') return 'ci-ok';
    if (status === 'failure') return 'ci-fail';
    if (status === 'pending') return 'ci-pending';
    return 'ci-neutral';
  }

  function gateLabel(score: number | null): string {
    if (score === null) return '—';
    return `${score.toFixed(1)}%`;
  }

  function kindBadge(kind: string): string {
    if (kind === 'main') return 'MAIN';
    if (kind === 'program') return 'PROG';
    if (kind === 'build') return 'BUILD';
    if (kind === 'wave_cluster') return 'WAVE';
    return kind.toUpperCase();
  }

  function hitlLabel(state: string): string {
    if (state === 'pending') return 'HITL pending';
    if (state === 'resolved') return 'HITL resolved';
    return null as unknown as string;
  }

  // ── Navigation ────────────────────────────────────────────────────────────

  function handleClick() {
    onclose();
    goto('/builds');
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); onclose(); }
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); handleClick(); }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="branch-tooltip"
  class:flip={flipBelow}
  role="tooltip"
  id="branch-tooltip-{nodeId}"
  style:left="{cardLeft}px"
  style:top="{cardTop}px"
  onkeydown={handleKeydown}
>
  {#if node}
    <header class="tt-header">
      <span class="kind-badge" data-kind={node.kind}>{kindBadge(node.kind)}</span>
      <span class="tt-name">{node.name}</span>
    </header>

    <dl class="tt-grid">
      {#if node.overlay.phase}
        <dt>PHASE</dt>
        <dd class="mono">{node.overlay.phase}</dd>
      {/if}

      <dt>GATE</dt>
      <dd class="mono gate-score">{gateLabel(node.overlay.gate_score)}</dd>

      <dt>AGE</dt>
      <dd class="mono">{node.overlay.age_days}d</dd>

      <dt>CI</dt>
      <dd class="mono {ciClass(node.overlay.ci_status)}">{ciLabel(node.overlay.ci_status)}</dd>

      {#if node.overlay.hitl_state !== 'none'}
        <dt>HITL</dt>
        <dd class="mono hitl">{hitlLabel(node.overlay.hitl_state)}</dd>
      {/if}

      {#if node.build_progress}
        <dt>WAVES</dt>
        <dd class="mono">{node.build_progress.waves_done}/{node.build_progress.waves_total}</dd>
      {/if}

      {#if node.worktrees.length > 0}
        <dt>AGENTS</dt>
        <dd class="mono">{node.worktrees.length} active</dd>
      {/if}
    </dl>

    {#if node.overlay.lifecycle === 'merged'}
      <div class="tt-merged-badge">merged</div>
    {:else if node.overlay.lifecycle === 'abandoned'}
      <div class="tt-ghost-badge">abandoned</div>
    {/if}

    <footer class="tt-footer">
      <button class="tt-action" onclick={handleClick}>Open Builds</button>
    </footer>
  {:else}
    <div class="tt-empty">Loading…</div>
  {/if}
</div>

<style>
  .branch-tooltip {
    position: fixed;
    z-index: 200;
    width: 240px;
    background: var(--la-bg-panel, #0d1117);
    border: 1px solid var(--la-hair-strong, #30363d);
    border-radius: 6px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.04);
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
    color: var(--la-text-base, #c9d1d9);
    pointer-events: auto;
    animation: tt-in 100ms ease-out;
    transform-origin: top center;
  }

  .branch-tooltip.flip { transform-origin: bottom center; }

  @keyframes tt-in {
    from { opacity: 0; transform: scaleY(0.92); }
    to   { opacity: 1; transform: scaleY(1); }
  }

  .tt-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px 6px;
    border-bottom: 1px solid var(--la-hair-faint, rgba(255,255,255,0.06));
  }

  .kind-badge {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 2px 5px;
    border-radius: 3px;
    flex-shrink: 0;
    background: var(--la-bg-card, #161b22);
    color: var(--la-text-dim, #8b949e);
  }

  .kind-badge[data-kind="build"]        { color: var(--la-focus-ring, #58a6ff); }
  .kind-badge[data-kind="wave_cluster"] { color: var(--la-agent-researcher, #17c3b2); }
  .kind-badge[data-kind="program"]      { color: var(--la-agent-security, #ef4444); }

  .tt-name {
    font-size: 10px;
    font-weight: 600;
    color: var(--la-text-stark, #e6edf3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tt-grid {
    display: grid;
    grid-template-columns: 52px 1fr;
    gap: 4px 8px;
    margin: 0;
    padding: 8px 10px;
  }

  .tt-grid dt {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-mute, #6e7681);
    padding-top: 1px;
    text-transform: uppercase;
  }

  .tt-grid dd {
    margin: 0;
    color: var(--la-text-base, #c9d1d9);
  }

  .mono { font-variant-numeric: tabular-nums; }

  .gate-score { color: var(--la-agent-performance, #f97316); }
  .ci-ok      { color: #3fb950; }
  .ci-fail    { color: var(--la-agent-security, #ef4444); }
  .ci-pending { color: var(--la-agent-performance, #f97316); }
  .ci-neutral { color: var(--la-text-dim, #8b949e); }
  .hitl       { color: var(--la-focus-ring, #58a6ff); }

  .tt-merged-badge,
  .tt-ghost-badge {
    margin: 0 10px;
    padding: 2px 6px;
    border-radius: 3px;
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    display: inline-block;
  }

  .tt-merged-badge { background: rgba(62, 185, 80, 0.15); color: #3fb950; }
  .tt-ghost-badge  { background: rgba(139, 148, 158, 0.15); color: var(--la-text-dim, #8b949e); }

  .tt-footer {
    padding: 6px 10px 8px;
    border-top: 1px solid var(--la-hair-faint, rgba(255,255,255,0.06));
  }

  .tt-action {
    background: none;
    border: none;
    padding: 0;
    color: var(--la-focus-ring, #58a6ff);
    font: inherit;
    font-size: 9px;
    letter-spacing: 0.06em;
    cursor: pointer;
  }

  .tt-action:hover { text-decoration: underline; }

  .tt-empty {
    padding: 12px 10px;
    color: var(--la-text-mute, #6e7681);
    font-size: 9px;
  }

  @media (prefers-reduced-motion: reduce) {
    .branch-tooltip { animation: none; }
  }
</style>
