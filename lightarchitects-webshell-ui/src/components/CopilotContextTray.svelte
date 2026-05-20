<script lang="ts">
  import type { CopilotContextSnapshot, ContextRetrievalStatus } from '$lib/types';

  interface Props {
    snapshot: CopilotContextSnapshot | null;
    status: ContextRetrievalStatus;
    onRefresh: () => void;
  }

  let { snapshot, status, onRefresh }: Props = $props();

  let expanded = $state(false);

  /** Rough byte total of all event payloads in the snapshot. */
  const totalBytes = $derived(
    snapshot
      ? snapshot.recentEvents.reduce(
          (sum, e) => sum + new TextEncoder().encode(JSON.stringify(e.event)).length,
          0,
        )
      : 0,
  );

  const eventCount = $derived(snapshot?.recentEvents.length ?? 0);
  const oversizeCount = $derived(snapshot?.oversizeIndices.length ?? 0);

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes}B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}K`;
    return `${(bytes / 1024 / 1024).toFixed(2)}M`;
  }

  /** Rough token estimate: 1 token ≈ 4 bytes. */
  function formatTokens(bytes: number): string {
    const t = Math.round(bytes / 4);
    if (t < 1000) return `~${t}t`;
    return `~${(t / 1000).toFixed(1)}kt`;
  }

  /** Colour tier based on total payload weight. */
  const byteClass = $derived(
    totalBytes > 65_536
      ? 'text-red-400'
      : totalBytes > 16_384
        ? 'text-amber-400'
        : 'text-emerald-500',
  );

  const statusDot = $derived(
    status === 'ready'
      ? { bg: 'bg-emerald-500', glow: '0 0 5px #22c55e', pulse: false }
      : status === 'capturing'
        ? { bg: 'bg-amber-400', glow: '0 0 5px rgba(251,191,36,0.8)', pulse: true }
        : status === 'error'
          ? { bg: 'bg-red-500', glow: '0 0 5px rgba(239,68,68,0.8)', pulse: false }
          : { bg: 'bg-[var(--la-text-dim)]', glow: 'none', pulse: false },
  );

  let spinning = $state(false);

  function handleRefresh() {
    spinning = true;
    onRefresh();
    setTimeout(() => { spinning = false; }, 600);
  }

  /** Format a source identifier for display (truncate at 12 chars). */
  function displaySource(source: string): string {
    return source.length > 12 ? source.slice(0, 11) + '…' : source;
  }

  /** Colour a source chip by its category. */
  function sourceColor(source: string): string {
    if (source.startsWith('AYIN')) return 'text-sky-400 border-sky-400/30 bg-sky-400/5';
    if (source.startsWith('CORSO') || source.startsWith('Supervisor')) return 'text-violet-400 border-violet-400/30 bg-violet-400/5';
    if (source.startsWith('SOUL')) return 'text-emerald-400 border-emerald-400/30 bg-emerald-400/5';
    if (source.startsWith('Copilot')) return 'text-[var(--la-focus-ring)] border-[var(--la-focus-ring)]/30 bg-[var(--la-focus-ring)]/5';
    if (source.startsWith('GitForest')) return 'text-lime-400 border-lime-400/30 bg-lime-400/5';
    if (source.startsWith('BuildRunner')) return 'text-orange-400 border-orange-400/30 bg-orange-400/5';
    return 'text-[var(--la-text-dim)] border-[var(--la-drawer-border)] bg-transparent';
  }
</script>

{#if eventCount > 0 || status !== 'idle'}
  <div class="context-tray shrink-0 border-t border-[var(--la-drawer-border)]/50 bg-[var(--la-bg-void)]/40">
    <!-- ── Compact status bar ─────────────────────────────── -->
    <div class="flex items-center gap-1.5 px-3 py-1 h-6">
      <!-- Status dot -->
      <div
        class="w-1.5 h-1.5 rounded-full shrink-0 {statusDot.bg} {statusDot.pulse ? 'animate-pulse' : ''}"
        style="box-shadow: {statusDot.glow};"
      ></div>

      <!-- Event count -->
      <span class="text-[9px] font-mono text-[var(--la-text-dim)] shrink-0">
        {eventCount}<span class="text-[var(--la-text-dim)]/60">ev</span>
      </span>

      <!-- Byte / token badge -->
      {#if totalBytes > 0}
        <span class="text-[9px] font-mono {byteClass} shrink-0" title="{formatBytes(totalBytes)} payload">
          {formatTokens(totalBytes)}
        </span>
      {/if}

      <!-- Oversize warning chips -->
      {#if oversizeCount > 0}
        <span
          class="text-[8px] font-mono px-1 py-px rounded bg-amber-400/10 text-amber-400 border border-amber-400/20 shrink-0"
          title="{oversizeCount} event{oversizeCount > 1 ? 's' : ''} exceed 4 KiB"
        >⚠ {oversizeCount}</span>
      {/if}

      <div class="flex-1"></div>

      <!-- Expand toggle -->
      {#if eventCount > 0}
        <button
          onclick={() => { expanded = !expanded; }}
          class="text-[8px] font-mono text-[var(--la-text-dim)] hover:text-[var(--la-text-label)] px-1 transition-colors"
          title="{expanded ? 'Collapse' : 'Inspect'} event context"
          aria-expanded={expanded}
        >
          {expanded ? '▴' : '▾'}
        </button>
      {/if}

      <!-- Refresh -->
      <button
        onclick={handleRefresh}
        class="text-[9px] text-[var(--la-text-dim)] hover:text-[var(--la-focus-ring)] transition-colors px-1 leading-none"
        title="Refresh context snapshot"
        aria-label="Refresh context"
      >
        <span class="inline-block {spinning ? 'spin-once' : ''}">↻</span>
      </button>
    </div>

    <!-- ── Expanded event list ─────────────────────────────── -->
    {#if expanded && snapshot}
      <div
        class="px-3 pb-2 max-h-36 overflow-y-auto space-y-px"
        role="list"
        aria-label="Buffered context events"
      >
        {#each snapshot.recentEvents as event, i (event.seq)}
          {@const isOversize = snapshot.oversizeIndices.includes(i)}
          <div
            class="flex items-baseline gap-1.5 font-mono text-[8px] leading-relaxed"
            role="listitem"
          >
            <span class="text-[var(--la-text-dim)]/50 w-6 text-right shrink-0">
              {event.seq}
            </span>
            <span class="px-1 py-px rounded border text-[7px] shrink-0 {sourceColor(event.source)}">
              {displaySource(event.source)}
            </span>
            {#if isOversize}
              <span class="text-amber-400/70 shrink-0" title="Payload exceeds 4 KiB">⚠</span>
            {/if}
            <span class="text-[var(--la-text-dim)]/60 truncate flex-1">
              {typeof event.event === 'object' && event.event !== null && 'type' in event.event
                ? String((event.event as Record<string, unknown>).type)
                : '…'}
            </span>
          </div>
        {/each}
      </div>
    {/if}

    <!-- ── Pre-flight error block ─────────────────────────── -->
    {#if status === 'error'}
      <div class="px-3 pb-1.5 flex items-center gap-1.5">
        <span class="text-[8px] text-red-400 font-mono">context snapshot failed — events not included</span>
        <button
          onclick={handleRefresh}
          class="text-[8px] text-[var(--la-text-dim)] hover:text-red-300 underline transition-colors"
        >retry</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .spin-once {
    animation: spin-once 0.5s ease-out;
  }

  @keyframes spin-once {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }

  .context-tray {
    transition: height 120ms ease-out;
  }

  /* Scrollbar styling for event list */
  .context-tray :global(::-webkit-scrollbar) {
    width: 3px;
  }
  .context-tray :global(::-webkit-scrollbar-track) {
    background: transparent;
  }
  .context-tray :global(::-webkit-scrollbar-thumb) {
    background: var(--la-drawer-border);
    border-radius: 2px;
  }
</style>
