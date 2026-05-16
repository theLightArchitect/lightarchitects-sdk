<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import {
    activityFeed, pillarStream, logEntries,
    eventsOverlayOpen,
  } from '$lib/stores';
  import { api } from '$lib/api';
  import type { ActivityEntry, SupervisorAlert, PillarUpdatePayload, LogEntry } from '$lib/types';
  import EventStream, { type StreamRow, logLevelToSeverity } from './EventStream.svelte';
  import { sourceColor } from '$lib/atmosphere';

  let { route = '/' }: { route?: string } = $props();

  // ── /api/events/global SSE subscription ───────────────────────────────────

  interface GlobalEntry {
    seq:       number;
    timestamp: string;
    source:    unknown;
    event:     { type?: string; chunk?: string; summary?: string; [k: string]: unknown };
  }

  let globalEntries = $state<{ entry: GlobalEntry; ts: number }[]>([]);
  let lastSeq = $state(0);
  let globalEs: EventSource | null = null;

  function openGlobalSse() {
    globalEs?.close();
    globalEs = api.subscribeGlobalEvents(
      (data) => {
        const e = data as GlobalEntry;
        if (!e?.seq) return;
        if (e.seq <= lastSeq) return;
        lastSeq = e.seq;
        globalEntries = [...globalEntries.slice(-119), { entry: e, ts: new Date(e.timestamp).getTime() }];
      },
      { lastSeq },
    );
    globalEs.onerror = () => {
      // backend may not be running — reconnect silently after 5 s
      setTimeout(openGlobalSse, 5_000);
    };
  }

  onMount(openGlobalSse);
  onDestroy(() => globalEs?.close());

  // ── Unified internal entry type ───────────────────────────────────────────

  type OverlayEntry =
    | { kind: 'activity'; entry: ActivityEntry; ts: number }
    | { kind: 'pillar';   payload: PillarUpdatePayload; ts: number }
    | { kind: 'log';      entry: LogEntry; ts: number };

  // ── Context-aware title from current route ────────────────────────────────

  const SCREEN_TITLES: Record<string, string> = {
    '/ops':      'MISSION CONTROL',
    '/dispatch': 'EXECUTION CONSOLE',
    '/builds':   'PIPELINE',
    '/helix':    'KNOWLEDGE VAULT',
    '/intake':   'NEW MISSION',
    '/project':  'PROJECT',
  };

  let title = $derived.by(() => {
    const path = (route || '/').split('?')[0];
    for (const [prefix, label] of Object.entries(SCREEN_TITLES)) {
      if (path === prefix || path.startsWith(`${prefix}/`)) return label;
    }
    return 'LIVE EVENTS';
  });

  // ── Unread badge ──────────────────────────────────────────────────────────

  let lastSeenCount = $state(0);
  let totalCount = $derived(
    $activityFeed.length + $pillarStream.length + $logEntries.length + globalEntries.length
  );
  let unread = $derived($eventsOverlayOpen ? 0 : Math.max(0, totalCount - lastSeenCount));

  $effect(() => {
    if ($eventsOverlayOpen) {
      lastSeenCount = totalCount;
    }
  });

  // ── Merged feed (newest-first, capped) ────────────────────────────────────

  const MAX_ENTRIES = 120;

  type OverlayEntryWithGlobal =
    | OverlayEntry
    | { kind: 'global'; entry: GlobalEntry; ts: number };

  let merged = $derived.by((): OverlayEntryWithGlobal[] => {
    const out: OverlayEntryWithGlobal[] = [];

    for (const entry of $activityFeed) {
      out.push({ kind: 'activity', entry, ts: entryTimestamp(entry) });
    }
    const now = Date.now();
    for (let i = 0; i < $pillarStream.length; i++) {
      out.push({ kind: 'pillar', payload: $pillarStream[i], ts: now - i });
    }
    for (const entry of $logEntries) {
      out.push({ kind: 'log', entry, ts: new Date(entry.timestamp).getTime() });
    }
    for (const { entry, ts } of globalEntries) {
      out.push({ kind: 'global', entry, ts });
    }

    out.sort((a, b) => b.ts - a.ts);
    return out.slice(0, MAX_ENTRIES);
  });

  function entryTimestamp(e: ActivityEntry): number {
    if (e.source === 'copilot') return new Date(e.event.timestamp).getTime();
    if (e.source === 'ayin')    return new Date(e.span.timestamp).getTime();
    return e.alert.timestamp;
  }

  // ── Convert merged entries to StreamRow[] for EventStream ─────────────────

  function formatTime(ts: number): string {
    const d = new Date(ts);
    return (
      String(d.getHours()).padStart(2, '0') + ':' +
      String(d.getMinutes()).padStart(2, '0') + ':' +
      String(d.getSeconds()).padStart(2, '0')
    );
  }

  function toStreamRow(e: OverlayEntryWithGlobal): StreamRow {
    if (e.kind === 'global') {
      const ev = e.entry.event;
      const text = typeof ev.chunk === 'string'   ? ev.chunk
                 : typeof ev.summary === 'string' ? ev.summary
                 : ev.type ?? 'event';
      return {
        ts:       e.ts,
        time:     formatTime(e.ts),
        source:   'global',
        color:    '#6366f1',
        text,
        severity: 'info',
      };
    }
    if (e.kind === 'log') {
      return {
        ts:       e.ts,
        time:     formatTime(e.ts),
        source:   e.entry.source,
        color:    sourceColor(e.entry.source),
        text:     e.entry.message,
        severity: logLevelToSeverity(e.entry.level),
      };
    }
    if (e.kind === 'pillar') {
      const p = e.payload;
      const text = p.phase === 'output'    ? (p.line ?? '') :
                   p.phase === 'started'   ? `[${p.pillar}] started` :
                   `[${p.pillar}] completed (exit ${p.exit_code ?? '?'})`;
      return {
        ts:       e.ts,
        time:     formatTime(e.ts),
        source:   p.pillar,
        color:    sourceColor('pillar'),
        text,
        severity: p.exit_code !== undefined && p.exit_code !== 0 ? 'err' : 'info',
      };
    }
    // activity
    const ae = e.entry;
    if (ae.source === 'supervisor') {
      const a: SupervisorAlert = ae.alert;
      return {
        ts:       e.ts,
        time:     formatTime(e.ts),
        source:   `${a.sibling}/${a.gate}`,
        color:    a.verdict === 'FAIL' ? '#ef4444' : a.verdict === 'WARN' ? '#f59e0b' : '#22c55e',
        text:     a.message,
        severity: a.verdict === 'FAIL' ? 'err' : a.verdict === 'WARN' ? 'warn' : 'ok',
      };
    }
    if (ae.source === 'ayin') {
      return {
        ts:       e.ts,
        time:     formatTime(e.ts),
        source:   ae.span.actor,
        color:    sourceColor('ayin'),
        text:     `${ae.span.action} (${ae.span.duration_ms}ms)`,
        severity: 'info',
      };
    }
    // copilot
    return {
      ts:       e.ts,
      time:     formatTime(e.ts),
      source:   'copilot',
      color:    sourceColor('eva'),
      text:     ae.event.summary ?? ae.event.kind,
      severity: 'info',
    };
  }

  let streamRows = $derived(merged.map((e) => toStreamRow(e)));

  // ── Keyboard shortcut — E to open, Esc to close ───────────────────────────

  function handleKeyDown(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    const isEventsToggle =
      ((e.key === 'e' || e.key === 'E') && !e.metaKey && !e.ctrlKey && !e.altKey) ||
      ((e.key === 'e' || e.key === 'E') && (e.metaKey || e.ctrlKey) && !e.altKey);
    if (isEventsToggle) {
      e.preventDefault();
      eventsOverlayOpen.update(v => !v);
    } else if (e.key === 'Escape' && $eventsOverlayOpen) {
      eventsOverlayOpen.set(false);
    }
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if $eventsOverlayOpen}
  <!-- Backdrop click to close -->
  <div
    class="fixed inset-0 z-20"
    onclick={() => eventsOverlayOpen.set(false)}
    aria-hidden="true"
  ></div>
{/if}

