<script lang="ts">
  import type { DomainAgent } from '$lib/squadComm';
  import PhaseSquares from './PhaseSquares.svelte';
  import StatusPip from './StatusPip.svelte';
  import { DOMAIN_AGENT_COLORS } from '$lib/design-tokens';

  let {
    agentId,
    state = 'idle',
    phases = [],
    tool,
    metrics = { files: 0, tokens: 0, ms: 0 },
    justDispatched = false,
    onclick,
  }: {
    agentId: DomainAgent;
    state?: 'idle' | 'pending' | 'running' | 'complete' | 'failed';
    phases?: { id: string; status: 'pending' | 'active' | 'complete' }[];
    tool?: string;
    metrics?: { files: number; tokens: number; ms: number };
    justDispatched?: boolean;
    onclick?: () => void;
  } = $props();

  let agentColor = $derived(DOMAIN_AGENT_COLORS[agentId] ?? 'var(--la-text-mute)');

  function pipState(): 'idle' | 'active' | 'complete' | 'failed' {
    if (state === 'running' || state === 'pending') return 'active';
    if (state === 'complete') return 'complete';
    if (state === 'failed') return 'failed';
    return 'idle';
  }

  function pipShape(): 'filled' | 'outlined' | 'x' {
    if (state === 'failed') return 'x';
    if (state === 'idle') return 'outlined';
    return 'filled';
  }

  function formatTokens(n: number): string {
    if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
    return String(n);
  }

  function formatMs(n: number): string {
    if (n >= 60000) return `${Math.floor(n / 60000)}m`;
    if (n >= 1000)  return `${(n / 1000).toFixed(1)}s`;
    return `${n}ms`;
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions a11y_no_noninteractive_tabindex -->
<div
  class="agent-rail"
  data-agent={agentId}
  data-state={state}
  data-just-dispatched={justDispatched ? 'true' : undefined}
  style:--rc={agentColor}
  role={onclick ? 'button' : 'listitem'}
  tabindex={onclick ? 0 : undefined}
  {onclick}
  onkeydown={onclick ? (e) => (e.key === 'Enter' || e.key === ' ') && onclick?.() : undefined}
>
  <!-- Edge: agent-color indicator bar -->
  <div class="rail-edge" aria-hidden="true"></div>

  <!-- Scan overlay: running state horizontal sweep -->
  <div class="rail-scan" aria-hidden="true"></div>

  <!-- ID column -->
  <div class="rail-id">
    <span class="rail-id-code">{agentId.toUpperCase()}</span>
    <span class="rail-id-name">{agentId}</span>
  </div>

  <!-- Phase squares column -->
  <div class="rail-phases">
    <PhaseSquares {phases} {agentColor} {justDispatched} />
  </div>

  <!-- Tool column -->
  <div class="rail-tool">
    {#if state === 'running' && tool}
      {@const parts = tool.match(/^(\w+)\s+(.+)$/)}
      {#if parts}
        <span class="tool-action">{parts[1]}</span>
        <span class="tool-target">{parts[2]}</span>
      {:else}
        <span class="tool-target">{tool}</span>
      {/if}
      <span class="tool-cursor" aria-hidden="true"></span>
    {:else if state === 'idle' || state === 'pending'}
      <span class="rail-tool-empty">—</span>
    {:else if state === 'complete'}
      <span class="rail-tool-empty">done</span>
    {:else if state === 'failed'}
      <span class="rail-tool-empty">failed</span>
    {/if}
  </div>

  <!-- Metrics column -->
  <div class="rail-metrics" aria-label="files: {metrics.files}, tokens: {formatTokens(metrics.tokens)}, elapsed: {formatMs(metrics.ms)}">
    <span class="rail-metric">
      <span class="lbl">FILES</span>
      <span class="val">{metrics.files}</span>
    </span>
    <span class="rail-metric">
      <span class="lbl">TOK</span>
      <span class="val">{formatTokens(metrics.tokens)}</span>
    </span>
    <span class="rail-metric">
      <span class="lbl">TIME</span>
      <span class="val">{formatMs(metrics.ms)}</span>
    </span>
  </div>

  <!-- Tail: status pip -->
  <div class="rail-tail">
    <StatusPip
      color={agentColor}
      state={pipState()}
      shape={pipShape()}
      ariaLabel="{agentId} is {state}"
    />
  </div>
</div>

<style>
  .agent-rail {
    --rc: var(--la-text-mute);
    display: grid;
    grid-template-columns: 4px 142px 188px 1fr 152px 18px;
    gap: 8px;
    align-items: center;
    min-height: 52px;
    border-bottom: 1px solid var(--la-hair-faint);
    padding-right: 16px;
    position: relative;
    overflow: hidden;
    transition: background var(--la-t-snap);
    cursor: default;
  }
  .agent-rail:last-child { border-bottom: none; }
  .agent-rail:hover { background: var(--la-bg-elev-1); }
  [role="button"].agent-rail { cursor: pointer; }

  /* Edge bar */
  .rail-edge {
    height: 100%;
    background: var(--rc);
    opacity: 0.15;
    transform-origin: top;
    transition: opacity var(--la-t-base);
  }
  .agent-rail[data-state="running"] .rail-edge,
  .agent-rail[data-state="pending"] .rail-edge {
    opacity: 1;
    animation: edge-breathe 2.4s ease-in-out infinite;
  }
  .agent-rail[data-state="complete"] .rail-edge { opacity: 0.7; }
  .agent-rail[data-state="failed"]   .rail-edge { background: var(--la-agent-security); opacity: 1; }

  /* Just-dispatched edge fire */
  .agent-rail[data-just-dispatched="true"] .rail-edge {
    animation: rail-fire 0.5s var(--la-ease-mech) backwards;
  }

  @keyframes edge-breathe {
    0%, 100% { opacity: 1; }
    50%      { opacity: 0.65; }
  }
  @keyframes rail-fire {
    0%   { transform: scaleY(0); opacity: 0; }
    60%  { transform: scaleY(1); opacity: 1; }
    100% { opacity: 1; }
  }

  /* Scan sweep (running) */
  .rail-scan {
    position: absolute;
    inset: 0;
    pointer-events: none;
    opacity: 0;
  }
  .agent-rail[data-state="running"] .rail-scan {
    opacity: 1;
    background: linear-gradient(90deg, transparent 0%, transparent 70%, color-mix(in srgb, var(--rc) 12%, transparent) 100%);
    animation: scan-sweep 3.5s linear infinite;
  }
  @keyframes scan-sweep {
    0%   { transform: translateX(-100%); }
    100% { transform: translateX(0%); }
  }

  /* ID column */
  .rail-id {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding-left: 8px;
  }
  .rail-id-code {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: var(--la-tk-mid);
    color: var(--la-text-base);
    font-variant-numeric: tabular-nums;
    transition: color var(--la-t-base);
  }
  .agent-rail[data-state="running"] .rail-id-code,
  .agent-rail[data-state="pending"] .rail-id-code,
  .agent-rail[data-state="complete"] .rail-id-code { color: var(--rc); }
  .agent-rail[data-state="failed"]   .rail-id-code { color: var(--la-agent-security); }
  .agent-rail[data-state="idle"]     .rail-id-code { opacity: 0.32; }

  .rail-id-name {
    font-size: 8px;
    letter-spacing: var(--la-tk-loose);
    color: var(--la-text-mute);
    text-transform: uppercase;
  }

  /* Phases column */
  .rail-phases {
    display: flex;
    align-items: center;
  }
  .agent-rail[data-state="idle"] .rail-phases { opacity: 0.32; }

  /* Tool column */
  .rail-tool {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--la-text-bright);
    letter-spacing: var(--la-tk-tight);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-variant-numeric: tabular-nums;
  }
  .agent-rail[data-state="idle"] .rail-tool { opacity: 0.32; }

  .tool-action {
    color: var(--rc);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: var(--la-tk-mid);
    font-size: 10px;
  }
  .tool-cursor {
    display: inline-block;
    width: 7px;
    height: 12px;
    background: var(--la-text-stark);
    margin-left: 4px;
    vertical-align: middle;
    animation: cursor-blink 1s steps(2) infinite;
  }
  .rail-tool-empty {
    color: var(--la-text-mute);
    font-style: italic;
    letter-spacing: var(--la-tk-mid);
    text-transform: uppercase;
    font-size: 10px;
  }

  @keyframes cursor-blink {
    0%, 49%   { opacity: 1; }
    50%, 100% { opacity: 0; }
  }

  /* Metrics column */
  .rail-metrics {
    display: flex;
    gap: 8px;
    font-size: 10px;
    color: var(--la-text-dim);
    font-variant-numeric: tabular-nums;
    letter-spacing: var(--la-tk-mid);
    justify-content: flex-end;
  }
  .agent-rail[data-state="idle"] .rail-metrics { opacity: 0.32; }

  .rail-metric { display: flex; gap: 4px; align-items: baseline; }
  .lbl { color: var(--la-text-mute); font-size: 9px; }
  .val { color: var(--la-text-bright); font-weight: 700; }

  /* Tail column */
  .rail-tail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  /* scan-sweep color-mix fallback */
  @supports not (color: color-mix(in srgb, red 50%, blue)) {
    .agent-rail[data-state="running"] .rail-scan {
      background: linear-gradient(90deg, transparent 0%, transparent 70%, rgba(255, 255, 255, 0.06) 100%);
    }
  }
</style>
