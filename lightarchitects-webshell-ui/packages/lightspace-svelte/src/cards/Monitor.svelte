<!--
  @component Monitor — build topology + test ratchet sparkline.
  @reads content as BuildTopologyContent (monitor_kind: "build_topology")
         Phases rendered as horizontal status bars; active phase pulses.
         test_ratchet rendered as a mini count sparkline below the bars.
-->
<script lang="ts">
  import type { BuildTopologyContent } from '../types';

  let { data }: { data: unknown } = $props();
  const d = $derived((data as BuildTopologyContent | null) ?? null);
  const phases = $derived(d?.phases ?? []);
  const ratchet = $derived(d?.test_ratchet ?? []);

  const STATUS_COLOR: Record<string, string> = {
    completed: 'var(--ls-acc-green)',
    active:    'var(--ls-acc)',
    failed:    'var(--ls-acc-red)',
    pending:   'var(--ls-sunken)',
  };
</script>

<div class="ls-mon-wrap">
  <div class="ls-mon-phases">
    {#each phases as ph}
      <div
        class="ls-mon-phase"
        class:ls-mon-active={ph.status === 'active'}
        style="--ph-color: {STATUS_COLOR[ph.status] ?? 'var(--ls-border)'}"
      >
        <span class="ls-mon-label">{ph.label}</span>
        <div class="ls-mon-bar"><div class="ls-mon-fill"></div></div>
      </div>
    {/each}
    {#if phases.length === 0}
      <div class="ls-mon-empty">awaiting topology…</div>
    {/if}
  </div>
  {#if ratchet.length > 0}
    <div class="ls-mon-ratchet">
      {#each ratchet as r}
        <span class="ls-mon-ratchet-item" title="Wave {r.wave}: {r.count} tests">{r.count}</span>
      {/each}
    </div>
  {/if}
</div>

<style>
.ls-mon-wrap { display: flex; flex-direction: column; gap: 6px; }
.ls-mon-phases { display: flex; flex-direction: column; gap: 4px; }
.ls-mon-phase { display: flex; align-items: center; gap: 6px; }
.ls-mon-label { font-size: 9px; color: var(--ls-text-mute); min-width: 56px; text-align: right; }
.ls-mon-bar { flex: 1; height: 4px; background: var(--ls-sunken); border-radius: 2px; overflow: hidden; }
.ls-mon-fill { height: 100%; background: var(--ph-color, var(--ls-border)); width: 100%; transition: background 0.4s; }
.ls-mon-active .ls-mon-fill { animation: ls-pulse 1.4s ease-in-out infinite; }
@keyframes ls-pulse {
  0%, 100% { opacity: 1; }
  50%       { opacity: 0.45; }
}
.ls-mon-empty { font-size: 9px; color: var(--ls-text-ghost); }
.ls-mon-ratchet { display: flex; gap: 4px; padding-top: 4px; border-top: 1px solid var(--ls-border); }
.ls-mon-ratchet-item { font-size: 9px; font-family: var(--ls-font-code); color: var(--ls-text-dim); padding: 1px 4px; background: var(--ls-sunken); border: 1px solid var(--ls-border); border-radius: 2px; }
</style>
