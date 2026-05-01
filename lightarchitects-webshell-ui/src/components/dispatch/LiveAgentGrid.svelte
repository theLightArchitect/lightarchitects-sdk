<script lang="ts">
  import {
    DOMAIN_AGENT_LABELS,
    type DomainAgent,
    type AgentLiveState,
  } from '$lib/dispatch';

  interface Props {
    agents: DomainAgent[];
    agentStates?: Map<DomainAgent, AgentLiveState>;
    onRetry?: (agent: DomainAgent) => void;
  }

  let { agents, agentStates = new Map(), onRetry }: Props = $props();

  const AGENT_META: Record<DomainAgent, { code: string; idx: string; gate: string }> = {
    engineer:      { code: 'ENG', idx: '01', gate: 'A' },
    quality:       { code: 'QLT', idx: '02', gate: 'Q' },
    security:      { code: 'SEC', idx: '03', gate: 'S' },
    ops:           { code: 'OPS', idx: '04', gate: 'O' },
    researcher:    { code: 'RES', idx: '05', gate: 'R' },
    knowledge:     { code: 'KNW', idx: '06', gate: 'K' },
    performance:   { code: 'PRF', idx: '07', gate: 'P' },
    testing:       { code: 'TST', idx: '08', gate: 'T' },
    documentation: { code: 'DOC', idx: '09', gate: 'D' },
  };

  const PHASES = ['CLASSIFY', 'PLAN', 'EXEC', 'VERIFY', 'REPORT'] as const;
  type PhaseStatus = 'pending' | 'active' | 'complete';

  function railState(live: AgentLiveState | undefined): string {
    return live?.state ?? 'idle';
  }

  function phaseStatus(live: AgentLiveState | undefined, pIdx: number): PhaseStatus {
    const st = live?.state;
    if (!st) return 'pending';
    if (st === 'complete') return 'complete';
    if (st === 'failed' || st === 'cancelled') return 'pending';
    // running — advance through phases based on message count
    const active = Math.min(Math.floor((live?.messages.length ?? 0) / 2), PHASES.length - 1);
    if (pIdx < active) return 'complete';
    if (pIdx === active) return 'active';
    return 'pending';
  }

  function toolText(live: AgentLiveState | undefined): string {
    return live?.messages.at(-1) ?? '';
  }
</script>

