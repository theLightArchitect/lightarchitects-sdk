<script lang="ts">
  // WHY: operator_experience_layer phase-3/phase-4 widget — wave dispatch progress
  // + per-runner test result. AYIN span agent.skill.build forwarded via SSE BuildUpdate.

  interface WaveEntry {
    id: string;
    phase: string;
    status: 'pending' | 'running' | 'complete' | 'error';
    runner?: string;
    durationMs?: number;
    testDelta?: number;
  }

  let { waves = [] }: { waves?: WaveEntry[] } = $props();

  function statusIcon(s: WaveEntry['status']): string {
    if (s === 'complete') return '✓';
    if (s === 'running') return '●';
    if (s === 'error') return '✗';
    return '○';
  }
</script>

<div class="wave-strip" data-testid="wave-strip">
  {#if waves.length === 0}
    <span class="ws-empty">No waves dispatched</span>
  {:else}
    {#each waves as w}
      <div
        class="ws-entry"
        class:ws-running={w.status === 'running'}
        class:ws-complete={w.status === 'complete'}
        class:ws-error={w.status === 'error'}
      >
        <span class="ws-icon">{statusIcon(w.status)}</span>
        <span class="ws-phase">{w.phase}</span>
        {#if w.runner}<span class="ws-runner">{w.runner}</span>{/if}
        {#if w.durationMs !== undefined}<span class="ws-dur">{(w.durationMs / 1000).toFixed(1)}s</span>{/if}
        {#if w.testDelta !== undefined && w.testDelta > 0}<span class="ws-tests">+{w.testDelta}t</span>{/if}
      </div>
    {/each}
  {/if}
</div>

<style>
  .wave-strip {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    background: var(--la-bg-elev-1, #111);
    border-bottom: 1px solid var(--la-border, #333);
    overflow-x: auto;
    font-size: 10px;
    font-family: var(--la-font-chrome, monospace);
    color: var(--la-text-dim, #888);
    white-space: nowrap;
  }

  .ws-entry {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 2px 6px;
    border: 1px solid var(--la-border-dim, #222);
    border-radius: 3px;
  }

  .ws-running { border-color: var(--la-focus-ring, #FFD700); color: var(--la-focus-ring, #FFD700); }
  .ws-complete { border-color: var(--la-agent-knowledge, #4caf50); color: var(--la-agent-knowledge, #4caf50); }
  .ws-error { border-color: var(--la-agent-security, #f55); color: var(--la-agent-security, #f55); }

  .ws-icon { font-size: 9px; }
  .ws-phase { font-weight: 500; }
  .ws-runner { color: var(--la-text-mute, #555); font-size: 9px; }
  .ws-dur { color: var(--la-text-mute, #555); }
  .ws-tests { color: var(--la-agent-testing, #4fc3f7); font-size: 9px; }
  .ws-empty { color: var(--la-text-mute, #555); font-style: italic; font-size: 10px; font-family: var(--la-font-chrome, monospace); }
</style>
