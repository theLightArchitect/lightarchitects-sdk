<script lang="ts">
  import type { LogEntry } from '$lib/types';
  import { sourceColor } from '$lib/atmosphere';

  interface Props {
    entries: LogEntry[];
    maxDisplay?: number;
  }

  let { entries, maxDisplay = 100 }: Props = $props();

  const TOOL_ICONS: Record<string, string> = {
    file_read:  '📄',
    file_write: '✍',
    bash:       '⚡',
    mcp:        '🔌',
    reasoning:  '💭',
    search:     '🔍',
    dispatch:   '📡',
  };

  type EntryType = 'reasoning' | 'write' | 'error' | 'tool' | 'info';

  function deriveType(entry: LogEntry): EntryType {
    if (entry.level === 'error') return 'error';
    if (entry.level === 'success') return 'write';
    const src = entry.source.toLowerCase();
    if (src.includes('reason') || src.includes('think')) return 'reasoning';
    if (src.includes('tool') || src.includes('bash') || src.includes('mcp')) return 'tool';
    return 'info';
  }

  function toolIcon(entry: LogEntry): string {
    const msg = entry.message.toLowerCase();
    if (msg.includes('write') || msg.includes('edit')) return TOOL_ICONS.file_write;
    if (msg.includes('read') || msg.includes('open')) return TOOL_ICONS.file_read;
    if (msg.includes('bash') || msg.includes('exec') || msg.includes('run')) return TOOL_ICONS.bash;
    if (msg.includes('mcp') || msg.includes('dispatch')) return TOOL_ICONS.dispatch;
    if (msg.includes('search') || msg.includes('grep') || msg.includes('find')) return TOOL_ICONS.search;
    if (deriveType(entry) === 'reasoning') return TOOL_ICONS.reasoning;
    return TOOL_ICONS.mcp;
  }

  function formatTime(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }

  // Agent presence: unique sources active in last 90 seconds
  const activeAgents = $derived.by(() => {
    const cutoff = Date.now() - 90_000;
    const seen = new Map<string, { lastTs: number; action: string }>();
    for (const e of entries) {
      if (!e.source) continue;
      const ts = new Date(e.timestamp).getTime();
      if (ts < cutoff) continue;
      const existing = seen.get(e.source);
      if (!existing || ts > existing.lastTs) {
        seen.set(e.source, { lastTs: ts, action: e.message.slice(0, 28) });
      }
    }
    return [...seen.entries()].map(([id, v]) => ({
      id,
      color: sourceColor(id),
      action: v.action,
    }));
  });

  let visible = $derived(entries.slice(-maxDisplay));
</script>

<div class="wh-wrap">
  <!-- Agent presence bar -->
  <div class="presence-bar" aria-label="Active agents">
    <span class="presence-label">ACTIVE</span>
    {#if activeAgents.length === 0}
      <span class="presence-none">—</span>
    {:else}
      {#each activeAgents as a}
        <div class="presence-chip" style="--c: {a.color}">
          <span class="chip-dot" aria-hidden="true"></span>
          <span class="chip-id">{a.id.toUpperCase().slice(0, 6)}</span>
          {#if a.action}
            <span class="chip-action">{a.action}</span>
          {/if}
        </div>
      {/each}
    {/if}
  </div>

  <!-- History feed -->
  <div class="history-feed" role="log" aria-live="polite">
    {#if visible.length === 0}
      <div class="history-empty">— waiting for build output —</div>
    {:else}
      {#each visible as entry (entry.id)}
        {@const type = deriveType(entry)}
        {@const icon = toolIcon(entry)}
        <div class="history-entry" data-type={type}>
          <div class="entry-meta">
            <span
              class="agent-tag"
              style="--c: {sourceColor(entry.source)}"
            >{entry.source.toUpperCase().slice(0, 8)}</span>
            <span class="tool-icon" aria-hidden="true">{icon}</span>
            <span class="entry-ts">{formatTime(entry.timestamp)}</span>
          </div>
          <div class="entry-body">{entry.message}</div>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .wh-wrap {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  /* ── presence bar ── */
  .presence-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    padding: 5px 14px;
    border-bottom: 1px solid var(--la-hair-faint);
    background: var(--la-bg-elev-1);
    flex-shrink: 0;
    min-height: 28px;
  }
  .presence-label {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--la-text-mute);
    flex-shrink: 0;
  }
  .presence-none {
    font-size: 9px;
    color: var(--la-text-mute);
    opacity: 0.5;
  }
  .presence-chip {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 7px;
    border: 1px solid var(--c);
    border-radius: 2px;
    font-size: 8px;
    background: color-mix(in srgb, var(--c) 8%, transparent);
  }
  .chip-dot {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--c);
    animation: agent-pulse 1.4s ease-in-out infinite;
    flex-shrink: 0;
  }
  .chip-id {
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--c);
  }
  .chip-action {
    color: var(--la-text-mute);
    font-size: 8px;
    max-width: 100px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @keyframes agent-pulse {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.35; }
  }

  /* ── history feed ── */
  .history-feed {
    flex: 1;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--la-hair-base) transparent;
  }
  .history-empty {
    padding: 24px;
    text-align: center;
    font-size: 10px;
    font-style: italic;
    color: var(--la-text-mute);
    letter-spacing: 0.08em;
  }
  .history-entry {
    padding: 5px 14px;
    border-bottom: 1px solid rgba(71, 85, 105, 0.12);
    border-left: 2px solid transparent;
  }
  .history-entry[data-type="reasoning"] { border-left-color: var(--la-focus-ring, #00c8ff); }
  .history-entry[data-type="write"]     { border-left-color: var(--la-agent-researcher, #4dffe6); }
  .history-entry[data-type="error"]     { border-left-color: var(--la-agent-security, #ff4d4d); }
  .history-entry[data-type="tool"]      { border-left-color: var(--la-agent-performance, #ff8e3c); }
  .entry-meta {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-bottom: 2px;
  }
  .agent-tag {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--c);
    padding: 1px 4px;
    border: 1px solid var(--c);
    border-radius: 2px;
    opacity: 0.9;
  }
  .tool-icon {
    font-size: 10px;
    opacity: 0.6;
  }
  .entry-ts {
    font-size: 8px;
    color: var(--la-text-mute);
    margin-left: auto;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.03em;
  }
  .entry-body {
    font-size: 10px;
    color: var(--la-text-dim);
    line-height: 1.4;
    word-break: break-word;
  }
  .history-entry[data-type="error"] .entry-body { color: var(--la-agent-security, #ff4d4d); opacity: 0.9; }
  .history-entry[data-type="reasoning"] .entry-body { color: var(--la-text-base); font-style: italic; }
</style>
