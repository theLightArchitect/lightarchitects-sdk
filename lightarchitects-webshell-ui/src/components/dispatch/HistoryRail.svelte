<script lang="ts">
  import {
    DOMAIN_AGENT_COLORS,
    type DispatchHistoryEntry,
    type DomainAgent,
  } from '$lib/dispatch';

  interface Props {
    history?: DispatchHistoryEntry[];
    onSelect?: (entry: DispatchHistoryEntry) => void;
    onClear?: () => void;
  }

  let { history = [], onSelect, onClear }: Props = $props();

  function statusColor(status: DispatchHistoryEntry['status']): string {
    switch (status) {
      case 'complete':  return 'var(--la-agent-researcher)';
      case 'error':     return 'var(--la-agent-security)';
      case 'cancelled': return 'var(--la-text-dim)';
      case 'running':   return 'var(--la-agent-performance)';
    }
  }

  function statusGlyph(status: DispatchHistoryEntry['status']): string {
    switch (status) {
      case 'complete':  return '✓';
      case 'error':     return '✗';
      case 'cancelled': return '⊘';
      case 'running':   return '▶';
    }
  }

  function agentDots(agents: DomainAgent[]): string[] {
    return agents.slice(0, 6).map((a) => DOMAIN_AGENT_COLORS[a]);
  }

  function relativeTime(ts: number): string {
    const diff = Math.floor((Date.now() - ts) / 1000);
    if (diff < 60) return `${diff}s`;
    if (diff < 3600) return `${Math.floor(diff / 60)}m`;
    return `${Math.floor(diff / 3600)}h`;
  }
</script>

<div class="history-strip">
  <span class="history-label">HISTORY</span>

  {#if history.length === 0}
    <span class="history-empty">— no past dispatches —</span>
  {:else}
    <div class="history-pills" role="list">
      {#each history as entry (entry.id)}
        <button
          class="history-pill"
          onclick={() => onSelect?.(entry)}
          title={entry.task}
        >
          <span class="pill-glyph" style="color: {statusColor(entry.status)}">
            {statusGlyph(entry.status)}
          </span>
          <span class="pill-task">
            {entry.task.length > 36 ? entry.task.slice(0, 36) + '…' : entry.task}
          </span>
          <span class="pill-dots" aria-hidden="true">
            {#each agentDots(entry.agents) as color}
              <span class="pill-dot" style="background: {color}"></span>
            {/each}
          </span>
          <span class="pill-time">{relativeTime(entry.startedAt)}</span>
        </button>
      {/each}
    </div>

    <button class="clear-btn" onclick={onClear} aria-label="Clear history">CLR</button>
  {/if}
</div>

<style>
  .history-strip {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 100%;
    padding: 0 16px;
    overflow: hidden;
  }

  .history-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.18em;
    color: var(--la-text-mute);
    flex-shrink: 0;
    text-transform: uppercase;
  }

  .history-empty {
    font-size: 9px;
    color: var(--la-text-mute);
    font-style: italic;
    letter-spacing: 0.08em;
  }

  .history-pills {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    flex: 1;
    min-width: 0;
    scrollbar-width: none;
  }
  .history-pills::-webkit-scrollbar { display: none; }

  .history-pill {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 8px;
    height: 24px;
    border: 1px solid var(--la-hair-base);
    background: transparent;
    color: var(--la-text-dim);
    font-family: inherit;
    font-size: 9px;
    letter-spacing: 0.02em;
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
    transition: border-color 80ms, background 80ms;
  }
  .history-pill:hover {
    border-color: var(--la-hair-strong);
    background: var(--la-bg-elev-1, #111214);
    color: var(--la-text-bright);
  }

  .pill-glyph {
    font-size: 9px;
    font-weight: 700;
    flex-shrink: 0;
  }

  .pill-task {
    color: var(--la-text-base);
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .pill-dots {
    display: flex;
    gap: 2px;
    align-items: center;
    flex-shrink: 0;
  }

  .pill-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    display: inline-block;
  }

  .pill-time {
    color: var(--la-text-mute);
    font-size: 8px;
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }

  .clear-btn {
    flex-shrink: 0;
    background: transparent;
    border: 1px solid var(--la-hair-base);
    color: var(--la-text-mute);
    font-family: inherit;
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.12em;
    padding: 2px 6px;
    cursor: pointer;
    transition: border-color 80ms, color 80ms;
  }
  .clear-btn:hover {
    border-color: var(--la-agent-security);
    color: var(--la-agent-security);
  }
</style>
