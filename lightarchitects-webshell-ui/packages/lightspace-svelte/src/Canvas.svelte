<!--
  @component Canvas — per-session card grid bound to the Map-based canvasStore.
  @reads canvasStore (Map<string, CardData>) from ./stores
         Card kind dispatches to 6 card components; unknown kinds render a fallback.
  @mutates none (read-only view; mutations come from sse.ts dispatch)
-->
<script lang="ts">
  import { canvasStore } from './stores';
  import type { CardData } from './types';
  import Monitor    from './cards/Monitor.svelte';
  import Instrument from './cards/Instrument.svelte';
  import Trace      from './cards/Trace.svelte';
  import Research   from './cards/Research.svelte';
  import Artifact   from './cards/Artifact.svelte';
  import BranchLane from './cards/BranchLane.svelte';

  const cards = $derived([...$canvasStore.values()].filter(c => c.state === 'attached'));

  // Determine CSS column span by card kind
  const KIND_SPAN: Record<string, string> = {
    monitor:    '3',
    instrument: '3',
    trace:      '4',
    thinking:   '6',
    toolcall:   '4',
    bash:       '4',
    agentspawn: '4',
    diff:       '12',
    artifact:   '3',
    research:   '6',
    archgallery:'12',
    branchlane: '12',
  };
</script>

<div class="ls-canvas-grid">
  {#each cards as card (card.id)}
    <div
      class="ls-canvas-card"
      style="--col-span: {KIND_SPAN[card.kind] ?? '4'}"
    >
      <header class="ls-canvas-card-hd">
        <span class="ls-canvas-card-kind">{card.kind}</span>
        <span class="ls-canvas-card-title">{card.title}</span>
      </header>
      <div class="ls-canvas-card-body">
        {#if card.kind === 'monitor'}
          <Monitor data={card.content} />
        {:else if card.kind === 'instrument'}
          <Instrument data={card.content} />
        {:else if card.kind === 'trace' || card.kind === 'thinking' || card.kind === 'toolcall' || card.kind === 'bash'}
          <Trace data={card.content} />
        {:else if card.kind === 'research' || card.kind === 'archgallery'}
          <Research data={card.content} />
        {:else if card.kind === 'artifact' || card.kind === 'diff'}
          <Artifact data={card.content} />
        {:else if card.kind === 'branchlane' || card.kind === 'agentspawn'}
          <BranchLane data={card.content} />
        {:else}
          <div class="ls-canvas-fallback">{card.kind}</div>
        {/if}
      </div>
    </div>
  {/each}
  {#if cards.length === 0}
    <div class="ls-canvas-empty">canvas empty — awaiting events…</div>
  {/if}
</div>

<style>
.ls-canvas-grid {
  display: grid;
  grid-template-columns: repeat(12, 1fr);
  gap: 8px;
  width: 100%;
  padding: 8px;
  box-sizing: border-box;
  overflow-y: auto;
}
.ls-canvas-card {
  grid-column: span var(--col-span, 4);
  background: var(--ls-card);
  border: 1px solid var(--ls-border);
  border-radius: 4px;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  min-height: 80px;
}
.ls-canvas-card-hd {
  display: flex;
  gap: 6px;
  align-items: baseline;
  padding: 5px 8px 3px;
  border-bottom: 1px solid var(--ls-border);
}
.ls-canvas-card-kind {
  font-size: 7px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--ls-acc);
}
.ls-canvas-card-title {
  font-size: 9px;
  color: var(--ls-text-dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ls-canvas-card-body  { padding: 8px; flex: 1; }
.ls-canvas-fallback   { font-size: 9px; color: var(--ls-text-ghost); }
.ls-canvas-empty      { grid-column: 1 / -1; font-size: 10px; color: var(--ls-text-ghost); text-align: center; padding: 24px; }
</style>
