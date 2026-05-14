<script lang="ts">
  // Multi-source log tail pane with stream filter (stdout / stderr / all).
  // Accepts lines from exec output handles; parent feeds new lines as they arrive.

  interface LogLine {
    seq: number;
    stream: 'stdout' | 'stderr';
    line: string;
  }

  interface Props {
    lines: LogLine[];
    maxDisplay?: number;
    title?: string;
  }

  let { lines, maxDisplay = 200, title = 'LOG' }: Props = $props();

  let filter = $state<'all' | 'stdout' | 'stderr'>('all');
  let searchQuery = $state('');

  let visible = $derived(
    lines
      .filter((l) => filter === 'all' || l.stream === filter)
      .filter((l) => !searchQuery || l.line.toLowerCase().includes(searchQuery.toLowerCase()))
      .slice(-maxDisplay),
  );

  let scrollEl: HTMLDivElement | null = $state(null);
  let autoScroll = $state(true);

  $effect(() => {
    if (autoScroll && scrollEl) {
      scrollEl.scrollTop = scrollEl.scrollHeight;
    }
  });

  function onScroll() {
    if (!scrollEl) return;
    const atBottom = scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight < 32;
    autoScroll = atBottom;
  }
</script>

<div class="log-pane">
  <div class="pane-header">
    <span class="pane-label">{title}</span>
    <div class="filter-group" role="group" aria-label="Stream filter">
      {#each (['all', 'stdout', 'stderr'] as const) as f}
        <button
          class="filter-btn"
          class:active={filter === f}
          onclick={() => (filter = f)}
          aria-pressed={filter === f}
        >{f}</button>
      {/each}
    </div>
    <input
      class="search-input"
      type="search"
      placeholder="Filter…"
      bind:value={searchQuery}
      aria-label="Filter log lines"
    />
    <span class="line-count">{visible.length}</span>
  </div>

  <div
    class="pane-body"
    bind:this={scrollEl}
    onscroll={onScroll}
    role="log"
    aria-live="polite"
    aria-label="Process output log"
  >
    {#each visible as entry (entry.seq)}
      <div class="log-line" class:stderr={entry.stream === 'stderr'}>
        <span class="seq">{entry.seq}</span>
        <span class="stream-tag" class:err={entry.stream === 'stderr'}>{entry.stream[0]}</span>
        <span class="content">{entry.line}</span>
      </div>
    {/each}
    {#if visible.length === 0}
      <p class="empty">No log lines yet.</p>
    {/if}
  </div>

  {#if !autoScroll}
    <button
      class="scroll-btn"
      onclick={() => {
        autoScroll = true;
        if (scrollEl) scrollEl.scrollTop = scrollEl.scrollHeight;
      }}
    >↓ Jump to bottom</button>
  {/if}
</div>

<style>
  .log-pane {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--la-drawer-border, #2a2a3a);
    border-radius: 6px;
    overflow: hidden;
    background: #0a0a0a;
    position: relative;
  }

  .pane-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    background: var(--la-bg-elev-1, #111118);
    border-bottom: 1px solid var(--la-drawer-border, #2a2a3a);
    flex-shrink: 0;
    flex-wrap: wrap;
  }

  .pane-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--la-text-label, #8888aa);
    letter-spacing: 0.08em;
  }

  .filter-group {
    display: flex;
    gap: 2px;
  }

  .filter-btn {
    font-size: 10px;
    padding: 2px 6px;
    border-radius: 3px;
    background: none;
    border: 1px solid var(--la-drawer-border, #2a2a3a);
    color: var(--la-text-dim, #555570);
    cursor: pointer;
  }

  .filter-btn.active {
    background: var(--la-accent-dim, #3b1f6a);
    border-color: var(--la-accent, #a78bfa);
    color: var(--la-accent, #a78bfa);
  }

  .filter-btn:hover:not(.active) { color: var(--la-text-label, #8888aa); }

  .search-input {
    flex: 1;
    min-width: 80px;
    font-size: 11px;
    padding: 2px 6px;
    background: var(--la-bg-frame, #0d0d14);
    border: 1px solid var(--la-drawer-border, #2a2a3a);
    border-radius: 3px;
    color: var(--la-text-primary, #e2e8f0);
    outline: none;
  }

  .search-input:focus { border-color: var(--la-accent, #a78bfa); }

  .line-count {
    font-size: 10px;
    color: var(--la-text-dim, #555570);
    min-width: 24px;
    text-align: right;
  }

  .pane-body {
    flex: 1;
    overflow-y: auto;
    max-height: 350px;
    padding: 4px 0;
  }

  .log-line {
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 1px 10px;
    font-family: monospace;
    font-size: 11px;
    color: var(--la-text-primary, #e2e8f0);
    line-height: 1.5;
  }

  .log-line.stderr { background: rgba(248, 113, 113, 0.04); }

  .seq {
    color: var(--la-text-dim, #555570);
    font-size: 9px;
    min-width: 28px;
    user-select: none;
  }

  .stream-tag {
    font-size: 9px;
    font-weight: 700;
    width: 10px;
    user-select: none;
    color: #4ade80;
    flex-shrink: 0;
  }

  .stream-tag.err { color: #f87171; }

  .content {
    flex: 1;
    word-break: break-all;
    white-space: pre-wrap;
  }

  .empty {
    font-size: 11px;
    color: var(--la-text-dim, #555570);
    padding: 12px 16px;
  }

  .scroll-btn {
    position: absolute;
    bottom: 8px;
    right: 12px;
    font-size: 10px;
    padding: 4px 8px;
    background: var(--la-bg-elev-1, #111118);
    border: 1px solid var(--la-drawer-border, #2a2a3a);
    border-radius: 4px;
    color: var(--la-text-label, #8888aa);
    cursor: pointer;
  }

  .scroll-btn:hover { color: var(--la-accent, #a78bfa); border-color: var(--la-accent, #a78bfa); }
</style>
