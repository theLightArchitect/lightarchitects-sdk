<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import { authHeaders } from '$lib/auth';

  interface Props {
    handle: string;
    pollIntervalMs?: number;
  }

  let { handle, pollIntervalMs = 300 }: Props = $props();

  let termEl: HTMLDivElement | null = $state(null);
  let complete = $state(false);
  let exitCode: number | null = $state(null);
  let killed = $state(false);
  let error = $state('');

  let term: Terminal | null = null;
  let fitAddon: FitAddon | null = null;
  let cursor = 0;
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let ro: ResizeObserver | null = null;

  onMount(() => {
    if (!termEl) return;

    term = new Terminal({
      cursorBlink: false,
      fontSize: 12,
      fontFamily: '"Fira Code", monospace',
      theme: {
        background: '#0a0a0a',
        foreground: '#e2e8f0',
        cursor: '#FFD700',
      },
      scrollback: 10000,
      convertEol: true,
    });
    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(termEl);
    fitAddon.fit();

    ro = new ResizeObserver(() => fitAddon?.fit());
    ro.observe(termEl);

    pollTimer = setInterval(poll, pollIntervalMs);
    poll();
  });

  onDestroy(() => {
    if (pollTimer !== null) clearInterval(pollTimer);
    ro?.disconnect();
    term?.dispose();
  });

  async function poll() {
    if (complete) {
      if (pollTimer !== null) clearInterval(pollTimer);
      return;
    }
    try {
      const res = await fetch(
        `/api/exec/output/${encodeURIComponent(handle)}?cursor=${cursor}`,
        { headers: authHeaders() },
      );
      if (!res.ok) {
        error = `HTTP ${res.status}`;
        return;
      }
      const data = await res.json();
      for (const chunk of data.chunks as Array<{ seq: number; stream: string; line: string }>) {
        const prefix = chunk.stream === 'stderr' ? '\x1b[31m' : '';
        const reset = chunk.stream === 'stderr' ? '\x1b[0m' : '';
        term?.writeln(`${prefix}${chunk.line}${reset}`);
      }
      cursor = data.next_cursor as number;
      if (data.complete as boolean) {
        complete = true;
        exitCode = data.exit_code as number | null;
        killed = data.killed as boolean;
        if (pollTimer !== null) clearInterval(pollTimer);
      }
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="output-viewer">
  <div class="viewer-header">
    <span class="viewer-label">OUTPUT</span>
    <span class="handle-chip">{handle.slice(0, 8)}</span>
    {#if complete}
      {#if killed}
        <span class="status killed">KILLED</span>
      {:else}
        <span class="status" class:ok={exitCode === 0} class:fail={exitCode !== 0}>
          EXIT {exitCode ?? '?'}
        </span>
      {/if}
    {:else}
      <span class="status running">RUNNING</span>
    {/if}
    {#if error}
      <span class="status fail">{error}</span>
    {/if}
  </div>
  <div class="term-wrap" bind:this={termEl}></div>
</div>

<style>
  .output-viewer {
    display: flex;
    flex-direction: column;
    background: #0a0a0a;
    border: 1px solid var(--la-drawer-border, #2a2a3a);
    border-radius: 6px;
    overflow: hidden;
    min-height: 300px;
  }

  .viewer-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--la-bg-elev-1, #111118);
    border-bottom: 1px solid var(--la-drawer-border, #2a2a3a);
    flex-shrink: 0;
  }

  .viewer-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--la-text-label, #8888aa);
    letter-spacing: 0.08em;
  }

  .handle-chip {
    font-size: 10px;
    font-family: monospace;
    color: var(--la-text-dim, #555570);
  }

  .status {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 3px;
    letter-spacing: 0.06em;
  }

  .status.running { background: #1a3a1a; color: #4ade80; }
  .status.ok      { background: #1a3a1a; color: #4ade80; }
  .status.fail    { background: #3a1a1a; color: #f87171; }
  .status.killed  { background: #3a2a1a; color: #fb923c; }

  .term-wrap {
    flex: 1;
    overflow: hidden;
    padding: 4px;
  }

  :global(.term-wrap .xterm) { height: 100%; }
</style>
