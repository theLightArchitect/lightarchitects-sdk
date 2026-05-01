<script lang="ts">
  import {
    DOMAIN_AGENT_COLORS,
    DOMAIN_AGENT_LABELS,
    type DispatchEvent,
    type DomainAgent,
    isComplete,
    isError,
  } from '$lib/dispatch';

  interface MailLine {
    id: number;
    time: string;
    from: DomainAgent | null;
    to: DomainAgent | null;
    body: string;
    kind: 'mailbox' | 'state' | 'complete' | 'error';
  }

  interface Props {
    events?: DispatchEvent[];
    maxLines?: number;
    label?: string;
    msgCount?: number;
  }

  let { events = [], maxLines = 200, label, msgCount }: Props = $props();

  let scrollEl: HTMLDivElement | null = null;

  function timestamp(): string {
    const t = new Date();
    return (
      String(t.getHours()).padStart(2, '0') + ':' +
      String(t.getMinutes()).padStart(2, '0') + ':' +
      String(t.getSeconds()).padStart(2, '0') + '.' +
      String(t.getMilliseconds()).padStart(3, '0')
    );
  }

  // Derive lines from events — newest-first (prepend model matching prototype)
  const lines = $derived.by((): MailLine[] => {
    const result: MailLine[] = [];
    let seq = 0;
    for (const e of events) {
      if ('MailboxMessage' in e) {
        const { agent, text } = e.MailboxMessage;
        // Parse "to: body" pattern from text if present
        const colonIdx = text.indexOf(':');
        let to: DomainAgent | null = null;
        let body = text;
        if (colonIdx > 0) {
          const candidate = text.slice(0, colonIdx).trim().toLowerCase() as DomainAgent;
          if (DOMAIN_AGENT_LABELS[candidate]) {
            to = candidate;
            body = text.slice(colonIdx + 1).trim();
          }
        }
        result.push({ id: seq++, time: timestamp(), from: agent, to, body, kind: 'mailbox' });
      } else if ('PerAgentState' in e) {
        const { agent, state, message } = e.PerAgentState;
        if (message) {
          result.push({
            id: seq++, time: timestamp(), from: agent, to: null,
            body: `[${state}] ${message}`, kind: 'state',
          });
        }
      } else if (isComplete(e)) {
        result.push({
          id: seq++, time: timestamp(), from: null, to: null,
          body: `Dispatch complete in ${(e.Complete.elapsed_ms / 1000).toFixed(1)}s`,
          kind: 'complete',
        });
      } else if (isError(e)) {
        result.push({
          id: seq++, time: timestamp(), from: e.Error.agent, to: null,
          body: e.Error.message, kind: 'error',
        });
      }
    }
    // Newest-first: reverse so the latest event is at top
    return result.slice(-maxLines).reverse();
  });

  // Scroll to top when new messages arrive (newest at top)
  $effect(() => {
    void events.length;
    if (scrollEl) scrollEl.scrollTop = 0;
  });

  function fromColor(agent: DomainAgent | null): string {
    if (!agent) return 'var(--la-text-base)';
    return DOMAIN_AGENT_COLORS[agent] ?? 'var(--la-text-mute)';
  }

  function toColor(agent: DomainAgent | null): string {
    if (!agent) return 'var(--la-text-base)';
    return DOMAIN_AGENT_COLORS[agent] ?? 'var(--la-text-mute)';
  }
</script>

<!-- mailbox section header is rendered by parent (Dispatch.svelte / SquadDispatch.svelte) -->
<div
  bind:this={scrollEl}
  class="mailbox-stream"
  data-testid="mailbox-stream"
>
  {#if lines.length === 0}
    <div class="mb-empty">— awaiting transmission —</div>
  {:else}
    {#each lines as line (line.id)}
      <div class="mb-msg" data-kind={line.kind}>
        <span class="mb-time">{line.time}</span>
        <span class="mb-from" style="color: {fromColor(line.from)}">
          {line.from ? (DOMAIN_AGENT_LABELS[line.from] ?? line.from) : (line.kind === 'complete' ? 'system' : '◀ replay')}
        </span>
        <span class="mb-arrow" aria-hidden="true">▸</span>
        <span class="mb-body">
          {#if line.to}
            <span class="mb-to" style="color: {toColor(line.to)}">
              {DOMAIN_AGENT_LABELS[line.to] ?? line.to}
            </span>{' '}
          {/if}
          {line.body}
        </span>
      </div>
    {/each}
  {/if}
</div>

<style>
  .mailbox-stream {
    height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-width: thin;
    scrollbar-color: var(--la-hair-base) transparent;
  }
  .mailbox-stream::-webkit-scrollbar { width: 4px; }
  .mailbox-stream::-webkit-scrollbar-track { background: transparent; }
  .mailbox-stream::-webkit-scrollbar-thumb { background: var(--la-hair-base); }

  .mb-empty {
    padding: 8px 12px;
    font-size: 9px;
    color: var(--la-text-mute);
    font-style: italic;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  /* mb-msg grid: time(100px) from(110px) arrow(16px) body(1fr) */
  .mb-msg {
    display: grid;
    grid-template-columns: 100px 110px 16px 1fr;
    align-items: baseline;
    gap: 0;
    padding: 2px 12px;
    border-bottom: 1px solid var(--la-hair-faint);
    font-size: 9px;
    font-variant-numeric: tabular-nums;
    line-height: 1.6;
    animation: msg-arrive 0.15s ease-out backwards;
  }

  @keyframes msg-arrive {
    from { opacity: 0; transform: translateY(-2px); }
    to   { opacity: 1; transform: none; }
  }

  .mb-time {
    color: var(--la-text-mute);
    letter-spacing: 0.02em;
    font-size: 8px;
    flex-shrink: 0;
  }

  .mb-from {
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding-right: 6px;
  }

  .mb-arrow {
    color: var(--la-text-mute);
    font-size: 9px;
    text-align: center;
  }

  .mb-body {
    color: var(--la-text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding-left: 4px;
  }

  .mb-to {
    font-weight: 700;
    text-transform: uppercase;
    font-size: 9px;
    letter-spacing: 0.04em;
  }

  /* kind-specific coloring */
  .mb-msg[data-kind="complete"] .mb-from { color: var(--la-agent-researcher); }
  .mb-msg[data-kind="error"]    .mb-from { color: var(--la-agent-security); }
  .mb-msg[data-kind="error"]    .mb-body { color: var(--la-agent-security); opacity: 0.8; }
</style>