<div class="rails" data-testid="live-agent-grid">
  {#if agents.length === 0}
    <div class="rails-empty">— awaiting dispatch —</div>
  {:else}
    {#each agents as agent}
      {@const live = agentStates.get(agent)}
      {@const state = railState(live)}
      {@const meta = AGENT_META[agent]}
      {@const msg = toolText(live)}

      <div class="rail" data-agent={agent} data-state={state} data-testid="agent-rail-{agent}">
        <div class="rail-edge"></div>
        <div class="rail-scan"></div>

        <div class="rail-id">
          <div class="rail-id-code">{meta.code}/{meta.idx}</div>
          <div class="rail-id-name">{DOMAIN_AGENT_LABELS[agent]}</div>
          <div class="rail-id-gate">GATE · {meta.gate}</div>
        </div>

        <div class="phase-strip">
          {#each PHASES as pname, pi}
            <div
              class="phase-square"
              data-status={phaseStatus(live, pi)}
              title={pname}
            ></div>
          {/each}
        </div>

        <div class="rail-tool">
          {#if msg}
            <span class="tool-action">{meta.code}</span>
            <span class="tool-target">{msg.length > 72 ? msg.slice(0, 72) + '…' : msg}</span>
            {#if state === 'running' || state === 'dispatched'}
              <span class="tool-cursor" aria-hidden="true"></span>
            {/if}
          {:else}
            <span class="rail-tool-empty">— {state === 'idle' ? 'STANDBY' : state.toUpperCase()}</span>
          {/if}
        </div>

        <div class="rail-metrics">
          <span class="rail-metric">
            <span class="lbl">FILES</span>
            <span class="val">{live?.files_touched ?? 0}</span>
          </span>
          <span class="rail-metric">
            <span class="lbl">TOK</span>
            <span class="val">{live?.token_count ?? 0}</span>
          </span>
          <span class="rail-metric">
            <span class="lbl">MS</span>
            <span class="val">{live?.elapsed_ms ?? 0}</span>
          </span>
        </div>

        <div class="rail-tail">
          <span class="status-pip" aria-hidden="true"></span>
        </div>

        {#if state === 'failed'}
          <button
            class="retry-btn"
            onclick={() => onRetry?.(agent)}
            aria-label="Retry {DOMAIN_AGENT_LABELS[agent]}"
          >RTY ↻</button>
        {/if}
      </div>
    {/each}
  {/if}
</div>

<style>
  /* per-agent color tokens */
  .rail[data-agent="engineer"]      { --rc: var(--la-agent-engineer); }
  .rail[data-agent="quality"]       { --rc: var(--la-agent-quality); }
  .rail[data-agent="security"]      { --rc: var(--la-agent-security); }
  .rail[data-agent="ops"]           { --rc: var(--la-agent-ops); }
  .rail[data-agent="researcher"]    { --rc: var(--la-agent-researcher); }
  .rail[data-agent="knowledge"]     { --rc: var(--la-agent-knowledge); }
  .rail[data-agent="performance"]   { --rc: var(--la-agent-performance); }
  .rail[data-agent="testing"]       { --rc: var(--la-agent-testing); }
  .rail[data-agent="documentation"] { --rc: var(--la-agent-documentation); }

  .rails {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .rails-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--la-text-mute);
    font-size: 10px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    font-style: italic;
  }

  /* ── rail row ── */
  .rail {
    --rc: var(--la-text-mute);
    display: grid;
    grid-template-columns: 4px 142px 188px 1fr 152px 18px;
    gap: 12px;
    align-items: center;
    flex: 1;
    min-height: 52px;
    border-bottom: 1px solid var(--la-hair-faint);
    padding-right: 16px;
    position: relative;
    overflow: hidden;
    transition: background 80ms var(--la-ease-mech);
  }
  .rail:last-child { border-bottom: none; }
  .rail:hover { background: var(--la-bg-elev-1, #111214); }

  /* ── colored left edge ── */
  .rail-edge {
    height: 100%;
    background: var(--rc);
    opacity: 0.15;
    transform-origin: top;
    transition: opacity 200ms;
  }
  .rail[data-state="dispatched"] .rail-edge,
  .rail[data-state="running"] .rail-edge {
    opacity: 1;
    animation: edge-breathe 2.4s ease-in-out infinite;
  }
  .rail[data-state="complete"] .rail-edge { opacity: 0.7; }
  .rail[data-state="failed"]   .rail-edge { background: var(--la-agent-security); opacity: 1; }

  @keyframes edge-breathe {
    0%, 100% { opacity: 1; }
    50%      { opacity: 0.65; }
  }

  /* ── rightward scan sweep when running ── */
  .rail-scan {
    position: absolute;
    inset: 0;
    pointer-events: none;
    opacity: 0;
  }
  .rail[data-state="running"] .rail-scan {
    opacity: 1;
    background: linear-gradient(
      90deg,
      transparent 0%,
      transparent 70%,
      color-mix(in srgb, var(--rc) 12%, transparent) 100%
    );
    animation: scan-sweep 3.5s linear infinite;
  }
  @keyframes scan-sweep {
    0%   { transform: translateX(-100%); }
    100% { transform: translateX(0%); }
  }

  /* ── idle dimming ── */
  .rail[data-state="idle"] .rail-id-code,
  .rail[data-state="idle"] .rail-tool,
  .rail[data-state="idle"] .rail-metrics { opacity: 0.32; }

  /* ── agent id block ── */
  .rail-id {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding-left: 8px;
  }
  .rail-id-code {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-base);
    font-variant-numeric: tabular-nums;
    transition: color 200ms;
  }
  .rail[data-state="dispatched"] .rail-id-code,
  .rail[data-state="running"]    .rail-id-code,
  .rail[data-state="complete"]   .rail-id-code { color: var(--rc); }
  .rail[data-state="failed"]     .rail-id-code { color: var(--la-agent-security); }

  .rail-id-name {
    font-size: 8px;
    letter-spacing: 0.18em;
    color: var(--la-text-mute);
    text-transform: uppercase;
  }
  .rail-id-gate {
    font-size: 7px;
    letter-spacing: 0.18em;
    color: var(--la-text-mute);
    opacity: 0.6;
    margin-top: 1px;
  }

  /* ── phase squares (CLASSIFY/PLAN/EXEC/VERIFY/REPORT) ── */
  .phase-strip { display: flex; gap: 3px; }
  .phase-square {
    width: 30px;
    height: 14px;
    border: 1px solid var(--la-hair-base);
    position: relative;
    transition: border-color 80ms;
  }
  .phase-square::after {
    content: "";
    position: absolute;
    inset: 0;
    background: var(--rc);
    opacity: 0;
    transition: opacity 80ms var(--la-ease-mech);
  }
  .phase-square::before {
    content: "";
    position: absolute;
    width: 4px;
    height: 4px;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%) scale(0);
    background: var(--rc);
    transition: transform 80ms var(--la-ease-mech);
  }
  .phase-square[data-status="active"]           { border-color: var(--rc); }
  .phase-square[data-status="active"]::after    { opacity: 0.9; animation: phase-flicker 0.8s steps(3) infinite; }
  .phase-square[data-status="complete"]         { border-color: var(--rc); }
  .phase-square[data-status="complete"]::before { transform: translate(-50%, -50%) scale(1); }

  @keyframes phase-flicker {
    0%, 60%, 100% { opacity: 0.9; }
    30%, 80%      { opacity: 0.5; }
  }

  /* ── current tool / last message ── */
  .rail-tool {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--la-text-bright);
    letter-spacing: 0.02em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-variant-numeric: tabular-nums;
  }
  .tool-action {
    color: var(--rc);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 10px;
    flex-shrink: 0;
  }
  .tool-target {
    color: var(--la-text-bright);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tool-cursor {
    display: inline-block;
    width: 7px;
    height: 12px;
    background: var(--la-text-stark);
    vertical-align: middle;
    animation: cursor-blink 1s steps(2) infinite;
    flex-shrink: 0;
  }
  @keyframes cursor-blink {
    0%, 49%   { opacity: 1; }
    50%, 100% { opacity: 0; }
  }
  .rail-tool-empty {
    color: var(--la-text-mute);
    font-style: italic;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    font-size: 10px;
  }

  /* ── FILES / TOK / MS metrics ── */
  .rail-metrics {
    display: flex;
    gap: 12px;
    font-size: 10px;
    color: var(--la-text-dim);
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.08em;
    justify-content: flex-end;
  }
  .rail-metric { display: flex; gap: 4px; align-items: baseline; }
  .rail-metric .lbl { color: var(--la-text-mute); font-size: 9px; }
  .rail-metric .val { color: var(--la-text-bright); font-weight: 700; }

  /* ── status pip ── */
  .rail-tail {
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .status-pip {
    width: 8px;
    height: 8px;
    background: var(--la-text-mute);
  }
  .rail[data-state="dispatched"] .status-pip,
  .rail[data-state="running"]    .status-pip {
    background: var(--rc);
    animation: pip-blink 1s steps(2) infinite;
    box-shadow: 0 0 6px var(--rc);
  }
  .rail[data-state="complete"] .status-pip {
    background: var(--rc);
    box-shadow: 0 0 4px color-mix(in srgb, var(--rc) 40%, transparent);
  }
  .rail[data-state="failed"] .status-pip {
    background: var(--la-agent-security);
    animation: pip-blink 0.4s steps(2) infinite;
    box-shadow: 0 0 8px var(--la-agent-security);
  }
  @keyframes pip-blink {
    0%, 49%   { opacity: 1; }
    50%, 100% { opacity: 0.2; }
  }

  /* ── retry button (failed state only) ── */
  .retry-btn {
    background: transparent;
    border: 1px solid var(--la-agent-security);
    color: var(--la-agent-security);
    font-family: inherit;
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 1px 5px;
    cursor: pointer;
    position: absolute;
    top: 50%;
    right: 32px;
    transform: translateY(-50%);
    z-index: 2;
  }
  .retry-btn:hover { background: var(--la-agent-security); color: var(--la-bg-void); }
</style>
