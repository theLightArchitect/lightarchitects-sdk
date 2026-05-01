<script lang="ts">
  import {
    DOMAIN_AGENT_COLORS,
    DOMAIN_AGENT_LABELS,
    type DomainAgent,
    type AgentLiveState,
  } from '$lib/dispatch';

  interface Props {
    agent: DomainAgent;
    live: AgentLiveState | undefined;
    onClose: () => void;
    onRetry?: (agent: DomainAgent) => void;
  }

  let { agent, live, onClose, onRetry }: Props = $props();

  const PHASES = ['CLASSIFY', 'PLAN', 'EXEC', 'VERIFY', 'REPORT'] as const;
  type PhaseStatus = 'pending' | 'active' | 'complete';

  const agentColor = $derived(DOMAIN_AGENT_COLORS[agent] ?? 'var(--la-focus-ring)');
  const state = $derived(live?.state ?? 'idle');
  const label = $derived(DOMAIN_AGENT_LABELS[agent]);

  function phaseStatus(pIdx: number): PhaseStatus {
    const st = live?.state;
    if (!st || st === 'failed' || st === 'cancelled') return 'pending';
    if (st === 'complete') return 'complete';
    const active = Math.min(Math.floor((live?.messages.length ?? 0) / 2), PHASES.length - 1);
    if (pIdx < active) return 'complete';
    if (pIdx === active) return 'active';
    return 'pending';
  }

  function stateLabel(s: string): string {
    switch (s) {
      case 'pending':    return 'QUEUED';
      case 'running':    return 'RUNNING';
      case 'complete':   return 'DONE';
      case 'failed':     return 'FAILED';
      case 'cancelled':  return 'CANCELLED';
      default:           return 'STANDBY';
    }
  }

  let messagesEl: HTMLElement | null = null;

  $effect(() => {
    // Auto-scroll message log when new messages arrive
    if (live?.messages && messagesEl) {
      messagesEl.scrollTop = messagesEl.scrollHeight;
    }
  });

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div
  class="agent-detail"
  style="--dc: {agentColor};"
  data-testid="agent-detail-{agent}"
  role="complementary"
  aria-label="{label} agent detail"
