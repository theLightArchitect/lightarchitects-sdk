<script lang="ts">
  import {
    DOMAIN_AGENTS,
    DOMAIN_AGENT_COLORS,
    DOMAIN_AGENT_LABELS,
    type DomainAgent,
    type Classification,
  } from '$lib/dispatch';

  const AGENT_META: Record<DomainAgent, { code: string; gate: string; perm: 'W' | 'R' }> = {
    engineer:   { code: 'ENG', gate: 'A', perm: 'W' },
    quality:    { code: 'QLT', gate: 'Q', perm: 'W' },
    security:   { code: 'SEC', gate: 'S', perm: 'R' },
    ops:        { code: 'OPS', gate: 'O', perm: 'W' },
    researcher: { code: 'RES', gate: 'R', perm: 'R' },
    knowledge:  { code: 'KNW', gate: 'K', perm: 'R' },
    testing:    { code: 'TST', gate: 'T', perm: 'W' },
    squad:      { code: 'SQD', gate: 'SQ', perm: 'R' },
  };

  interface Props {
    selected?: DomainAgent[];
    classification?: Classification | null;
    disabled?: boolean;
    showValidation?: boolean;
    onchange?: (agents: DomainAgent[]) => void;
  }

  let {
    selected = $bindable([]),
    classification = null,
    disabled = false,
    showValidation = false,
    onchange,
  }: Props = $props();

  function toggle(agent: DomainAgent) {
    if (disabled) return;
    const next = selected.includes(agent)
      ? selected.filter((a) => a !== agent)
      : [...selected, agent];
    selected = next;
    onchange?.(next);
  }

  function applyClassification() {
    if (!classification || disabled) return;
    selected = [...classification.agents];
    onchange?.(selected);
  }

  function selectAll() {
    if (disabled) return;
    selected = [...DOMAIN_AGENTS];
    onchange?.(selected);
  }

  function clearAll() {
    if (disabled) return;
    selected = [];
    onchange?.(selected);
  }
</script>

<div class="cls-panel" data-testid="agent-selector">
  <div class="cls-header">
    <span class="cls-header-label">
      AGENTS · <span class="cls-count">{selected.length} QUEUED</span>
    </span>
    <div class="cls-actions">
      {#if classification?.agents.length}
        <button
          class="cls-action-btn cls-action-auto"
          onclick={applyClassification}
          {disabled}
          title="Apply auto-classification"
        >AUTO·{classification.agents.length}</button>
      {/if}
      <button class="cls-action-btn" onclick={selectAll} {disabled}>ALL</button>
      <button class="cls-action-btn" onclick={clearAll} {disabled}>CLR</button>
    </div>
  </div>

  <div class="cls-chips">
    {#each DOMAIN_AGENTS as agent}
      {@const meta = AGENT_META[agent]}
      {@const color = DOMAIN_AGENT_COLORS[agent]}
      {@const isSelected = selected.includes(agent)}
      {@const isSuggested = classification?.agents.includes(agent) ?? false}
      <button
        class="cls-chip"
        class:selected={isSelected}
        class:suggested={isSuggested && !isSelected}
        onclick={() => toggle(agent)}
        {disabled}
        data-agent={agent}
        data-perm={meta.perm}
        data-testid="agent-btn-{agent.toLowerCase()}"
        aria-pressed={isSelected}
        style="--cc: {color}"
      >
        <span class="chip-code">{meta.code}</span>
        <span class="chip-name">{DOMAIN_AGENT_LABELS[agent]}</span>
        <span class="chip-gate">GATE · {meta.gate}</span>
        <span class="chip-perm">{meta.perm}</span>
      </button>
    {/each}
  </div>

  {#if classification?.rationale}
    <p class="cls-rationale">{classification.rationale}</p>
  {/if}

  {#if showValidation && selected.length === 0 && !disabled}
    <p class="cls-warn" role="alert">↑ Select at least one agent above to dispatch.</p>
  {/if}
</div>

<style>
  .cls-panel {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .cls-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .cls-header-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--la-text-mute);
  }

  .cls-count {
    color: var(--la-text-base);
  }

  .cls-actions {
    display: flex;
    gap: 4px;
  }

  .cls-action-btn {
    background: transparent;
    border: 1px solid var(--la-hair-base);
    color: var(--la-text-mute);
    font-family: inherit;
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    padding: 2px 6px;
    cursor: pointer;
    transition: border-color 80ms, color 80ms;
  }
  .cls-action-btn:hover:not(:disabled) {
    border-color: var(--la-hair-strong);
    color: var(--la-text-base);
  }
  .cls-action-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  .cls-action-auto {
    border-color: var(--la-agent-researcher);
    color: var(--la-agent-researcher);
  }
  .cls-action-auto:hover:not(:disabled) {
    background: color-mix(in srgb, var(--la-agent-researcher) 12%, transparent);
  }

  /* chip grid — 3 per row */
  .cls-chips {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 3px;
  }

  .cls-chip {
    --cc: var(--la-text-mute);
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px 7px;
    border: 1px solid var(--la-hair-base);
    background: transparent;
    font-family: inherit;
    cursor: pointer;
    transition: border-color 80ms, background 80ms;
    position: relative;
    text-align: left;
  }
  .cls-chip:hover:not(:disabled) {
    border-color: var(--la-hair-strong);
    background: var(--la-bg-elev-1);
  }
  .cls-chip:disabled { opacity: 0.4; cursor: not-allowed; }

  /* suggested: dot indicator */
  .cls-chip.suggested {
    border-color: color-mix(in srgb, var(--cc) 40%, transparent);
  }
  .cls-chip.suggested::after {
    content: '';
    position: absolute;
    top: 3px;
    right: 3px;
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--cc);
    opacity: 0.7;
  }

  /* selected */
  .cls-chip.selected {
    border-color: var(--cc);
    background: color-mix(in srgb, var(--cc) 10%, transparent);
  }

  .chip-code {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--la-text-base);
    transition: color 80ms;
  }
  .cls-chip.selected .chip-code { color: var(--cc); }

  .chip-name {
    font-size: 7px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--la-text-mute);
  }

  .chip-gate {
    font-size: 7px;
    letter-spacing: 0.1em;
    color: var(--la-text-mute);
    opacity: 0.6;
  }

  .chip-perm {
    position: absolute;
    top: 4px;
    right: 6px;
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-mute);
    opacity: 0.5;
  }
  .cls-chip.selected .chip-perm {
    color: var(--cc);
    opacity: 0.7;
  }
  .cls-chip[data-perm="W"] .chip-perm { color: var(--la-agent-engineer); }
  .cls-chip.selected[data-perm="W"]   .chip-perm { color: var(--la-agent-engineer); opacity: 1; }

  .cls-rationale {
    font-size: 9px;
    color: var(--la-text-mute);
    line-height: 1.5;
    margin: 0;
    padding: 0 2px;
    font-style: italic;
  }

  .cls-warn {
    font-size: 9px;
    color: var(--la-agent-security);
    opacity: 0.8;
    margin: 0;
    padding: 0 2px;
  }

  @supports not (color: color-mix(in srgb, red 50%, blue)) {
    .cls-chip.selected { background: rgba(255, 255, 255, 0.06); }
    .cls-action-auto:hover:not(:disabled) { background: rgba(255, 255, 255, 0.04); }
  }
</style>