<!-- Overlay panel — slides in from right, pushes content via app.svelte padding transition -->
<div
  class="fixed top-0 right-0 bottom-0 z-30 flex flex-col"
  style="width: 320px; background: #0a0a0f; border-left: 1px solid #1e293b; transform: translateX({$eventsOverlayOpen ? '0' : '100%'}); transition: transform 260ms cubic-bezier(0.4,0,0.2,1);"
  data-testid="events-overlay"
  aria-hidden={!$eventsOverlayOpen}
  inert={$eventsOverlayOpen ? undefined : true}
>
  <!-- Header -->
  <div
    class="flex items-center justify-between px-3 border-b border-[#1e293b] bg-[#0a0a0f] shrink-0"
    style="height: 44px;"
  >
    <div class="flex items-center gap-2">
      <span class="text-[9px] font-mono tracking-[0.15em] text-[#475569]">EVENTS</span>
      <span class="text-[9px] font-mono tracking-[0.1em] text-[#1e293b]">/</span>
      <span class="text-[9px] font-mono tracking-[0.15em] text-[#64748b]">{title}</span>
      {#if unread > 0}
        <span class="text-[9px] font-mono font-bold text-[#FFD700] bg-[#FFD700]/10 px-1 rounded">{unread}</span>
      {/if}
    </div>
    <div class="flex items-center gap-2">
      <span class="text-[9px] font-mono text-[#334155]">{merged.length}</span>
      <button
        onclick={() => eventsOverlayOpen.set(false)}
        class="text-[#475569] hover:text-[#94a3b8] transition-colors text-[11px] px-1"
        aria-label="Close events overlay"
      >✕</button>
    </div>
  </div>

  <!-- Entry list — delegates to EventStream for row rendering -->
  <EventStream rows={streamRows} newestFirst maxDisplay={MAX_ENTRIES} />

  <!-- Footer -->
  <div
    class="px-3 py-2 border-t border-[#1e293b] shrink-0 flex items-center gap-2"
    style="height: 28px;"
  >
    <span class="text-[9px] font-mono text-[#1e293b]">E · ESC to close</span>
  </div>
</div>