>
  <div class="detail-edge"></div>

  <div class="detail-body">
    <header class="detail-header">
      <div class="detail-agent-info">
        <span class="detail-agent-label">{label}</span>
        <span
          class="detail-state-badge"
          class:state-running={state === 'running' || state === 'pending'}
          class:state-complete={state === 'complete'}
          class:state-failed={state === 'failed'}
        >{stateLabel(state)}</span>
      </div>
      <button class="detail-close" onclick={onClose} aria-label="Close agent detail">×</button>
    </header>

    <div class="detail-phases">
      {#each PHASES as pname, pi}
        <div
          class="detail-phase"
          data-status={phaseStatus(pi)}
          title={pname}
        >
          <span class="detail-phase-fill"></span>
          <span class="detail-phase-name">{pname}</span>
        </div>
      {/each}
    </div>

    <div class="detail-messages" bind:this={messagesEl}>
      {#if !live?.messages?.length}
        <span class="detail-messages-empty">— no output yet —</span>
      {:else}
        {#each live.messages as msg, i (i)}
          <div class="detail-msg">{msg}</div>
        {/each}
      {/if}
    </div>

    <footer class="detail-footer">
      <div class="detail-metrics">
        <span class="detail-metric">
          <span class="dm-lbl">FILES</span>
          <span class="dm-val">{live?.files_touched ?? 0}</span>
        </span>
        <span class="detail-metric">
          <span class="dm-lbl">TOK</span>
          <span class="dm-val">{live?.token_count ?? 0}</span>
        </span>
        <span class="detail-metric">
          <span class="dm-lbl">MS</span>
          <span class="dm-val">{live?.elapsed_ms ?? 0}</span>
        </span>
      </div>
      {#if state === 'failed'}
        <button
          class="detail-retry"
          onclick={() => onRetry?.(agent)}
          aria-label="Retry {label}"
        >RTY ↻</button>
      {/if}
    </footer>
  </div>
</div>

<style>
  .agent-detail {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: 340px;
    display: flex;
    flex-direction: row;
    background: var(--la-drawer-bg);
    border-left: 1px solid var(--la-drawer-border);
    z-index: 5;
    animation: detail-slide-in var(--la-transition-med, 200ms) var(--la-ease-mech, ease) both;
    font-family: var(--la-font-chrome);
  }

  @keyframes detail-slide-in {
    from { transform: translateX(100%); opacity: 0; }
    to   { transform: translateX(0);    opacity: 1; }
  }

  .detail-edge {
    width: 3px;
    flex-shrink: 0;
    background: var(--dc);
    opacity: 0.85;
  }

  .detail-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  /* ── header ── */
  .detail-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 10px 6px;
    border-bottom: 1px solid var(--la-drawer-border);
    flex-shrink: 0;
  }
  .detail-agent-info {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .detail-agent-label {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--dc);
    text-transform: uppercase;
  }
  .detail-state-badge {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--la-text-mute);
    font-family: var(--la-font-mono);
  }
  .detail-state-badge.state-running  { color: var(--dc); animation: badge-pulse 1.2s ease-in-out infinite; }
  .detail-state-badge.state-complete { color: var(--la-agent-researcher); }
  .detail-state-badge.state-failed   { color: var(--la-agent-security); }

  @keyframes badge-pulse {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.4; }
  }

  .detail-close {
    background: transparent;
    border: none;
    color: var(--la-text-mute);
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
    padding: 0 2px;
    transition: color var(--la-transition-fast, 100ms);
  }
  .detail-close:hover { color: var(--la-text-body); }

  /* ── phase strip ── */
  .detail-phases {
    display: flex;
    gap: 4px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--la-drawer-border);
    flex-shrink: 0;
  }
  .detail-phase {
    flex: 1;
    height: 20px;
    border: 1px solid var(--la-hair-base);
    position: relative;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    overflow: hidden;
  }
  .detail-phase-fill {
    position: absolute;
    inset: 0;
    background: var(--dc);
    opacity: 0;
    transition: opacity 80ms;
  }
  .detail-phase-name {
    position: relative;
    font-size: 6px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-dim);
    text-transform: uppercase;
    padding-bottom: 2px;
    z-index: 1;
  }
  .detail-phase[data-status="active"] {
    border-color: var(--dc);
  }
  .detail-phase[data-status="active"] .detail-phase-fill {
    opacity: 0.8;
    animation: phase-flicker 0.8s steps(3) infinite;
  }
  .detail-phase[data-status="active"] .detail-phase-name { color: var(--la-bg-frame); }
  .detail-phase[data-status="complete"] {
    border-color: var(--dc);
  }
  .detail-phase[data-status="complete"] .detail-phase-fill { opacity: 0.25; }
  .detail-phase[data-status="complete"] .detail-phase-name { color: var(--dc); }

  @keyframes phase-flicker {
    0%, 60%, 100% { opacity: 0.8; }
    30%, 80%      { opacity: 0.45; }
  }

  /* ── message log ── */
  .detail-messages {
    flex: 1;
    overflow-y: auto;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-height: 0;
    scrollbar-width: thin;
    scrollbar-color: var(--la-hair-strong) transparent;
  }
  .detail-messages-empty {
    font-size: 9px;
    color: var(--la-text-mute);
    font-style: italic;
    letter-spacing: 0.08em;
  }
  .detail-msg {
    font-size: 10px;
    color: var(--la-text-base);
    line-height: 1.5;
    font-family: var(--la-font-mono);
    word-break: break-word;
    padding: 2px 0;
    border-bottom: 1px solid var(--la-hair-faint);
  }
  .detail-msg:last-child { border-bottom: none; }

  /* ── footer metrics ── */
  .detail-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    border-top: 1px solid var(--la-drawer-border);
    flex-shrink: 0;
  }
  .detail-metrics {
    display: flex;
    gap: 14px;
    font-size: 9px;
    font-variant-numeric: tabular-nums;
    font-family: var(--la-font-mono);
  }
  .detail-metric { display: flex; gap: 4px; align-items: baseline; }
  .dm-lbl { color: var(--la-text-mute); font-size: 8px; letter-spacing: 0.1em; }
  .dm-val { color: var(--la-text-bright); font-weight: 700; }

  .detail-retry {
    background: transparent;
    border: 1px solid var(--la-agent-security);
    color: var(--la-agent-security);
    font-family: inherit;
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 2px 8px;
    cursor: pointer;
    transition: background var(--la-transition-fast, 100ms);
  }
  .detail-retry:hover {
    background: var(--la-agent-security);
    color: var(--la-bg-void);
  }
</style>
