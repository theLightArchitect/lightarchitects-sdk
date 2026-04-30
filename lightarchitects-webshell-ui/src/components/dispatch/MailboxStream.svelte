<script lang="ts">
  import { onMount } from 'svelte';
  import {
    DOMAIN_AGENT_COLORS,
    DOMAIN_AGENT_LABELS,
    type DispatchEvent,
    type DomainAgent,
    isComplete,
    isError,
  } from '$lib/dispatch';

  interface LogLine {
    id: number;
    timestamp: string;
    kind: 'state' | 'mailbox' | 'complete' | 'error';
    agent: DomainAgent | null;
    text: string;
  }

  interface Props {
    events?: DispatchEvent[];
    maxLines?: number;
  }

  let { events = [], maxLines = 200 }: Props = $props();

  let scrollEl: HTMLDivElement | null = null;
  let autoScroll = $state(true);

  const lines = $derived.by(() => {
    const result: LogLine[] = [];
    let seq = 0;
    for (const e of events) {
      const ts = new Date().toLocaleTimeString('en-US', { hour12: false });
      if ('PerAgentState' in e) {
        const { agent, state, message } = e.PerAgentState;
        result.push({
          id: seq++,
          timestamp: ts,
          kind: 'state',
          agent,
          text: message ? `[${state}] ${message}` : `→ ${state}`,
        });
      } else if ('MailboxMessage' in e) {
        const { agent, text } = e.MailboxMessage;
        result.push({ id: seq++, timestamp: ts, kind: 'mailbox', agent, text });
      } else if (isComplete(e)) {
        result.push({
          id: seq++,
          timestamp: ts,
          kind: 'complete',
          agent: null,
          text: `Dispatch complete in ${(e.Complete.elapsed_ms / 1000).toFixed(1)}s`,
        });
      } else if (isError(e)) {
        result.push({
          id: seq++,
          timestamp: ts,
          kind: 'error',
          agent: e.Error.agent,
          text: e.Error.message,
        });
      }
    }
    return result.slice(-maxLines);
  });

  $effect(() => {
    // Reactive to events length
    void events.length;
    if (autoScroll && scrollEl) {
      scrollEl.scrollTop = scrollEl.scrollHeight;
    }
  });

  function handleScroll() {
    if (!scrollEl) return;
    const nearBottom = scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight < 40;
    autoScroll = nearBottom;
  }

  function lineColor(line: LogLine): string {
    if (line.kind === 'complete') return '#10b981';
    if (line.kind === 'error') return '#ef4444';
    if (line.agent) return DOMAIN_AGENT_COLORS[line.agent] ?? '#94a3b8';
    return '#94a3b8';
  }
</script>

<div
  bind:this={scrollEl}
  onscroll={handleScroll}
  class="h-full overflow-y-auto font-mono text-[9px] leading-relaxed space-y-0.5
         scrollbar-thin scrollbar-thumb-[#1e293b]"
>
  {#if lines.length === 0}
    <div class="text-[#334155] italic py-2 text-center">Waiting for events…</div>
  {/if}

  {#each lines as line (line.id)}
    <div class="flex gap-2 py-0.5 border-b border-[#0f172a]">
      <span class="flex-shrink-0 text-[#334155] w-14 text-right">{line.timestamp}</span>
      {#if line.agent}
        <span class="flex-shrink-0 w-16 truncate" style="color: {DOMAIN_AGENT_COLORS[line.agent]}">
          {DOMAIN_AGENT_LABELS[line.agent]}
        </span>
      {:else}
        <span class="flex-shrink-0 w-16 text-[#475569]">system</span>
      {/if}
      <span style="color: {lineColor(line)}" class="break-all">{line.text}</span>
    </div>
  {/each}
</div>

{#if !autoScroll && lines.length > 0}
  <button
    onclick={() => { autoScroll = true; scrollEl?.scrollTo({ top: scrollEl.scrollHeight, behavior: 'smooth' }); }}
    class="absolute bottom-2 right-2 px-2 py-0.5 text-[9px] rounded bg-[#1e293b]
           text-[#94a3b8] border border-[#334155] hover:border-[#475569]"
  >
    ↓ bottom
  </button>
{/if}
